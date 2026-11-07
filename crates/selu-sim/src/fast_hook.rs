//! Exact count-hook fast path. Faults still use mlua's protected hook boundary.
use crate::core::{Ctx, Shared, SharedBudget, lock};
use mlua::{Lua, Thread, ffi};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};

thread_local! {
    // Lua is thread-confined. Owners outlive lua_close; coroutine addresses are
    // only used as keys, never dereferenced by this map.
    // The active Lua state repeats for millions of hooks. Avoid hashing its
    // address each time; Owner::drop invalidates the cache before freeing State.
    static ACTIVE: Cell<(usize,*const State)> = const {Cell::new((0,std::ptr::null()))};
    static TARGETS: RefCell<HashMap<usize, *const State>> = RefCell::new(HashMap::new());
}
struct State {
    budget: SharedBudget,
    stale: Shared<std::collections::HashSet<String>>,
    active: Cell<usize>,
    original: Cell<Option<ffi::lua_Hook>>,
    quantum: Cell<u32>,
    #[cfg(debug_assertions)]
    samples: bool,
    #[cfg(debug_assertions)]
    sample_credit: Cell<u32>,
}
struct Owner(Box<State>);
#[cfg(debug_assertions)]
struct LegacyLogged;
impl Drop for Owner {
    fn drop(&mut self) {
        let pointer = &*self.0 as *const State;
        let _ = ACTIVE.try_with(|active| {
            if active.get().1 == pointer {
                active.set((0, std::ptr::null()));
            }
        });
        let _ =
            TARGETS.try_with(|targets| targets.borrow_mut().retain(|_, value| *value != pointer));
    }
}

unsafe extern "C-unwind" fn hook(lua: *mut ffi::lua_State, debug: *mut ffi::lua_Debug) {
    let cached = ACTIVE.with(Cell::get);
    let pointer = if cached.0 == lua as usize {
        Some(cached.1)
    } else {
        TARGETS.with(|targets| targets.borrow().get(&(lua as usize)).copied())
    };
    let Some(pointer) = pointer else {
        // A newly created coroutine inherits the native hook pointer. As with
        // mlua, it is armed only by the coordinated resume boundary.
        unsafe { ffi::lua_sethook(lua, None, 0, 0) };
        return;
    };
    // SAFETY: the VM's app-data Owner retains this allocation through lua_close.
    // Owner::drop removes only its own entries, including collected coroutines.
    let state = unsafe { &*pointer };
    if state.active.get() != lua as usize {
        // Match mlua's single active hook target, including inherited hooks.
        unsafe { ffi::lua_sethook(lua, None, 0, 0) };
        return;
    }
    let quantum = state.quantum.get();
    #[cfg(debug_assertions)]
    if state.samples {
        let credit = state.sample_credit.get().saturating_add(quantum);
        if credit >= 10_000 {
            state.sample_credit.set(credit - 10_000);
            unsafe { crate::profile::lua_sample(lua) };
        } else {
            state.sample_credit.set(credit);
        }
    }
    let stale = !lock(&state.stale).is_empty();
    let delegate = {
        let mut budget = state.budget.borrow_mut();
        if stale
            || budget.exhausted
            || budget.used.saturating_add(u64::from(quantum))
                > budget
                    .limit
                    .saturating_mul(u64::from(crate::core::HOOK_INTERVAL))
        {
            true
        } else {
            budget.used = budget.used.saturating_add(u64::from(quantum));
            false
        }
    };
    if delegate {
        // No lock, RefCell guard, or owning Rust value crosses a Lua longjmp.
        if let Some(original) = state.original.get() {
            unsafe { original(lua, debug) };
        }
    } else {
        crate::profile::instruction_hook(quantum);
    }
}

pub(crate) fn install(lua: &Lua, budget: SharedBudget, quantum: u32, thread: Option<Thread>) {
    #[cfg(debug_assertions)]
    if std::env::var_os("SELU_SIM_PROFILE_LEGACY_HOOK").is_some() {
        if std::env::var_os("SELU_SIM_PROFILE").is_some()
            && lua.app_data_ref::<LegacyLogged>().is_none()
        {
            eprintln!("SELU_SIM_PROFILE count hook: legacy mlua boundary comparison active");
            lua.set_app_data(LegacyLogged);
        }
        return;
    }
    if lua.app_data_ref::<Owner>().is_none() {
        let Some(ctx) = lua.app_data_ref::<Ctx>() else {
            return;
        };
        let stale = ctx.stale_sources.clone();
        drop(ctx);
        #[cfg(debug_assertions)]
        if std::env::var_os("SELU_SIM_PROFILE").is_some() {
            eprintln!(
                "SELU_SIM_PROFILE count hook: native exact accounting active; faults delegate to mlua"
            );
        }
        lua.set_app_data(Owner(Box::new(State {
            budget,
            stale,
            active: Cell::new(0),
            original: Cell::new(None),
            quantum: Cell::new(quantum),
            #[cfg(debug_assertions)]
            samples: std::env::var_os("SELU_SIM_PROFILE_LUA_SAMPLES").is_some(),
            #[cfg(debug_assertions)]
            sample_credit: Cell::new(0),
        })));
    }
    let owner = lua.app_data_ref::<Owner>().unwrap();
    let pointer = &*owner.0 as *const State;
    // SAFETY: installation touches only the protected argument stack and the
    // native hook pointer. It runs no Lua and leaves no stack values behind.
    let args = mlua::MultiValue::from_vec(thread.into_iter().map(mlua::Value::Thread).collect());
    unsafe {
        lua.exec_raw::<()>(args, |caller| {
            let target = if ffi::lua_gettop(caller) == 0 {
                caller
            } else {
                ffi::lua_tothread(caller, 1)
            };
            let original = ffi::lua_gethook(target);
            owner.0.original.set(original);
            owner.0.active.set(target as usize);
            ACTIVE.with(|active| active.set((target as usize, pointer)));
            owner.0.quantum.set(quantum);
            TARGETS.with(|targets| {
                targets.borrow_mut().insert(target as usize, pointer);
            });
            ffi::lua_sethook(target, Some(hook), ffi::LUA_MASKCOUNT, quantum as i32);
            ffi::lua_settop(caller, 0);
        })
    }
    .expect("native hook installation");
}

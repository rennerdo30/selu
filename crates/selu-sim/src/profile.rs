//! Development-only startup measurements. Enable with SELU_SIM_PROFILE=1.
//! Durations are exclusive: nested owners are subtracted, never added twice.
#[cfg(debug_assertions)]
mod enabled {
    use std::{
        cell::RefCell,
        collections::BTreeMap,
        time::{Duration, Instant},
    };
    #[derive(Default)]
    struct Data {
        active: bool,
        stack: Vec<Duration>,
        totals: BTreeMap<&'static str, (Duration, u64)>,
        counts: BTreeMap<&'static str, u64>,
        calls: bool,
        sites: BTreeMap<usize, (String, u64)>,
        lua_samples: BTreeMap<(String, i32), u64>,
        cpu_start: Option<Duration>,
    }
    thread_local! {
        static DATA: RefCell<Data> = RefCell::new(Data {
            active: std::env::var_os("SELU_SIM_PROFILE").is_some(),
            calls: std::env::var_os("SELU_SIM_PROFILE_CALLS").is_some(),
            cpu_start: if std::env::var_os("SELU_SIM_PROFILE").is_some() {cpu_time()} else {None}, ..Data::default()
        });
        static HOOKS: std::cell::Cell<(u64,u64)> = const { std::cell::Cell::new((0,0)) };
    }
    #[cfg(windows)]
    fn cpu_time() -> Option<Duration> {
        #[repr(C)]
        #[derive(Default)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            #[link_name = "GetCurrentThread"]
            fn current_thread() -> *mut std::ffi::c_void;
            #[link_name = "GetThreadTimes"]
            fn thread_times(
                handle: *mut std::ffi::c_void,
                created: *mut FileTime,
                exited: *mut FileTime,
                kernel: *mut FileTime,
                user: *mut FileTime,
            ) -> i32;
        }
        let (mut created, mut exited, mut kernel, mut user) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        // SAFETY: the pseudo-handle refers to this thread and all output
        // pointers reference live repr(C) FILETIME-compatible stack values.
        if unsafe {
            thread_times(
                current_thread(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return None;
        }
        let ticks = |value: FileTime| (u64::from(value.high) << 32) | u64::from(value.low);
        Some(Duration::from_nanos(
            ticks(kernel)
                .saturating_add(ticks(user))
                .saturating_mul(100),
        ))
    }
    #[cfg(not(windows))]
    fn cpu_time() -> Option<Duration> {
        None
    }
    pub fn instruction_hook(quantum: u32) {
        if !DATA.with(|data| data.borrow().active) {
            return;
        }
        HOOKS.with(|hooks| {
            let (precise, coarse) = hooks.get();
            hooks.set(if quantum == 1 {
                (precise + 1, coarse)
            } else {
                (precise, coarse + 1)
            });
        });
    }
    /// Instruction samples, not wall-time samples. No Lua code is executed.
    ///
    /// # Safety
    /// `lua` must be the live state currently invoking its count hook.
    pub unsafe fn lua_sample(lua: *mut mlua::ffi::lua_State) {
        // SAFETY: called only inside a live count hook. S/l metadata queries
        // neither push values nor run Lua. Source is VM-owned, NUL terminated.
        unsafe {
            let mut debug = std::mem::zeroed::<mlua::ffi::lua_Debug>();
            if mlua::ffi::lua_getstack(lua, 0, &mut debug) != 0
                && mlua::ffi::lua_getinfo(lua, c"Sl".as_ptr(), &mut debug) != 0
                && !debug.source.is_null()
            {
                let source = std::ffi::CStr::from_ptr(debug.source).to_string_lossy();
                DATA.with(|data| {
                    let mut data = data.borrow_mut();
                    *data
                        .lua_samples
                        .entry((source.into_owned(), debug.currentline))
                        .or_default() += 1;
                });
            }
        }
    }
    pub fn secure_call(function: &mlua::Function, rawget: bool) {
        DATA.with(|data| {
            let mut data = data.borrow_mut();
            if !data.active || !data.calls {
                return;
            }
            let site = data
                .sites
                .entry(function.to_pointer() as usize)
                .or_insert_with(|| {
                    let info = function.info();
                    (
                        if rawget {
                            "builtin rawget".into()
                        } else {
                            format!(
                                "{}:{}",
                                info.short_src.unwrap_or_default(),
                                info.line_defined.unwrap_or_default()
                            )
                        },
                        0,
                    )
                });
            site.1 += 1;
        });
    }
    pub fn report_hooks(label: &str) {
        if DATA.with(|data| data.borrow().active) {
            HOOKS.with(|hooks| {
                eprintln!(
                    "PROFILE_HOOKS {label}: precise={} coarse={}",
                    hooks.get().0,
                    hooks.get().1
                )
            });
        }
    }
    pub struct Span(Option<(Instant, &'static str)>);
    impl Span {
        pub fn new(name: &'static str) -> Self {
            Self(DATA.with(|data| {
                let mut data = data.borrow_mut();
                if !data.active {
                    return None;
                }
                data.stack.push(Duration::ZERO);
                Some((Instant::now(), name))
            }))
        }
    }
    impl Drop for Span {
        fn drop(&mut self) {
            let Some((start, name)) = self.0 else {
                return;
            };
            let elapsed = start.elapsed();
            DATA.with(|data| {
                let mut data = data.borrow_mut();
                let children = data.stack.pop().unwrap_or_default();
                if let Some(parent) = data.stack.last_mut() {
                    *parent += elapsed;
                }
                let entry = data.totals.entry(name).or_default();
                entry.0 += elapsed.saturating_sub(children);
                entry.1 += 1;
            });
        }
    }
    pub fn count(name: &'static str) {
        DATA.with(|data| {
            let mut data = data.borrow_mut();
            if data.active {
                *data.counts.entry(name).or_default() += 1;
            }
        });
    }
    pub fn report() {
        report_hooks("total");
        DATA.with(|data| {
            let data = data.borrow();
            if !data.active {
                return;
            }
            eprintln!(
                "SELU_SIM_PROFILE active; exclusive durations; {} spans still active",
                data.stack.len()
            );
            if let (Some(start),Some(end))=(data.cpu_start,cpu_time()) {
                eprintln!("PROFILE_WORKER_CPU since first span: {:.6}s (scheduled CPU time, not wall time)",end.saturating_sub(start).as_secs_f64());
            }
            for (name, (duration, calls)) in &data.totals {
                eprintln!(
                    "PROFILE {name}: {:.6}s ({calls} calls)",
                    duration.as_secs_f64()
                );
            }
            for (name, count) in &data.counts {
                eprintln!("PROFILE_COUNT {name}: {count}");
            }
            if !data.lua_samples.is_empty() {
                eprintln!("SELU_SIM_PROFILE Lua instruction sampling active (one per 10000 instructions; not wall time)");
                let mut samples=data.lua_samples.iter().collect::<Vec<_>>();samples.sort_by_key(|(_,count)|std::cmp::Reverse(**count));
                for ((source,line),count) in samples.into_iter().take(25) {eprintln!("PROFILE_LUA_SAMPLE {count}: {source}:{line}");}
            }
            if data.calls {
                let mut grouped = BTreeMap::<&str, u64>::new();
                for (name, count) in data.sites.values() {
                    *grouped.entry(name).or_default() += count;
                }
                let mut sites = grouped.into_iter().collect::<Vec<_>>();
                sites.sort_by_key(|site| std::cmp::Reverse(site.1));
                for (name, count) in sites.into_iter().take(25) {
                    eprintln!("PROFILE_SECURE_CALL {count}: {name}");
                }
            }
        });
    }
}
#[cfg(debug_assertions)]
pub use enabled::*;
#[cfg(not(debug_assertions))]
pub struct Span;
#[cfg(not(debug_assertions))]
impl Span {
    pub fn new(_: &'static str) -> Self {
        Self
    }
}
#[cfg(not(debug_assertions))]
pub fn count(_: &'static str) {}
#[cfg(not(debug_assertions))]
pub fn report() {}
#[cfg(not(debug_assertions))]
pub fn instruction_hook(_: u32) {}
#[cfg(not(debug_assertions))]
pub fn secure_call(_: &mlua::Function, _: bool) {}
#[cfg(not(debug_assertions))]
pub fn report_hooks(_: &str) {}

/// A bounded diagnostic A/B arm, never enabled by the product settings.
pub(crate) struct GcPause<'a>(Option<&'a mlua::Lua>);
impl<'a> GcPause<'a> {
    pub fn new(lua: &'a mlua::Lua) -> Self {
        #[cfg(debug_assertions)]
        if std::env::var_os("SELU_SIM_PROFILE_GC_PAUSE").is_some() {
            eprintln!(
                "SELU_SIM_PROFILE_GC_PAUSE active: paused during source construction, full collection on exit"
            );
            lua.gc_stop();
            return Self(Some(lua));
        }
        let _ = lua;
        Self(None)
    }
}
impl Drop for GcPause<'_> {
    fn drop(&mut self) {
        if let Some(lua) = self.0.take() {
            let _profile = Span::new("gc.final_collection");
            lua.gc_restart();
            if let Err(error) = lua.gc_collect() {
                eprintln!("GC measurement cleanup: {error}");
            }
        }
    }
}

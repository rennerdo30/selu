//! VM-owned handles, with canonical bidirectional transport for frame methods.
use mlua::{Function, Lua, MultiValue, RegistryKey, Table, Value};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
type Key = (usize, usize, usize);
#[derive(Default)]
pub(crate) struct Cache {
    handles: HashMap<Key, Rc<RegistryKey>>,
    vms: Vec<Lua>,
    closed: bool,
}
pub(crate) type State = Rc<RefCell<Cache>>;
pub(crate) fn state(lua: &Lua) -> State {
    if let Some(s) = lua.app_data_ref::<State>() {
        return s.clone();
    }
    let s = State::default();
    lua.set_app_data(s.clone());
    s
}
pub(crate) fn clear(lua: &Lua) {
    if let Some(s) = lua.remove_app_data::<State>() {
        let vms = {
            let mut s = s.borrow_mut();
            s.closed = true;
            s.handles.clear();
            std::mem::take(&mut s.vms)
        };
        for vm in vms {
            let _ = vm.gc_collect();
        }
    }
}
fn id(lua: &Lua) -> usize {
    lua.globals().to_pointer() as usize
}
pub(crate) fn value(v: Value, from: &Lua, to: &Lua, s: &State) -> mlua::Result<Value> {
    if id(from) == id(to) {
        return Ok(v);
    }
    let pointer = match &v {
        Value::Table(t) => t.to_pointer() as usize,
        Value::Function(f) => f.to_pointer() as usize,
        _ => return selu_sim::session::HostValue::capture(&v)?.to_lua(to),
    };
    let key = (id(from), id(to), pointer);
    if let Some(k) = s.borrow().handles.get(&key) {
        return to.registry_value(k);
    }
    let original = Rc::new(from.create_registry_value(v.clone())?);
    let origin = from.weak();
    let destination = to.weak();
    let cache = s.clone();
    let held = original.clone();
    let proxy = match v {
        Value::Function(_) => Value::Function(to.create_function(move |_, args: MultiValue| {
            let (origin, destination) = upgrade(&origin, &destination, &cache)?;
            let args = multi(args, &destination, &origin, &cache)?;
            let f: Function = origin.registry_value(&held)?;
            let ctx = origin.app_data_ref::<selu_sim::Ctx>().unwrap().clone();
            let result = selu_sim::core::dispatch(&origin, &ctx, || f.call::<MultiValue>(args))?;
            multi(result, &origin, &destination, &cache)
        })?),
        Value::Table(ref table) => {
            let proxy = to.create_table()?;
            let meta = to.create_table()?;
            if !table
                .raw_get::<Value>(selu_sim::frames::arena::identity_key())?
                .is_nil()
            {
                meta.raw_set("__selu_test_frame", true)?;
            }
            meta.set(
                "__index",
                to.create_function(move |_, (_, k): (Value, Value)| {
                    let (origin, destination) = upgrade(&origin, &destination, &cache)?;
                    let t: Table = origin.registry_value(&held)?;
                    let k = value(k, &destination, &origin, &cache)?;
                    value(t.get(k)?, &origin, &destination, &cache)
                })?,
            )?;
            let origin = from.weak();
            let destination = to.weak();
            let cache = s.clone();
            let held = original.clone();
            meta.set(
                "__newindex",
                to.create_function(move |_, (_, k, v): (Value, Value, Value)| {
                    let (origin, destination) = upgrade(&origin, &destination, &cache)?;
                    let t: Table = origin.registry_value(&held)?;
                    t.set(
                        value(k, &destination, &origin, &cache)?,
                        value(v, &destination, &origin, &cache)?,
                    )
                })?,
            )?;
            proxy.set_metatable(Some(meta));
            Value::Table(proxy)
        }
        _ => unreachable!(),
    };
    let proxy_pointer = match &proxy {
        Value::Table(t) => t.to_pointer(),
        Value::Function(f) => f.to_pointer(),
        _ => unreachable!(),
    } as usize;
    let proxy_key = Rc::new(to.create_registry_value(proxy.clone())?);
    let mut s = s.borrow_mut();
    s.handles.insert(key, proxy_key);
    s.handles
        .insert((id(to), id(from), proxy_pointer), original);
    if !s.vms.iter().any(|vm| id(vm) == id(from)) {
        s.vms.push(from.clone());
    }
    if !s.vms.iter().any(|vm| id(vm) == id(to)) {
        s.vms.push(to.clone());
    }
    Ok(proxy)
}
pub(crate) fn multi(v: MultiValue, from: &Lua, to: &Lua, s: &State) -> mlua::Result<MultiValue> {
    v.into_iter().map(|v| value(v, from, to, s)).collect()
}

fn upgrade(from: &mlua::WeakLua, to: &mlua::WeakLua, s: &State) -> mlua::Result<(Lua, Lua)> {
    if s.borrow().closed {
        return Err(mlua::Error::runtime("test bridge is closed"));
    }
    Ok((
        from.try_upgrade()
            .ok_or_else(|| mlua::Error::runtime("test VM has ended"))?,
        to.try_upgrade()
            .ok_or_else(|| mlua::Error::runtime("test VM has ended"))?,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cross_vm_callback_handles_are_released() {
        let source = selu_sim::session::Vm::new().unwrap();
        let target = selu_sim::session::Vm::new().unwrap();
        let a = source.lua().weak();
        let b = target.lua().weak();
        let cache = state(source.lua());
        let callback = source.lua().create_function(|_, ()| Ok(())).unwrap();
        let callback = value(
            Value::Function(callback),
            source.lua(),
            target.lua(),
            &cache,
        )
        .unwrap();
        target.lua().globals().set("callback", callback).unwrap();
        clear(source.lua());
        drop(source);
        drop(target);
        assert!(a.try_upgrade().is_none());
        assert!(b.try_upgrade().is_none());
    }
}

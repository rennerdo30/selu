//! Native key binding state. Binding bodies come from addon XML, never a
//! simulator-authored implementation of a Blizzard panel or button.
use mlua::{Function, Lua, Table};

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    if lua
        .named_registry_value::<Option<Table>>("selu.binding_keys")?
        .is_some()
    {
        return Ok(());
    }
    let keys = lua.create_table()?;
    // The client's default Escape binding; no account binding files are read.
    keys.raw_set("ESCAPE", "TOGGLEGAMEMENU")?;
    lua.set_named_registry_value("selu.binding_keys", keys)?;
    lua.set_named_registry_value("selu.binding_actions", lua.create_table()?)?;
    let globals = lua.globals();
    for (name, keys) in [
        ("IsShiftKeyDown", &["LSHIFT", "RSHIFT"][..]),
        ("IsControlKeyDown", &["LCTRL", "RCTRL"][..]),
        ("IsAltKeyDown", &["LALT", "RALT"][..]),
        ("IsLeftShiftKeyDown", &["LSHIFT"][..]),
        ("IsRightShiftKeyDown", &["RSHIFT"][..]),
        ("IsLeftControlKeyDown", &["LCTRL"][..]),
        ("IsRightControlKeyDown", &["RCTRL"][..]),
        ("IsLeftAltKeyDown", &["LALT"][..]),
        ("IsRightAltKeyDown", &["RALT"][..]),
        (
            "IsModifierKeyDown",
            &["LSHIFT", "RSHIFT", "LCTRL", "RCTRL", "LALT", "RALT"][..],
        ),
    ] {
        globals.raw_set(
            name,
            lua.create_function(move |lua, ()| {
                let arena = crate::frames::arena::store(lua);
                let arena = arena.borrow();
                Ok(keys.iter().any(|key| arena.keys.contains(*key)))
            })?,
        )?;
    }
    globals.raw_set(
        "GetCurrentKeyBoardFocus",
        lua.create_function(|lua, ()| {
            let arena = crate::frames::arena::store(lua);
            let arena = arena.borrow();
            Ok(arena.focus.map(|i| arena.regions[i].table.get()))
        })?,
    )?;
    globals.raw_set(
        "GetCursorPosition",
        lua.create_function(|lua, ()| Ok(crate::frames::arena::store(lua).borrow().cursor))?,
    )?;
    globals.raw_set(
        "GetMouseFoci",
        lua.create_function(|lua, ()| {
            let arena = crate::frames::arena::store(lua);
            let arena = arena.borrow();
            lua.create_sequence_from(arena.hover.map(|i| arena.regions[i].table.get()))
        })?,
    )?;
    globals.raw_set(
        "__selu_binding_actions",
        lua.named_registry_value::<Table>("selu.binding_actions")?,
    )?;
    globals.raw_set(
        "GetBindingAction",
        lua.create_function(|lua, (key, _): (String, Option<bool>)| {
            Ok(lua
                .named_registry_value::<Table>("selu.binding_keys")?
                .raw_get::<Option<String>>(key)?
                .unwrap_or_default())
        })?,
    )?;
    globals.raw_set(
        "SetBinding",
        lua.create_function(|lua, (key, action): (String, Option<String>)| {
            lua.named_registry_value::<Table>("selu.binding_keys")?
                .raw_set(key, action)?;
            Ok(true)
        })?,
    )?;
    globals.raw_set(
        "RunBinding",
        lua.create_function(|lua, (action, up): (String, Option<bool>)| {
            run(lua, &action, !up.unwrap_or(false))
        })?,
    )?;
    Ok(())
}

fn run(lua: &Lua, action: &str, down: bool) -> mlua::Result<()> {
    let actions = lua.named_registry_value::<Table>("selu.binding_actions")?;
    let Some(entry) = actions.raw_get::<Option<Table>>(action)? else {
        return Ok(());
    };
    if !down && !entry.raw_get::<bool>("runOnUp")? {
        return Ok(());
    }
    let function: Function = entry.raw_get("handler")?;
    let ctx = lua.app_data_ref::<crate::Ctx>().unwrap().clone();
    crate::core::script_callback(lua, &ctx, || {
        crate::core::call_lua::<()>(lua, function, if down { "down" } else { "up" })
    })
}

pub(crate) fn key(lua: &Lua, key: &str, down: bool) -> mlua::Result<()> {
    let Some(keys) = lua.named_registry_value::<Option<Table>>("selu.binding_keys")? else {
        return Ok(());
    };
    if let Some(action) = keys.raw_get::<Option<String>>(key)? {
        run(lua, &action, down)?;
    }
    Ok(())
}

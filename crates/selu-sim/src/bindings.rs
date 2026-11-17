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
    globals.raw_set(
        "GetNumBindings",
        lua.create_function(|lua, ()| Ok(binding_order(lua)?.raw_len()))?,
    )?;
    globals.raw_set(
        "GetBinding",
        lua.create_function(|lua, index: usize| {
            let order = binding_order(lua)?;
            let Some(action) = order.raw_get::<Option<String>>(index)? else {
                return Ok((None, None, None, None));
            };
            let actions = lua.named_registry_value::<Table>("selu.binding_actions")?;
            let header = actions
                .raw_get::<Table>(action.as_str())?
                .raw_get::<Option<String>>("header")?;
            let keys = lua.named_registry_value::<Table>("selu.binding_keys")?;
            let mut bound = Vec::new();
            for pair in keys.pairs::<String, String>() {
                let (key, value) = pair?;
                if value == action {
                    bound.push(key);
                }
            }
            bound.sort();
            Ok((
                Some(action),
                header,
                bound.first().cloned(),
                bound.get(1).cloned(),
            ))
        })?,
    )?;
    // Override bindings (SetOverrideBinding*, ClearOverrideBindings): per
    // owner frame, a key runs another action or clicks a named button. They
    // win over the ordinary bindings, priority ones first, the most recently
    // set first within each.
    lua.set_named_registry_value("selu.override_bindings", lua.create_table()?)?;
    fn set_override(
        lua: &Lua,
        owner: Table,
        priority: bool,
        key: String,
        target: Option<(&str, String, String)>,
    ) -> mlua::Result<()> {
        let list = lua.named_registry_value::<Table>("selu.override_bindings")?;
        let key = key.to_ascii_uppercase();
        let kept = lua.create_table()?;
        for entry in list.sequence_values::<Table>() {
            let entry = entry?;
            let same = entry.raw_get::<Table>("owner")? == owner
                && entry.raw_get::<String>("key")? == key;
            if !same {
                kept.raw_push(entry)?;
            }
        }
        if let Some((kind, value, button)) = target {
            let entry = lua.create_table()?;
            entry.raw_set("owner", owner)?;
            entry.raw_set("priority", priority)?;
            entry.raw_set("key", key)?;
            entry.raw_set("kind", kind)?;
            entry.raw_set("value", value)?;
            entry.raw_set("button", button)?;
            kept.raw_push(entry)?;
        }
        lua.set_named_registry_value("selu.override_bindings", kept)
    }
    // The frame a click binding names: the frame registered under that name.
    globals.raw_set(
        "GetClickFrame",
        lua.create_function(|lua, name: String| {
            let value = lua.globals().raw_get::<Option<Table>>(name)?;
            Ok(value.filter(|t| crate::frames::arena::id(t).is_ok()))
        })?,
    )?;
    globals.raw_set(
        "SetOverrideBinding",
        lua.create_function(
            |lua, (owner, priority, key, command): (Table, Option<bool>, String, Option<String>)| {
                let target = command.map(|c| ("action", c, String::new()));
                set_override(lua, owner, priority.unwrap_or(false), key, target)
            },
        )?,
    )?;
    globals.raw_set(
        "SetOverrideBindingClick",
        lua.create_function(
            |lua,
             (owner, priority, key, name, button): (
                Table,
                Option<bool>,
                String,
                Option<String>,
                Option<String>,
            )| {
                let target =
                    name.map(|n| ("click", n, button.unwrap_or_else(|| "LeftButton".into())));
                set_override(lua, owner, priority.unwrap_or(false), key, target)
            },
        )?,
    )?;
    globals.raw_set(
        "ClearOverrideBindings",
        lua.create_function(|lua, owner: Table| {
            let list = lua.named_registry_value::<Table>("selu.override_bindings")?;
            let kept = lua.create_table()?;
            for entry in list.sequence_values::<Table>() {
                let entry = entry?;
                if entry.raw_get::<Table>("owner")? != owner {
                    kept.raw_push(entry)?;
                }
            }
            lua.set_named_registry_value("selu.override_bindings", kept)
        })?,
    )?;
    globals.raw_set(
        "GetBindingCategoryName",
        lua.create_function(|lua, header: String| {
            Ok(lua
                .globals()
                .get::<Option<String>>(header.as_str())?
                .unwrap_or(header))
        })?,
    )?;
    Ok(())
}

fn binding_order(lua: &Lua) -> mlua::Result<Table> {
    let actions = lua.named_registry_value::<Table>("selu.binding_actions")?;
    let mut names = actions
        .pairs::<String, mlua::Value>()
        .map(|entry| entry.map(|(name, _)| name))
        .collect::<mlua::Result<Vec<_>>>()?;
    names.sort();
    lua.create_sequence_from(names)
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

/// The override binding a key runs now: priority ones first, the most
/// recently set first within each.
fn override_for(lua: &Lua, key: &str) -> mlua::Result<Option<Table>> {
    let Some(list) = lua.named_registry_value::<Option<Table>>("selu.override_bindings")? else {
        return Ok(None);
    };
    let entries: Vec<Table> = list.sequence_values::<Table>().collect::<mlua::Result<_>>()?;
    for priority in [true, false] {
        for entry in entries.iter().rev() {
            if entry.raw_get::<String>("key")? == key && entry.raw_get::<bool>("priority")? == priority {
                return Ok(Some(entry.clone()));
            }
        }
    }
    Ok(None)
}

/// A click binding clicks its button with the binding's mouse button, on key
/// down or key up as the button registered for clicks (RegisterForClicks).
fn click_binding(lua: &Lua, name: &str, button: &str, down: bool) -> mlua::Result<()> {
    let Some(frame) = lua.globals().raw_get::<Option<Table>>(name)? else {
        return Ok(());
    };
    let Ok(index) = crate::frames::arena::id(&frame) else {
        return Ok(());
    };
    let suffix = if down { "Down" } else { "Up" };
    let registered = crate::frames::arena::store(lua).borrow().regions[index]
        .click_buttons
        .iter()
        .any(|b| *b == format!("{button}{suffix}") || *b == format!("Any{suffix}"));
    if !registered {
        return Ok(());
    }
    let click: Function = frame.get("Click")?;
    let ctx = lua.app_data_ref::<crate::Ctx>().unwrap().clone();
    crate::core::script_callback(lua, &ctx, || {
        crate::core::call_lua::<()>(lua, click, (frame.clone(), button, down))
    })
}

pub(crate) fn key(lua: &Lua, key: &str, down: bool) -> mlua::Result<()> {
    if let Some(entry) = override_for(lua, key)? {
        let value: String = entry.raw_get("value")?;
        if entry.raw_get::<String>("kind")? == "click" {
            let button: String = entry.raw_get("button")?;
            return click_binding(lua, &value, &button, down);
        }
        return run(lua, &value, down);
    }
    let Some(keys) = lua.named_registry_value::<Option<Table>>("selu.binding_keys")? else {
        return Ok(());
    };
    if let Some(action) = keys.raw_get::<Option<String>>(key)? {
        run(lua, &action, down)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn binding_enumeration_uses_registered_bodies_and_assigned_keys() {
        let s = crate::Session::new(Default::default()).unwrap();
        s.eval("assert(GetNumBindings()==0);__selu_binding_actions.SELF_ACTION={header='SELF_HEADER',handler=function()end,runOnUp=false};SELF_HEADER='Self category';SetBinding('K','SELF_ACTION');SetBinding('L','SELF_ACTION');local action,category,a,b=GetBinding(1);assert(GetNumBindings()==1 and action=='SELF_ACTION' and category=='SELF_HEADER' and a=='K' and b=='L');assert(GetBindingCategoryName(category)=='Self category' and GetBinding(2)==nil)").unwrap();
    }
}

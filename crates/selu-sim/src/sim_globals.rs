use crate::{Ctx, SimResult};
use mlua::{Lua, Table, Value};

/// Single construction boundary for SIM2's visual font model.
pub fn create_font(lua: &Lua, name: &str) -> SimResult<Table> {
    let constructor: mlua::Function = lua.load(r#"
        return function(name)
            local f = { name = name, path = "Fonts/FRIZQT__.TTF", size = 12, flags = "" }
            function f:GetObjectType() return "Font" end
            function f:GetName() return self.name end
            function f:GetFont() return self.path, self.size, self.flags end
            function f:SetFont(path, size, flags) self.path, self.size, self.flags = path, size, flags or ""; return true end
            function f:SetFontObject(other) self:SetFont(other:GetFont()) end
            function f:GetFontObject() return self end
            function f:SetTextColor(...) self.color = {...} end
            function f:GetTextColor() return unpack(self.color or {1,1,1,1}) end
            function f:SetShadowColor(...) self.shadow = {...} end
            function f:GetShadowColor() return unpack(self.shadow or {0,0,0,1}) end
            function f:SetShadowOffset(x,y) self.sx,self.sy=x,y end
            function f:GetShadowOffset() return self.sx or 0,self.sy or 0 end
            function f:SetJustifyH(v) self.h=v end
            function f:GetJustifyH() return self.h or "LEFT" end
            function f:SetJustifyV(v) self.v=v end
            function f:GetJustifyV() return self.v or "MIDDLE" end
            function f:SetSpacing(v) self.spacing=v end
            function f:GetSpacing() return self.spacing or 0 end
            return f
        end
    "#).eval()?;
    constructor.call(name)
}

fn json_value(lua: &Lua, value: &serde_json::Value) -> SimResult<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(v) => Value::Boolean(*v),
        serde_json::Value::Number(v) => Value::Number(v.as_f64().unwrap_or_default()),
        serde_json::Value::String(v) => Value::String(lua.create_string(v)?),
        serde_json::Value::Array(values) => {
            let table = lua.create_table()?;
            for (i, v) in values.iter().enumerate() {
                table.raw_set(i + 1, json_value(lua, v)?)?;
            }
            Value::Table(table)
        }
        serde_json::Value::Object(values) => {
            let table = lua.create_table()?;
            for (k, v) in values {
                table.raw_set(k.as_str(), json_value(lua, v)?)?;
            }
            Value::Table(table)
        }
    })
}

pub fn install_manifest(
    lua: &Lua,
    manifest: &selu_manifest::Manifest,
    _ctx: &Ctx,
) -> SimResult<()> {
    let globals = lua.globals();
    let shapes: Table = globals.raw_get("__selu_global_shapes")?;
    for function in manifest
        .functions
        .iter()
        .filter(|f| f.available && !f.name.contains('.'))
    {
        shapes.raw_set(function.name.as_str(), "function")?;
    }
    for (name, ty) in manifest
        .globals
        .iter()
        .map(|g| (&g.name, g.ty.as_deref()))
        .chain(
            manifest
                .data_globals
                .iter()
                .map(|g| (&g.name, g.ty.as_deref())),
        )
    {
        if let Some(ty) = ty {
            let shape = if manifest.widgets.iter().any(|w| w.name == ty) {
                format!("widget:{ty}")
            } else if manifest.tables.iter().any(|t| t.name == ty) {
                "table".into()
            } else {
                ty.into()
            };
            shapes.raw_set(name.as_str(), shape)?;
        }
    }
    for global in &manifest.globals {
        if matches!(globals.raw_get::<Value>(global.name.as_str())?, Value::Nil)
            && let Some(value) = &global.value
        {
            globals.raw_set(global.name.as_str(), json_value(lua, value)?)?;
        }
    }
    for global in &manifest.data_globals {
        if !global.values.is_empty()
            && matches!(globals.raw_get::<Value>(global.name.as_str())?, Value::Nil)
        {
            let table = lua.create_table()?;
            for (key, value) in &global.values {
                table.raw_set(key.as_str(), json_value(lua, value)?)?;
            }
            globals.raw_set(global.name.as_str(), table)?;
        }
    }
    Ok(())
}

pub fn lookup(lua: &Lua, key: &str, ctx: &Ctx) -> SimResult<Option<Value>> {
    let globals = lua.globals();
    let shapes: Table = globals.raw_get("__selu_global_shapes")?;
    // Native Minimap exists in Blizzard_Minimap/Minimap.xml even when an
    // extraction revision has no typed global record for it.
    if key == "Minimap" {
        shapes.raw_set(key, "widget:Minimap")?;
    }
    let Some(shape) = shapes.raw_get::<Option<String>>(key)? else {
        return Ok(None);
    };
    let value = match shape.as_str() {
        "function" => {
            let ctx = ctx.clone();
            let name = key.to_owned();
            Value::Function(lua.create_function(move |_, _: mlua::MultiValue| {
                ctx.not_simulated(&name);
                Ok(Value::Nil)
            })?)
        }
        "number" => {
            ctx.not_simulated(key);
            Value::Number(0.0)
        }
        "string" => {
            ctx.not_simulated(key);
            Value::String(lua.create_string("")?)
        }
        "boolean" => {
            ctx.not_simulated(key);
            Value::Boolean(false)
        }
        "Font" | "font" => Value::Table(create_font(lua, key)?),
        "Table" | "table" => Value::Table(lua.create_table()?),
        ty if ty.starts_with("widget:")
            || matches!(
                ty,
                "Frame" | "Button" | "Texture" | "FontString" | "EditBox" | "ScrollFrame"
            ) =>
        {
            let create: mlua::Function = globals.raw_get("CreateFrame")?;
            Value::Table(create.call((ty.strip_prefix("widget:").unwrap_or(ty), key))?)
        }
        _ => return Ok(None),
    };
    globals.raw_set(key, value.clone())?;
    Ok(Some(value))
}

pub fn install(lua: &Lua, ctx: &Ctx) -> SimResult<()> {
    lua.globals().set(
        "debugstack",
        lua.create_function(
            |lua, (start, top, bottom): (Option<usize>, Option<usize>, Option<usize>)| {
                let mut lines = Vec::new();
                for level in start.unwrap_or(1)..1024 {
                    let Some(frame) = lua.inspect_stack(level) else {
                        break;
                    };
                    let source = frame.source();
                    lines.push(format!(
                        "{}:{}: in function '{}'",
                        source.short_src.as_deref().unwrap_or("[C]"),
                        frame.curr_line(),
                        frame.names().name.as_deref().unwrap_or("?")
                    ));
                }
                let top = top.unwrap_or(12).min(1024);
                let bottom = bottom.unwrap_or(10).min(1024);
                if lines.len() > top + bottom {
                    let tail = lines.split_off(lines.len() - bottom);
                    lines.truncate(top);
                    lines.push("...".into());
                    lines.extend(tail);
                }
                Ok(lines.join("\n"))
            },
        )?,
    )?;
    let clock = ctx.clock.clone();
    lua.globals().set(
        "debugprofilestop",
        lua.create_function(move |_, ()| Ok(*clock.lock().unwrap() * 1000.0))?,
    )?;
    lua.globals().set(
        "CreateFont",
        lua.create_function(|lua, name: String| {
            let font = create_font(lua, &name)?;
            lua.globals().raw_set(name, font.clone())?;
            Ok(font)
        })?,
    )?;
    lua.globals()
        .set("StaticPopupDialogs", lua.create_table()?)?;
    lua.globals().set("__selu_popups", lua.create_table()?)?;
    lua.load(r#"
        tinsert = table.insert
        tremove = table.remove
        function wipe(t) for k in pairs(t) do t[k]=nil end; return t end
        function StaticPopup_Show(which, text1, text2, data)
            local definition = StaticPopupDialogs[which]
            if not definition then return nil end
            local popup = { which = which, text = definition.text or "", button1 = definition.button1, button2 = definition.button2, data = data }
            local ok, text = pcall(string.format, popup.text, text1, text2)
            if ok then popup.text = text end
            if #__selu_popups >= 16 then table.remove(__selu_popups, 1) end
            table.insert(__selu_popups, popup)
            print("popup:", which, popup.text, popup.button1 or "", popup.button2 or "")
            return popup
        end
    "#).exec()
}

/// Screen resolution is session configuration, never the owner's desktop.
pub fn configure_screen(lua: &Lua, size: (f64, f64)) -> SimResult<()> {
    lua.globals().raw_set("__selu_screen_width", size.0)?;
    lua.globals().raw_set("__selu_screen_height", size.1)?;
    lua.load(
        r#"
        function GetPhysicalScreenSize() return __selu_screen_width, __selu_screen_height end
        function GetScreenWidth() return __selu_screen_width / UIParent:GetEffectiveScale() end
        function GetScreenHeight() return __selu_screen_height / UIParent:GetEffectiveScale() end
    "#,
    )
    .exec()
}

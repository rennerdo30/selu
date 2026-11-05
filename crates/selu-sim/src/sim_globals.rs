use crate::{Ctx, SimResult};
use mlua::{Lua, Table, Value};

/// Single construction boundary for SIM2's visual font model.
pub fn create_font(lua: &Lua, name: &str) -> SimResult<Table> {
    let constructor:mlua::Function=lua.load(r#"
        return function(name,size,color,offset,invalidate)
            local f={_font={"Fonts/FRIZQT__.TTF",size,""},_name=name}
            local defaults={TextColor=color,ShadowColor={0,0,0,1},ShadowOffset={offset,-offset},JustifyH={"LEFT"},JustifyV={"MIDDLE"},Spacing={0}}
            f._defaults=defaults
            function f:GetName() return self._name end
            function f:GetObjectType() return "Font" end
            function f:SetFont(path,size,flags) invalidate();self._font_explicit=true;self._font={path,size,flags or ""};return true end
            function f:GetFont() if self._parent and not self._font_explicit then return self._parent:GetFont() end;return unpack(self._font) end
            function f:SetFontObject(parent)
                if type(parent)=="string" then parent=_G[parent] end
                assert(type(parent)=="table" and parent.GetFont,"font object expected")
                local p=parent
                while p do assert(p~=self,"font inheritance cycle");p=rawget(p,"_parent") end
                invalidate();self._parent=parent;self._font_explicit=false
            end
            function f:GetFontObject() return self._parent or self end
            for _,key in ipairs({"TextColor","ShadowColor","ShadowOffset","JustifyH","JustifyV","Spacing"}) do
                f["Set"..key]=function(self,...) invalidate();self["_"..key]={...} end
                f["Get"..key]=function(self)
                    local value=rawget(self,"_"..key)
                    if value then return unpack(value) end
                    if self._parent then return self._parent["Get"..key](self._parent) end
                    return unpack(defaults[key])
                end
            end
            return f
        end
    "#).eval()?;
    let native = crate::fonts::native_style(name);
    let (size, color) = native.unwrap_or((12.0, [1.0; 4]));
    let color = lua.create_sequence_from(color)?;
    let invalidate = lua.create_function(|lua, ()| {
        let store = crate::frames::arena::store(lua);
        let mut a = store.borrow_mut();
        a.dirty = true;
        a.full_layout = true;
        a.styles_dirty = true;
        a.dirty_regions.clear();
        Ok(())
    })?;
    constructor.call((
        name,
        size,
        color,
        if native.is_some() { 1.0 } else { 0.0 },
        invalidate,
    ))
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
    // Client contract: Blizzard_SharedXMLBase/Color.lua. Runtime objects only;
    // public typing remains owned by the manifest.
    lua.load(r#"
        function CreateColor(r,g,b,a)
            local c={r=r,g=g,b=b,a=a or 1}
            function c:GetRGB() return self.r,self.g,self.b end
            function c:GetRGBA() return self.r,self.g,self.b,self.a end
            function c:SetRGBA(r,g,b,a) self.r=r;self.g=g;self.b=b;self.a=a or 1 end
            function c:SetRGB(r,g,b) self:SetRGBA(r,g,b) end
            function c:OnLoad(...) self:SetRGBA(...) end
            function c:IsRGBEqualTo(o) return self.r==o.r and self.g==o.g and self.b==o.b end
            function c:IsEqualTo(o) return self:IsRGBEqualTo(o) and self.a==o.a end
            local function byte(v) return math.floor(v*255+0.5) end
            function c:GetRGBAsBytes() return byte(self.r),byte(self.g),byte(self.b) end
            function c:GetRGBAAsBytes() return byte(self.r),byte(self.g),byte(self.b),byte(self.a) end
            function c:GenerateHexColorNoAlpha() return string.format('%02X%02X%02X',self:GetRGBAsBytes()) end
            function c:GenerateHexColor() return string.format('%02X',byte(self.a))..self:GenerateHexColorNoAlpha() end
            function c:GenerateHexColorMarkup() return '|c'..self:GenerateHexColor() end
            function c:WrapTextInColorCode(text) return self:GenerateHexColorMarkup()..text..'|r' end
            function c:GetHSL()
                local r,g,b=self:GetRGB();local hi=math.max(r,g,b);local lo=math.min(r,g,b)
                local l=(hi+lo)/2;local d=hi-lo;local h,s=0,0
                if d~=0 then
                    s=d/(1-math.abs(2*l-1))
                    if hi==r then h=((g-b)/d)%6 elseif hi==g then h=(b-r)/d+2 else h=(r-g)/d+4 end
                    h=h/6
                end
                return h,s,l,self.a
            end
            return c
        end
    "#).exec()?;

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

use crate::{Ctx, SimResult};
use mlua::{Lua, Table, Value};

/// Single construction boundary for SIM2's visual font model.
pub fn create_font(lua: &Lua, name: &str) -> SimResult<Table> {
    let constructor: mlua::Function = crate::host_lua::factory(
        lua,
        "selu.helper.font",
        r#"
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
            function f:SetIndentedWordWrap(value) self._indentedWordWrap=value end
            function f:CopyFontObject(source)
                if type(source)=='string' then source=_G[source] end
                self:SetFont(source:GetFont())
                for _,key in ipairs({'TextColor','ShadowColor','ShadowOffset','JustifyH','JustifyV','Spacing'}) do self['Set'..key](self,source['Get'..key](source)) end
            end
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
    "#,
    )?;
    let (size, color) = (12.0, [1.0; 4]);
    let color = lua.create_sequence_from(color)?;
    let invalidate = lua.create_function(|lua, ()| {
        let store = crate::frames::arena::store(lua);
        let mut a = store.borrow_mut();
        a.dirty = true;
        a.styles_dirty = true;
        Ok(())
    })?;
    constructor.call((name, size, color, 0.0, invalidate))
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

pub fn install_manifest(lua: &Lua, manifest: &selu_manifest::Manifest, ctx: &Ctx) -> SimResult<()> {
    let globals = lua.globals();
    let shapes: Table = globals.raw_get("__selu_global_shapes")?;
    let standins = globals
        .raw_get::<Option<Table>>("__selu_native_standins")?
        .unwrap_or(lua.create_table()?);
    globals.raw_set("__selu_native_standins", standins.clone())?;
    let enums = globals
        .raw_get::<Option<Table>>("Enum")?
        .unwrap_or(lua.create_table()?);
    for declaration in &manifest.enums {
        let table = enums
            .raw_get::<Option<Table>>(declaration.name.as_str())?
            .unwrap_or(lua.create_table()?);
        for (key, value) in &declaration.values {
            table.raw_set(key.as_str(), json_value(lua, value)?)?;
        }
        let numbers: Vec<_> = declaration
            .values
            .values()
            .filter_map(|v| v.as_f64())
            .collect();
        if !numbers.is_empty() {
            let name = format!("{}Meta", declaration.name);
            let meta = enums
                .raw_get::<Option<Table>>(name.as_str())?
                .unwrap_or(lua.create_table()?);
            meta.raw_set("NumValues", numbers.len())?;
            meta.raw_set(
                "MinValue",
                numbers.iter().copied().fold(f64::INFINITY, f64::min),
            )?;
            meta.raw_set(
                "MaxValue",
                numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            )?;
            enums.raw_set(name, meta)?;
        }
        enums.raw_set(declaration.name.as_str(), table)?;
    }
    globals.raw_set("Enum", enums)?;
    let constants: Vec<_> = manifest
        .tables
        .iter()
        .chain(&manifest.enums)
        .filter(|d| d.ty.as_deref() == Some("Constants"))
        .collect();
    let root = globals
        .raw_get::<Option<Table>>("Constants")?
        .unwrap_or(lua.create_table()?);
    for declaration in &constants {
        if root.raw_get::<Value>(declaration.name.as_str())?.is_nil() {
            root.raw_set(declaration.name.as_str(), lua.create_table()?)?;
        }
    }
    globals.raw_set("Constants", root.clone())?;
    for _ in 0..=constants.len() {
        let mut changed = false;
        for declaration in &constants {
            let table: Table = root.raw_get(declaration.name.as_str())?;
            for field in &declaration.fields {
                if !table.raw_get::<Value>(field.name.as_str())?.is_nil() {
                    continue;
                }
                let Some(value) = field.attributes.get("Value") else {
                    continue;
                };
                let value = if let Some(reference) = value.get("reference").and_then(|v| v.as_str())
                {
                    let mut value = Value::Table(globals.clone());
                    for part in reference.split('.') {
                        value = if let Value::Table(table) = value {
                            table.raw_get(part)?
                        } else {
                            Value::Nil
                        };
                    }
                    value
                } else if !value.is_object() {
                    json_value(lua, value)?
                } else {
                    Value::Nil
                };
                if !value.is_nil() {
                    table.raw_set(field.name.as_str(), value)?;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut schemas = std::collections::HashMap::new();
    for schema in manifest.tables.iter().chain(&manifest.widgets) {
        schemas.insert(schema.name.clone(), schema.clone());
    }
    for schema in &manifest.enums {
        let mut schema = schema.clone();
        schema.ty = Some("enum".into());
        schemas.insert(format!("Enum.{}", schema.name), schema.clone());
        schemas.insert(schema.name.clone(), schema);
    }
    let schemas = std::sync::Arc::new(schemas);
    // Native stand-ins use checked manifest return declarations. Existing
    // simulator implementations always win, including namespace members.
    for entry in manifest.functions.iter().filter(|f| f.available) {
        let Some(signature) = &entry.signature else {
            continue;
        };
        let mut owner = globals.clone();
        let parts: Vec<_> = entry.name.split('.').collect();
        for part in &parts[..parts.len() - 1] {
            owner = match owner.raw_get::<Value>(*part)? {
                Value::Table(table) => table,
                Value::Nil => {
                    let table = lua.create_table()?;
                    owner.raw_set(*part, table.clone())?;
                    table
                }
                _ => break,
            };
        }
        let name = parts[parts.len() - 1];
        if !owner.raw_get::<Value>(name)?.is_nil() {
            continue;
        }
        let returns = signature.returns.clone();
        let schemas = schemas.clone();
        let context = ctx.clone();
        let full_name = entry.name.clone();
        let native = lua.create_function(move |lua, _: mlua::MultiValue| {
            let _profile = crate::profile::Span::new("native.defaults");
            crate::native_defaults::record(lua, &full_name);
            context.not_simulated(&full_name);
            crate::native_defaults::values(lua, &returns, &schemas, &context, 0)
        })?;
        standins.raw_set(entry.name.as_str(), native.clone())?;
        owner.raw_set(name, native)?;
    }
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
            let shape = if ty.eq_ignore_ascii_case("font") {
                "Font".into()
            } else if manifest.widgets.iter().any(|w| w.name == ty) {
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
    let _profile = crate::profile::Span::new("globals.lookup");
    let globals = lua.globals();
    // The locally installed GlobalStrings DB2 can be absent. These source-
    // documented localized string families get visible synthetic labels in
    // the opt-in default UI; other unknown globals still remain unknown.
    let shape = globals
        .raw_get::<Table>("__selu_global_shapes")?
        .raw_get::<Option<String>>(key)?;
    let known_string = globals
        .raw_get::<Option<Table>>("__selu_string_tags")?
        .is_some_and(|tags| tags.raw_get::<bool>(key).unwrap_or(false))
        || globals
            .raw_get::<Table>("__selu_global_shapes")?
            .raw_get::<Option<String>>(key)?
            .is_some_and(|shape| shape == "string");
    let localized_family = ["ERR_", "OPTIONS_MENU_", "OPTION_TOOLTIP_"]
        .iter()
        .any(|p| key.starts_with(p))
        || ["_LABEL", "_TEXT", "_TOOLTIP", "_DESCRIPTION"]
            .iter()
            .any(|s| key.ends_with(s));
    let numeric_family = ["LE_", "NUM_", "MAX_", "MIN_"]
        .iter()
        .any(|p| key.starts_with(p));
    if !numeric_family
        && shape.as_deref().is_none_or(|s| s == "string")
        && (known_string
            || localized_family
            || [
                "HUD_EDIT_MODE_",
                "ICON_TAG_RAID_TARGET_",
                "CHAT_MSG_",
                "PROMPT_",
            ]
            .iter()
            .any(|prefix| key.starts_with(prefix)))
        && lua
            .named_registry_value::<Option<bool>>("selu.synthetic_strings")?
            .unwrap_or(false)
    {
        ctx.not_simulated(&format!("localized client string: {key}"));
        let value = Value::String(lua.create_string(key.replace('_', " "))?);
        globals.raw_set(key, value.clone())?;
        return Ok(Some(value));
    }
    let shapes: Table = globals.raw_get("__selu_global_shapes")?;
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
            // Blizzard uses missing globals to detect not-yet-loaded frames.
            // A manifest-shaped placeholder here poisons those checks and
            // leaves early anchors pointing at a different arena region.
            if lua
                .named_registry_value::<Option<bool>>("selu.default_ui_strings")?
                .unwrap_or(false)
            {
                return Ok(None);
            }
            let create: mlua::Function = globals.raw_get("CreateFrame")?;
            let frame: Table = create.call((ty.strip_prefix("widget:").unwrap_or(ty), key))?;
            Value::Table(frame)
        }
        _ => return Ok(None),
    };
    globals.raw_set(key, value.clone())?;
    Ok(Some(value))
}

pub fn install(lua: &Lua, ctx: &Ctx) -> SimResult<()> {
    crate::host_lua::load(
        lua,
        r#"
        format = string.format
        function strtrim(text, chars)
            chars = chars or "%s"
            return (text:gsub("^["..chars.."]+", ""):gsub("["..chars.."]+$", ""))
        end
        function strjoin(separator, ...) return table.concat({...}, separator) end
        function strsplit(separator, text, limit)
            local result, start = {}, 1
            while not limit or #result < limit - 1 do
                local first, last = string.find(text, separator, start, true)
                if not first then break end
                result[#result+1] = string.sub(text, start, first-1)
                start = last+1
                if separator == "" then break end
            end
            result[#result+1] = string.sub(text, start)
            return unpack(result)
        end
    "#,
    )
    .exec()?;
    // Client contract: Blizzard_SharedXMLBase/Color.lua. Runtime objects only;
    // public typing remains owned by the manifest.
    crate::host_lua::load(lua,r#"
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
    crate::host_lua::load(lua,r#"
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
    crate::host_lua::load(
        lua,
        r#"
        function GetPhysicalScreenSize() return __selu_screen_width, __selu_screen_height end
        function GetScreenWidth() return __selu_screen_width / UIParent:GetEffectiveScale() end
        function GetScreenHeight() return __selu_screen_height / UIParent:GetEffectiveScale() end
    "#,
    )
    .exec()
}

#[cfg(test)]
mod sg_tests {
    #[test]
    fn native_defaults_follow_manifest_returns_and_log_once() {
        let mut manifest: selu_manifest::Manifest = serde_json::from_str(include_str!(
            "../../../manifests/fixtures/forever-1.0.0.1000.json"
        ))
        .unwrap();
        manifest.enums.push(
            serde_json::from_value(
                serde_json::json!({"name":"SyntheticEnum","values":{"Ready":7}}),
            )
            .unwrap(),
        );
        manifest.tables.push(serde_json::from_value(serde_json::json!({"name":"SyntheticRecord","fields":[{"name":"count","type":"number"}],"methods":[{"name":"Reset","returns":[{"name":"ok","type":"boolean"}]}]})).unwrap());
        manifest.tables.push(serde_json::from_value(serde_json::json!({"name":"SyntheticConstants","type":"Constants","fields":[{"name":"Negative","type":"number","attributes":{"Value":-1}},{"name":"Reference","type":"number","attributes":{"Value":{"reference":"Enum.SyntheticEnum.Ready"}}}]})).unwrap());
        manifest.functions.push(serde_json::from_value(serde_json::json!({
            "name":"C_Synthetic.Native","evidence":"verified",
            "signature":{"name":"Native","returns":[{"name":"text","type":"string"},{"name":"flag","type":"boolean"},{"name":"count","type":"number"},{"name":"record","type":"SyntheticRecord"}]}
        })).unwrap());
        let s = crate::Session::new(crate::SessionConfig {
            manifest: Some(manifest),
            ..Default::default()
        })
        .unwrap();
        s.eval("assert(Enum.SyntheticEnum.Ready==7 and Enum.SyntheticEnumMeta.NumValues==1); assert(Constants.SyntheticConstants.Negative==-1 and Constants.SyntheticConstants.Reference==7); for i=1,2 do local s,b,n,r=C_Synthetic.Native(); assert(s=='' and b==false and n==0); assert(r.count==0 and r:Reset()==false and r.undeclared==nil) end").unwrap();
        assert_eq!(
            s.logs()
                .iter()
                .filter(|s| s.contains("not simulated: C_Synthetic.Native"))
                .count(),
            1
        );
    }
    #[test]
    fn wow_string_aliases_and_frame_ids() {
        let sim = crate::Sim::new().unwrap();
        sim.eval(
            r#"
            assert(format == string.format)
            assert(format('%.1f', 1.26) == '1.3')
            assert(format('%s %.1f %d', 'sample', 1.25, 3) == string.format('%s %.1f %d', 'sample', 1.25, 3))
            assert(strtrim('  hello  ') == 'hello')
            assert(strjoin(':', 'a', 2, 'b') == 'a:2:b')
            local a,b,c = strsplit(':','a::c'); assert(a=='a' and b=='' and c=='c')
            local a,b = strsplit(':','a:b:c',2); assert(a=='a' and b=='b:c')
            local f=CreateFrame('Frame'); local b=CreateFrame('Button',nil,f)
            f:SetID(4); b:SetID(9); assert(f:GetID()==4 and b:GetID()==9)
        "#,
        )
        .unwrap();
    }
}

#[cfg(test)]
mod minimap_tests {
    #[test]
    fn none_mode_does_not_synthesize_blizzard_objects_by_name() {
        let session = crate::Session::new(Default::default()).unwrap();
        // A Session with no explicit Blizzard load is None mode, even though
        // Studio now defaults to the real Core subset.
        session.eval("assert(Minimap == nil); local f=CreateFrame('Minimap','SourceMap'); f:SetSize(198,198); f:SetPoint('CENTER'); assert(f:GetRect() ~= nil)").unwrap();
    }
}

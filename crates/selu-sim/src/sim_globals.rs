//! Native font object constructor, shared with session stand-in globals.
use mlua::{Lua, Table};
/// Construct a named Font object. SIM1 can use this for GameFont globals.
pub fn create_font(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let size = 12.0;
    let constructor: mlua::Function = lua.load(r#"
        return function(name, size)
            local f = { _font = { "fallback", size, "" }, _name = name }
            function f:GetName() return self._name end
            function f:GetObjectType() return "Font" end
            function f:SetFont(path, size, flags) self._parent = nil; self._font = {path,size,flags or ""}; return true end
            function f:GetFont()
                if self._parent then return self._parent:GetFont() end
                return unpack(self._font)
            end
            function f:SetFontObject(parent) self._parent = parent end
            function f:GetFontObject() return self._parent end
            for _, key in ipairs({"TextColor", "ShadowColor", "ShadowOffset", "JustifyH", "JustifyV"}) do
                f["Set"..key] = function(self, ...) self["_"..key] = {...} end
                f["Get"..key] = function(self)
                    local value = self["_"..key]
                    if value then return unpack(value) end
                    if self._parent then return self._parent["Get"..key](self._parent) end
                end
            end
            return f
        end
    "#).eval()?;
    let object: Table = constructor.call((name, size))?;
    lua.globals().set(name, object.clone())?;
    Ok(object)
}

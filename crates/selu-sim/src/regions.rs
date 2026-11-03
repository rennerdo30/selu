//! Small stateful region model shared by frames, textures and font strings.
use mlua::{Function, Lua, Table};

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let attach: Function = lua
        .load(
            r#"
local methods = {}
function methods:SetSize(width, height) self.__width = width; self.__height = height end
function methods:GetWidth() return self.__width or 0 end
function methods:GetHeight() return self.__height or 0 end
function methods:GetParent() return self.__parent end
function methods:SetText(text)
  if text == nil then self.__text = nil else self.__text = tostring(text) end
end
function methods:GetText() return self.__text end
function methods:SetPoint(point, relative, relativePoint, x, y)
  local points = self.__points
  for i, p in ipairs(points) do
    if p[1] == point then points[i] = {point, relative, relativePoint, x, y}; return end
  end
  points[#points + 1] = {point, relative, relativePoint, x, y}
end
function methods:GetPoint(index)
  local p = self.__points[index or 1]
  if p then return p[1], p[2], p[3], p[4], p[5] end
end
function methods:GetNumPoints() return #self.__points end
local function attach(object)
  object.__points = {}
  for name, method in pairs(methods) do object[name] = method end
  return object
end
function methods:CreateTexture(name)
  return attach({ __kind = 'Texture', __parent = self, __name = name })
end
function methods:CreateFontString(name)
  return attach({ __kind = 'FontString', __parent = self, __name = name })
end
return attach
"#,
        )
        .eval()?;
    lua.set_named_registry_value("selu.regions.attach", attach)
}
pub(crate) fn attach(lua: &Lua, frame: &Table) -> mlua::Result<()> {
    lua.named_registry_value::<Function>("selu.regions.attach")?
        .call::<()>(frame.clone())
}

#[cfg(test)]
mod tests {
    #[test]
    fn regions_keep_parent_geometry_and_text() {
        let sim = crate::Sim::new().unwrap();
        sim.eval(
            r#"
local parent = CreateFrame('Frame')
parent:SetSize(160, 60)
local a = parent:CreateTexture()
local b = parent:CreateFontString()
a:SetSize(8, 12)
b:SetText('hello')
print(a:GetParent() == parent, b:GetParent() == parent)
print(parent:GetWidth(), parent:GetHeight(), a:GetWidth(), a:GetHeight())
print(b:GetText(), b:GetWidth(), a:GetText())
a:SetPoint('CENTER', parent, 'CENTER', 2, 3)
a:SetPoint('TOP', parent, 'TOP', 4, 5)
a:SetPoint('CENTER', parent, 'CENTER', 6, 7)
local point, relative, anchor, x, y = a:GetPoint()
print(a:GetNumPoints(), point, relative == parent, anchor, x, y)
print(b:GetNumPoints())
"#,
        )
        .unwrap();
        assert_eq!(
            sim.prints(),
            [
                "true\ttrue",
                "160\t60\t8\t12",
                "hello\t0\tnil",
                "2\tCENTER\ttrue\tCENTER\t6\t7",
                "0"
            ]
        );
        assert!(sim.logs().is_empty(), "{:?}", sim.logs());
    }
}

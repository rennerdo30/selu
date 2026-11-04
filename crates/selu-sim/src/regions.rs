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
-- Stored for snapshots only (frames/mod.rs resolves a basic layout from
-- them until S1's arena replaces this model).
function methods:ClearAllPoints() self.__points = {} end
function methods:SetAllPoints(relative)
  self.__points = {
    {'TOPLEFT', relative, 'TOPLEFT', 0, 0},
    {'BOTTOMRIGHT', relative, 'BOTTOMRIGHT', 0, 0},
  }
end
function methods:SetColorTexture(r, g, b, a)
  self.__color = {r or 0, g or 0, b or 0, a or 1}; self.__texture = nil
end
function methods:SetTexture(texture) self.__texture = texture; self.__color = nil end
function methods:GetTexture() return self.__texture end
function methods:SetAlpha(alpha) self.__alpha = alpha end
function methods:GetAlpha() return self.__alpha or 1 end
function methods:Show() self.__shown = true end
function methods:Hide() self.__shown = false end
function methods:SetShown(shown) if shown then self:Show() else self:Hide() end end
function methods:IsShown() return self.__shown ~= false end
local function attach(object)
  object.__points = {}
  for name, method in pairs(methods) do object[name] = method end
  return object
end
-- Textures and font strings, for snapshots (frames are in __selu_sim_frames).
__selu_sim_regions = {}
local function child(kind, parent, name, layer)
  local object = attach({ __kind = kind, __parent = parent, __name = name, __layer = layer })
  __selu_sim_regions[#__selu_sim_regions + 1] = object
  return object
end
function methods:CreateTexture(name, layer)
  return child('Texture', self, name, layer)
end
function methods:CreateFontString(name, layer)
  return child('FontString', self, name, layer)
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

/// Only backdrop storage is modeled; no textures, borders or XML scripts.
pub(crate) fn templates(lua: &Lua, frame: &Table, templates: Option<&str>) -> mlua::Result<()> {
    if templates.is_some_and(|s| s.split(',').any(|t| t.trim() == "BackdropTemplate")) {
        let install: Function = lua
            .load(
                r#"
return function(frame)
  function frame:SetBackdrop(info)
    self.__backdrop = info
    self.__backdropColor = info and {1, 1, 1, 1} or nil
  end
  function frame:GetBackdrop()
    if not self.__backdrop then return nil end
    local seen = {}
    local function copy(value)
      if type(value) ~= 'table' then return value end
      if seen[value] then return seen[value] end
      local result = {}; seen[value] = result
      for key, child in pairs(value) do result[key] = copy(child) end
      return result
    end
    return copy(self.__backdrop)
  end
  function frame:SetBackdropColor(r, g, b, a)
    if self.__backdrop then self.__backdropColor = {r, g, b, a or 1} end
  end
  function frame:GetBackdropColor()
    if self.__backdropColor then return unpack(self.__backdropColor) end
  end
end
"#,
            )
            .eval()?;
        install.call::<()>(frame.clone())?;
    }
    Ok(())
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

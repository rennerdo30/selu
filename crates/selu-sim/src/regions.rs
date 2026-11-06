//! Small stateful region model shared by frames, textures and font strings.
use mlua::{Function, Lua, Table};

pub(crate) fn install(_lua: &Lua) -> mlua::Result<()> {
    Ok(())
}

/// Only backdrop storage is modeled; no textures, borders or XML scripts.
pub(crate) fn templates(lua: &Lua, frame: &Table, templates: Option<&str>) -> mlua::Result<()> {
    if let (Some(names), Some(apply)) = (
        templates,
        lua.globals()
            .raw_get::<Option<Function>>("__selu_xml_apply")?,
    ) {
        apply.call::<()>((frame.clone(), names))?;
    }
    if templates.is_some_and(|s| s.split(',').any(|t| t.trim() == "BackdropTemplate")) {
        let install: Function = crate::host_lua::factory(
            lua,
            "selu.helper.backdrop",
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
        )?;
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

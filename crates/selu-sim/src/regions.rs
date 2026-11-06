//! Small stateful region model shared by frames, textures and font strings.
use mlua::{Function, Lua, Table};

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    // Legacy Frame API storage is generic. Modern XML/Lua mixins may override
    // it; no template name selects an implementation here.
    let methods: Table = crate::host_lua::load(
        lua,
        r#"
local methods={}
function methods:SetBackdrop(info)
    self.__backdrop=info;self.__backdropColor=info and {1,1,1,1} or nil
end
function methods:GetBackdrop()
    local seen={}
    local function copy(value)
        if type(value)~='table' then return value end
        if seen[value] then return seen[value] end
        local result={};seen[value]=result
        for key,child in pairs(value) do result[key]=copy(child) end
        return result
    end
    return copy(self.__backdrop)
end
function methods:SetBackdropColor(r,g,b,a)
    if self.__backdrop then self.__backdropColor={r,g,b,a or 1} end
end
function methods:GetBackdropColor()
    if self.__backdropColor then return unpack(self.__backdropColor) end
end
return methods
"#,
    )
    .eval()?;
    lua.set_named_registry_value("selu.legacy_frame_methods", methods)
}

/// Template behavior is supplied by XML declarations.
pub(crate) fn templates(lua: &Lua, frame: &Table, templates: Option<&str>) -> mlua::Result<()> {
    if let (Some(names), Some(apply)) = (
        templates,
        lua.globals()
            .raw_get::<Option<Function>>("__selu_xml_apply")?,
    ) {
        apply.call::<()>((frame.clone(), names))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_backdrop_state_is_a_widget_capability_without_named_templates() {
        let sim = crate::Sim::new().unwrap();
        sim.eval("local f=CreateFrame('Frame');f:SetBackdrop({bgFile='sample'});f:SetBackdropColor(.2,.3,.4);assert(f:GetBackdrop().bgFile=='sample');local r,g,b,a=f:GetBackdropColor();assert(r==.2 and g==.3 and b==.4 and a==1)").unwrap();
    }
    #[test]
    fn startup_batch_publishes_final_geometry_and_visibility() {
        let sim = crate::Sim::new().unwrap();
        let batch = crate::frames::arena::LayoutBatch::new(sim.lua());
        sim.eval("BatchShows=0; local p=CreateFrame('Frame','BatchParent'); p:SetSize(200,100); p:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',10,20); local c=CreateFrame('Frame','BatchChild',p); c:SetAllPoints(p); c:SetScript('OnShow',function() BatchShows=BatchShows+1 end); local hidden=CreateFrame('Frame','BatchHidden',p); hidden:Hide(); hidden:SetScript('OnShow',function() error('hidden startup frame shown') end); assert(BatchShows==0); assert(c:IsVisible() and not hidden:IsVisible())").unwrap();
        drop(batch);
        let snapshot = sim.snapshot().unwrap();
        let child = snapshot
            .regions
            .iter()
            .find(|r| r.name.as_deref() == Some("BatchChild"))
            .unwrap();
        let rect = child.rect.unwrap();
        assert_eq!(
            (rect.left, rect.bottom, rect.width, rect.height),
            (10.0, 20.0, 200.0, 100.0)
        );
        sim.advance_time(1.0 / 60.0);
        assert_eq!(sim.lua().globals().get::<u32>("BatchShows").unwrap(), 1);
        assert!(
            sim.logs()
                .iter()
                .all(|s| !s.contains("hidden startup frame shown"))
        );
    }
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

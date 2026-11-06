//! WoW XML is simulator input, never a Selu language declaration boundary.
//! roxmltree is MIT OR Apache-2.0. No external entities are enabled.
use std::path::Path;

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for byte in text.bytes() {
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            32..=126 => out.push(byte as char),
            _ => out.push_str(&format!("\\{byte:03}")),
        }
    }
    out.push('"');
    out
}

fn encode(node: roxmltree::Node<'_, '_>, depth: usize) -> Result<String, String> {
    if depth > 128 {
        return Err("XML nesting exceeds 128".into());
    }
    let mut out = format!("{{tag={},a={{", quote(node.tag_name().name()));
    for a in node.attributes() {
        out.push_str(&format!("[{}]={},", quote(a.name()), quote(a.value())));
    }
    out.push_str("},text=");
    out.push_str(&quote(
        &node
            .children()
            .filter_map(|n| n.text().filter(|_| n.is_text()))
            .collect::<String>(),
    ));
    out.push_str(",children={");
    for child in node.children().filter(|n| n.is_element()) {
        out.push_str(&encode(child, depth + 1)?);
        out.push(',');
    }
    out.push_str("}}");
    Ok(out)
}

/// Expand includes in place, retaining Lua/XML source paths as chunk names.
/// Canonical containment is checked for every file, including symlinks.
pub(crate) fn read_client(
    root: &Path,
    path: &Path,
    stack: &mut Vec<std::path::PathBuf>,
    out: &mut Vec<(String, String)>,
    diagnostic: &mut impl FnMut(String),
    client: Option<(&selu_gamedata::Storage, &Path)>,
) -> Result<(), String> {
    let _profile = crate::profile::Span::new("xml.read_parse_encode");
    let path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !path.starts_with(root) {
        return Err(format!(
            "XML include escapes addon folder: {}",
            path.display()
        ));
    }
    if stack.len() >= 64 || stack.contains(&path) {
        return Err(format!("XML include cycle/depth: {}", path.display()));
    }
    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
        return Err(format!("XML input too large: {}", path.display()));
    }
    let source = crate::loader::read_ui_text(&path, client, diagnostic)?;
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("lua"))
    {
        out.push((path.display().to_string(), source));
        return Ok(());
    }
    let doc =
        roxmltree::Document::parse(&source).map_err(|e| format!("{}: {e}", path.display()))?;
    stack.push(path.clone());
    out.push((
        "@selu/xml-runtime".into(),
        include_str!("xml_runtime.lua").into(),
    ));
    fn declarations<'a, 'input>(
        parent: roxmltree::Node<'a, 'input>,
        out: &mut Vec<roxmltree::Node<'a, 'input>>,
    ) {
        for node in parent.children().filter(|node| node.is_element()) {
            if node.tag_name().name() == "ScopedModifier" {
                declarations(node, out);
            } else {
                out.push(node);
            }
        }
    }
    let mut nodes = Vec::new();
    declarations(doc.root_element(), &mut nodes);
    if doc
        .descendants()
        .any(|node| node.has_tag_name("ScopedModifier"))
    {
        diagnostic("not simulated: XML ScopedModifier forbidden-object policy".into());
    }
    for node in nodes {
        if matches!(node.tag_name().name(), "Include" | "Script") {
            if let Some(file) = node.attribute("file") {
                let child = path.parent().unwrap().join(file.replace('\\', "/"));
                read_client(root, &child, stack, out, diagnostic, client)?;
            } else if node.tag_name().name() == "Script" {
                out.push((path.display().to_string(), node.text().unwrap_or("").into()));
            }
        } else {
            for element in node.descendants().filter(|n| n.is_element()) {
                let tag = element.tag_name().name();
                if !SUPPORTED.contains(&tag)
                    && !element
                        .parent_element()
                        .is_some_and(|p| p.tag_name().name() == "Scripts")
                {
                    diagnostic(format!("not simulated: XML element {tag}"));
                }
            }
            out.push((
                path.display().to_string(),
                format!("__selu_xml_declare({})", encode(node, 0)?),
            ));
        }
    }
    stack.pop();
    Ok(())
}

const SUPPORTED: &[&str] = &[
    "Frame",
    "Button",
    "CheckButton",
    "EditBox",
    "Slider",
    "StatusBar",
    "ScrollFrame",
    "Minimap",
    "GameTooltip",
    "Model",
    "PlayerModel",
    "DressUpModel",
    "SimpleHTML",
    "Texture",
    "FontString",
    "Font",
    "Layers",
    "Layer",
    "Frames",
    "Animations",
    "AnimationGroup",
    "Alpha",
    "Translation",
    "Anchors",
    "Anchor",
    "Size",
    "AbsDimension",
    "Offset",
    "AbsOffset",
    "Scripts",
    "OnLoad",
    "OnShow",
    "OnHide",
    "OnEvent",
    "OnUpdate",
    "OnClick",
    "OnEnter",
    "OnLeave",
    "OnMouseDown",
    "OnMouseUp",
    "KeyValues",
    "BarTexture",
    "BarColor",
    "KeyValue",
    "FontFamily",
    "FontHeight",
    "AbsValue",
    "Member",
    "Color",
    "TexCoords",
    "NormalTexture",
    "PushedTexture",
    "DisabledTexture",
    "HighlightTexture",
    "ScrollChild",
    "ButtonText",
    "NormalFont",
    "HighlightFont",
    "DisabledFont",
    "Mixins",
    "Mixin",
];

#[cfg(test)]
mod tests {
    use super::*;
    fn run(sim: &crate::Sim, source: &str) {
        sim.eval(include_str!("xml_runtime.lua")).unwrap();
        let doc = roxmltree::Document::parse(source).unwrap();
        for n in doc.root_element().children().filter(|n| n.is_element()) {
            sim.eval(&format!("__selu_xml_declare({})", encode(n, 0).unwrap()))
                .unwrap();
        }
    }
    #[test]
    fn nested_intrinsic_types_apply_before_instance_fields() {
        let sim = crate::Sim::new().unwrap();
        run(
            &sim,
            r#"<Ui><Frame name="NativeBase" intrinsic="true"><KeyValues><KeyValue key="base" type="number" value="1"/></KeyValues></Frame><NativeBase name="NativeDerived" intrinsic="true"><Size x="40" y="20"/></NativeBase><NativeDerived name="Instance"/></Ui>"#,
        );
        sim.eval("assert(Instance.base==1 and Instance:GetWidth()==40); local f=CreateFrame('NativeDerived'); assert(f.base==1 and f:GetWidth()==40 and f:GetObjectType()=='Frame')").unwrap();
    }
    #[test]
    fn templates_inherit_mixins_parent_keys_arrays_and_names() {
        let sim = crate::Sim::new().unwrap();
        sim.eval("DemoMixin={Answer=function() return 42 end}")
            .unwrap();
        run(
            &sim,
            r#"<Ui>
          <Frame name="Base" virtual="true"><Size x="80" y="20"/></Frame>
          <Frame name="SelectionTemplate" virtual="true" parentKey="Selection" parentArray="selections"/>
          <Frame name="Other" virtual="true"><KeyValues><KeyValue key="value" type="number" value="7"/></KeyValues></Frame>
          <Frame name="Nested" virtual="true" inherits="Base,Other" mixin="DemoMixin">
            <Frames><Button name="$parentButton" parentKey="Button" parentArray="buttons"/><Frame inherits="SelectionTemplate"/></Frames>
          </Frame>
          <Frame name="Root" inherits="Nested" id="17"/>
        </Ui>"#,
        );
        sim.eval("assert(Root:GetID()==17 and Root:GetWidth()==80); assert(Root.value==7); assert(Root:Answer()==42); assert(Root.Button==RootButton); assert(Root.buttons[1]==RootButton); assert(RootButton:GetParent()==Root); local f=CreateFrame('Frame','FromLua',nil,'Nested'); assert(f.Button==FromLuaButton)").unwrap();
        sim.eval("local f=CreateFrame('Frame',nil,Root,'Nested',23); assert(f:GetID()==23)")
            .unwrap();
        sim.eval("assert(Root.Selection and Root.Selection:GetParent()==Root); assert(#Root.selections==1 and Root.selections[1]==Root.Selection)").unwrap();
    }
    #[test]
    fn intrinsic_constructors_and_postcall_handlers_survive_normal_scripts() {
        let sim = crate::Sim::new().unwrap();
        sim.eval("NativeMixin={Init=function(self) self.ready=true; self.shows=0 end,Shown=function(self) self.shows=self.shows+1; self.order=(self.order or '')..'native' end}").unwrap();
        run(
            &sim,
            r#"<Ui><Frame name="NativeEventFrame" intrinsic="true" mixin="NativeMixin"><Scripts><OnLoad method="Init"/><OnShow method="Shown" intrinsicOrder="postcall"/></Scripts></Frame><NativeEventFrame name="Root" hidden="true"><Scripts><OnLoad>assert(self.ready)</OnLoad><OnShow>self.order='normal-'</OnShow></Scripts></NativeEventFrame><Button name="NativeItem" intrinsic="true"><Scripts><OnLoad intrinsicOrder="postcall">assert(self.loaded and self:GetNormalTexture()); self.postLoaded=true</OnLoad></Scripts><NormalTexture parentKey="Normal"/></Button><NativeItem name="Item"><Scripts><OnLoad>self.loaded=true</OnLoad></Scripts></NativeItem></Ui>"#,
        );
        sim.eval("Root:Show(); assert(Root.order=='normal-native' and Root.shows==1); Root:SetScript('OnShow',nil); Root:Hide(); Root:Show(); assert(Root.shows==2); local f=CreateFrame('NativeEventFrame'); assert(f.ready)").unwrap();
        sim.eval("assert(Item.postLoaded)").unwrap();
    }
    #[test]
    fn initial_hidden_attribute_does_not_fire_inherited_onhide() {
        let sim = crate::Sim::new().unwrap();
        run(
            &sim,
            r#"<Ui><Frame name="Base" virtual="true"><Scripts><OnHide>HiddenDuringCreation=true</OnHide></Scripts></Frame><Frame name="Root" inherits="Base" hidden="true"/></Ui>"#,
        );
        sim.eval("assert(not Root:IsShown() and not HiddenDuringCreation); Root:Show(); Root:Hide(); assert(HiddenDuringCreation)").unwrap();
    }
    #[test]
    fn animation_groups_are_available_to_frame_onload() {
        let sim = crate::Sim::new().unwrap();
        run(
            &sim,
            r#"<Ui><Frame name="Root"><Animations><AnimationGroup parentKey="Anim" looping="REPEAT"><Alpha duration="2" fromAlpha="0" toAlpha="1"/></AnimationGroup></Animations><Scripts><OnLoad>assert(self.Anim:GetLooping()=='REPEAT'); assert(self.Anim:GetDuration()==2)</OnLoad></Scripts></Frame></Ui>"#,
        );
    }
    #[test]
    fn relative_keys_anchor_to_anonymous_parent_members() {
        let sim = crate::Sim::new().unwrap();
        run(
            &sim,
            r#"<Ui><Frame name="Root"><Frames><Frame parentKey="Group"><Frames><Frame parentKey="Reference"/><Frame parentKey="Child"><Anchors><Anchor point="TOP" relativeKey="$parent.Reference" relativePoint="BOTTOM" x="3" y="4"/></Anchors></Frame></Frames></Frame></Frames></Frame></Ui>"#,
        );
        sim.eval("local p,r,rp,x,y=Root.Group.Child:GetPoint(); assert(r==Root.Group.Reference and p=='TOP' and rp=='BOTTOM' and x==3 and y==4)").unwrap();
    }
    #[test]
    fn inherited_children_observe_final_parent_keyvalues_and_mixins() {
        let sim = crate::Sim::new().unwrap();
        sim.eval("ParentMixin={Read=function(self) return self.value end}")
            .unwrap();
        run(
            &sim,
            r#"<Ui><Frame name="ParentBase" virtual="true"><Frames><Frame parentKey="Controller"><Scripts><OnLoad>local parent=self:GetParent(); assert(parent.value==42 and parent:Read()==42); self.ready=true</OnLoad></Scripts></Frame></Frames></Frame><Frame name="Root" inherits="ParentBase" mixin="ParentMixin"><KeyValues><KeyValue key="value" value="42" type="number"/></KeyValues></Frame></Ui>"#,
        );
        sim.eval("assert(Root.Controller.ready)").unwrap();
    }
    #[test]
    fn scoped_modifiers_preserve_inner_template_declarations() {
        let sim = crate::Sim::new().unwrap();
        sim.eval("NestedMixin={value=42}").unwrap();
        run(
            &sim,
            r#"<Ui><ScopedModifier useForbiddenObjectTable="true"><Frame name="ScopedBase" virtual="true"><Size x="80" y="20"/><Mixins><Mixin key="NestedMixin" source="secure"/></Mixins></Frame></ScopedModifier><Frame name="Root" inherits="ScopedBase"/></Ui>"#,
        );
        sim.eval("assert(Root:GetWidth()==80 and Root:GetHeight()==20 and Root.value==42); assert(ScopedBase==nil)").unwrap();
    }
    #[test]
    fn script_inherit_positions_the_inherited_handler() {
        let sim = crate::Sim::new().unwrap();
        run(
            &sim,
            r#"<Ui><Frame name="BaseScript" virtual="true"><Scripts><OnLoad>self.order=(self.order or '')..'base'</OnLoad></Scripts></Frame><Frame name="Append" inherits="BaseScript"><Scripts><OnLoad inherit="append">self.order='local-'</OnLoad></Scripts></Frame><Frame name="Prepend" inherits="BaseScript"><Scripts><OnLoad inherit="prepend">self.order=self.order..'-local'</OnLoad></Scripts></Frame></Ui>"#,
        );
        sim.eval("assert(Append.order=='local-base'); assert(Prepend.order=='base-local')")
            .unwrap();
    }
    #[test]
    fn scripts_methods_and_inheritance_cycles() {
        let sim = crate::Sim::new().unwrap();
        sim.eval("DemoMixin={Load=function(self) self.loaded=true end}")
            .unwrap();
        run(
            &sim,
            r#"<Ui><Frame name="Root" mixin="DemoMixin"><Scripts><OnLoad method="Load"/></Scripts></Frame></Ui>"#,
        );
        sim.eval("assert(Root.loaded)").unwrap();
        run(
            &sim,
            r#"<Ui><Frame name="Cycle" virtual="true" inherits="Cycle"/></Ui>"#,
        );
        assert!(sim.eval("CreateFrame('Frame',nil,nil,'Cycle')").is_err());
    }
    #[test]
    fn scroll_children_and_button_text_are_real_regions() {
        let sim = crate::Sim::new().unwrap();
        run(
            &sim,
            r#"<Ui><Font name="DemoFont" font="Fonts/FRIZQT__.TTF"><FontHeight><AbsValue val="17"/></FontHeight></Font><ScrollFrame name="Root"><Size x="100" y="60"/><ScrollChild><Frame parentKey="ScrollChild"><Size x="100" y="200"/><Frames><Button parentKey="Button"><ButtonText parentKey="Text"/><NormalFont style="DemoFont"/></Button></Frames></Frame></ScrollChild></ScrollFrame></Ui>"#,
        );
        sim.eval("assert(Root:GetScrollChild()==Root.ScrollChild); local b=Root.ScrollChild.Button; assert(b:GetFontString()==b.Text); b:SetFormattedText('%s %d','Hello',7); assert(b.Text:GetText()=='Hello 7'); local _,size=b.Text:GetFont(); assert(size==17)").unwrap();
    }
    #[test]
    fn button_texture_slots_keep_xml_regions_and_lua_replacements() {
        let sim = crate::Sim::new().unwrap();
        run(
            &sim,
            r#"<Ui><Button name="Root"><Size x="80" y="30"/><Anchors><Anchor point="CENTER"/></Anchors><NormalTexture parentKey="Art" file="Interface/Buttons/WHITE8X8"/><HighlightTexture parentKey="Highlight" file="Interface/Buttons/WHITE8X8"/></Button></Ui>"#,
        );
        sim.eval("assert(Root:GetNormalTexture()==Root.Art); assert(Root:GetHighlightTexture()==Root.Highlight); assert(not Root.Highlight:IsShown()); Root:SetNormalTexture('Interface/Buttons/WHITE8X8'); assert(Root:GetNormalTexture()==Root.Art and Root.Art:IsShown()); Root:SetNormalTexture(nil); assert(Root:GetNormalTexture()==nil and not Root.Art:IsShown())").unwrap();
        sim.eval("Root:SetNormalAtlas('ui-hud-micromenu-character-Up'); assert(Root:GetNormalTexture():GetAtlas()=='ui-hud-micromenu-character-Up')").unwrap();
        sim.eval("Root:SetDisabledTexture(\"Interface/Buttons/WHITE8X8\"); Root:Disable(); assert(not Root:GetNormalTexture():IsShown() and Root:GetDisabledTexture():IsShown()); Root:Enable(); assert(Root:GetNormalTexture():IsShown() and not Root:GetDisabledTexture():IsShown())").unwrap();
    }
    #[test]
    fn utf8_and_multiline_scripts_survive_encoding() {
        let sim = crate::Sim::new().unwrap();
        run(
            &sim,
            "<Ui><Frame name=\"Root\"><Scripts><OnLoad>\nself.label='日本語'\n</OnLoad></Scripts></Frame></Ui>",
        );
        sim.eval("assert(Root.label=='日本語')").unwrap();
    }
}

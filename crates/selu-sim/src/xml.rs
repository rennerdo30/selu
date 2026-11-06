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
    for node in doc.root_element().children().filter(|n| n.is_element()) {
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
                if !SUPPORTED.contains(&tag) && !element.parent_element().is_some_and(|p|p.tag_name().name()=="Scripts") {
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
    "Member",
    "Color",
    "TexCoords",
    "NormalTexture",
    "PushedTexture",
    "DisabledTexture",
    "HighlightTexture",
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
        let sim=crate::Sim::new().unwrap();
        run(&sim,r#"<Ui><Frame name="NativeBase" intrinsic="true"><KeyValues><KeyValue key="base" type="number" value="1"/></KeyValues></Frame><NativeBase name="NativeDerived" intrinsic="true"><Size x="40" y="20"/></NativeBase><NativeDerived name="Instance"/></Ui>"#);
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
          <Frame name="Other" virtual="true"><KeyValues><KeyValue key="value" type="number" value="7"/></KeyValues></Frame>
          <Frame name="Nested" virtual="true" inherits="Base,Other" mixin="DemoMixin">
            <Frames><Button name="$parentButton" parentKey="Button" parentArray="buttons"/></Frames>
          </Frame>
          <Frame name="Root" inherits="Nested"/>
        </Ui>"#,
        );
        sim.eval("assert(Root:GetWidth()==80); assert(Root.value==7); assert(Root:Answer()==42); assert(Root.Button==RootButton); assert(Root.buttons[1]==RootButton); assert(RootButton:GetParent()==Root); local f=CreateFrame('Frame','FromLua',nil,'Nested'); assert(f.Button==FromLuaButton)").unwrap();
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
    fn utf8_and_multiline_scripts_survive_encoding() {
        let sim = crate::Sim::new().unwrap();
        run(
            &sim,
            "<Ui><Frame name=\"Root\"><Scripts><OnLoad>\nself.label='日本語'\n</OnLoad></Scripts></Frame></Ui>",
        );
        sim.eval("assert(Root.label=='日本語')").unwrap();
    }
}

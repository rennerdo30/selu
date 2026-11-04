//! XML names and template metadata, never XML text.
use quick_xml::{Reader, events::Event};
use selu_manifest::{Declaration, Evidence, GlobalEntry, Manifest};
use std::collections::BTreeMap;
pub fn declaration(name: &str, ty: &str, file: &str) -> Declaration {
    serde_json::from_value(serde_json::json!({"name":name,"type":ty,"evidence":"source","attributes":{"file":file.strip_prefix("Interface/AddOns/").unwrap_or(file)}})).unwrap()
}
type ChildDefinitions = BTreeMap<String, Vec<(String, String, Vec<String>)>>;
#[derive(Default)]
pub struct Xml {
    globals: BTreeMap<String, (String, Vec<String>)>,
    templates: BTreeMap<String, Declaration>,
    types: BTreeMap<String, String>,
    children: ChildDefinitions,
}
impl Xml {
    /// Read widget tag identities from this pin's schema, including intrinsic aliases.
    pub fn schema(&mut self, text: &str) -> Result<(), String> {
        let mut reader = Reader::from_str(text);
        loop {
            match reader.read_event().map_err(|e| e.to_string())? {
                Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == "element" => {
                    let mut a = BTreeMap::new();
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| e.to_string())?;
                        a.insert(
                            attr.key.as_ref().to_owned(),
                            attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map_err(|e| e.to_string())?
                                .into_owned(),
                        );
                    }
                    if a.contains_key("substitutionGroup")
                        && let (Some(name), Some(ty)) = (a.get("name"), a.get("type"))
                    {
                        self.types.insert(
                            name.clone(),
                            ty.trim_start_matches("ui:")
                                .trim_end_matches("Type")
                                .to_owned(),
                        );
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        Ok(())
    }
    pub fn parse(&mut self, file: &str, text: &str) -> Result<(), String> {
        let mut reader = Reader::from_str(text.trim_start_matches('\u{feff}'));
        let mut stack: Vec<(Option<String>, bool, Option<String>)> = vec![];
        loop {
            let event = reader.read_event().map_err(|e| format!("{file}: {e}"))?;
            match event {
                Event::Start(ref e) | Event::Empty(ref e) => {
                    let tag = e.local_name().as_ref().to_owned();
                    let mut attrs = BTreeMap::new();
                    for a in e.attributes() {
                        let a = a.map_err(|e| format!("{file}: {e}"))?;
                        attrs.insert(
                            a.key.as_ref().to_owned(),
                            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map_err(|e| e.to_string())?
                                .into_owned(),
                        );
                    }
                    let parent = stack.last().cloned().unwrap_or_default();
                    let font = tag == "Font" || tag == "FontFamily";
                    let virtual_node = parent.1
                        || (!font
                            && ["virtual", "intrinsic"]
                                .iter()
                                .any(|key| attrs.get(*key).is_some_and(|v| v == "true")));
                    let resolved = attrs.get("name").and_then(|n| {
                        if n.contains("$parent") {
                            parent.0.as_ref().map(|p| n.replace("$parent", p))
                        } else {
                            Some(n.clone())
                        }
                    });
                    let widget = self.types.contains_key(&tag)
                        || matches!(
                            tag.as_str(),
                            "Frame"
                                | "Button"
                                | "CheckButton"
                                | "EditBox"
                                | "ScrollFrame"
                                | "Slider"
                                | "StatusBar"
                                | "GameTooltip"
                                | "Texture"
                                | "FontString"
                                | "Font"
                                | "FontFamily"
                                | "Model"
                                | "PlayerModel"
                                | "DressUpModel"
                                | "Cooldown"
                                | "ColorSelect"
                                | "SimpleHTML"
                                | "MessageFrame"
                                | "ScrollingMessageFrame"
                                | "Minimap"
                                | "ModelScene"
                                | "Browser"
                        );
                    let mut inherits: Vec<String> = attrs
                        .get("inherits")
                        .map(|v| v.split(',').map(|s| s.trim().to_owned()).collect())
                        .unwrap_or_default();
                    if self.types.get(&tag).is_some_and(|ty| ty != &tag) && !inherits.contains(&tag)
                    {
                        inherits.insert(0, tag.clone());
                    }
                    if widget && let Some(n) = &resolved {
                        if virtual_node {
                            if !parent.1 {
                                let mut d = declaration(n, &tag, file);
                                d.inherits = inherits.clone();
                                if let Some(mixins) = attrs.get("mixin") {
                                    d.attributes.insert(
                                        "mixins".into(),
                                        serde_json::json!(
                                            mixins.split(',').map(str::trim).collect::<Vec<_>>()
                                        ),
                                    );
                                }
                                self.templates.entry(n.clone()).or_insert(d);
                            } else if let Some(root) = &parent.2 {
                                let pattern = n
                                    .strip_prefix(root)
                                    .map(|suffix| format!("$parent{suffix}"))
                                    .unwrap_or_else(|| n.clone());
                                self.children.entry(root.clone()).or_default().push((
                                    pattern,
                                    self.types.get(&tag).cloned().unwrap_or(tag.clone()),
                                    inherits.clone(),
                                ));
                            }
                        } else {
                            self.globals.entry(n.clone()).or_insert((
                                if font {
                                    "Font".into()
                                } else {
                                    self.types.get(&tag).cloned().unwrap_or(tag)
                                },
                                inherits,
                            ));
                        }
                    }
                    if matches!(event, Event::Start(_)) {
                        let root = if virtual_node && !parent.1 {
                            resolved.clone()
                        } else {
                            parent.2
                        };
                        stack.push((if widget { resolved } else { parent.0 }, virtual_node, root));
                    }
                }
                Event::End(_) => {
                    stack.pop();
                }
                Event::Eof => {
                    if !stack.is_empty() {
                        return Err(format!("{file}: unclosed XML elements at EOF"));
                    }
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn extend_manifest(mut self, m: &mut Manifest) {
        let instances: Vec<_> = self
            .globals
            .iter()
            .map(|(n, (ty, parents))| (n.clone(), ty.clone(), parents.clone()))
            .collect();
        for (name, ty, parents) in instances {
            for template in parents.iter().chain(std::iter::once(&ty)) {
                expand_children(
                    template,
                    &name,
                    &self.templates,
                    &self.children,
                    &mut self.globals,
                    &mut Vec::new(),
                );
            }
        }
        if self.globals.values().any(|(ty, _)| ty == "Font")
            && !m.widgets.iter().any(|w| w.name == "Font")
        {
            let mut font = declaration("Font", "ScriptObject", "");
            font.attributes.clear();
            font.inherits.push("SimpleFontAPI".into());
            m.widgets.push(font);
        }
        for (name, (ty, templates)) in self.globals {
            if let Some(g) = m.globals.iter_mut().find(|g| g.name == name) {
                if g.ty.is_none() {
                    g.ty = Some(ty);
                    g.templates = templates;
                }
            } else {
                m.globals.push(GlobalEntry {
                    value: None,
                    name,
                    ty: Some(ty),
                    evidence: Evidence::Source,
                    templates,
                });
            }
        }
        for (_, mut template) in self.templates {
            if let Some(mixins) = template.attributes.get("mixins").and_then(|v| v.as_array()) {
                for mixin in mixins {
                    if let Some(d) = m
                        .data_globals
                        .iter()
                        .find(|d| Some(d.name.as_str()) == mixin.as_str())
                    {
                        template
                            .methods
                            .extend(d.methods.iter().map(|n| declaration(n, "", "")));
                        template
                            .fields
                            .extend(d.fields.iter().map(|n| declaration(n, "", "")));
                    }
                }
            }
            template.methods.sort_by(|a, b| a.name.cmp(&b.name));
            template.methods.dedup_by(|a, b| a.name == b.name);
            let entry = compact(template, m);
            m.templates.push(entry);
        }
    }
}
// A template cycle terminates per traversal path, including child inheritance cycles.
fn expand_children(
    template: &str,
    instance: &str,
    definitions: &BTreeMap<String, Declaration>,
    children: &ChildDefinitions,
    globals: &mut BTreeMap<String, (String, Vec<String>)>,
    path: &mut Vec<String>,
) {
    if path.iter().any(|t| t == template) {
        return;
    }
    path.push(template.into());
    if let Some(definition) = definitions.get(template) {
        for parent in &definition.inherits {
            expand_children(parent, instance, definitions, children, globals, path);
        }
    }
    if let Some(children_of_template) = children.get(template) {
        for (pattern, ty, parents) in children_of_template {
            let name = pattern.replace("$parent", instance);
            globals
                .entry(name.clone())
                .or_insert((ty.clone(), parents.clone()));
            for parent in parents.iter().chain(std::iter::once(ty)) {
                expand_children(parent, &name, definitions, children, globals, path);
            }
        }
    }
    path.pop();
}
pub fn compact(d: Declaration, m: &mut Manifest) -> selu_manifest::DataEntry {
    let file = d
        .attributes
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let source = m
        .source_files
        .iter()
        .position(|s| *s == file)
        .unwrap_or_else(|| {
            m.source_files.push(file);
            m.source_files.len() - 1
        });
    selu_manifest::DataEntry {
        name: d.name,
        source,
        evidence: Evidence::Source,
        ty: d.ty.filter(|s| !s.is_empty()),
        fields: d.fields.into_iter().map(|d| d.name).collect(),
        methods: d.methods.into_iter().map(|d| d.name).collect(),
        inherits: d.inherits,
        mixins: d
            .attributes
            .get("mixins")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        values: d.values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        serde_json::from_value(serde_json::json!({"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1})).unwrap()
    }
    #[test]
    fn review_grouped_globals_roundtrip() {
        let mut xml = Xml::default();
        xml.parse(
            "groups.xml",
            r#"<Ui><Frame name="A"/><Frame name="B"/></Ui>"#,
        )
        .unwrap();
        let mut m = manifest();
        xml.extend_manifest(&mut m);
        let bytes = crate::encode(&mut m).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["globals"][0]["names"], serde_json::json!(["A", "B"]));
        let loaded: Manifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(loaded, m);
    }
    #[test]
    fn review_intrinsic_definitions() {
        let mut xml = Xml::default();
        xml.parse("intrinsic.xml", r#"<Ui><Frame name="EventFrame" intrinsic="true"><Button name="$parentChild"/></Frame><Frame name="Concrete"/></Ui>"#).unwrap();
        let mut m = manifest();
        xml.extend_manifest(&mut m);
        assert!(!m.has_global("EventFrame"));
        assert!(!m.has_global("EventFrameChild"));
        assert!(m.templates.iter().any(|t| t.name == "EventFrame"));
        assert!(m.has_global("Concrete"));
    }
    #[test]
    fn review_inherited_parent_children() {
        let mut xml = Xml::default();
        xml.parse("children.xml", r#"<Ui><Frame name="Base" virtual="true"><Frames><Button name="$parentItemButton" inherits="ButtonBase"/><FontString name="$parentName"/></Frames></Frame><Button name="ButtonBase" virtual="true"><Texture name="$parentIcon"/></Button><Frame name="Derived" virtual="true" inherits="Base"/><Frame name="MerchantItem1" inherits="Derived"/></Ui>"#).unwrap();
        let mut m = manifest();
        xml.extend_manifest(&mut m);
        for n in [
            "MerchantItem1Name",
            "MerchantItem1ItemButton",
            "MerchantItem1ItemButtonIcon",
        ] {
            assert!(m.has_global(n), "{n}");
        }
        for n in ["Base", "Derived", "BaseName", "ButtonBaseIcon"] {
            assert!(!m.has_global(n), "{n}");
        }
    }
    #[test]
    fn review_truncated_xml() {
        let error = Xml::default()
            .parse("truncated.xml", r#"<Ui><Frame name="X">"#)
            .unwrap_err();
        assert!(
            error.contains("truncated.xml") && error.contains("unclosed"),
            "{error}"
        );
    }
    #[test]
    fn xml_names() {
        let mut xml = Xml::default();
        xml.parse("test.xml", r#"<Ui><!-- PRIVATE --><Frame name="Shop"><Frames><Button name="$parentBuy"/></Frames></Frame><Font name="GameFont" virtual="true"/><Frame name="Base" virtual="true" mixin="Mixin"><Button name="$parentChild"/></Frame></Ui>"#).unwrap();
        assert_eq!(xml.globals.get("ShopBuy").unwrap().0, "Button");
        assert_eq!(xml.globals.get("GameFont").unwrap().0, "Font");
        assert!(!xml.globals.contains_key("BaseChild"));
        assert_eq!(
            xml.templates["Base"].attributes["mixins"],
            serde_json::json!(["Mixin"])
        );
        let mut m: Manifest = serde_json::from_value(serde_json::json!({"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1})).unwrap();
        let mut lua = crate::source::Sources::default();
        lua.parse("test.lua", "Mixin = {}; function Mixin:Apply() end")
            .unwrap();
        lua.extend_manifest(&mut m);
        xml.extend_manifest(&mut m);
        assert_eq!(m.templates[0].methods, ["Apply"]);
        assert_eq!(m.source_files[m.templates[0].source], "test.xml");
        assert!(
            m.widgets
                .iter()
                .any(|w| w.name == "Font" && w.inherits == ["SimpleFontAPI"])
        );
        let bytes = crate::encode(&mut m).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("PRIVATE"));
        let decoded: Manifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded, m);
    }
}

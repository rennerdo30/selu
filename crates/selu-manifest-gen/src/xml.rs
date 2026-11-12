//! XML names and template metadata, never XML text.
use quick_xml::{Reader, events::Event};
use selu_manifest::{ChildKey, Declaration, Evidence, GlobalEntry, Manifest};
use std::collections::BTreeMap;
pub fn declaration(name: &str, ty: &str, file: &str) -> Declaration {
    serde_json::from_value(serde_json::json!({"name":name,"type":ty,"evidence":"source","attributes":{"file":file.strip_prefix("Interface/AddOns/").unwrap_or(file)}})).unwrap()
}
/// Per template: its named children as (name pattern, type, templates,
/// the owner of the child's keyed children when it has a key).
type ChildDefinitions = BTreeMap<String, Vec<(String, String, Vec<String>, Option<String>)>>;
#[derive(Default)]
pub struct Xml {
    globals: BTreeMap<String, (String, Vec<String>)>,
    templates: BTreeMap<String, Declaration>,
    types: BTreeMap<String, String>,
    children: ChildDefinitions,
    /// Per named element (template or frame): its own mixins, inherited
    /// templates and KeyValues, for `mixin_loops`.
    own_mixins: BTreeMap<String, Vec<String>>,
    own_inherits: BTreeMap<String, Vec<String>>,
    numbers: BTreeMap<String, BTreeMap<String, f64>>,
    strings: BTreeMap<String, BTreeMap<String, String>>,
    /// Keyed children (`parentKey`) per element: a named frame, a template,
    /// or a keyed child as `<owner>.<key>`.
    keys: BTreeMap<String, Vec<ChildKey>>,
    /// Globals a template's named child becomes per instance, with the
    /// owner of that child's keyed children.
    key_aliases: BTreeMap<String, String>,
    /// Lists (`parentArray`) per element: every child naming the list, as
    /// (widget type, templates), in XML order.
    arrays: BTreeMap<String, BTreeMap<String, Vec<ArrayElement>>>,
}
type ArrayElement = (String, Vec<String>);
/// (nearest named widget, inside a template, template root, element owner)
type Open = (Option<String>, bool, Option<String>, Option<String>);
/// Region and animation tags that can carry a `parentKey` but are not in the
/// schema's widget substitution group, with the widget type each creates.
fn keyed_region(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "NormalTexture"
        | "PushedTexture"
        | "DisabledTexture"
        | "HighlightTexture"
        | "CheckedTexture"
        | "DisabledCheckedTexture"
        | "ThumbTexture"
        | "BarTexture"
        | "ColorWheelTexture"
        | "ColorWheelThumbTexture"
        | "ColorValueTexture"
        | "ColorValueThumbTexture"
        | "ColorAlphaTexture"
        | "ColorAlphaThumbTexture"
        | "SwipeTexture"
        | "EdgeTexture"
        | "BlingTexture" => "Texture",
        "ButtonText" => "FontString",
        "MaskTexture" => "MaskTexture",
        "Line" => "Line",
        "AnimationGroup" => "AnimationGroup",
        // Animations are documented under their Simple* names.
        "Animation" => "SimpleAnim",
        "Alpha" => "SimpleAnimAlpha",
        "Scale" => "SimpleAnimScale",
        "Translation" => "SimpleAnimTranslation",
        "Rotation" => "SimpleAnimRotation",
        "Path" => "SimpleAnimPath",
        "FlipBook" => "SimpleAnimFlipBook",
        "TextureCoordTranslation" => "SimpleAnimTextureCoordTranslation",
        "VertexColor" => "SimpleAnimVertexColor",
        _ => return None,
    })
}
fn identifier(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
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
        let mut stack: Vec<Open> = vec![];
        let mut key_value_owner: Option<String> = None;
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
                    // <KeyValue key= value= type=> belongs to the nearest named widget.
                    // The schema lists KeyValues among the layout fields, so its
                    // own stack entry has no name: remember the widget above it.
                    if tag == "KeyValues" {
                        key_value_owner = parent.0.clone();
                    }
                    if tag == "KeyValue"
                        && let (Some(owner), Some(key), Some(value)) =
                            (&key_value_owner, attrs.get("key"), attrs.get("value"))
                    {
                        match attrs.get("type").map(String::as_str) {
                            Some("number") => {
                                if let Ok(number) = value.parse::<f64>() {
                                    self.numbers
                                        .entry(owner.clone())
                                        .or_default()
                                        .insert(key.clone(), number);
                                }
                            }
                            Some("string") | None => {
                                self.strings
                                    .entry(owner.clone())
                                    .or_default()
                                    .insert(key.clone(), value.clone());
                            }
                            _ => {}
                        }
                    }
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
                    // Keyed children: `parent.<key>` is this element. A named
                    // frame or template owns its keys under its name, a keyed
                    // child under `<owner>.<key>`; an unkeyed anonymous child
                    // cannot be reached, so nothing below it is recorded.
                    let region = keyed_region(&tag);
                    let keyed = widget && !font || region.is_some();
                    let key = attrs.get("parentKey").filter(|k| identifier(k));
                    let path = match (&parent.3, key) {
                        (Some(owner), Some(key)) => Some(format!("{owner}.{key}")),
                        _ => None,
                    };
                    let named = widget && !font && (!virtual_node || !parent.1);
                    let element = if !keyed {
                        parent.3.clone()
                    } else if named && resolved.is_some() {
                        resolved.clone()
                    } else {
                        path.clone()
                    };
                    if keyed && let (Some(owner), Some(key)) = (&parent.3, key) {
                        let children = self.keys.entry(owner.clone()).or_default();
                        if !children.iter().any(|c| &c.key == key) {
                            children.push(ChildKey {
                                key: key.clone(),
                                ty: region.map(str::to_owned).unwrap_or_else(|| {
                                    self.types.get(&tag).cloned().unwrap_or(tag.clone())
                                }),
                                templates: inherits.clone(),
                                element: element.clone().filter(|e| Some(e) != path.as_ref()),
                                array: false,
                            });
                        }
                    }
                    // A list: `parent.<name>` holds every child naming it.
                    if keyed
                        && let (Some(owner), Some(list)) = (
                            &parent.3,
                            attrs.get("parentArray").filter(|k| identifier(k)),
                        )
                    {
                        self.arrays
                            .entry(owner.clone())
                            .or_default()
                            .entry(list.clone())
                            .or_default()
                            .push((
                                region.map(str::to_owned).unwrap_or_else(|| {
                                    self.types.get(&tag).cloned().unwrap_or(tag.clone())
                                }),
                                inherits.clone(),
                            ));
                    }
                    if widget && let Some(n) = &resolved {
                        if let Some(mixins) = attrs.get("mixin") {
                            self.own_mixins.entry(n.clone()).or_insert_with(|| {
                                mixins.split(',').map(|s| s.trim().to_owned()).collect()
                            });
                        }
                        self.own_inherits
                            .entry(n.clone())
                            .or_insert_with(|| inherits.clone());
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
                                    path.clone(),
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
                        stack.push((
                            if widget { resolved } else { parent.0 },
                            virtual_node,
                            root,
                            element,
                        ));
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
    /// Every named, non-virtual frame with its effective mixins and
    /// KeyValues: its own, then those of the templates it inherits (nearest
    /// first, so an instance overrides its template).
    pub fn instances(&self) -> Vec<crate::mixin_loops::Instance> {
        self.globals
            .keys()
            .map(|name| {
                let mut instance = crate::mixin_loops::Instance {
                    name: name.clone(),
                    ..Default::default()
                };
                let mut seen = Vec::new();
                self.collect(name, &mut instance, &mut seen);
                instance
            })
            .collect()
    }

    fn collect(
        &self,
        element: &str,
        out: &mut crate::mixin_loops::Instance,
        seen: &mut Vec<String>,
    ) {
        if seen.iter().any(|s| s == element) {
            return;
        }
        seen.push(element.to_owned());
        for mixin in self.own_mixins.get(element).into_iter().flatten() {
            if !out.mixins.contains(mixin) {
                out.mixins.push(mixin.clone());
            }
        }
        for (key, value) in self.numbers.get(element).into_iter().flatten() {
            out.numbers.entry(key.clone()).or_insert(*value);
        }
        for (key, value) in self.strings.get(element).into_iter().flatten() {
            out.strings
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
        for parent in self.own_inherits.get(element).into_iter().flatten() {
            self.collect(parent, out, seen);
        }
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
                    &mut self.key_aliases,
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
        // Children of unknown widget types are dropped in `encode`, once the
        // overlays have added the canonical widgets.
        // Lists become one keyed entry each. The element type is the one
        // every child shares; children of different frame types are frames;
        // a list mixing frames and regions is not recorded.
        for (owner, lists) in std::mem::take(&mut self.arrays) {
            for (name, elements) in lists {
                let children = self.keys.entry(owner.clone()).or_default();
                if children.iter().any(|c| c.key == name) {
                    continue;
                }
                let first = &elements[0];
                let same_type = elements.iter().all(|e| e.0 == first.0);
                let region = |ty: &str| {
                    matches!(
                        ty,
                        "Texture" | "FontString" | "MaskTexture" | "Line" | "AnimationGroup"
                    ) || ty.starts_with("SimpleAnim")
                };
                let ty = if same_type {
                    first.0.clone()
                } else if elements.iter().all(|e| !region(&e.0)) {
                    "Frame".to_owned()
                } else {
                    continue;
                };
                let templates = if elements.iter().all(|e| e.1 == first.1) {
                    first.1.clone()
                } else {
                    Vec::new()
                };
                children.push(ChildKey {
                    key: name,
                    ty,
                    templates,
                    element: None,
                    array: true,
                });
            }
        }
        // A template's named child is a global per instance: that global
        // has the keyed children its template definition gave it inline.
        for (global, element) in std::mem::take(&mut self.key_aliases) {
            if self.keys.contains_key(&global) {
                continue;
            }
            if let Some(children) = self.keys.get(&element) {
                let children = children
                    .iter()
                    .map(|child| ChildKey {
                        element: (!child.array).then(|| {
                            child
                                .element
                                .clone()
                                .unwrap_or_else(|| format!("{element}.{}", child.key))
                        }),
                        ..child.clone()
                    })
                    .collect();
                self.keys.insert(global, children);
            }
        }
        let mut child_keys = Vec::new();
        for (owner, mut children) in std::mem::take(&mut self.keys) {
            children.sort_by(|a, b| a.key.cmp(&b.key));
            child_keys.push(selu_manifest::ChildKeys { owner, children });
        }
        m.child_keys = child_keys;
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
    aliases: &mut BTreeMap<String, String>,
    path: &mut Vec<String>,
) {
    if path.iter().any(|t| t == template) {
        return;
    }
    path.push(template.into());
    if let Some(definition) = definitions.get(template) {
        for parent in &definition.inherits {
            expand_children(
                parent,
                instance,
                definitions,
                children,
                globals,
                aliases,
                path,
            );
        }
    }
    if let Some(children_of_template) = children.get(template) {
        for (pattern, ty, parents, element) in children_of_template {
            let name = pattern.replace("$parent", instance);
            globals
                .entry(name.clone())
                .or_insert((ty.clone(), parents.clone()));
            if let Some(element) = element {
                aliases.entry(name.clone()).or_insert(element.clone());
            }
            for parent in parents.iter().chain(std::iter::once(ty)) {
                expand_children(parent, &name, definitions, children, globals, aliases, path);
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
        let json: serde_json::Value = selu_manifest::format::decode(&bytes).unwrap();
        assert_eq!(json["globals"][0]["names"], serde_json::json!(["A", "B"]));
        let loaded: Manifest = selu_manifest::format::decode(&bytes).unwrap();
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
    fn keyed_children_of_frames_templates_and_nested_children() {
        let mut xml = Xml::default();
        xml.parse(
            "keys.xml",
            r#"<Ui>
            <Frame name="PortraitBase" virtual="true">
              <Frames>
                <Frame parentKey="PortraitContainer">
                  <Layers><Layer><Texture parentKey="portrait"/><Texture/></Layer></Layers>
                </Frame>
                <Frame name="$parentInset" parentKey="Inset" inherits="InsetBase"/>
                <Frame name="$parentCard" parentKey="Card"><Layers><Layer><Texture parentKey="Icon"/></Layer></Layers></Frame>
                <Frame><Layers><Layer><Texture parentKey="Unreachable"/></Layer></Layers></Frame>
              </Frames>
              <Layers><Layer><FontString parentKey="TitleText"/><Texture parentKey="1bad"/></Layer></Layers>
            </Frame>
            <Frame name="InsetBase" virtual="true">
              <Layers><Layer><Texture parentKey="Bg"/></Layer></Layers>
            </Frame>
            <Frame name="Shop" inherits="PortraitBase">
              <Frames>
                <Button name="$parentBuy" parentKey="Buy">
                  <NormalTexture parentKey="Art"/>
                  <ButtonText parentKey="Label"/>
                  <Animations><AnimationGroup parentKey="Pulse"><Alpha parentKey="Fade"/></AnimationGroup></Animations>
                </Button>
                <Frame parentKey="Odd"><Frames><Frame parentKey="Nested"/></Frames></Frame>
                <Unknown parentKey="Skipped"/>
              </Frames>
            </Frame></Ui>"#,
        )
        .unwrap();
        let mut m = manifest();
        for widget in [
            "Frame",
            "Button",
            "Texture",
            "FontString",
            "AnimationGroup",
            "SimpleAnimAlpha",
        ] {
            let mut d = declaration(widget, "ScriptObject", "");
            d.attributes.clear();
            m.widgets.push(d);
        }
        xml.extend_manifest(&mut m);
        let bytes = crate::encode(&mut m).unwrap();
        let keys = |owner: &str| -> Vec<(String, String, Vec<String>, Option<String>)> {
            m.child_keys
                .iter()
                .find(|c| c.owner == owner)
                .map(|c| {
                    c.children
                        .iter()
                        .map(|k| {
                            (
                                k.key.clone(),
                                k.ty.clone(),
                                k.templates.clone(),
                                k.element.clone(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let plain = |key: &str, ty: &str| (key.to_owned(), ty.to_owned(), vec![], None);
        assert_eq!(
            keys("PortraitBase"),
            [
                plain("Card", "Frame"),
                (
                    "Inset".to_owned(),
                    "Frame".to_owned(),
                    vec!["InsetBase".to_owned()],
                    None
                ),
                plain("PortraitContainer", "Frame"),
                plain("TitleText", "FontString"),
            ]
        );
        assert_eq!(
            keys("PortraitBase.PortraitContainer"),
            [plain("portrait", "Texture")]
        );
        assert_eq!(keys("InsetBase"), [plain("Bg", "Texture")]);
        // A template's named child is a global per instance; its inline keys follow it.
        assert_eq!(
            keys("ShopCard"),
            [(
                "Icon".to_owned(),
                "Texture".to_owned(),
                vec![],
                Some("PortraitBase.Card.Icon".to_owned())
            )]
        );
        assert_eq!(keys("PortraitBase.Card"), [plain("Icon", "Texture")]);
        // A named global child owns its keys under its global name.
        assert_eq!(
            keys("Shop"),
            [
                (
                    "Buy".to_owned(),
                    "Button".to_owned(),
                    vec![],
                    Some("ShopBuy".to_owned())
                ),
                plain("Odd", "Frame"),
            ]
        );
        assert_eq!(
            keys("ShopBuy"),
            [
                plain("Art", "Texture"),
                plain("Label", "FontString"),
                plain("Pulse", "AnimationGroup"),
            ]
        );
        assert_eq!(keys("ShopBuy.Pulse"), [plain("Fade", "SimpleAnimAlpha")]);
        assert_eq!(keys("Shop.Odd"), [plain("Nested", "Frame")]);
        let owners: Vec<_> = m.child_keys.iter().map(|c| c.owner.as_str()).collect();
        let mut sorted = owners.clone();
        sorted.sort();
        assert_eq!(owners, sorted);
        assert!(!owners.iter().any(|o| o.contains("Unreachable")));
        let decoded: Manifest = selu_manifest::format::decode(&bytes).unwrap();
        assert_eq!(decoded, m);
    }
    #[test]
    fn lists_of_children_are_recorded_with_their_common_element_type() {
        let mut xml = Xml::default();
        xml.parse(
            "lists.xml",
            r#"<Ui>
            <Frame name="TabOwner" virtual="true">
              <Frames>
                <Button parentArray="Tabs" inherits="TabTemplate"/>
                <Button parentArray="Tabs" inherits="TabTemplate"/>
                <CheckButton parentArray="Mixed"/>
                <Frame parentArray="Mixed" inherits="Other"/>
                <Frame parentArray="Lost"/>
              </Frames>
              <Layers><Layer>
                <Texture parentArray="Lost"/>
                <Texture parentKey="Tabs"/>
                <Texture parentArray="Pieces"/><Texture parentArray="Pieces"/>
              </Layer></Layers>
            </Frame>
            <Frame name="Window" inherits="TabOwner">
              <Frames><Button name="$parentTab1" parentArray="Own"/></Frames>
            </Frame></Ui>"#,
        )
        .unwrap();
        let mut m = manifest();
        for widget in ["Frame", "Button", "CheckButton", "Texture"] {
            let mut d = declaration(widget, "ScriptObject", "");
            d.attributes.clear();
            m.widgets.push(d);
        }
        xml.extend_manifest(&mut m);
        crate::encode(&mut m).unwrap();
        let of = |owner: &str| -> Vec<(String, String, Vec<String>, bool)> {
            m.child_keys
                .iter()
                .find(|c| c.owner == owner)
                .map(|c| {
                    c.children
                        .iter()
                        .map(|k| (k.key.clone(), k.ty.clone(), k.templates.clone(), k.array))
                        .collect()
                })
                .unwrap_or_default()
        };
        assert_eq!(
            of("TabOwner"),
            [
                // Frames of different types are frames; their templates differ.
                ("Mixed".to_owned(), "Frame".to_owned(), vec![], true),
                ("Pieces".to_owned(), "Texture".to_owned(), vec![], true),
                // A key of the same name wins over the list.
                ("Tabs".to_owned(), "Texture".to_owned(), vec![], false),
            ]
        );
        assert_eq!(
            of("Window"),
            [("Own".to_owned(), "Button".to_owned(), vec![], true)]
        );
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
        let json: serde_json::Value = selu_manifest::format::decode(&bytes).unwrap();
        assert!(!serde_json::to_string(&json).unwrap().contains("PRIVATE"));
        let decoded: Manifest = selu_manifest::format::decode(&bytes).unwrap();
        assert_eq!(decoded, m);
    }
}

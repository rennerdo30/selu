//! Native object identities backed by each build's own documentation and UI.xsd.
use selu_manifest::{Declaration, Evidence, Manifest};
use serde_json::json;

pub fn extend(m: &mut Manifest, sources: &[(String, String)]) {
    reviewed_scripts(m);
    let schema = sources.iter().find(|(p, _)| p.ends_with("/UI.xsd"));
    if m.widgets.iter().any(|w| w.name == "SimpleObjectAPI") {
        let mut object = Declaration {
            name: "CScriptObject".into(),
            ty: Some("ScriptObject".into()),
            evidence: Some(Evidence::Documented),
            ..empty()
        };
        object.inherits.push("SimpleObjectAPI".into());
        object
            .attributes
            .insert("SeluAliasOf".into(), json!("SimpleObjectAPI"));
        if let Some(existing) = m.widgets.iter_mut().find(|w| w.name == object.name) {
            *existing = object;
        } else {
            m.widgets.push(object);
        }
    }
    let Some((_, schema)) = schema else { return };
    if m.widgets.iter().any(|w| w.name == "SimpleAnimAPI")
        && !m.widgets.iter().any(|w| w.name == "SimpleAnim")
    {
        m.widgets.push(Declaration {
            name: "SimpleAnim".into(),
            ty: Some("ScriptObject".into()),
            inherits: vec!["SimpleAnimAPI".into(), "SimpleObjectAPI".into()],
            evidence: Some(Evidence::Documented),
            ..empty()
        });
    }
    let mut animations = std::collections::BTreeSet::new();
    let mut reader = quick_xml::Reader::from_str(schema);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e))
                if e.local_name().as_ref() == "element" =>
            {
                let attrs: std::collections::BTreeMap<_, _> = e
                    .attributes()
                    .filter_map(Result::ok)
                    .map(|a| (a.key.as_ref().to_owned(), a.value.into_owned()))
                    .collect();
                if attrs
                    .get("substitutionGroup")
                    .is_some_and(|s| s == "AnimationRef" || s == "ui:AnimationRef")
                    && let Some(name) = attrs.get("name")
                {
                    animations.insert(name.to_owned());
                }
            }
            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    for kind in [
        "Alpha",
        "Translation",
        "Scale",
        "Rotation",
        "LineScale",
        "LineTranslation",
        "Path",
        "TextureCoordTranslation",
        "VertexColor",
        "FlipBook",
    ] {
        let api = format!("SimpleAnim{kind}API");
        if !animations.contains(kind) || !m.widgets.iter().any(|w| w.name == api) {
            continue;
        }
        let name = format!("SimpleAnim{kind}");
        if m.widgets.iter().any(|w| w.name == name) {
            continue;
        }
        let mut d = Declaration {
            name,
            ty: Some("ScriptObject".into()),
            evidence: Some(Evidence::Documented),
            inherits: vec![api, "SimpleAnim".into()],
            ..empty()
        };
        d.attributes.insert("SeluAnimationType".into(), json!(kind));
        m.widgets.push(d);
    }
}

fn empty() -> Declaration {
    serde_json::from_value(json!({"name":""})).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        serde_json::from_value(json!({"flavour":"wow", "build":{"major":12,"minor":1,"patch":5,"build":70391}, "interface":120105,"luaDialects":["lua51"]})).unwrap()
    }
    #[test]
    fn missing_documentation_never_creates_animation_contract() {
        let mut m = manifest();
        extend(
            &mut m,
            &[(
                "Interface/AddOns/Blizzard_SharedXML/UI.xsd".into(),
                "<element name=\"Alpha\" substitutionGroup=\"AnimationRef\"/>".into(),
            )],
        );
        assert!(m.widgets.is_empty());
    }
    #[test]
    fn animation_requires_both_schema_and_documented_methods() {
        let mut m = manifest();
        m.widgets.push(Declaration {
            name: "SimpleAnimAlphaAPI".into(),
            ..empty()
        });
        extend(&mut m, &[]);
        assert_eq!(m.widgets.len(), 1);
        extend(
            &mut m,
            &[(
                "Interface/AddOns/Blizzard_SharedXML/UI.xsd".into(),
                "<element name=\"Alpha\" substitutionGroup=\"AnimationRef\"/>".into(),
            )],
        );
        assert_eq!(m.widgets[1].attributes["SeluAnimationType"], "Alpha");
        extend(
            &mut m,
            &[(
                "Interface/AddOns/Blizzard_SharedXML/UI.xsd".into(),
                "<element name=\"Alpha\" substitutionGroup=\"AnimationRef\"/>".into(),
            )],
        );
        assert_eq!(m.widgets.len(), 2);
    }
}

#[cfg(test)]
mod checking_tests {
    use super::*;
    fn codes(source: &str) -> Vec<String> {
        let mut m: Manifest = serde_json::from_value(json!({"flavour":"wow","build":{"major":12,"minor":1,"patch":5,"build":70391},"interface":120105,"luaDialects":["lua51"],"source":{"commit":"18c6d677b605b312f3d813d498404631fb941977","version":"12.1.5","interfaceSource":"fixture"},
          "globals":[{"name":"frame","type":"SimpleScriptRegionAPI","evidence":"documented"},{"name":"group","type":"SimpleAnimGroupAPI","evidence":"documented"},{"name":"object","type":"CScriptObject","evidence":"documented"}],
          "widgets":[
            {"name":"SimpleScriptRegionAPI","methods":[{"name":"SetScript","arguments":[{"name":"script","type":"string"},{"name":"handler","type":"LuaFunctionReference","attributes":{"Nilable":true}}]},{"name":"HookScript","arguments":[{"name":"script","type":"string"},{"name":"handler","type":"LuaFunctionReference"}]}]},
            {"name":"SimpleObjectAPI","methods":[{"name":"GetParent","attributes":{"SecretReturnsForAspect":["Hierarchy"]},"returns":[{"name":"parent","type":"CScriptObject"}]},{"name":"GetDebugName","returns":[{"name":"name","type":"string"}]}]},
            {"name":"SimpleAnimGroupAPI","methods":[{"name":"CreateAnimation","arguments":[{"name":"kind","type":"string"}],"returns":[{"name":"animation","type":"SimpleAnim"}]}]},
            {"name":"SimpleAnimAPI"},
            {"name":"SimpleAnimAlphaAPI","methods":[{"name":"SetFromAlpha","arguments":[{"name":"alpha","type":"number"}]}]}
          ]})).unwrap();
        extend(
            &mut m,
            &[(
                "Interface/AddOns/Blizzard_SharedXML/UI.xsd".into(),
                "<element name=\"Alpha\" substitutionGroup=\"AnimationRef\"/>".into(),
            )],
        );
        let p = selu_syntax::parse("native.selu", source).unwrap();
        selu_check::check(&p, &m)
            .iter()
            .map(|d| d.code.to_string())
            .collect()
    }
    #[test]
    fn scripts_contextualize_and_reject_wrong_annotations() {
        assert!(
            codes(
                "frame.SetScript(\"OnMouseWheel\", (self, delta) => { const n: number = delta; });"
            )
            .is_empty()
        );
        assert!(
            codes("frame.HookScript(\"OnMouseWheel\", (self, delta: string) => {});")
                .contains(&"SELU0481".into())
        );
        assert!(codes("frame.SetScript(\"OnMouseWheel\", null);").is_empty());
        assert!(codes("frame.SetScript(\"Unreviewed\", (self, payload) => {});").is_empty());
    }
    #[test]
    fn alpha_factory_and_wrong_argument() {
        assert!(codes("group.CreateAnimation(\"Alpha\").SetFromAlpha(0.15);").is_empty());
        assert!(
            codes("group.CreateAnimation(\"Alpha\").SetFromAlpha(true);")
                .contains(&"SELU0481".into())
        );
    }
    #[test]
    fn parent_requires_secret_unwrap() {
        assert!(!codes("const p = object.GetParent(); p.GetDebugName();").is_empty());
        assert!(
            codes("const p = readable(object.GetParent()); if (p != null) { p.GetDebugName(); }")
                .is_empty()
        );
    }
}

// Reviewed FrameXML use at the immutable source pins below. Unreviewed source
// revisions retain LuaFunctionReference until their evidence is reviewed.
fn reviewed_scripts(m: &mut Manifest) {
    let Some(source) = &m.source else { return };
    let click_line = match source.commit.as_str() {
        "18c6d677b605b312f3d813d498404631fb941977" => 1335,
        "ca8368f2f2380cbe696854f7f2646387c278a61b" | "eac30af597e5aaa43a60f6675ef5fdcea100375f" => {
            1396
        }
        "e5d9b3a22c3f0f6f41ebac0e454f4969594261cf"
        | "68e0a2ec559d9761dfc4e7f864c3944f9d758a49"
        | "4192ac60067fc55566182291bc67c85dd9b6794d"
        | "7014c309a25338905c41dc3799a93f9719ebb533" => 1366,
        _ => return,
    };
    let mut scripts = serde_json::Map::new();
    let mut definitions = vec![
        (
            "OnMouseWheel",
            vec![("delta", "number")],
            "Blizzard_SharedXML/PagedList.lua:13-15; delta > 0",
        ),
        (
            "OnClick",
            vec![("button", "string"), ("down", "bool?")],
            "Blizzard_ActionBar/Shared/ActionButton.lua:1335-1364; button == RightButton; Blizzard_SharedXML/ButtonGroup.lua:40-50 invokes saved handler without down",
        ),
        (
            "OnEnter",
            vec![("motion", "opaque:ScriptMotion")],
            "Blizzard_RestrictedAddOnEnvironment/SecureHandlers.lua:292-305; Wrapped_OnEnter; truthiness/forwarding only, motion opaque",
        ),
    ];
    if matches!(
        source.commit.as_str(),
        "18c6d677b605b312f3d813d498404631fb941977" | "eac30af597e5aaa43a60f6675ef5fdcea100375f"
    ) {
        definitions.extend([
            ("OnLeave", vec![("motion", "opaque:ScriptMotion")], "Blizzard_RestrictedAddOnEnvironment/SecureHandlers.lua:312-325; truthiness/forwarding only, motion opaque"),
            ("OnAttributeChanged", vec![("name", "string"), ("value", "opaque:ScriptAttributeValue")], "Blizzard_RestrictedAddOnEnvironment/SecureHandlers.lua:380-383; name:match; forwarded value opaque"),
            ("OnShow", vec![], "Blizzard_RestrictedAddOnEnvironment/SecureHandlers.lua:349; self wrapper"),
            ("OnHide", vec![], "Blizzard_RestrictedAddOnEnvironment/SecureHandlers.lua:349; self wrapper"),
            ("OnKeyDown", vec![("key", "opaque:ScriptKey")], "Blizzard_SharedXML/CustomBindingButtonMixin.lua:57-81; BindingUtil.lua:66-67 conversion can return original input, key opaque"),
            ("OnKeyUp", vec![("key", "opaque:ScriptKey")], "Blizzard_SharedXML/CustomBindingButtonMixin.lua:61-81; BindingUtil.lua:66-67 conversion can return original input, key opaque"),
            ("OnUpdate", vec![("elapsed", "number")], "Blizzard_SharedXML/Mainline/ModelFrameMixin.lua:126; UpdateRotation multiplies elapsedTime"),
        ]);
    }
    for (name, parameters, citation) in definitions {
        let mut args = vec![json!({"name":"self", "type":"CScriptObject"})];
        args.extend(parameters.into_iter().map(|(name, ty)| {
            if let Some(slot) = ty.strip_prefix("opaque:") {
                json!({"name":name,"attributes":{"SeluOpaque":slot,"Nilable":true}})
            } else if let Some(ty) = ty.strip_suffix('?') {
                json!({"name":name,"type":ty,"attributes":{"Nilable":true}})
            } else {
                json!({"name":name,"type":ty})
            }
        }));
        let mut citation = if name == "OnClick" {
            citation.replace("1335-1364", &format!("{click_line}-{}", click_line + 29))
        } else {
            citation.to_owned()
        };
        if source.commit == "eac30af597e5aaa43a60f6675ef5fdcea100375f" {
            citation = citation
                .replace(":57-81", ":59-84")
                .replace(":61-81", ":63-84")
                .replace(":126;", ":109-115,132;");
        }
        scripts.insert(name.into(), json!({"name":name,"type":"CallbackType","arguments":args,"attributes":{"SeluSourceCitation":format!("{}: {citation}", source.commit)}}));
    }
    for widget in &mut m.widgets {
        if widget.name == "SimpleFrameAPI" {
            for method in &mut widget.methods {
                if method.name == "SetAttribute" {
                    for argument in &mut method.arguments {
                        if argument.name == "value" {
                            argument
                                .attributes
                                .insert("SeluOpaqueInput".into(), json!("ScriptAttributeValue"));
                            argument.attributes.insert("Nilable".into(), json!(true));
                            argument.attributes.insert("SeluSourceCitation".into(), json!(format!("{}: Blizzard_RestrictedAddOnEnvironment/SecureHandlers.lua:312-325,380-390; SetAttribute clears with nil; OnAttributeChanged forwards native attribute value", source.commit)));
                        }
                    }
                }
            }
        }
        if widget.name != "SimpleScriptRegionAPI" {
            continue;
        }
        for method in &mut widget.methods {
            if matches!(method.name.as_str(), "SetScript" | "HookScript") {
                method
                    .attributes
                    .insert("SeluScriptCallbacks".into(), scripts.clone().into());
            }
        }
    }
}

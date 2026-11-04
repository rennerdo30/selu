use selu_manifest::{Manifest, load_manifest};
use selu_manifest_gen::{merge_system, read_documentation};
fn empty() -> Manifest {
    serde_json::from_str(
        r#"{"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1}"#,
    )
    .unwrap()
}
fn manifests() -> Vec<Manifest> {
    [("retail", "12.1.5.70391"), ("forever", "1.60.1.70532")]
        .iter()
        .map(|(f, b)| {
            load_manifest(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("../../manifests/{f}/{b}.json")),
            )
            .unwrap()
        })
        .collect()
}
#[test]
fn predicates_and_truthful_summaries() {
    let mut m = empty();
    for s in read_documentation(r#"APIDocumentation:AddDocumentationTable({Name="Test",Environment="SecureOnly",Functions={{Name="NoSecret",SecretReturns=false,TransliteratorAllowed=true},{Name="Conditional",SecretWhenUnitIdentityRestricted=true,RequiresValidActionSlot=true}},Predicates={{Name="TransliteratorAllowed",Type="Precondition",FailureMode="ReturnNothing"},{Name="RequiresValidActionSlot",Type="Precondition",FailureMode="ReturnNothing"}}})"#).unwrap() {merge_system(&mut m,s);}
    let no = &m.functions[0];
    let conditional = &m.functions[1];
    assert!(!no.returns_secret, "false policy is not secret");
    assert!(
        conditional.returns_secret,
        "conditional policy must be represented"
    );
    assert!(!no.available);
    assert!(
        conditional
            .restrictions
            .contains(&"RequiresValidActionSlot".into())
    );
    let json = serde_json::to_value(&m).unwrap();
    assert_eq!(json["predicates"].as_array().unwrap().len(), 2);
    assert_eq!(
        no.signature.as_ref().unwrap().attributes["TransliteratorAllowed"],
        true
    );
    assert_eq!(
        no.signature.as_ref().unwrap().attributes["Environment"],
        "SecureOnly"
    );
}
#[test]
fn review_compiler_probes() {
    for m in manifests() {
        for n in [
            "InCombatLockdown",
            "SetCVar",
            "GetCVar",
            "UnitGUID",
            "GetTime",
            "hooksecurefunc",
            "PlaySound",
            "GetRealmName",
            "IsInGroup",
            "IsShiftKeyDown",
        ] {
            assert!(m.function(n).is_some(), "{} {n}", m.flavour);
        }
        {
            let src =
                r#"on PLAYER_LOGIN() { InCombatLockdown(); SetCVar("showHonorAsExperience", 1); }"#;
            let ast = selu_syntax::parse("probe.selu", src).unwrap();
            let checked = selu_check::check_program(&ast, &m);
            assert!(
                checked.diagnostics.is_empty(),
                "{}: {:?}",
                m.flavour,
                checked.diagnostics
            );
            assert!(!selu_lower::lower(&ast, &m, &checked.types).lua.is_empty());
        }
    }
}
#[test]
fn secure_only_not_available() {
    for m in manifests() {
        for name in [
            "C_CombatLogSecure.ClearEventFilters",
            "C_PingSecure.ClearHitTestPingInfo",
        ] {
            assert!(
                m.functions.iter().any(|f| f.name == name && !f.available),
                "{name}"
            );
            assert!(m.function(name).is_none());
            let src = format!("on PLAYER_LOGIN() {{ {name}(); }}");
            let ast = selu_syntax::parse("secure.selu", &src).unwrap();
            let checked = selu_check::check_program(&ast, &m);
            assert!(
                checked.diagnostics.iter().any(|d| d.code == "SELU0301"),
                "{:?}",
                checked.diagnostics
            );
        }
    }
}
#[test]
fn inherited_widget_signatures() {
    for m in manifests() {
        let frame = m
            .function("CreateFrame")
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .returns[0]
            .ty
            .as_deref()
            .unwrap();
        for name in [
            "SetPoint",
            "Hide",
            "SetScript",
            "Show",
            "SetSize",
            "ClearAllPoints",
            "CreateTexture",
            "SetWidth",
            "RegisterEvent",
            "GetObjectType",
        ] {
            assert!(
                lookup(&m, frame, name, &mut vec![]).is_some(),
                "{} {frame}.{name}",
                m.flavour
            );
        }
        let text = lookup(&m, frame, "CreateFontString", &mut vec![])
            .unwrap()
            .returns[0]
            .ty
            .as_deref()
            .unwrap();
        assert!(lookup(&m, text, "SetText", &mut vec![]).is_some());
        let point = lookup(&m, frame, "SetPoint", &mut vec![]).unwrap();
        assert_eq!(point.arguments[0].ty.as_deref(), Some("FramePoint"));
        assert_eq!(point.arguments[1].ty.as_deref(), Some("ScriptRegion"));
        assert_eq!(point.arguments[3].ty.as_deref(), Some("uiUnit"));
        assert_eq!(point.attributes["IsProtectedFunction"], true);
    }
}
fn lookup<'a>(
    m: &'a Manifest,
    ty: &str,
    name: &str,
    seen: &mut Vec<String>,
) -> Option<&'a selu_manifest::Declaration> {
    if seen.iter().any(|v| v == ty) {
        return None;
    }
    seen.push(ty.into());
    let w = m.widgets.iter().find(|w| w.name == ty)?;
    w.methods.iter().find(|f| f.name == name).or_else(|| {
        w.inherits
            .iter()
            .find_map(|base| lookup(m, base, name, seen))
    })
}
#[test]
fn compact_and_overlay_evidence() {
    for m in manifests() {
        let value = serde_json::to_value(&m).unwrap();
        assert!(value["functions"][0].get("available").is_none());
        assert!(value["enums"][0].get("values").is_some());
        assert!(
            m.functions
                .iter()
                .filter(|f| f.name == "CreateFrame" || f.name == "print")
                .all(|f| format!("{:?}", f.evidence) != "Verified")
        );
        assert!(value["source"]["interfaceBasis"].is_string());
    }
}

#[test]
fn overlay_requires_evidence_and_live_probe() {
    let mut m = empty();
    assert!(
        selu_manifest_gen::apply_overlay(&mut m, "[[globals]]\nname='X'\nnote='source'").is_err()
    );
    assert!(
        selu_manifest_gen::apply_overlay(
            &mut m,
            "[[globals]]\nname='X'\nnote='source'\nevidence='verified'"
        )
        .is_err()
    );
    assert!(
        selu_manifest_gen::apply_overlay(
            &mut m,
            "[[globals]]\nname='X'\nnote='source'\nevidence='source'\nunrecognized=1"
        )
        .is_err()
    );
}

#[test]
fn review_enum_compiler_probe() {
    let src = "on PLAYER_LOGIN() { print(Enum.UIMapType.Continent); }";
    for m in manifests() {
        let ast = selu_syntax::parse("enum.selu", src).unwrap();
        let checked = selu_check::check_program(&ast, &m);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert!(!selu_lower::lower(&ast, &m, &checked.types).lua.is_empty());
    }
}

#[test]
fn generated_mixin_signature_checks_template_arguments() {
    let mut m = empty();
    m.widgets.push(
        serde_json::from_value(serde_json::json!({"name":"Frame","type":"ScriptObject"})).unwrap(),
    );
    let mut lua = selu_manifest_gen::source::Sources::default();
    lua.parse("mixin.lua", "TestMixin = {};\n---@param value number\n---@return boolean\nfunction TestMixin:SetValue(value) return value > 0 end").unwrap();
    lua.extend_manifest(&mut m);
    let mut xml = selu_manifest_gen::xml::Xml::default();
    xml.parse("test.xml", r#"<Ui><Frame name="Base" virtual="true" mixin="TestMixin"/><Frame name="Panel" inherits="Base"/></Ui>"#).unwrap();
    xml.extend_manifest(&mut m);
    let good =
        selu_syntax::parse("good.selu", "const result: boolean = Panel.SetValue(3);").unwrap();
    assert!(selu_check::check_program(&good, &m).diagnostics.is_empty());
    let bad = selu_syntax::parse("bad.selu", "Panel.SetValue(\"bad\");").unwrap();
    assert!(
        selu_check::check_program_with_options(
            &bad,
            &m,
            selu_check::CheckOptions { strict_api: true }
        )
        .diagnostics
        .iter()
        .any(|d| d.code == "SELU0481")
    );
}

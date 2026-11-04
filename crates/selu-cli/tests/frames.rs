use selu_check::{CheckOptions, ModuleBindings, check_module_targets_with_options};
use selu_manifest::Manifest;
use std::path::Path;

fn retail() -> Manifest {
    selu_manifest::load_manifest_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .unwrap()
        .into_iter()
        .find(|m| m.flavour == "retail" && !m.fixture)
        .unwrap()
}
fn check(src: &str, targets: &[Manifest], strict: bool) -> selu_check::Checked {
    let p = selu_syntax::parse("frames.selu", src).unwrap();
    check_module_targets_with_options(
        &p,
        &targets[0],
        ModuleBindings::default(),
        targets,
        CheckOptions { strict_api: strict },
    )
    .0
}
#[test]
fn unknown_literals_warn_but_dynamic_templates_stay_open() {
    let m = retail();
    let d = check(
        r#"const f = CreateFrame("Frame", null, null, "AddonTemplate");"#,
        std::slice::from_ref(&m),
        true,
    );
    assert_eq!(d.diagnostics.len(), 1);
    assert_eq!(d.diagnostics[0].code, "SELU0580");
    assert!(!d.has_errors());
    assert!(check(r#"const template = "AddonTemplate"; const f = CreateFrame("Frame", null, null, template);"#, &[m], true).diagnostics.is_empty());
}
#[test]
fn missing_inherited_member_names_the_stricter_target() {
    let m = retail();
    let mut other = m.clone();
    other.flavour = "without-backdrop".into();
    other.templates.retain(|t| t.name != "BackdropTemplate");
    let src = r#"const create = CreateFrame; const f = create("Frame", null, null, " BackdropTemplate, UIPanelButtonTemplate "); f.SetBackdrop({});"#;
    let d = check(src, &[m, other], false);
    assert!(
        d.diagnostics
            .iter()
            .any(|d| d.code == "SELU0581" && d.message.contains("without-backdrop")),
        "{:?}",
        d.diagnostics
    );
}
#[test]
fn named_child_is_a_widget_and_templates_follow_mixin_inheritance() {
    let mut m = retail();
    let child = m
        .globals
        .iter()
        .find(|g| g.ty.as_deref() == Some("Button"))
        .unwrap()
        .clone();
    let mut child = child;
    child.name = "GameTooltipTestChild".into();
    m.globals.push(child);
    let d = check(
        "GameTooltip.TestChild.SetSize(true, 10);",
        std::slice::from_ref(&m),
        true,
    );
    assert!(
        d.diagnostics.iter().any(|d| d.code == "SELU0481"),
        "{:?}",
        d.diagnostics
    );
    let mut other = m.clone();
    other.flavour = "without-mixins".into();
    other
        .data_globals
        .retain(|d| d.name != "BackdropTemplateMixin");
    for t in &mut other.templates {
        t.methods.retain(|s| s != "SetBackdrop");
    }
    let d = check(
        r#"const f = CreateFrame("Frame", null, null, "BackdropTemplate"); f.SetBackdrop({});"#,
        &[m, other],
        false,
    );
    assert!(d.has_errors(), "{:?}", d.diagnostics);
}

#[test]
fn named_templates_and_simulator_backdrop_storage() {
    let m = retail();
    let mut other = m.clone();
    other.flavour = "without-tooltip".into();
    other.templates.retain(|d| d.name != "GameTooltipTemplate");
    let d = check("GameTooltip.OnLoadGameTooltip();", &[m, other], false);
    assert!(
        d.diagnostics.iter().any(|d| d.code == "SELU0581"),
        "{:?}",
        d.diagnostics
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval(
        r#"
local panel = CreateFrame('Frame', nil, nil, ' BackdropTemplate ')
panel:SetBackdrop({bgFile = 'background'})
panel:SetBackdropColor(.1, .2, .3)
print(panel:GetBackdrop().bgFile)
print(panel:GetBackdropColor())
"#,
    )
    .unwrap();
    assert_eq!(sim.prints(), vec!["background", "0.1\t0.2\t0.3\t1"]);
    assert!(sim.logs().is_empty(), "{:?}", sim.logs());
}

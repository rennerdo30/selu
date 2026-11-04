use selu_check::{CheckOptions, ModuleBindings, check_module_targets_with_options};
use selu_manifest::Manifest;

fn retail() -> Manifest {
    cached_manifests()
        .clone()
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
fn named_child_is_global_and_templates_follow_mixin_inheritance() {
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
        d.diagnostics.iter().any(|d| d.code == "SELU0582"),
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
    let d = check("GameTooltip.RefreshDataNextUpdate();", &[m, other], false);
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

#[test]
fn review_global_children_are_not_fields_and_run_as_globals() {
    let m = retail();
    for (src, global) in [
        ("MerchantFrame.Tab1.SetText(\"ok\");", "MerchantFrameTab1"),
        (
            "GameTooltip.Texture1.SetTexture(123);",
            "GameTooltipTexture1",
        ),
    ] {
        let d = check(src, std::slice::from_ref(&m), false);
        assert!(
            d.diagnostics
                .iter()
                .any(|d| d.code == "SELU0582" && d.help.iter().any(|h| h.contains(global))),
            "{:?}",
            d.diagnostics
        );
    }
    let src = "MerchantFrameTab1.SetText(\"ok\"); print(MerchantFrameTab1.GetText() ?? \"\");";
    let p = selu_syntax::parse("frames.selu", src).unwrap();
    let c = check(src, std::slice::from_ref(&m), false);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let lua = selu_lower::lower(&p, &m, &c.types).lua;
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("MerchantFrame = CreateFrame('Frame'); MerchantFrameTab1 = CreateFrame('Button'); assert(rawget(MerchantFrame, 'Tab1') == nil)").unwrap();
    sim.load_files(&[("frames.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), vec!["ok"]);
}

#[test]
fn review_scoped_guards_filter_named_and_created_frames() {
    let m = retail();
    let other = cached_manifests()
        .clone()
        .into_iter()
        .find(|m| m.flavour == "classic-era" && !m.fixture)
        .unwrap();
    let targets = [m, other];
    for src in [
        "if (has(AbandonHouseConfirmationDialog)) { GameTooltip.RefreshDataNextUpdate(); }",
        "if (has(AbandonHouseConfirmationDialog)) { const f = CreateFrame(\"GameTooltip\", null, null, \"GameTooltipTemplate\"); f.RefreshDataNextUpdate(); }",
        "function setup() { if (!has(AbandonHouseConfirmationDialog)) { return; } GameTooltip.RefreshDataNextUpdate(); } setup();",
    ] {
        for primary in &targets {
            let p = selu_syntax::parse("frames.selu", src).unwrap();
            let d = check_module_targets_with_options(
                &p,
                primary,
                ModuleBindings::default(),
                &targets,
                CheckOptions::default(),
            )
            .0;
            assert!(!d.has_errors(), "{src}: {:?}", d.diagnostics);
            for shared in [false, true] {
                let universe = selu_check::targets::with_fallbacks(primary, &targets);
                let lua = selu_lower::lower_module_layout(
                    &p,
                    &universe,
                    &d.types,
                    Default::default(),
                    shared,
                    primary,
                )
                .lua;
                let sim = selu_sim::Sim::new().unwrap();
                let active = primary.flavour == "retail";
                sim.eval(&format!("setmetatable(_G, nil); AbandonHouseConfirmationDialog = {}; GameTooltip = {{ RefreshDataNextUpdate = function() print('refresh') end }}; local create = CreateFrame; CreateFrame = function(...) local f = create(...); f.RefreshDataNextUpdate = function() print('refresh') end; return f end", if active { "{}" } else { "nil" })).unwrap();
                sim.load_addon("Frames", &[("frames.lua".into(), lua)])
                    .unwrap();
                assert_eq!(
                    sim.prints(),
                    if active { vec!["refresh"] } else { vec![] },
                    "{src}, shared={shared}"
                );
            }
        }
    }
    assert!(check("GameTooltip.RefreshDataNextUpdate();", &targets, false).has_errors());
}

fn collision_manifest() -> Manifest {
    let mut m = retail();
    let mut template = m
        .templates
        .iter()
        .find(|t| t.name == "BackdropTemplate")
        .unwrap()
        .clone();
    template.name = "ReviewA".into();
    template.methods = vec!["Probe".into()];
    template.inherits.clear();
    template.mixins.clear();
    m.templates.push(template.clone());
    template.name = "ReviewB".into();
    m.templates.push(template);
    let mut f = m
        .functions
        .iter()
        .find(|f| f.signature.is_some())
        .unwrap()
        .clone();
    let mut d = f.signature.clone().unwrap();
    d.arguments.clear();
    d.returns.clear();
    d.overloads.clear();
    let mut a = d.clone();
    a.name = "value".into();
    a.ty = Some("number".into());
    a.attributes.clear();
    d.arguments.push(a.clone());
    f.name = "ReviewA.Probe".into();
    f.signature = Some(d.clone());
    m.functions.push(f.clone());
    a.ty = Some("string".into());
    d.arguments = vec![a];
    f.name = "ReviewB.Probe".into();
    f.signature = Some(d);
    m.functions.push(f);
    m
}
#[test]
fn review_later_overrides_win_including_untyped_and_inherited() {
    let mut m = collision_manifest();
    let src = r#"const f = CreateFrame("Frame", null, null, "ReviewA,ReviewB"); f.Probe(true);"#;
    let d = check(src, std::slice::from_ref(&m), true);
    assert!(
        d.diagnostics
            .iter()
            .any(|d| d.code == "SELU0481" && d.message.contains("string")),
        "{:?}",
        d.diagnostics
    );
    m.functions.retain(|f| f.name != "ReviewB.Probe");
    assert!(
        check(src, std::slice::from_ref(&m), true)
            .diagnostics
            .is_empty()
    );
    let mut parent = m
        .templates
        .iter()
        .find(|t| t.name == "ReviewA")
        .unwrap()
        .clone();
    parent.name = "ReviewParent".into();
    parent.methods.clear();
    parent.inherits = vec!["ReviewA".into(), "ReviewB".into()];
    m.templates.push(parent);
    assert!(
        check(
            &src.replace("ReviewA,ReviewB", "ReviewParent"),
            std::slice::from_ref(&m),
            true
        )
        .diagnostics
        .is_empty()
    );
    let parent = m.templates.last_mut().unwrap();
    parent.mixins = std::mem::take(&mut parent.inherits);
    parent.inherits = vec!["ReviewParent".into()];
    assert!(
        check(&src.replace("ReviewA,ReviewB", "ReviewParent"), &[m], true)
            .diagnostics
            .is_empty()
    );
}
#[test]
fn review_unknown_template_spans_track_entries_and_escapes() {
    let m = retail();
    let src = r#"const f = CreateFrame("Frame", null, null, "BackdropTemplate,\tWrongOne, WrongTwo, Wrong\\Three, Wrong\"Four");"#;
    let d = check(src, &[m], false);
    let spans: Vec<_> = d
        .diagnostics
        .iter()
        .filter(|d| d.code == "SELU0580")
        .map(|d| &src[d.span.as_ref().unwrap().range()])
        .collect();
    assert_eq!(
        spans,
        vec!["WrongOne", "WrongTwo", r"Wrong\\Three", r#"Wrong\"Four"#]
    );
}
#[test]
fn review_backdrop_getters_clear_and_copy() {
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval(
        r#"
local f = CreateFrame('Frame', nil, nil, 'BackdropTemplate')
assert(f:GetBackdropColor() == nil)
f:SetBackdrop({bgFile = 'original', insets = {left = 2}})
local copy = f:GetBackdrop(); copy.bgFile = 'changed'
copy.insets.left = 10
assert(f:GetBackdrop().bgFile == 'original')
assert(f:GetBackdrop().insets.left == 2)
f:SetBackdropColor(.1, .2, .3)
f:SetBackdrop(nil)
assert(f:GetBackdrop() == nil and f:GetBackdropColor() == nil)
"#,
    )
    .unwrap();
}
#[test]
fn review_widget_annotations_accept_templates_and_preserve_assignability() {
    let m = retail();
    let src = r#"function style(f: Frame) { f.SetSize(12, 24); } const f = CreateFrame("Frame", null, null, "BackdropTemplate"); style(f); print(f.GetWidth() ?? 0); function text(t: FontString, texture: Texture, b: Button) {}"#;
    let d = check(src, std::slice::from_ref(&m), true);
    assert!(d.diagnostics.is_empty(), "{:?}", d.diagnostics);
    let program = selu_syntax::parse("frames.selu", src).unwrap();
    let lua = selu_lower::lower(&program, &m, &d.types).lua;
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("frames.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), vec!["12"]);
    assert!(
        check(
            r#"function buttonOnly(b: Button) {} buttonOnly(CreateFrame("Frame"));"#,
            &[m],
            true
        )
        .has_errors()
    );
}

/// Every manifest, parsed once per test binary (parsing all clients' JSON
/// for each test made this file one of the slowest in the suite).
fn cached_manifests() -> &'static Vec<selu_manifest::Manifest> {
    static ALL: std::sync::OnceLock<Vec<selu_manifest::Manifest>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        selu_manifest::load_manifest_dir(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"),
        )
        .unwrap()
    })
}

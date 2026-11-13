//! The checked boundary to other addons: `addon("Name").global("X")`,
//! `addonFields(frame).field` and protected field reads on `unknown`.
use selu_check::{CheckOptions, ModuleBindings, check_module_targets_with_options};
use selu_manifest::Manifest;

fn manifest() -> Manifest {
    selu_manifest::store::load_build(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store"),
        "wowf@1.60.1.70790",
    )
    .unwrap()
}
fn check(src: &str, m: &Manifest) -> (selu_syntax::Program, selu_check::Checked) {
    let p = selu_syntax::parse("boundary.selu", src).unwrap();
    let c = check_module_targets_with_options(
        &p,
        m,
        ModuleBindings::default(),
        std::slice::from_ref(m),
        CheckOptions::default(),
    )
    .0;
    (p, c)
}
fn codes(src: &str, m: &Manifest) -> Vec<String> {
    check(src, m)
        .1
        .diagnostics
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}
fn run(src: &str, m: &Manifest, setup: &str) -> Vec<String> {
    let (p, c) = check(src, m);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let lua = selu_lower::lower(&p, m, &c.types).lua;
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval(setup).unwrap();
    sim.load_files(&[("t.lua".into(), lua.clone())])
        .unwrap_or_else(|e| panic!("{e}\n{lua}"));
    sim.prints()
}

const READ: &str = r#"
    const frame = addon("Lorewalker").global("LWDialogFrame");
    print(typeof frame);
    const shown = frame.isShown;
    if (typeof shown == "boolean") { print(shown); }
    print(typeof frame.deep.deeper);
"#;

#[test]
fn another_addons_global_is_unknown_and_null_until_it_is_loaded() {
    let m = manifest();
    let loaded = "C_AddOns = C_AddOns or {}; C_AddOns.IsAddOnLoaded = function(name) return name == 'Lorewalker' end; LWDialogFrame = { isShown = true }";
    assert_eq!(run(READ, &m, loaded), ["table", "true", "nil"]);
    // Not loaded: the global is never read.
    let absent = "C_AddOns = C_AddOns or {}; C_AddOns.IsAddOnLoaded = function(name) return false end; LWDialogFrame = { isShown = true }";
    assert_eq!(run(READ, &m, absent), ["nil", "nil"]);
}

#[test]
fn field_reads_on_unknown_never_raise() {
    let m = manifest();
    // A string, a number and a table whose __index raises.
    let setup = "C_AddOns = C_AddOns or {}; C_AddOns.IsAddOnLoaded = function() return true end; A = 'text'; B = 7; C = setmetatable({}, { __index = function() error('boom') end })";
    let src = r#"
        print(typeof addon("Other").global("A").x);
        print(typeof addon("Other").global("B").x);
        print(typeof addon("Other").global("C").x);
    "#;
    assert_eq!(run(src, &m, setup), ["nil", "nil", "nil"]);
}

#[test]
fn unknown_must_still_be_narrowed_before_use() {
    let m = manifest();
    let src = r#"const v = addon("Other").global("Value"); const n: number = v;"#;
    assert!(!codes(src, &m).is_empty());
    let calls = r#"const v = addon("Other").global("Value"); v.run();"#;
    assert!(!codes(calls, &m).is_empty());
}

#[test]
fn names_are_literal_and_never_the_games() {
    let m = manifest();
    assert_eq!(
        codes(r#"const n = "Q"; addon(n).global("X");"#, &m),
        ["SELU0700"]
    );
    assert_eq!(
        codes(r#"const n = "X"; addon("Q").global(n);"#, &m),
        ["SELU0700"]
    );
    // A game global, a game function and a Blizzard addon stay typed.
    assert_eq!(codes(r#"addon("Q").global("UIParent");"#, &m), ["SELU0702"]);
    assert_eq!(
        codes(r#"addon("Q").global("CreateFrame");"#, &m),
        ["SELU0702"]
    );
    assert_eq!(
        codes(r#"addon("Blizzard_NamePlates").global("X");"#, &m),
        ["SELU0702"]
    );
    assert_eq!(codes(r#"addon("Q");"#, &m), ["SELU0701"]);
    assert_eq!(codes(r#"addon("Q").thing("X");"#, &m), ["SELU0701"]);
}

#[test]
fn addon_fields_reads_raw_fields_but_never_declared_members() {
    let m = manifest();
    let src = r#"
        const b = CreateFrame("Button");
        const bag = addonFields(b).bag;
        print(typeof bag);
    "#;
    assert_eq!(run(src, &m, ""), ["nil"]);
    // A declared method or a keyed child of the frame's template is refused.
    assert_eq!(
        codes(
            r#"const b = CreateFrame("Button"); const x = addonFields(b).GetName;"#,
            &m
        ),
        ["SELU0705"]
    );
    assert_eq!(
        codes(
            r#"const f = CreateFrame("Button", null, UIParent, "NamePlateUnitFrameTemplate"); const x = addonFields(f).HealthBarsContainer;"#,
            &m
        ),
        ["SELU0705"]
    );
    // Only frames, and only as `.field`.
    assert_eq!(
        codes(r#"const x = addonFields({ a: 1 }).a;"#, &m),
        ["SELU0704"]
    );
    assert_eq!(
        codes(
            r#"const b = CreateFrame("Button"); const all = addonFields(b);"#,
            &m
        ),
        ["SELU0704"]
    );
}

#[test]
fn optional_dependencies_are_collected() {
    let m = manifest();
    let (_, c) = check(
        r#"addon("Questie").global("QuestieLoader"); addon("Lorewalker").global("LWDialogFrame");"#,
        &m,
    );
    assert_eq!(
        c.types.optional_deps().into_iter().collect::<Vec<_>>(),
        ["Lorewalker", "Questie"]
    );
}

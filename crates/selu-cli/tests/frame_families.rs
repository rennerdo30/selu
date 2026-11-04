use selu_check::{CheckOptions, ModuleBindings, check_module_targets_with_options};
use selu_manifest::Manifest;
fn manifest() -> Manifest {
    selu_manifest::load_manifest(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/retail/12.1.5.70391.json"),
    )
    .unwrap()
}
fn check(src: &str, targets: &[Manifest]) -> (selu_syntax::Program, selu_check::Checked) {
    let p = selu_syntax::parse("families.selu", src).unwrap();
    let c = check_module_targets_with_options(
        &p,
        &targets[0],
        ModuleBindings::default(),
        targets,
        CheckOptions::default(),
    )
    .0;
    (p, c)
}
#[test]
fn checked_nullable_lookup_runs_in_simulator() {
    let m = manifest();
    let src = r#"const index = 1; const f = frameFamily("MerchantItem", index); if (f != null) { f.SetSize(42, 10); print(f.GetWidth() ?? 0); } const missing = frameFamily("MerchantItem", 2); print(missing == null);"#;
    let (p, c) = check(src, std::slice::from_ref(&m));
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let lua = selu_lower::lower(&p, &m, &c.types).lua;
    assert!(lua.contains("_G["), "{lua}");
    let sim = selu_sim::Sim::new().unwrap();
    sim.register_frame_family("MerchantItem", "").unwrap();
    sim.eval("CreateFrame('Frame', 'MerchantItem1')").unwrap();
    sim.eval(&lua).unwrap();
    assert_eq!(sim.prints(), vec!["42", "true"]);
}
#[test]
fn diagnostics_and_closed_global_boundary() {
    let m = manifest();
    for (src, code) in [
        (r#"frameFamily("Absent", 1);"#, "SELU0640"),
        (r#"frameFamily("MerchantItem", 1, "Absent");"#, "SELU0641"),
        (
            r#"const p = "MerchantItem"; frameFamily(p, 1);"#,
            "SELU0642",
        ),
        (
            r#"const s = "NameFrame"; frameFamily("MerchantItem", 1, s);"#,
            "SELU0642",
        ),
        (r#"frameFamily("MerchantItem", "1");"#, "SELU0643"),
        (r#"frameFamily("MerchantItem", 9999);"#, "SELU0645"),
        (r#"frameFamily("MerchantItem");"#, "SELU0646"),
        (r#"frameFamily("MerchantItem", -1);"#, "SELU0645"),
    ] {
        let (_, c) = check(src, std::slice::from_ref(&m));
        assert!(
            c.diagnostics.iter().any(|d| d.code == code),
            "{src}: {:?}",
            c.diagnostics
        );
    }
    for src in [
        r#"_G["MerchantItem1"].Show();"#,
        r#"frameFamily("MerchantItem", 1).Show();"#,
    ] {
        assert!(check(src, std::slice::from_ref(&m)).1.has_errors());
    }
    let mut other = m.clone();
    other.flavour = "missing".into();
    other.frame_families.clear();
    let (_, c) = check(r#"frameFamily("MerchantItem", 1);"#, &[m, other]);
    assert!(
        c.diagnostics
            .iter()
            .any(|d| d.code == "SELU0644" && d.message.contains("missing"))
    );
}
#[test]
fn suffix_widget_and_shadowing() {
    let m = manifest();
    for src in [
        r#"const f = frameFamily("MailItem", 1, "Subject"); if (f != null) { f.SetText("hello"); }"#,
        r#"function frameFamily(n: number) { return n; } print(frameFamily(42));"#,
    ] {
        let (p, c) = check(src, std::slice::from_ref(&m));
        assert!(!c.has_errors(), "{:?}", c.diagnostics);
        let lua = selu_lower::lower(&p, &m, &c.types).lua;
        if src.starts_with("function") {
            assert!(!lua.contains("_G["));
        }
    }
}

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

#[test]
fn review_family_children_and_environment_bindings_are_closed() {
    let m = manifest();
    for src in [
        r#"const f = frameFamily("MailItem", 1); if (f != null) { f.Subject.SetText(123); }"#,
        r#"const f = frameFamily("MailItem", 1); if (f != null) { const n: number = f.Subject; print(n + 1); }"#,
        r#"const f = MailItem1; const n: number = f.Subject;"#,
    ] {
        let p = selu_syntax::parse("review.selu", src).unwrap();
        let c = check_module_targets_with_options(
            &p,
            &m,
            Default::default(),
            std::slice::from_ref(&m),
            CheckOptions { strict_api: true },
        )
        .0;
        assert!(
            c.diagnostics.iter().any(|d| d.code == "SELU0582"),
            "{src}: {:?}",
            c.diagnostics
        );
    }
    let mut single = m.clone();
    single
        .frame_families
        .retain(|f| f.prefix != "MailItem" || f.suffix != "Subject");
    let (_, c) = check(
        r#"const f=frameFamily("MailItem",1);if(f!=null){const n:number=f.Subject;}"#,
        &[single],
    );
    assert!(c.diagnostics.iter().any(|d| d.code == "SELU0582"));
    for src in [
        r#"const _G = { MerchantItem1: 42 }; const f = frameFamily("MerchantItem", 1); if (f != null) { f.Show(); }"#,
        "function f(_G: number) { return _G; }",
        "const [_G] = [1];",
        "function _G() {}",
    ] {
        assert!(check(src, std::slice::from_ref(&m)).1.has_errors(), "{src}");
    }
}
#[test]
fn review_zero_index_and_production_session_reload() {
    let m = manifest();
    assert!(
        !check(
            r#"frameFamily("CharacterBag", 0, "Slot");"#,
            std::slice::from_ref(&m)
        )
        .1
        .has_errors()
    );
    let mut session = selu_sim::Session::new(selu_sim::SessionConfig {
        target: Some(m.target_string()),
        ..Default::default()
    })
    .unwrap();
    for reload in [false, true] {
        if reload {
            session.reload().unwrap();
        }
        session
            .eval("assert(_G['MerchantItem2'] == nil); UnmodelledAPI(1); UnmodelledAPI(2)")
            .unwrap();
        assert_eq!(
            session
                .logs()
                .iter()
                .filter(|s| s.contains("UnmodelledAPI"))
                .count(),
            1
        );
    }
}

#[test]
fn review_probes_preserve_evaluation_and_intrinsic_shadowing() {
    let m = manifest();
    for (src, expected) in [
        (
            r#"let i=0; function next(){ i++; return i; } const f=frameFamily("MerchantItem",next()+next()-1); print(i); if(f!=null){print(f.GetName() ?? "missing");}"#,
            vec!["2", "MerchantItem2"],
        ),
        (
            r#"let i=0; function next(){i++;return i;} const f=frameFamily("MerchantItem",next());print(i);if(f!=null){print(f.GetName() ?? "missing");}"#,
            vec!["1", "MerchantItem1"],
        ),
        (
            r#"const frameFamily=(n:number)=>n;print(frameFamily(3));"#,
            vec!["3"],
        ),
    ] {
        let (p, c) = check(src, std::slice::from_ref(&m));
        assert!(!c.has_errors(), "{:?}", c.diagnostics);
        let session = selu_sim::Session::new(selu_sim::SessionConfig {
            manifest: Some(m.clone()),
            ..Default::default()
        })
        .unwrap();
        session
            .eval("CreateFrame('Frame','MerchantItem1');CreateFrame('Frame','MerchantItem2')")
            .unwrap();
        session
            .eval(&selu_lower::lower(&p, &m, &c.types).lua)
            .unwrap();
        assert_eq!(session.prints(), expected);
    }
}
#[test]
fn review_loaded_project_manifest_and_manual_family_survive_reload() {
    let mut m = manifest();
    m.functions.clear();
    m.events.clear();
    m.widgets.clear();
    m.globals.clear();
    m.templates.clear();
    m.data_globals.clear();
    m.tables.clear();
    m.enums.clear();
    let root = std::env::temp_dir().join(format!("selu-f2-session-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::create_dir(root.join("manifests")).unwrap();
    std::fs::write(
        root.join("manifests/target.json"),
        serde_json::to_vec(&m).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("selu.toml"),
        format!(
            "[addon]\nname = \"FamilySession\"\ntargets = [\"{}\"]\n",
            m.target_string()
        ),
    )
    .unwrap();
    let addon = root.join("FamilySession");
    std::fs::create_dir(&addon).unwrap();
    std::fs::write(addon.join("FamilySession.toc"), "main.lua\n").unwrap();
    std::fs::write(addon.join("main.lua"), "assert(_G['MerchantItem2'] == nil)").unwrap();
    let mut session = selu_sim::Session::new(Default::default()).unwrap();
    session.load_addon_dir(&addon).unwrap();
    session.register_frame_family("Extra", "Child").unwrap();
    session.reload().unwrap();
    session.eval("assert(_G['MerchantItem2'] == nil); assert(_G['Extra1Child'] == nil); UnmodelledAPI(); UnmodelledAPI()").unwrap();
    assert_eq!(
        session
            .logs()
            .iter()
            .filter(|s| s.contains("UnmodelledAPI"))
            .count(),
        1
    );
}

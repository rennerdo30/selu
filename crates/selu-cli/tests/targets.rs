mod common;
#[test]
fn has_is_boolean_and_folds() {
    let lua = common::compile("if (has(GetTime)) { print(\"yes\"); } else { print(\"no\"); }");
    assert!(!lua.contains("if "), "{lua}");
    assert_eq!(
        common::run("if (has(GetTime)) { print(\"yes\"); } else { print(\"no\"); }"),
        ["yes"]
    );
}
#[test]
fn local_has_is_an_ordinary_function() {
    assert_eq!(
        common::run(
            "function has(n: number): boolean { return n > 0; } if (has(1)) { print(\"local\"); }"
        ),
        ["local"]
    );
}
fn targets() -> Vec<selu_manifest::Manifest> {
    let a = common::manifest();
    let mut b = a.clone();
    b.flavour = "other".into();
    b.interface = 42;
    b.functions.retain(|f| f.name != "GetTime");
    b.events.retain(|e| e.name != "UNIT_HEALTH");
    vec![a, b]
}
fn checked(src: &str) -> selu_check::Checked {
    let p = selu_syntax::parse("t.selu", src).unwrap();
    selu_check::check_module_targets(&p, &targets()[0], Default::default(), &targets()).0
}
#[test]
fn strictest_target_and_scoped_guards() {
    for source in ["GetTime();", "const f = GetTime;", "on UNIT_HEALTH() {}"] {
        let c = checked(source);
        assert_eq!(c.diagnostics.len(), 1, "{:?}", c.diagnostics);
        assert_eq!(c.diagnostics[0].code, "SELU0500");
        assert!(c.diagnostics[0].message.contains("other@"));
    }
    for source in [
        "if (has(GetTime)) { GetTime(); }",
        "if (!has(GetTime)) {} else { GetTime(); }",
        "function f() { if (!has(GetTime)) return; GetTime(); }",
        "let later = () => {}; if (has(GetTime)) { later = () => { GetTime(); }; } later();",
        "if (has(UNIT_HEALTH)) { on UNIT_HEALTH() {} }",
    ] {
        let c = checked(source);
        assert!(c.diagnostics.is_empty(), "{source}: {:?}", c.diagnostics);
    }
    assert_eq!(
        checked("if (has(GetTime)) { GetTime(); } GetTime();")
            .diagnostics
            .len(),
        1
    );
    assert_eq!(checked("has(123);").diagnostics[0].code, "SELU0501");
}
#[test]
fn both_layouts_fold_or_register_only_available_events() {
    let src = "const C_EventUtils = { IsEventValid: () => false }; if (has(GetTime)) { print(\"available\"); } else { print(\"absent\"); } if (has(UNIT_HEALTH)) { on UNIT_HEALTH() { print(\"health\"); } }";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let ts = targets();
    for (i, m) in ts.iter().enumerate() {
        let (c, _) = selu_check::check_module_targets(&p, m, Default::default(), &ts);
        assert!(c.diagnostics.is_empty(), "{:?}", c.diagnostics);
        for shared in [false, true] {
            let universe = selu_check::targets::with_fallbacks(m, &ts);
            let lua = selu_lower::lower_module_layout(
                &p,
                &universe,
                &c.types,
                Default::default(),
                shared,
                m,
            )
            .lua;
            assert_eq!(lua.contains("C_EventUtils.IsEventValid"), shared, "{lua}");
            if !shared {
                assert!(
                    !lua.contains("if false") && !lua.contains("if true"),
                    "{lua}"
                );
                assert_eq!(lua.contains("\"health\""), i == 0);
            }
            let sim = selu_sim::Sim::new().unwrap();
            sim.eval(&format!(
                "C_EventUtils = {{ IsEventValid = function() return {} end }}; {}",
                i == 0,
                if i == 1 {
                    "GetTime = nil; setmetatable(_G, nil)"
                } else {
                    ""
                }
            ))
            .unwrap();
            sim.load_addon("Test", &[("main.lua".into(), lua.clone())])
                .unwrap_or_else(|e| panic!("{e}\n{lua}"));
            sim.fire("UNIT_HEALTH", vec![]).unwrap();
            assert_eq!(
                sim.prints(),
                if i == 0 {
                    vec!["available", "health"]
                } else {
                    vec!["absent"]
                }
            );
        }
    }
    assert!(
        selu_lower::targets::generate_shared_toc("Test", &ts, &["main.lua".into()])
            .starts_with("## Interface: 100000, 42\n")
    );
}
#[test]
fn real_manifests_check_missing_functions_and_events() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests");
    let ts: Vec<_> = selu_manifest::load_manifest_dir(&root)
        .unwrap()
        .into_iter()
        .filter(|m| !m.fixture && ["retail", "forever"].contains(&m.flavour.as_str()))
        .collect();
    assert_eq!(ts.len(), 2);
    for source in [
        "C_PaperDollInfo.CanCursorCanGoInSlot(1);",
        "on PET_SPELL_POWER_UPDATE() {}",
    ] {
        let p = selu_syntax::parse("t.selu", source).unwrap();
        let (c, _) = selu_check::check_module_targets(&p, &ts[0], Default::default(), &ts);
        assert_eq!(c.diagnostics.len(), 1, "{:?}", c.diagnostics);
        assert_eq!(c.diagnostics[0].code, "SELU0500");
        assert!(c.diagnostics[0].message.contains("forever@"));
    }
}
#[test]
fn cli_layout_and_unknown_build_keys() {
    use std::{
        path::Path,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let root = std::env::temp_dir().join(format!("selu-t2-layout-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/main.selu"),
        "on PLAYER_LOGIN() { if (has(GetTime)) { print(\"clock\"); } }",
    )
    .unwrap();
    for (build, ok, shared) in [
        ("layout = \"shared\"", true, true),
        ("layout = \"per-target\"", true, false),
        ("layuot = \"shared\"", false, false),
        ("layout = \"unknown\"", false, false),
    ] {
        std::fs::write(
            root.join("selu.toml"),
            format!(
                "[addon]\nname = \"Test\"\ntargets = [\"retail\",\"forever\"]\n[build]\n{build}\n"
            ),
        )
        .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_selu"))
            .arg("build")
            .arg(&root)
            .arg("--manifest-dir")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        let start = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("build PID {pid} timed out");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.success(), ok, "{build}, PID {pid}");
        if ok {
            for flavour in if shared {
                vec![""]
            } else {
                vec!["-retail", "-forever"]
            } {
                let dir = root.join(format!("dist/Test{flavour}"));
                let toc = std::fs::read_to_string(dir.join("Test.toc")).unwrap();
                assert_eq!(toc.lines().next().unwrap().contains(","), shared);
                let lua = std::fs::read_to_string(dir.join("main.lua")).unwrap();
                assert_eq!(lua.contains("GetTime ~= nil"), shared, "{lua}");
            }
        }
    }
}
#[test]
fn dotted_runtime_query_short_circuits_missing_namespace() {
    let mut ts = targets();
    let mut function = ts[0]
        .functions
        .iter()
        .find(|f| f.name == "GetTime")
        .unwrap()
        .clone();
    function.name = "C_Test.Query".into();
    ts[0].functions.push(function);
    let p = selu_syntax::parse(
        "t.selu",
        "if (has(C_Test.Query)) { print(C_Test.Query()); } else { print(\"missing\"); }",
    )
    .unwrap();
    let (c, _) = selu_check::check_module_targets(&p, &ts[0], Default::default(), &ts);
    assert!(c.diagnostics.is_empty(), "{:?}", c.diagnostics);
    let lua =
        selu_lower::lower_module_layout(&p, &ts[0], &c.types, Default::default(), true, &ts[0]).lua;
    assert!(
        lua.contains("C_Test ~= nil and C_Test.Query ~= nil"),
        "{lua}"
    );
    for setup in [
        "C_Test = { Query = function() return 'answer' end }",
        "C_Test = {};",
        "setmetatable(_G, nil)",
    ] {
        let sim = selu_sim::Sim::new().unwrap();
        sim.eval(setup).unwrap();
        sim.load_files(&[("t.lua".into(), lua.clone())]).unwrap();
        assert_eq!(
            sim.prints(),
            if setup.contains("answer") {
                vec!["answer"]
            } else {
                vec!["missing"]
            }
        );
    }
}
#[test]
fn namespace_guard_does_not_imply_every_member_exists() {
    let mut ts = targets();
    let mut f = ts[0].functions[0].clone();
    f.name = "C_Test.Common".into();
    ts[0].functions.push(f.clone());
    ts[1].functions.push(f.clone());
    f.name = "C_Test.Extra".into();
    ts[0].functions.push(f);
    let p = selu_syntax::parse("t.selu", "if (has(C_Test)) { C_Test.Extra(); }").unwrap();
    let (c, _) = selu_check::check_module_targets(&p, &ts[0], Default::default(), &ts);
    assert_eq!(c.diagnostics.len(), 1);
    assert_eq!(c.diagnostics[0].code, "SELU0500");
}

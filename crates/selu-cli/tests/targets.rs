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
            assert_eq!(
                lua.contains("local __selu_event_pcall = pcall"),
                shared,
                "{lua}"
            );
            if !shared {
                assert!(
                    !lua.contains("if false") && !lua.contains("if true"),
                    "{lua}"
                );
                assert_eq!(lua.contains("\"health\""), i == 0);
            }
            let sim = selu_sim::Sim::new().unwrap();
            sim.eval(&format!(
                "local create=CreateFrame; CreateFrame=function(...) local f=create(...); local register=f.RegisterEvent; f.RegisterEvent=function(self,event) if event=='UNIT_HEALTH' and not {} then error('unknown event') end; return register(self,event) end; return f end; C_EventUtils=nil; {}",
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
    let ts = real_targets();
    assert_eq!(ts.len(), 2);
    for source in [
        "C_PaperDollInfo.CanCursorCanGoInSlot(1);",
        "on PET_SPELL_POWER_UPDATE() {}",
    ] {
        let p = selu_syntax::parse("t.selu", source).unwrap();
        let (c, _) = selu_check::check_module_targets(&p, &ts[0], Default::default(), &ts);
        assert_eq!(c.diagnostics.len(), 1, "{:?}", c.diagnostics);
        assert_eq!(c.diagnostics[0].code, "SELU0500");
        assert!(c.diagnostics[0].message.contains("wowf@"));
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
                vec!["-wow", "-wowf"]
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
fn real_targets() -> Vec<selu_manifest::Manifest> {
    let store = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store");
    ["wowf@1.60.1.70532", "wow@12.1.5.70391"]
        .iter()
        .map(|target| selu_manifest::store::load_build(&store, target).unwrap())
        .collect()
}
fn review_check(src: &str) -> selu_check::Checked {
    let ts = real_targets();
    let p = selu_syntax::parse("review.selu", src).unwrap();
    selu_check::check_module_targets(&p, &ts[0], Default::default(), &ts).0
}
#[test]
fn review_function_reference() {
    let c = review_check("const f = C_PaperDollInfo.CanCursorCanGoInSlot; f(1);");
    assert_eq!(c.diagnostics.len(), 1, "{:?}", c.diagnostics);
    assert_eq!(c.diagnostics[0].code, "SELU0500");
}
#[test]
fn review_namespace_alias() {
    let c = review_check("const ns = C_PaperDollInfo; ns.CanCursorCanGoInSlot(1);");
    assert_eq!(c.diagnostics.len(), 1, "{:?}", c.diagnostics);
    assert_eq!(c.diagnostics[0].code, "SELU0502");
    for src in [
        "print(C_PaperDollInfo);",
        "function f() { return C_PaperDollInfo; }",
        "export const ns = C_PaperDollInfo;",
    ] {
        assert_eq!(review_check(src).diagnostics[0].code, "SELU0502");
    }
    assert!(
        review_check("if (has(C_PaperDollInfo)) { print(\"yes\"); }")
            .diagnostics
            .is_empty()
    );
}
#[test]
fn review_enum_availability() {
    let c = review_check("print(Enum.RecentAlliesFriendTag.Professions);");
    assert_eq!(c.diagnostics.len(), 1, "{:?}", c.diagnostics);
    assert_eq!(c.diagnostics[0].code, "SELU0500");
    assert!(review_check("if (has(Enum.RecentAlliesFriendTag)) { print(Enum.RecentAlliesFriendTag.Professions); }").diagnostics.is_empty());
    assert!(
        review_check("print(Enum.InventoryType.IndexHeadType);")
            .diagnostics
            .is_empty()
    );
}
#[test]
fn review_unavailable_fallback() {
    let mut ts = targets();
    let mut f = ts[0]
        .functions
        .iter()
        .find(|f| f.name == "GetTime")
        .unwrap()
        .clone();
    f.available = false;
    ts[1].functions.push(f);
    let effective = selu_check::targets::with_fallbacks(&ts[1], &ts);
    assert!(effective.function("GetTime").is_some());
    assert!(!selu_check::targets::exists(&ts[1], "GetTime"));
    let p = selu_syntax::parse("t.selu", "if (has(GetTime)) { GetTime(); }").unwrap();
    assert!(
        selu_check::check_module_targets(&p, &ts[1], Default::default(), &ts)
            .0
            .diagnostics
            .is_empty()
    );
}
fn review_lua(src: &str) -> String {
    let ts = targets();
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let c = selu_check::check_module_targets(&p, &ts[1], Default::default(), &ts).0;
    assert!(c.diagnostics.is_empty(), "{:?}", c.diagnostics);
    selu_lower::lower(&p, &ts[1], &c.types).lua
}
#[test]
fn review_compound_folding() {
    assert_eq!(
        review_lua(
            "function side(): boolean { print(\"side\"); return true; } if (has(GetTime) && side()) { print(\"yes\"); } else { print(\"no\"); }"
        ),
        "local function side()\n  print(\"side\")\n  return true\nend\ndo\n  print(\"no\")\nend\n"
    );
}
#[test]
fn review_else_if_folding() {
    assert_eq!(
        review_lua(
            "const flag = true; if (flag) { print(\"first\"); } else if (has(GetTime)) { print(\"dead\"); } else { print(\"last\"); }"
        ),
        "local flag = true\nif flag then\n  print(\"first\")\nelse\n  print(\"last\")\nend\n"
    );
}
#[test]
fn review_event_no_eventutils() {
    let ts = targets();
    let p = selu_syntax::parse(
        "t.selu",
        "if (has(UNIT_HEALTH)) { on UNIT_HEALTH() { print(\"health\"); } }",
    )
    .unwrap();
    let c = selu_check::check_module_targets(&p, &ts[0], Default::default(), &ts).0;
    let lua =
        selu_lower::lower_module_layout(&p, &ts[0], &c.types, Default::default(), true, &ts[0]).lua;
    assert!(!lua.contains("C_EventUtils"), "{lua}");
    assert!(lua.contains("pcall"), "{lua}");
    for available in [true, false] {
        let sim = selu_sim::Sim::new().unwrap();
        sim.eval(&format!("local create = CreateFrame; CreateFrame = function(...) local f = create(...); local register=f.RegisterEvent; f.RegisterEvent=function(self,event) if event == 'UNIT_HEALTH' and not {} then error('unknown event') end; return register(self,event) end; return f end; C_EventUtils=nil; setmetatable(_G,nil)",available)).unwrap();
        sim.load_addon("Test", &[("t.lua".into(), lua.clone())])
            .unwrap();
        sim.fire("UNIT_HEALTH", vec![]).unwrap();
        assert_eq!(
            sim.prints(),
            if available { vec!["health"] } else { vec![] }
        );
    }
}
fn review_cli(
    name: &str,
    config: &str,
    files: &[(&str, &str)],
    manifests: Option<&std::path::Path>,
) -> (bool, String) {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let dir = std::env::temp_dir().join(format!("selu-review-{name}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("selu.toml"), config).unwrap();
    for (file, src) in files {
        std::fs::write(dir.join("src").join(file), src).unwrap();
    }
    let log = std::fs::File::create(dir.join("run.log")).unwrap();
    let default = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests");
    let mut p = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(manifests.unwrap_or(&default))
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn()
        .unwrap();
    let pid = p.id();
    eprintln!("review CLI PID {pid}");
    let start = Instant::now();
    let status = loop {
        if let Some(s) = p.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > Duration::from_secs(60) {
            p.kill().unwrap();
            p.wait().unwrap();
            panic!("PID {pid} timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    (
        status.success(),
        std::fs::read_to_string(dir.join("run.log")).unwrap(),
    )
}
#[test]
fn review_exported_reference() {
    let (ok, log) = review_cli(
        "reference",
        "[addon]\nname='Test'\ntargets=['retail','forever']\n",
        &[
            (
                "api.selu",
                "export const f = C_PaperDollInfo.CanCursorCanGoInSlot;",
            ),
            ("main.selu", "import { f } from \"./api\"; f(1);"),
        ],
        None,
    );
    assert!(!ok, "{log}");
    assert!(log.contains("SELU0500"), "{log}");
}
#[test]
fn review_mixed_dialects() {
    let dir = std::env::temp_dir().join(format!("selu-review-dialects-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut ts = targets();
    ts[1].lua_dialects = vec!["lua52".into()];
    for m in ts {
        std::fs::write(
            dir.join(format!("{}.json", m.flavour)),
            serde_json::to_string(&m).unwrap(),
        )
        .unwrap();
    }
    let (ok, log) = review_cli(
        "dialects",
        "[addon]\nname='Test'\ntargets=['forever','other']\n[build]\nlayout='shared'\n",
        &[("main.selu", "print(\"hello\");")],
        Some(&dir),
    );
    assert!(!ok, "{log}");
    assert!(log.contains("same Lua dialect"), "{log}");
}
#[test]
fn review_effects_before_capability() {
    let src = "function side(): boolean { print(\"side\"); return true; } if (side() && has(GetTime)) { print(\"yes\"); } else { print(\"no\"); }";
    let lua = review_lua(src);
    assert_eq!(
        lua,
        "local function side()\n  print(\"side\")\n  return true\nend\nside()\ndo\n  print(\"no\")\nend\n"
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["side", "no"]);
}
#[test]
fn review_partial_compound_folding() {
    assert_eq!(
        review_lua(
            "function side(): boolean { return true; } if (!has(GetTime) && side()) { print(\"yes\"); }"
        ),
        "local function side()\n  return true\nend\nif side() then\n  print(\"yes\")\nend\n"
    );
}
#[test]
fn integration_availability_is_error_even_when_api_mismatches_warn() {
    use selu_diagnostics::Severity;
    let ts = real_targets();
    for strict_api in [false, true] {
        let p = selu_syntax::parse(
            "policy.selu",
            "CreateFrame(true); C_PaperDollInfo.CanCursorCanGoInSlot(1);",
        )
        .unwrap();
        let c = selu_check::check_module_targets_with_options(
            &p,
            &ts[0],
            Default::default(),
            &ts,
            selu_check::CheckOptions { strict_api },
        )
        .0;
        assert_eq!(c.diagnostics.len(), 2, "{:?}", c.diagnostics);
        assert_eq!(c.diagnostics[0].code, "SELU0481");
        assert_eq!(
            c.diagnostics[0].severity,
            if strict_api {
                Severity::Error
            } else {
                Severity::Warning
            }
        );
        assert_eq!(c.diagnostics[1].code, "SELU0500");
        assert_eq!(c.diagnostics[1].severity, Severity::Error);
        assert!(c.has_errors());
    }
    for strict in [false, true] {
        let (ok, log) = review_cli(
            if strict {
                "policy-strict"
            } else {
                "policy-warn"
            },
            &format!(
                "[addon]\nname='Test'\ntargets=['retail','forever']\n[build]\nstrict_api={strict}\nlayout='shared'\n"
            ),
            &[("main.selu", "C_PaperDollInfo.CanCursorCanGoInSlot(1);")],
            None,
        );
        assert!(!ok, "{log}");
        assert!(log.contains("error[SELU0500]"), "{log}");
    }
}
#[test]
fn integration_guard_borrows_typed_multiple_results_and_factory_metadata() {
    use selu_check::Type;
    let mut ts = real_targets();
    ts[0].functions.retain(|f| f.name != "UnitName");
    let p=selu_syntax::parse("typed.selu","const make = CreateFrame; const button = make(\"Button\"); button.SetSize(120,24); if (has(UnitName)) { const get = UnitName; const [name, realm] = get(\"player\"); print(name ?? \"unknown\", realm ?? \"\"); }").unwrap();
    let (c, exports) = selu_check::check_module_targets_with_options(
        &p,
        &ts[0],
        Default::default(),
        &ts,
        selu_check::CheckOptions { strict_api: true },
    );
    assert!(exports.is_empty());
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    fn expr(e: &selu_syntax::Expr, c: &selu_check::Checked, found: &mut [bool; 3]) {
        if let Some(t) = c.types.get(e.id) {
            match t {
                Type::Function {
                    multiple_results: true,
                    api,
                    api_semantics,
                    ..
                } => {
                    if api.0.as_ref().is_some_and(|x| x.name == "UnitName") {
                        found[0] = true;
                        let mut other = t.clone();
                        if let Type::Function { api, .. } = &mut other {
                            api.0.as_mut().unwrap().target = "diagnostic-only".into();
                        }
                        assert_eq!(*t, other);
                    }
                    if api_semantics.frame_factory {
                        found[1] = true;
                    }
                }
                Type::Widget(n, _) if n == "Button" => found[2] = true,
                _ => {}
            }
        }
        selu_syntax::visit::child_exprs_and_stmts(e, &mut |ch| child(ch, c, found));
    }
    fn child(ch: selu_syntax::visit::Child<'_>, c: &selu_check::Checked, found: &mut [bool; 3]) {
        match ch {
            selu_syntax::visit::Child::Expr(e) => expr(e, c, found),
            selu_syntax::visit::Child::Stmt(s) => {
                selu_syntax::visit::child_stmts_and_exprs(s, &mut |ch| child(ch, c, found))
            }
        }
    }
    let mut found = [false; 3];
    for s in &p.stmts {
        selu_syntax::visit::child_stmts_and_exprs(s, &mut |ch| child(ch, &c, &mut found));
    }
    assert_eq!(found, [true; 3]);
    let bad = selu_syntax::parse("typed.selu", "if (has(UnitName)) { UnitName(true); }").unwrap();
    let c = selu_check::check_module_targets_with_options(
        &bad,
        &ts[0],
        Default::default(),
        &ts,
        selu_check::CheckOptions { strict_api: true },
    )
    .0;
    assert_eq!(c.diagnostics[0].code, "SELU0481");
    assert!(c.has_errors());
}
#[test]
fn integration_namespaced_fallback_reference_keeps_signature_and_shapes() {
    let mut ts = targets();
    ts[1].functions.push(serde_json::from_str(r#"{"name":"C_Test.Read","evidence":"documented","signature":{"name":"Read","type":"Function","arguments":[{"name":"label","type":"string"}],"returns":[{"name":"row","type":"BorrowedRow"}]}}"#).unwrap());
    ts[1].tables.push(
        serde_json::from_str(
            r#"{"name":"BorrowedRow","type":"Structure","fields":[{"name":"label","type":"string"}]}"#,
        )
        .unwrap(),
    );
    let p=selu_syntax::parse("borrow.selu","if (has(C_Test.Read)) { const read = C_Test.Read; const row = read(true); const n: number = row.label; }").unwrap();
    let c = selu_check::check_module_targets_with_options(
        &p,
        &ts[0],
        Default::default(),
        &ts,
        selu_check::CheckOptions { strict_api: true },
    )
    .0;
    assert_eq!(c.diagnostics.len(), 2, "{:?}", c.diagnostics);
    assert_eq!(c.diagnostics[0].code, "SELU0481");
    assert!(
        c.diagnostics[1].message.contains("string"),
        "{:?}",
        c.diagnostics
    );
}
#[test]
fn integration_fold_preserves_effectful_property_reads() {
    let lua = review_lua(
        "if (UIParent.flag && has(GetTime)) { print(\"yes\"); } else { print(\"no\"); }",
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("UIParent=setmetatable({}, {__index=function(_,key) if key=='flag' then print('read'); return true end end})").unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["read", "no"]);
}

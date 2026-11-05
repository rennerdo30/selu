mod common;
use common::run;
fn compile(src: &str) -> String {
    let mut m = common::manifest();
    for name in ["ADDON_LOADED", "PLAYER_ENTERING_WORLD"] {
        m.events.push(
            serde_json::from_value(
                serde_json::json!({"name":name,"params":[],"evidence":"verified"}),
            )
            .unwrap(),
        );
    }
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let c = selu_check::check_program(&p, &m);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    selu_lower::lower(&p, &m, &c.types).lua
}
#[test]
fn reasoned_reads_keep_false_absence_and_failure() {
    assert_eq!(
        run(r#"
const n = readNumber(() => null); if (n.ok == false) { print(n.reason); }
const b = readBool(() => false); if (b.ok == true) { print(b.value); }
const absent = readBool(() => null); if (absent.ok == false) { print(absent.reason); }
const truth = readBool(() => 0); if (truth.ok == true) { print(truth.value); }
const fail = readNumber(() => { throw new Error("failed"); }); if (fail.ok == false) { print(fail.reason); }
const text = readString(() => "yes"); if (text.ok == true) { print(text.value); }
"#),
        ["none", "false", "unreadable", "true", "unreadable", "yes"]
    );
}
#[test]
fn unavailable_stack_is_nullable() {
    assert_eq!(
        run(r#"try { throw new Error("x"); } catch(e) { print(e.stack ?? "missing"); }"#),
        ["missing"]
    );
}
#[test]
fn custody_reasserts_without_accumulation_and_breaks_wrapper_cycles() {
    let lua = compile(
        r#"
const h = onError(e => { print(e.message); });
global custody = h;
on ADDON_LOADED() { h.ensureInstalled(); }
on PLAYER_ENTERING_WORLD() { h.ensureInstalled(); }

"#,
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("prior = 0; seterrorhandler(function() prior = prior + 1 end)")
        .unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    sim.eval(
        r#"
assert(custody.isInstalled())
local ours = geterrorhandler()
local wrapped = 0
seterrorhandler(function(...) wrapped = wrapped + 1; ours(...) end)
assert(not custody.isInstalled())
custody.ensureInstalled()
for i=1,100 do custody.ensureInstalled() end
assert(geterrorhandler() == ours)
geterrorhandler()("wrapped")
assert(wrapped == 1)
seterrorhandler(function() prior = prior + 1 end)
"#,
    )
    .unwrap();
    sim.fire("ADDON_LOADED", vec![]).unwrap();
    sim.eval("assert(custody.isInstalled()); geterrorhandler()('replaced'); assert(prior == 2)")
        .unwrap();
    sim.fire("PLAYER_ENTERING_WORLD", vec![]).unwrap();
    sim.eval("geterrorhandler()('world'); assert(prior == 3)")
        .unwrap();
    assert_eq!(sim.prints(), ["wrapped", "replaced", "world"]);
}

#[test]
fn registration_reuses_handle_and_replaces_callback() {
    let lua = compile(
        r#"const a = onError(e => { print("old"); }); const b = onError(e => { print("new"); }); print(a == b);"#,
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    sim.eval("geterrorhandler()('x')").unwrap();
    assert_eq!(sim.prints(), ["true", "new"]);
}
#[test]
fn stack_capture_uses_manifest_and_rejects_secret_trace() {
    let mut m = common::manifest();
    m.functions.push(
        serde_json::from_value(
            serde_json::json!({"name":"debugstack","available":true,"evidence":"verified"}),
        )
        .unwrap(),
    );
    let src = r#"try { throw new Error("x"); } catch(e) { print(e.stack ?? "missing"); }"#;
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let c = selu_check::check_program(&p, &m);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let lua = selu_lower::lower(&p, &m, &c.types).lua;
    for (setup, expected) in [
        ("function debugstack() return 'trace' end", "trace"),
        (
            "function debugstack() return 'trace' end; function issecretvalue(v) return v == 'trace' end",
            "missing",
        ),
        ("function debugstack() error('unavailable') end", "missing"),
    ] {
        let sim = selu_sim::Sim::new().unwrap();
        sim.eval(setup).unwrap();
        sim.load_files(&[("t.lua".into(), lua.clone())]).unwrap();
        assert_eq!(sim.prints(), [expected]);
    }
    m.functions.last_mut().unwrap().available = false;
    let lua = selu_lower::lower(&p, &m, &c.types).lua;
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("function debugstack() return 'not in this client' end")
        .unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["missing"]);
}
#[test]
fn shared_runtime_native_boundary_has_stack_helper() {
    use common::BoundedCommand;
    let root = std::env::temp_dir().join(format!("selu-se-shared-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("selu.toml"),
        "[addon]\nname = \"SEShared\"\ntargets = [\"wowf@1.0.0.1000\"]\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.selu"),
        r#"onError(e => { print(e.message); print(e.stack ?? "missing"); });"#,
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&root)
        .arg("--manifest-dir")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/fixtures"))
        .bounded_output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut session = selu_sim::Session::new(selu_sim::SessionConfig::default()).unwrap();
    session.load_addon_dir(&root.join("dist/SEShared")).unwrap();
    session.sim().eval("geterrorhandler()('native')").unwrap();
    assert_eq!(session.sim().prints(), ["native", "missing"]);
}
#[test]
fn review_wrapper_keeps_original_sink() {
    let lua = compile(r#"global custody = onError(e => { print(e.message); });"#);
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("prior=0; seterrorhandler(function(...) prior=prior+1; assert(select('#',...) == 4); local a,b,c,d=...; assert(a=='x' and b==nil and c==false and d==17) end)").unwrap();
    sim.load_files(&[("probe.lua".into(), lua)]).unwrap();
    sim.eval("local ours=geterrorhandler(); seterrorhandler(function(...) ours('changed') end); for i=1,100 do custody.ensureInstalled() end; geterrorhandler()('x',nil,false,17); assert(prior==1, 'original sink calls: '..prior)").unwrap();
    assert_eq!(sim.prints(), ["x"]);
}
#[test]
fn review_throwing_predecessor_is_contained() {
    let lua = compile(
        r#"global custody = onError(e => { print(e.message); }); const failures: number = custody.downstreamFailureCount();"#,
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("seterrorhandler(function() error('broken predecessor') end)")
        .unwrap();
    sim.load_files(&[("probe.lua".into(), lua)]).unwrap();
    sim.eval("local ok=pcall(geterrorhandler(),'x'); assert(ok,'predecessor exception escaped'); geterrorhandler()('again'); assert(custody.downstreamFailureCount()==2)").unwrap();
    assert_eq!(sim.prints(), ["x", "again"]);
}
#[test]
fn review_reasoned_reads_admit_secrecy_before_payload_comparison() {
    let lua = compile(
        "const n = readNumber(() => 0); const b = readBool(() => false); const s = readString(() => null);",
    );
    let runtime = &lua[lua.find("local function __selu_reasoned").unwrap()..];
    assert!(
        runtime
            .find("__selu_read_pcall(issecretvalue, value)")
            .unwrap()
            < runtime.find("value == nil").unwrap(),
        "secret admission must precede every payload comparison"
    );
}

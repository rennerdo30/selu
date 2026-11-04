use std::{path::Path, process::Command};
#[test]
fn project_runtime_is_first_and_helpers_leave_modules() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = root.join("examples/saved-settings");
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&project)
        .arg("--manifest-dir")
        .arg(root.join("manifests"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(project.join("selu.toml")).unwrap()).unwrap();
    let name = config["addon"]["name"].as_str().unwrap();
    let dist = project.join("dist").join(name);
    let toc = std::fs::read_to_string(dist.join(format!("{name}.toc"))).unwrap();
    assert_eq!(
        toc.lines().find(|s| !s.is_empty() && !s.starts_with("##")),
        Some("Selu.lua")
    );
    let main = std::fs::read_to_string(dist.join("main.lua")).unwrap();
    assert!(main.contains("local __selu_saved_runtime = __selu_ns.__selu_saved_runtime"));
    assert!(!main.contains("RegisterEvent(\"ADDON_LOADED\")"));
}
mod common;
#[test]
fn literal_nullish_needs_no_temporary_and_keeps_false() {
    let source = "print(null ?? false);";
    let lua = common::compile(source);
    assert!(!lua.contains("__selu_t"), "{lua}");
    assert_eq!(common::run(source), ["false"]);
}
#[test]
fn shared_error_widths_and_callback_dispatch_keep_values() {
    let manifest = common::manifest();
    let sources = [
        "function f(): [number,number] { try { return [2, 3]; } finally { print(\"cleanup\"); } } on PLAYER_LOGIN() { const [a,b] = f(); print(a,b); }",
        "on PLAYER_LOGIN() { try { throw new Error(\"caught\"); } catch(e) { print(e.message); } }",
    ];
    let mut files: Vec<_> = sources
        .iter()
        .map(|s| {
            let p = selu_syntax::parse("t.selu", s).unwrap();
            let c = selu_check::check_program(&p, &manifest);
            assert!(!c.has_errors(), "{:?}", c.diagnostics);
            selu_lower::lower(&p, &manifest, &c.types)
        })
        .collect();
    let runtime = selu_lower::runtime::share(&mut files).unwrap();
    assert_eq!(runtime.matches("shared event frame").count(), 1);
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon(
        "Runtime",
        &[
            ("Selu.lua".into(), runtime),
            ("a.lua".into(), files[0].lua.clone()),
            ("b.lua".into(), files[1].lua.clone()),
        ],
    )
    .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), ["cleanup", "2\t3", "caught"]);
}
#[test]
fn saved_initializes_before_user_addon_loaded_handler() {
    let manifests = selu_manifest::load_manifest_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"),
    )
    .unwrap();
    let manifest = manifests
        .into_iter()
        .find(|m| m.flavour == "forever" && !m.fixture)
        .unwrap();
    let p = selu_syntax::parse(
        "t.selu",
        "saved settings = { enabled: true }; on ADDON_LOADED(addon) { print(settings.enabled); }",
    )
    .unwrap();
    let c = selu_check::check_program(&p, &manifest);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let mut files = vec![selu_lower::lower(&p, &manifest, &c.types)];
    let runtime = selu_lower::runtime::share(&mut files).unwrap();
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon(
        "Runtime",
        &[
            ("Selu.lua".into(), runtime),
            ("main.lua".into(), files[0].lua.clone()),
        ],
    )
    .unwrap();
    assert_eq!(sim.prints(), ["true"]);
}
#[test]
fn hooks_need_no_localisation_runtime() {
    let manifest = common::manifest();
    let p = selu_syntax::parse(
        "t.selu",
        "const f = CreateFrame(\"Frame\"); hookScript(f, \"OnShow\", () => { print(1); });",
    )
    .unwrap();
    let c = selu_check::check_program(&p, &manifest);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let mut files = vec![selu_lower::lower(&p, &manifest, &c.types)];
    assert!(selu_lower::runtime::share(&mut files).is_none());
}

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
    assert!(main.contains("local __selu_saved_runtime = __selu_ns.__selu_saved"));
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
#[test]
fn runtime_name_cannot_overwrite_a_source_module() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("selu-k-collision-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"Collision\"\ntargets = [\"forever@1.0\"]\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/Selu.selu"), "export const value = 7;").unwrap();
    std::fs::write(
        dir.join("src/main.selu"),
        "import { value } from \"./Selu\"; on PLAYER_LOGIN() { print(value); }",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(root.join("manifests"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let dist = dir.join("dist/Collision");
    let toc = std::fs::read_to_string(dist.join("Collision.toc")).unwrap();
    let names: Vec<_> = toc.lines().filter(|s| s.ends_with(".lua")).collect();
    let unique: std::collections::HashSet<_> = names.iter().map(|s| s.to_lowercase()).collect();
    assert_eq!(names.len(), unique.len());
    let files: Vec<_> = names
        .iter()
        .map(|s| {
            (
                s.to_string(),
                std::fs::read_to_string(dist.join(s)).unwrap(),
            )
        })
        .collect();
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon("Collision", &files).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), ["7"]);
}

#[test]
fn saved_init_helpers_are_collectible_after_loading() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("selu-k-saved-gc-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"SavedGc\"\ntargets = [\"forever@1.0\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/state.selu"),
        "export saved settings = { count: 7 };",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.selu"),
        "import { settings } from \"./state\"; on PLAYER_LOGIN() { print(settings.count); }",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(root.join("manifests"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let dist = dir.join("dist/SavedGc");
    // A final ordinary Lua file captures the init table before ADDON_LOADED.
    std::fs::write(
        dist.join("observe.lua"),
        r#"local _, ns = ...
keptNamespace = ns
savedInitWeak = setmetatable({ ns.__selu_saved }, { __mode = "v" })
assert(savedInitWeak[1] ~= nil)
"#,
    )
    .unwrap();
    let toc_path = dist.join("SavedGc.toc");
    let mut toc = std::fs::read_to_string(&toc_path).unwrap();
    toc.push_str("observe.lua\n");
    std::fs::write(toc_path, &toc).unwrap();
    let files: Vec<_> = toc
        .lines()
        .filter(|s| s.ends_with(".lua"))
        .map(|s| {
            (
                s.to_string(),
                std::fs::read_to_string(dist.join(s)).unwrap(),
            )
        })
        .collect();
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon("SavedGc", &files).unwrap();
    sim.eval(
        r#"assert(keptNamespace ~= nil)
assert(keptNamespace.__selu_saved == nil)
collectgarbage("collect")
collectgarbage("collect")
assert(savedInitWeak[1] == nil, "saved init helpers retained after loading")
for key in pairs(keptNamespace) do
  assert(not key:match("^__selu_saved_"), "saved primitive capture exported: " .. key)
end"#,
    )
    .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), ["7"]);
}

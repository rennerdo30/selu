use selu_sim::Sim;

#[test]
fn saved_sessions_restore_false_nested_values_and_fire_loaded() {
    let files = vec![(
        "main.lua".into(),
        r#"
        local addon = ...
        local f = CreateFrame("Frame")
        f:RegisterEvent("ADDON_LOADED")
        f:SetScript("OnEvent", function(_, _, name)
            assert(name == addon)
            print("loaded")
        end)
    "#
        .into(),
    )];
    let sim = Sim::new().unwrap();
    sim.load_addon("Saved", &files).unwrap();
    assert_eq!(sim.prints(), vec!["loaded"]);
    sim.eval("SavedDB = { enabled = false, nested = { count = 7 }, [1] = 'first' }")
        .unwrap();
    let snapshot = sim.logout_saved(&["SavedDB"]).unwrap();
    sim.eval("SavedDB.nested.count = 99").unwrap();
    let next = Sim::login_saved("Saved", &files, &snapshot).unwrap();
    next.eval("assert(SavedDB.enabled == false); assert(SavedDB.nested.count == 7); assert(SavedDB[1] == 'first')").unwrap();
    assert_eq!(next.prints(), vec!["loaded"]);
    assert_eq!(next.logout_saved(&["SavedDB"]).unwrap(), snapshot);
}
mod common;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
static NEXT: AtomicU32 = AtomicU32::new(0);
fn project(source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "selu-saved-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"Saved\"\ntargets = [\"forever@1.0\"]\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/main.selu"), source).unwrap();
    dir
}
fn build(dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(dir)
        .arg("--manifest-dir")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .output()
        .unwrap()
}
fn files(dir: &Path) -> Vec<(String, String)> {
    let dist = dir.join("dist/Saved");
    std::fs::read_to_string(dist.join("Saved.toc"))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with("##") && !l.is_empty())
        .map(|l| (l.into(), std::fs::read_to_string(dist.join(l)).unwrap()))
        .collect()
}
fn success(dir: &Path) {
    let o = build(dir);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
#[test]
fn defaults_sessions_migrations_and_live_imports() {
    let dir = project(
        "import { settings } from \"./state\"; on PLAYER_LOGIN() { print(settings.enabled); print(settings.nested.count); }",
    );
    std::fs::write(dir.join("src/state.selu"),"export saved settings = { enabled: true, nested: { count: 3 } }; on PLAYER_LOGIN() { settings.enabled = false; settings.nested.count = 7; }").unwrap();
    success(&dir);
    let sim = Sim::new().unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["false", "7"]);
    let key = selu_lower::saved::global_name("Saved", "state", "settings");
    let saved = sim.logout_saved(&[&key]).unwrap();
    std::fs::write(dir.join("src/state.selu"),"export saved settings version 2 = { enabled: true, nested: { count: 3 }, added: 9 }; migrate settings 1 -> 2 (old) { old.nested.count = 12; }").unwrap();
    success(&dir);
    let next = Sim::login_saved("Saved", &files(&dir), &saved).unwrap();
    next.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(next.prints(), vec!["false", "12"]);
    next.eval(&format!(
        "assert({key}.added == 9); assert({key}.__selu_version == 2)"
    ))
    .unwrap();
    let toc = std::fs::read_to_string(dir.join("dist/Saved/Saved.toc")).unwrap();
    assert!(toc.contains(&format!("## SavedVariables: {key}\n")));
}
#[test]
fn shape_gate_preserves_previous_file_and_allows_additive_defaults() {
    let dir = project("saved settings = { a: 1 }; ");
    success(&dir);
    let before = std::fs::read(dir.join("selu.saved.toml")).unwrap();
    std::fs::write(dir.join("src/main.selu"), "saved settings = { a: false };").unwrap();
    let o = build(&dir);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("SELU0468"));
    assert_eq!(std::fs::read(dir.join("selu.saved.toml")).unwrap(), before);
    std::fs::write(
        dir.join("src/main.selu"),
        "saved settings = { a: 1, b: false };",
    )
    .unwrap();
    success(&dir);
    let stable = std::fs::read(dir.join("selu.saved.toml")).unwrap();
    success(&dir);
    assert_eq!(std::fs::read(dir.join("selu.saved.toml")).unwrap(), stable);
    std::fs::write(
        dir.join("src/main.selu"),
        "saved settings version 2 = { a: 1, b: false };",
    )
    .unwrap();
    assert!(!build(&dir).status.success());
}
#[test]
fn early_access_includes_writes_members_and_imports_but_allows_shadowing() {
    for source in [
        "saved s = { x: 1 }; print(s.x);",
        "saved s = { x: 1 }; s.x = 2;",
        "saved s = { x: 1 }; s = { x: 2 };",
    ] {
        assert!(
            common::codes(source).contains(&"SELU0460".into()),
            "{source}"
        );
    }
    assert!(
        common::codes(
            "saved s = { x: 1 }; function f() { print(s.x); } { const s = 3; print(s); }"
        )
        .is_empty()
    );
    let dir = project("import { s } from \"./state\"; print(s.x);");
    std::fs::write(dir.join("src/state.selu"), "export saved s = { x: 1 };").unwrap();
    let o = build(&dir);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("SELU0460"));
}
#[test]
fn failed_migration_rolls_back_and_retries_and_newer_data_is_untouched() {
    let dir = project(
        "saved s version 2 = { x: 1 }; migrate s 1 -> 2 (old) { old.x = 2; print(old.missing.value); } on PLAYER_LOGIN() { print(s.x); }",
    );
    success(&dir);
    let key = selu_lower::saved::global_name("Saved", "main", "s");
    let sim = Sim::new().unwrap();
    sim.eval(&format!("{key} = {{ x = 7, __selu_version = 1 }}"))
        .unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.eval(&format!(
        "assert({key}.x == 7); assert({key}.__selu_version == 1)"
    ))
    .unwrap();
    let snapshot = sim.logout_saved(&[&key]).unwrap();
    let next = Sim::login_saved("Saved", &files(&dir), &snapshot).unwrap();
    assert_eq!(next.logout_saved(&[&key]).unwrap(), snapshot);
    let newer = Sim::new().unwrap();
    newer
        .eval(&format!("{key} = {{ x = false, __selu_version = 9 }}"))
        .unwrap();
    let before = newer.logout_saved(&[&key]).unwrap();
    newer.load_addon("Saved", &files(&dir)).unwrap();
    assert_eq!(newer.logout_saved(&[&key]).unwrap(), before);
}
#[test]
fn runtime_globals_are_captured_and_forward_reads_capture_the_saved_local() {
    let dir = project(
        "const type = 1; const pairs = 1; const ipairs = 1; const rawget = 1; const pcall = 1; const error = 1; const string = 1; const _G = 1; const CreateFrame = 1; function read(): number { return s.x; } saved s = { x: 7 }; on PLAYER_LOGIN() { print(read()); }",
    );
    success(&dir);
    let sim = Sim::new().unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["7"]);
}
#[test]
fn per_character_defaults_arrays_maps_and_addon_names_are_isolated() {
    let dir = project(
        "saved perCharacter s = { enabled: true, inner: { x: 3 }, list: [1, 2], map: new Map<string, number>() }; on PLAYER_LOGIN() { print(s.enabled); print(s.inner.x); print(s.list[0]); s.list.push(4); s.map.set(\"key\", 5); }",
    );
    success(&dir);
    let key = selu_lower::saved::global_name("Saved", "main", "s");
    let sim = Sim::new().unwrap();
    sim.eval(&format!(
        "{key} = {{ enabled = false, inner = {{ x = 9 }} }}"
    ))
    .unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["false", "9", "1"]);
    let toc = std::fs::read_to_string(dir.join("dist/Saved/Saved.toc")).unwrap();
    assert!(toc.contains(&format!("## SavedVariablesPerCharacter: {key}\n")));
    assert_ne!(key, selu_lower::saved::global_name("Other", "main", "s"));
    let snapshot = sim.logout_saved(&[&key]).unwrap();
    let next = Sim::login_saved("Saved", &files(&dir), &snapshot).unwrap();
    next.eval(&format!(
        "assert({key}.map.key == 5); assert(#{key}.list == 3)"
    ))
    .unwrap();
}

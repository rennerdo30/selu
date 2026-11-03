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
    std::fs::write(dir.join("src/state.selu"),"export saved settings version 3 = { enabled: true, nested: { count: 3 }, added: 9 }; migrate settings 1 -> 2 (old) { old.nested.count = 12; } migrate settings 2 -> 3 (old) { old.nested.count += 1; }").unwrap();
    success(&dir);
    let next = Sim::login_saved("Saved", &files(&dir), &saved).unwrap();
    next.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(next.prints(), vec!["false", "13"]);
    next.eval(&format!(
        "assert({key}.added == 9); assert({key}.__selu_version == 3)"
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
        "saved s version 3 = { x: 1 }; migrate s 1 -> 2 (old) { old.x = 8; } migrate s 2 -> 3 (old) { old.x = 2; print(old.missing.value); } on PLAYER_LOGIN() { print(s.x); }",
    );
    success(&dir);
    let key = selu_lower::saved::global_name("Saved", "main", "s");
    let sim = Sim::new().unwrap();
    sim.eval(&format!("{key} = {{ x = 7, __selu_version = 1 }}"))
        .unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.eval(&format!(
        "assert({key}.x == 8); assert({key}.__selu_version == 2)"
    ))
    .unwrap();
    let snapshot = sim.logout_saved(&[&key]).unwrap();
    let next = Sim::login_saved("Saved", &files(&dir), &snapshot).unwrap();
    assert_eq!(next.logout_saved(&[&key]).unwrap(), snapshot);
    let warning_prefix = format!("{key}: migration failed: ");
    assert_eq!(
        sim.prints()
            .iter()
            .filter(|v| v.starts_with(&warning_prefix))
            .count(),
        1
    );
    assert_eq!(
        next.prints()
            .iter()
            .filter(|v| v.starts_with(&warning_prefix))
            .count(),
        1,
        "failed step must be attempted again at the second login"
    );
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
#[test]
fn review_generated_names_and_runtime_size() {
    for saved in [
        "data",
        "version",
        "fresh",
        "candidate",
        "migration",
        "ok",
        "message",
    ] {
        for (version, initial, migration, expected) in [
            ("", "", "", "7"),
            ("", "{ x = 9, __selu_version = 3 }", "", "9"),
            (
                " version 2",
                "{ x = 11 }",
                "migrate NAME 1 -> 2 (old) { old.x = 2; print(old.missing.value); }",
                "11",
            ),
        ] {
            let src = format!(
                "saved {saved}{version} = {{ x: 7 }}; {} on PLAYER_LOGIN() {{ print({saved}.x); }}",
                migration.replace("NAME", saved)
            );
            let dir = project(&src);
            success(&dir);
            let sim = Sim::new().unwrap();
            let key = selu_lower::saved::global_name("Saved", "main", saved);
            if !initial.is_empty() {
                sim.eval(&format!("{key} = {initial}")).unwrap();
            }
            sim.load_addon("Saved", &files(&dir)).unwrap();
            sim.fire("PLAYER_LOGIN", vec![]).unwrap();
            assert_eq!(sim.prints().last().unwrap(), expected, "{saved} {initial}");
        }
    }
    let source = (0..45)
        .map(|i| format!("saved s{i} = {{ x: {i} }};\n"))
        .collect::<String>()
        + "on PLAYER_LOGIN() { print(s44.x); }";
    let dir = project(&source);
    success(&dir);
    let fs = files(&dir);
    assert_eq!(fs[0].1.matches(" = ...").count(), 1);
    let sim = Sim::new().unwrap();
    sim.load_addon("Saved", &fs).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["44"]);
}
#[test]
fn review_adoption_and_readable_names() {
    let dir =
        project("saved global MyAddonDB = { x: 1 }; on PLAYER_LOGIN() { print(MyAddonDB.x); }");
    success(&dir);
    let sim = Sim::new().unwrap();
    sim.eval("MyAddonDB = { x = 99 }").unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["99"]);
    let snapshot = sim.logout_saved(&["MyAddonDB"]).unwrap();
    std::fs::write(dir.join("src/main.selu"),"saved global MyAddonDB version 2 = { x: 1 }; migrate MyAddonDB 1 -> 2 (old) { old.x += 1; } on PLAYER_LOGIN() { print(MyAddonDB.x); }").unwrap();
    success(&dir);
    let next = Sim::login_saved("Saved", &files(&dir), &snapshot).unwrap();
    next.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(next.prints(), vec!["100"]);
    let default = project("saved settings = { x: 1 };");
    success(&default);
    assert!(
        std::fs::read_to_string(default.join("dist/Saved/Saved.toc"))
            .unwrap()
            .contains("## SavedVariables: Saved_settings\n")
    );
    let sim = Sim::new().unwrap();
    sim.load_addon("Saved", &files(&default)).unwrap();
    assert_ne!(
        sim.logout_saved(&["Saved_settings"]).unwrap().0["Saved_settings"],
        selu_sim::saved::SavedValue::Nil
    );
}
#[test]
fn review_recursive_types_and_collection_shapes() {
    for src in [
        "saved s = { map: new Map<{ x: number }, number>() };",
        "saved s = { map: new Map<string, () => number>() };",
        "saved s = { map: new Map<string, { f: () => number }>() };",
        "saved s = { f: () => 1 };",
    ] {
        assert!(common::codes(src).contains(&"SELU0469".into()), "{src}");
    }
    let dir = project("saved s = { items: [{ x: 1 }] };");
    success(&dir);
    std::fs::write(
        dir.join("src/main.selu"),
        "saved s = { items: [{ x: 1, added: 2 }] };",
    )
    .unwrap();
    assert!(!build(&dir).status.success());
}
#[test]
fn review_invalid_metadata_isolation_and_retry_warning() {
    let dir = project(
        "saved s version 2 = { x: 1 }; migrate s 1 -> 2 (old) { old.x = 2; print(old.missing.value); } saved healthy = { x: 7 }; log.info`ready`; on PLAYER_LOGIN() { print(healthy.x); }",
    );
    success(&dir);
    let fs = files(&dir);
    let key = selu_lower::saved::global_name("Saved", "main", "s");
    for initial in [
        "{ x = 99, __selu_version = 'bad' }",
        "{ x = 99, __selu_version = false }",
        "{ x = 99, __selu_version = 0 }",
        "{ x = 99, __selu_version = -1 }",
        "{ x = 99, __selu_version = 1.5 }",
        "{ x = 99, __selu_version = 0/0 }",
        "42",
        "false",
    ] {
        let sim = Sim::new().unwrap();
        sim.eval(&format!("{key} = {initial}")).unwrap();
        sim.load_addon("Saved", &fs).unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert_eq!(sim.prints(), vec!["7"], "{initial}");
        sim.eval(&format!("local before = {initial}; local after = {key}; if type(before) == 'table' then assert(after.x == 99); if before.__selu_version ~= before.__selu_version then assert(after.__selu_version ~= after.__selu_version) else assert(after.__selu_version == before.__selu_version) end else assert(after == before) end")).unwrap();
    }
    let sim = Sim::new().unwrap();
    sim.eval(&format!("{key} = {{ x = 99 }}")).unwrap();
    let mut observed = fs.clone();
    observed.push(("inspect.lua".into(),"local _, ns = ...; local f=CreateFrame('Frame'); f:RegisterEvent('PLAYER_LOGIN'); f:SetScript('OnEvent', function() for _,v in ipairs(ns[false].buffer) do if v.level == 'warn' then print(v.level) end end end)".into()));
    sim.load_addon("Saved", &observed).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["7", "warn"]);
    let snapshot = sim.logout_saved(&[&key]).unwrap();
    let next = Sim::login_saved("Saved", &observed, &snapshot).unwrap();
    next.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(next.prints(), vec!["7", "warn"]);
}
#[test]
fn review_duplicate_names_and_migration_spans() {
    let dir = project("import { s as other } from \"./state\"; saved s = { x: 1 };");
    std::fs::write(dir.join("src/state.selu"), "export saved s = { x: 1 };").unwrap();
    let out = build(&dir);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("state.selu") && err.contains("main.selu"),
        "{err}"
    );
    for (src, part) in [
        ("migrate missing 1 -> 2 (old) {}", "missing"),
        (
            "saved s version 3 = { x: 1 }; migrate s 1 -> 3 (old) {}",
            "1 -> 3",
        ),
    ] {
        let p = selu_syntax::parse("t.selu", src).unwrap();
        let ds = selu_check::check(&p, &common::manifest());
        let d = ds.iter().find(|d| d.code == "SELU0465").unwrap();
        let span = d.span.as_ref().unwrap();
        assert_eq!(&src[span.start..span.end], part);
        assert!(d.help.is_some());
    }
}
#[test]
fn review_two_character_saved_sessions() {
    let dir = project(
        "saved account = { visits: 0 }; saved perCharacter character = { visits: 0 }; on PLAYER_LOGIN() { account.visits += 1; character.visits += 1; print(account.visits); print(character.visits); }",
    );
    success(&dir);
    let fs = files(&dir);
    let mut sessions = selu_sim::saved::SavedSessions::default();
    for (who, expected) in [
        ("Alice", vec!["1", "1"]),
        ("Bob", vec!["2", "1"]),
        ("Alice", vec!["3", "2"]),
    ] {
        let sim = Sim::login_character("Saved", &fs, &sessions, who).unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert_eq!(sim.prints(), expected, "{who}");
        sim.logout_character(&mut sessions, who, &["Saved_account"], &["Saved_character"])
            .unwrap();
    }
}
#[test]
fn review_newer_warnings_adopted_character_and_moving_modules() {
    let dir = project(
        "saved global perCharacter LegacyCharacter = { x: 7 }; saved s = { x: 1 }; log.info`ready`;",
    );
    success(&dir);
    let toc = std::fs::read_to_string(dir.join("dist/Saved/Saved.toc")).unwrap();
    assert!(toc.contains("## SavedVariablesPerCharacter: LegacyCharacter\n"));
    let mut fs = files(&dir);
    fs.push(("verify.lua".into(),"local _, ns = ...; local f=CreateFrame('Frame'); f:RegisterEvent('PLAYER_LOGIN'); f:SetScript('OnEvent',function() assert(#ns[false].buffer == 2); assert(ns[false].buffer[2].level == 'warn'); assert(ns[false].buffer[2].message == 'Saved_s: saved data is newer than this addon'); assert(ns.__selu_saved == nil); print(LegacyCharacter.x) end)".into()));
    let sim = Sim::new().unwrap();
    sim.eval("Saved_s = { x = 99, __selu_version = 3 }; LegacyCharacter = { x = 99 }")
        .unwrap();
    sim.load_addon("Saved", &fs).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["99"]);
    let dir = project("import { s } from \"./first\"; on PLAYER_LOGIN() { print(s.x); }");
    std::fs::write(dir.join("src/first.selu"), "export saved s = { x: 7 };").unwrap();
    success(&dir);
    let sim = Sim::new().unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.eval("Saved_s.x = 99").unwrap();
    let before = sim.logout_saved(&["Saved_s"]).unwrap();
    std::fs::write(
        dir.join("src/main.selu"),
        "import { s } from \"./second\"; on PLAYER_LOGIN() { print(s.x); }",
    )
    .unwrap();
    std::fs::write(dir.join("src/second.selu"), "export saved s = { x: 7 };").unwrap();
    success(&dir);
    let next = Sim::login_saved("Saved", &files(&dir), &before).unwrap();
    next.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(next.prints(), vec!["99"]);
}
#[test]
fn review_reserved_module_paths() {
    for path in ["__selu_saved", "folder/__selu_private/data"] {
        let src = format!("import {{ value }} from \"./{path}\"; saved s = {{ x: 1 }};");
        let dir = project(&src);
        let file = dir.join("src").join(format!("{path}.selu"));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, "export const value = 1;").unwrap();
        let out = build(&dir);
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("SELU0318") && stderr.contains("rename"),
            "{stderr}"
        );
        let p = selu_syntax::parse("t.selu", &src).unwrap();
        let ds = selu_check::check(&p, &common::manifest());
        let span = ds
            .iter()
            .find(|d| d.code == "SELU0318")
            .unwrap()
            .span
            .as_ref()
            .unwrap();
        assert_eq!(&src[span.start..span.end], format!("\"./{path}\""));
    }
}
#[test]
fn classes_saved_instance_fields_methods_and_preludes() {
    let source = "saved settings = { value: 7 }; class Reader { value = settings.value; read(): number { return settings.value; } } on PLAYER_LOGIN() { const reader = new Reader(); print(reader.value); print(reader.read()); }";
    let lua = common::compile(source);
    assert!(lua.contains("local __selu_setmetatable = setmetatable\nlocal __selu_saved__G = _G"));
    let sim = Sim::new().unwrap();
    sim.eval("Addon_settings = { value = 42 }").unwrap();
    sim.load_addon("Standalone", &[("main.lua".into(), lua)])
        .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["42", "42"]);
    let dir = project(
        "import { Reader } from \"./model\"; on PLAYER_LOGIN() { const reader = new Reader(); print(reader.value); print(reader.read()); }",
    );
    std::fs::write(
        dir.join("src/state.selu"),
        "export saved settings = { value: 7 };",
    )
    .unwrap();
    std::fs::write(dir.join("src/model.selu"),"import { settings } from \"./state\"; export class Reader { value = settings.value; read(): number { return settings.value; } }").unwrap();
    success(&dir);
    let sim = Sim::new().unwrap();
    sim.eval("Saved_settings = { value = 42 }").unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["42", "42"]);
}
#[test]
fn classes_saved_static_field_is_an_early_read() {
    let source = "saved settings = { value: 7 }; class Reader { static value = settings.value; }";
    let p = selu_syntax::parse("t.selu", source).unwrap();
    let ds = selu_check::check(&p, &common::manifest());
    let d = ds.iter().find(|d| d.code == "SELU0460").unwrap();
    let span = d.span.as_ref().unwrap();
    assert_eq!(&source[span.start..span.end], "settings");
}
#[test]
fn classes_saved_shapes_reject_class_instances_recursively() {
    for defaults in [
        "{ point: new Point() }",
        "{ points: new Map<string, Point>() }",
        "{ points: new Map<string, { point: Point }>() }",
    ] {
        let source = format!("class Point {{ x = 1; }} saved s = {defaults};");
        let p = selu_syntax::parse("t.selu", &source).unwrap();
        let ds = selu_check::check(&p, &common::manifest());
        assert!(
            ds.iter()
                .any(|d| d.code == "SELU0469" && d.message.contains("class instances")),
            "{source}: {ds:?}"
        );
    }
}
#[test]
fn classes_saved_writes_reject_instances_and_accept_plain_copies() {
    for operation in [
        "s.point = p;",
        "s.nested.point = p;",
        "s.items.push(p);",
        "s.items[0] = p;",
        "s.points.set(\"key\", p);",
    ] {
        let src = format!(
            "class Point {{ x = 1; }} saved s = {{ point: {{ x: 0 }}, nested: {{ point: {{ x: 0 }} }}, items: [{{ x: 0 }}], points: new Map<string, {{ x: number }}>() }}; on PLAYER_LOGIN() {{ const p = new Point(); {operation} }}"
        );
        let p = selu_syntax::parse("t.selu", &src).unwrap();
        let ds = selu_check::check(&p, &common::manifest());
        let d = ds
            .iter()
            .find(|d| d.code == "SELU0469" && d.message.contains("class instances"))
            .unwrap_or_else(|| panic!("{operation}: {ds:?}"));
        assert!(d.help.as_ref().unwrap().contains("{ x: p.x, y: p.y }"));
    }
    let dir = project(
        "import { s } from \"./state\"; import { Point } from \"./model\"; on PLAYER_LOGIN() { const p = new Point(); s.points.set(\"key\", p); }",
    );
    std::fs::write(
        dir.join("src/state.selu"),
        "export saved s = { points: new Map<string, { x: number }>() };",
    )
    .unwrap();
    std::fs::write(dir.join("src/model.selu"), "export class Point { x = 1; }").unwrap();
    let output = build(&dir);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("class instances cannot be saved"));
    let src = "class Point { x = 1; } saved s = { point: { x: 0 }, items: [{ x: 0 }], points: new Map<string, { x: number }>() }; on PLAYER_LOGIN() { const p = new Point(); s.point = { x: p.x }; s.items.push({ x: p.x }); s.points.set(\"key\", { x: p.x }); print(s.point.x); print(s.items.length); print(s.points.count()); }";
    let dir = project(src);
    success(&dir);
    let sim = Sim::new().unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["1", "2", "1"]);
}

use common::BoundedCommand;
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
    let sim = common::SavedVm::new();
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
        "[addon]\nname = \"Saved\"\ntargets = [\"wowf@1.0.0.1000\"]\n",
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
        .bounded_output()
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
        "import { settings } from \"./state\"; on PLAYER_LOGIN() {settings.whenReady((settings) => { print(settings.enabled); print(settings.nested.count); });}",
    );
    std::fs::write(dir.join("src/state.selu"),"export saved settings = { enabled: true, nested: { count: 3 } }; on PLAYER_LOGIN() {settings.whenReady((settings) => { settings.enabled = false; settings.nested.count = 7; });}").unwrap();
    success(&dir);
    let sim = common::SavedVm::new();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["false", "7"]);
    let key = selu_lower::saved::global_name("Saved", "state", "settings");
    let saved = sim.logout_saved(&[&key]).unwrap();
    std::fs::write(dir.join("src/state.selu"),"export saved settings version 3 = { enabled: true, nested: { count: 3 }, added: 9 }; migrate settings 1 -> 2 (old: {enabled:boolean; nested:{count:number}; added?:number}) { old.nested.count = 12; } migrate settings 2 -> 3 (old: {enabled:boolean; nested:{count:number}; added?:number}) { old.nested.count += 1; }").unwrap();
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
            "saved s = { x: 1 }; function f() { s.whenReady((data) => { print(data.x); }); } { const s = 3; print(s); }"
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
        "saved s version 3 = { x: 1 }; migrate s 1 -> 2 (old: {x:number}) { old.x = 8; } migrate s 2 -> 3 (old: {x:number}) { old.x = 2; throw new Error(\"migration fault\"); } on PLAYER_LOGIN() {s.whenReady((s) => { print(s.x); });}",
    );
    success(&dir);
    let key = selu_lower::saved::global_name("Saved", "main", "s");
    let sim = common::SavedVm::new();
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
    let warning_prefix = format!("[saved] {key}: migration 2 -> 3: failed: ");
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
    let newer = common::SavedVm::new();
    newer
        .eval(&format!("{key} = {{ x = false, __selu_version = 9 }}"))
        .unwrap();
    let before = newer.logout_saved(&[&key]).unwrap();
    newer.load_addon("Saved", &files(&dir)).unwrap();
    assert_eq!(newer.logout_saved(&[&key]).unwrap(), before);
}
#[test]
fn runtime_globals_are_captured_and_forward_reads_capture_the_saved_local() {
    // The environment name is now reserved; other runtime names can still
    // shadow normally, while the saved runtime retains its captured globals.
    let forbidden = project("const _G = 1; saved s = { x: 7 };");
    let output = build(&forbidden);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SELU0648"));
    let dir = project(
        "const type = 1; const pairs = 1; const ipairs = 1; const rawget = 1; const pcall = 1; const error = 1; const string = 1; const CreateFrame = 1; function read(): number { const data = s.data; if(data != null) { return data.x; } return 0; } saved s = { x: 7 }; on PLAYER_LOGIN() { print(read()); }",
    );
    success(&dir);
    let sim = common::SavedVm::new();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["7"]);
}
#[test]
fn per_character_defaults_arrays_maps_and_addon_names_are_isolated() {
    let dir = project(
        "saved perCharacter s = { enabled: true, inner: { x: 3 }, list: [1, 2], map: new Map<string, number>() }; on PLAYER_LOGIN() {s.whenReady((s) => { print(s.enabled); print(s.inner.x); print(s.list[0]); s.list.push(4); s.map.set(\"key\", 5); });}",
    );
    success(&dir);
    let key = selu_lower::saved::global_name("Saved", "main", "s");
    let sim = common::SavedVm::new();
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
            ("", "{ x = 9, __selu_version = 3 }", "", "newer"),
            (
                " version 2",
                "{ x = 11 }",
                "migrate NAME 1 -> 2 (old: {x:number}) { old.x = 2; throw new Error(\"migration fault\"); }",
                "failed",
            ),
        ] {
            let src = format!(
                "saved {saved}{version} = {{ x: 7 }}; {} on PLAYER_LOGIN() {{print({saved}.status); {saved}.whenReady(({saved}) => {{ print({saved}.x); }});}}",
                migration.replace("NAME", saved)
            );
            let dir = project(&src);
            success(&dir);
            let sim = common::SavedVm::new();
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
        + "on PLAYER_LOGIN() { s44.whenReady((s44) => { print(s44.x); }); }";
    let dir = project(&source);
    success(&dir);
    let fs = files(&dir);
    assert_eq!(fs[0].1.matches(" = ...").count(), 1);
    let sim = common::SavedVm::new();
    sim.load_addon("Saved", &fs).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["44"]);
}
#[test]
fn review_adoption_and_readable_names() {
    let dir = project(
        "saved global MyAddonDB = { x: 1 }; on PLAYER_LOGIN() {MyAddonDB.whenReady((MyAddonDB) => { print(MyAddonDB.x); });}",
    );
    success(&dir);
    let sim = common::SavedVm::new();
    sim.eval("MyAddonDB = { x = 99 }").unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["99"]);
    let snapshot = sim.logout_saved(&["MyAddonDB"]).unwrap();
    std::fs::write(dir.join("src/main.selu"),"saved global MyAddonDB version 2 = { x: 1 }; migrate MyAddonDB 1 -> 2 (old: {x:number}) { old.x += 1; } on PLAYER_LOGIN() {MyAddonDB.whenReady((MyAddonDB) => { print(MyAddonDB.x); });}").unwrap();
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
    let sim = common::SavedVm::new();
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
        "saved s version 2 = { x: 1 }; migrate s 1 -> 2 (old: {x:number}) { old.x = 2; throw new Error(\"migration fault\"); } saved healthy = { x: 7 }; log.info`ready`; on PLAYER_LOGIN() {healthy.whenReady((healthy) => { print(healthy.x); });}",
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
        let sim = common::SavedVm::new();
        sim.eval(&format!("{key} = {initial}")).unwrap();
        sim.load_addon("Saved", &fs).unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert_eq!(sim.prints(), vec!["7"], "{initial}");
        sim.eval(&format!("local before = {initial}; local after = {key}; if type(before) == 'table' then assert(after.x == 99); if before.__selu_version ~= before.__selu_version then assert(after.__selu_version ~= after.__selu_version) else assert(after.__selu_version == before.__selu_version) end else assert(after == before) end")).unwrap();
    }
    let sim = common::SavedVm::new();
    sim.eval(&format!("{key} = {{ x = 99 }}")).unwrap();
    let mut observed = fs.clone();
    observed.push(("inspect.lua".into(),"local _, ns = ...; local f=CreateFrame('Frame'); f:RegisterEvent('PLAYER_LOGIN'); f:SetScript('OnEvent', function() for _,v in ipairs(ns.__selu_wow.buffer) do if v.level == 'warn' then print(v.level) end end end)".into()));
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
        ("migrate missing 1 -> 2 (old: {x:number}) {}", "missing"),
        (
            "saved s version 3 = { x: 1 }; migrate s 1 -> 3 (old: {x:number}) {}",
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
        "saved account = { visits: 0 }; saved perCharacter character = { visits: 0 }; on PLAYER_LOGIN() {account.whenReady((account) => {character.whenReady((character) => { account.visits += 1; character.visits += 1; print(account.visits); print(character.visits); });});}",
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
    fs.push(("verify.lua".into(),"local _, ns = ...; local f=CreateFrame('Frame'); f:RegisterEvent('PLAYER_LOGIN'); f:SetScript('OnEvent',function() assert(#ns.__selu_wow.buffer == 2); assert(ns.__selu_wow.buffer[2].level == 'warn'); assert(ns.__selu_wow.buffer[2].message == '[saved] Saved_s: load: newer: saved version 3 is newer than supported version 1'); assert(ns.__selu_saved == nil); print(LegacyCharacter.x) end)".into()));
    let sim = common::SavedVm::new();
    sim.eval("Saved_s = { x = 99, __selu_version = 3 }; LegacyCharacter = { x = 99 }")
        .unwrap();
    sim.load_addon("Saved", &fs).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["99"]);
    let dir = project(
        "import { s } from \"./first\"; on PLAYER_LOGIN() {s.whenReady((s) => { print(s.x); });}",
    );
    std::fs::write(dir.join("src/first.selu"), "export saved s = { x: 7 };").unwrap();
    success(&dir);
    let sim = common::SavedVm::new();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.eval("Saved_s.x = 99").unwrap();
    let before = sim.logout_saved(&["Saved_s"]).unwrap();
    std::fs::write(
        dir.join("src/main.selu"),
        "import { s } from \"./second\"; on PLAYER_LOGIN() {s.whenReady((s) => { print(s.x); });}",
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
    let source = "saved settings = { value: 7 }; function readSettings(): number { const data = settings.data; if(data!=null) {return data.value;} return 0;} export class Reader { value = readSettings(); read(): number { return readSettings(); } } on PLAYER_LOGIN() { const reader = new Reader(); print(reader.value); print(reader.read()); }";
    let lua = common::compile(source);
    assert!(lua.contains("local __selu_setmetatable = setmetatable\nlocal __selu_saved__G = _G"));
    let sim = common::SavedVm::new();
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
    std::fs::write(dir.join("src/model.selu"),"import { settings } from \"./state\"; function readSettings(): number { const data = settings.data; if(data!=null) {return data.value;} return 0;} export class Reader { value = readSettings(); read(): number { return readSettings(); } }").unwrap();
    success(&dir);
    let sim = common::SavedVm::new();
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
            "class Point {{ x = 1; }} saved s = {{ point: {{ x: 0 }}, nested: {{ point: {{ x: 0 }} }}, items: [{{ x: 0 }}], points: new Map<string, {{ x: number }}>() }}; on PLAYER_LOGIN() {{ s.whenReady((s) => {{ const p = new Point(); {operation} }}); }}"
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
        "import { s } from \"./state\"; import { Point } from \"./model\"; on PLAYER_LOGIN() {s.whenReady((s) => { const p = new Point(); s.points.set(\"key\", p); });}",
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
    let src = "class Point { x = 1; } saved s = { point: { x: 0 }, items: [{ x: 0 }], points: new Map<string, { x: number }>() }; on PLAYER_LOGIN() {s.whenReady((s) => { const p = new Point(); s.point = { x: p.x }; s.items.push({ x: p.x }); s.points.set(\"key\", { x: p.x }); print(s.point.x); print(s.items.length); print(s.points.count()); });}";
    let dir = project(src);
    success(&dir);
    let sim = common::SavedVm::new();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), vec!["1", "2", "1"]);
}

fn assert_adoption_status(sim: &Sim, status: &str) {
    let messages = sim.prints();
    if status == "ready" {
        assert_eq!(messages, vec![status]);
    } else {
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(messages[0].starts_with("[saved] "), "{messages:?}");
        assert!(
            messages[0].contains(&format!(": {status}:")),
            "{messages:?}"
        );
        assert_eq!(messages[1], status);
    }
}

#[test]
fn adoption_mapped_version_refuses_future_and_preserves_identity() {
    let dir = project(
        r#"
        saved global RennerToolkitDB version 2 using dbVersion legacy RennersAddonsDB = { enabled: true, count: 0 };
        migrate RennerToolkitDB 1 -> 2 (old: { enabled: boolean; count: number }) { old.count += 1; }
        on PLAYER_LOGIN() {
            const data = RennerToolkitDB.data;
            if (data != null) { data.count += 10; }
            print(RennerToolkitDB.status);
        }
    "#,
    );
    success(&dir);
    let files = files(&dir);
    for (input, assertions, status) in [
        (
            "RennerToolkitDB = {dbVersion=2, enabled=false, count=7}; root=RennerToolkitDB",
            "assert(RennerToolkitDB == root and root.count == 17 and root.enabled == false and root.__selu_version == nil)",
            "ready",
        ),
        (
            "RennerToolkitDB = {dbVersion=3, count=7}; root=RennerToolkitDB",
            "assert(RennerToolkitDB == root and root.count == 7 and root.enabled == nil and root.__selu_version == nil)",
            "newer",
        ),
        (
            "RennersAddonsDB = {dbVersion=1, enabled=false, count=7}; root=RennersAddonsDB",
            "assert(RennerToolkitDB == root and root.count == 18 and RennersAddonsDB == nil and root.dbVersion == 2)",
            "ready",
        ),
        (
            "RennerToolkitDB = {dbVersion=2, enabled='bad', count=7}; root=RennerToolkitDB",
            "assert(RennerToolkitDB == root and root.count == 7 and root.enabled == 'bad')",
            "corrupt",
        ),
        (
            "RennerToolkitDB = {dbVersion=2, enabled=false, count=7}; RennersAddonsDB={history=42}; legacy=RennersAddonsDB; root=RennerToolkitDB",
            "assert(RennerToolkitDB == root and RennersAddonsDB == legacy and legacy.history == 42)",
            "collision",
        ),
    ] {
        let sim = common::SavedVm::new();
        sim.eval(
            "__selu_saved_names.RennerToolkitDB = true; __selu_saved_names.RennersAddonsDB = true",
        )
        .unwrap();
        sim.eval(input).unwrap();
        sim.load_addon("Saved", &files).unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        sim.eval(assertions).unwrap();
        assert_adoption_status(&sim, status);
    }
}

#[test]
fn adoption_ten_versions_context_retry_and_roundtrip() {
    let mut source = String::from(
        "saved global RennerToolkitDB version 10 using dbVersion legacy RennersAddonsDB = { count: 0, enabled: true }; ",
    );
    for v in 1..10 {
        source.push_str(&format!("migrate RennerToolkitDB {v} -> {} (old: {{ count: number; enabled: boolean }}, original: {{ count: number; enabled: boolean }}) {{ old.count += 1; }}", v+1));
    }
    source.push_str("on PLAYER_LOGIN() { const db = RennerToolkitDB.data; if (db != null) { db.count += 100; } print(RennerToolkitDB.status); }");
    let dir = project(&source);
    success(&dir);
    let built = files(&dir);
    for version in 1..=11 {
        let sim = common::SavedVm::new();
        sim.eval(
            "__selu_saved_names.RennerToolkitDB = true; __selu_saved_names.RennersAddonsDB = true",
        )
        .unwrap();
        sim.eval(&format!("RennerToolkitDB = {{dbVersion={version},count=20,enabled=false,extension={{[42]='x',['42']='y'}}}}; root=RennerToolkitDB; nested=root.extension; root.other=nested")).unwrap();
        sim.load_addon("Saved", &built).unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        let count = if version > 10 { 20 } else { 130 - version };
        sim.eval(&format!("assert(RennerToolkitDB == root and root.count == {count}); assert(root.enabled == false and root.__selu_version == nil); assert(root.extension == nested and root.other == nested and nested[42] == 'x' and nested['42'] == 'y')")).unwrap();
        let snapshot = sim
            .logout_saved(&["RennerToolkitDB", "RennersAddonsDB"])
            .unwrap();
        // An ordinary Lua consumer sees the mapped version and unprojected extensions.
        let rollback = Sim::login_saved("LuaMaster", &[], &snapshot).unwrap();
        rollback.eval(&format!("assert(RennerToolkitDB.dbVersion == {} and RennerToolkitDB.count == {count}); assert(RennerToolkitDB.extension[42] == 'x')", version.min(10).max(if version == 11 {11} else {10}))).unwrap();
    }
    // Deliberately mutate a migration: the simulator must observe a changed result.
    let mutation = built
        .iter()
        .map(|(n, s)| (n.clone(), s.replace("__selu_t2 + 1", "__selu_t2 + 2")))
        .collect::<Vec<_>>();
    let sim = common::SavedVm::new();
    sim.eval("__selu_saved_names.RennersAddonsDB=true; RennerToolkitDB={dbVersion=1,count=20,enabled=false}").unwrap();
    sim.load_addon("Saved", &mutation).unwrap();
    assert!(sim.eval("assert(RennerToolkitDB.count == 29)").is_err());
}

#[test]
fn adoption_nested_validation_and_failed_step_recovery() {
    let source = r#"
        saved global DB version 3 using dbVersion legacy OldDB = { count: 0, enabled: true, history: new Map<string, number>() };
        migrate DB 1 -> 2 (old: { count: number; enabled: boolean; history: Map<string, number> }) { old.count += 1; }
        migrate DB 2 -> 3 (old: { count: number; enabled: boolean; history: Map<string, number> }, original: { count: number }) { old.count += original.count; }
        on PLAYER_LOGIN() { print(DB.status); }
    "#;
    let dir = project(source);
    success(&dir);
    let built = files(&dir);
    for input in [
        "DB={dbVersion='3'}",
        "DB=false",
        "DB=false; OldDB={dbVersion=1,count=4,enabled=false,history={}}",
        "DB={dbVersion=1.5}",
        "DB={dbVersion=0}",
        "DB={dbVersion=3,count=0,enabled=false,history={x='bad'}}",
    ] {
        let sim = common::SavedVm::new();
        sim.eval("__selu_saved_names.DB=true; __selu_saved_names.OldDB=true")
            .unwrap();
        sim.eval(input).unwrap();
        let before = sim.logout_saved(&["DB", "OldDB"]).unwrap();
        sim.load_addon("Saved", &built).unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert_adoption_status(&sim, "corrupt");
        assert_eq!(sim.logout_saved(&["DB", "OldDB"]).unwrap(), before);
    }
    let sim = common::SavedVm::new();
    sim.eval("__selu_saved_names.DB=true; OldDB={dbVersion=1,count=4,enabled=false,history={}}; root=OldDB; nested=root.history").unwrap();
    let failing = built
        .iter()
        .map(|(n, s)| {
            (
                n.clone(),
                s.replace("= original.count", "= error('synthetic failure')"),
            )
        })
        .collect::<Vec<_>>();
    sim.load_addon("Saved", &failing).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_adoption_status(&sim, "failed");
    sim.eval("assert(DB == nil and OldDB == root and root.count == 5 and root.dbVersion == 2 and root.history == nested)").unwrap();
    let snapshot = sim.logout_saved(&["DB", "OldDB"]).unwrap();
    let retry = Sim::login_saved("Saved", &built, &snapshot).unwrap();
    retry
        .eval(
            "assert(DB.count == 9 and DB.dbVersion == 3 and OldDB == nil and DB.enabled == false and DB.__selu_saved_adoption_context == nil)",
        )
        .unwrap();
}

#[test]
fn adoption_defaults_callbacks_arrays_and_aliases() {
    let dir = project(
        r#"
        saved global DB using dbVersion legacy OldDB = { count: 0, enabled: true, rows: [1] };
        function attach() { DB.whenReady((data) => { data.count += 1; }); }
        attach();
        on PLAYER_LOGIN() { DB.whenReady((data) => { data.count += 10; }); print(DB.status); }
    "#,
    );
    success(&dir);
    let built = files(&dir);
    for (input, status, count) in [
        ("", "ready", 11),
        ("DB={}", "ready", 11),
        ("DB={count=0,enabled=false,rows={}}", "ready", 11),
        ("OldDB={}", "ready", 11),
        (
            "DB={dbVersion=1,count=0,enabled=false,rows={}}; OldDB=DB",
            "ready",
            11,
        ),
        (
            "DB={dbVersion=1,count=0,enabled=false,rows={[1]=1,[3]=3}}",
            "corrupt",
            0,
        ),
        (
            "DB={dbVersion=1,count=0,enabled=false,rows={false}}",
            "corrupt",
            0,
        ),
        ("DB={dbVersion=2,count=0}", "newer", 0),
    ] {
        let sim = common::SavedVm::new();
        sim.eval("__selu_saved_names.DB=true; __selu_saved_names.OldDB=true")
            .unwrap();
        sim.eval(input).unwrap();
        sim.load_addon("Saved", &built).unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert_adoption_status(&sim, status);
        if status == "ready" {
            sim.eval(&format!(
                "assert(DB.count=={count} and DB.dbVersion==1 and OldDB==nil)"
            ))
            .unwrap();
        } else {
            sim.eval("assert(DB.count == 0)").unwrap();
        }
    }
    // Removing the readiness check must activate the synthetic writer and fail the oracle.
    let mutation = built
        .iter()
        .map(|(n, s)| {
            (
                n.clone(),
                s.replace("if __selu_result.data then", "if true then"),
            )
        })
        .collect::<Vec<_>>();
    let sim = common::SavedVm::new();
    sim.eval("__selu_saved_names.OldDB=true; DB={dbVersion=2,count=0}")
        .unwrap();
    assert!(sim.load_addon("Saved", &mutation).is_err());
}

#[test]
fn adoption_rejects_untyped_readers_and_alias_collisions() {
    for source in [
        "saved global DB using dbVersion = {dbVersion: \"wrong\"};",
        "saved global DB using __selu_saved_adoption_context = {count:0};",
        "saved global DB using dbVersion legacy _G = {count:0};",
        "saved global _G using dbVersion = {count:0};",
        "saved global DB version 2 using dbVersion = {count:0}; migrate DB 1 -> 2 (old: {x:number}) { old.count=1; }",
        "saved global DB using dbVersion legacy Other = {count:0}; saved global Other = {x:1};",
        "saved global DB version 2 using dbVersion = {count:0}; migrate DB 1 -> 2 (old: {count:number}) { old.typo=1; }",
    ] {
        let dir = project(source);
        assert!(!build(&dir).status.success());
    }
}

#[test]
fn adoption_default_pollution_repair_uses_original_snapshot() {
    let dir = project(
        r#"
        type Aura = { threshold: number; enabled: boolean };
        type Historical = { auraFlash?: Aura; debuffFlash?: Aura };
        saved global DB version 4 using dbVersion = { auraFlash: {threshold:5, enabled:true} };
        function merge(db: Historical, repair: boolean, context: Historical) {
            const old = db.debuffFlash ?? context.debuffFlash;
            if (old != null) {
                const dest = db.auraFlash;
                if (dest == null) { db.auraFlash = old; }
                else {
                    if (repair && dest.threshold == 5 && old.threshold != 5) { dest.threshold = old.threshold; }
                    if (repair && dest.enabled == true && old.enabled == false) { dest.enabled = false; }
                }
                db.debuffFlash = null;
            }
        }
        migrate DB 1 -> 2 (old: Historical, context: Historical) { merge(old, false, context); }
        migrate DB 2 -> 3 (old: Historical, context: Historical) { merge(old, false, context); }
        migrate DB 3 -> 4 (old: Historical, context: Historical) { merge(old, true, context); }
    "#,
    );
    success(&dir);
    let built = files(&dir);
    for version in 1..4 {
        let sim = common::SavedVm::new();
        sim.eval(&format!("DB={{dbVersion={version},debuffFlash={{threshold=9,enabled=false}},auraFlash={{threshold=5,enabled=true}}, extension={{[0]='zero',[9]='sparse'}}}}; root=DB; aura=DB.auraFlash; extension=DB.extension")).unwrap();
        sim.load_addon("Saved", &built).unwrap();
        sim.eval("assert(DB==root and DB.dbVersion==4 and DB.auraFlash==aura and DB.auraFlash.threshold==9 and DB.auraFlash.enabled==false and DB.debuffFlash==nil and DB.extension==extension and extension[0]=='zero' and extension[9]=='sparse')").unwrap();
    }
}

#[test]
fn adoption_failure_at_each_step_keeps_last_success() {
    let mut source =
        String::from("saved global DB version 10 using dbVersion legacy OldDB = {count:0};");
    for v in 1..10 {
        source.push_str(&format!(
            "migrate DB {v} -> {} (old: {{count:number}}) {{ old.count += 1; }}",
            v + 1
        ));
    }
    let dir = project(&source);
    success(&dir);
    let built = files(&dir);
    for step in 1..10 {
        let marker = format!("entries[\"DB\"][{step}] = function(old)");
        let mutation = built
            .iter()
            .map(|(n, s)| {
                (
                    n.clone(),
                    s.replace(&marker, &format!("{marker} error('step failure');")),
                )
            })
            .collect::<Vec<_>>();
        let sim = common::SavedVm::new();
        sim.eval("__selu_saved_names.DB=true; OldDB={dbVersion=1,count=0}; root=OldDB")
            .unwrap();
        sim.load_addon("Saved", &mutation).unwrap();
        sim.eval(&format!(
            "assert(DB==nil and OldDB==root and root.dbVersion=={step} and root.count=={})",
            step - 1
        ))
        .unwrap();
    }
}

#[test]
fn adoption_mode_is_explicit_and_history_is_closed() {
    let dir = project(
        "saved settings version 2 = {status: \"text\", data:{count:0}}; migrate settings 1 -> 2 (old: {status:string; data:{count:number}}) { old.data.count += 1; }",
    );
    success(&dir);
    let dir = project(
        "saved global DB version 2 using dbVersion = {count:0}; migrate DB 1 -> 2 (old: {count:number}) { old.count += 1; }",
    );
    success(&dir);
    std::fs::write(dir.join("src/main.selu"),"saved global DB version 2 using dbVersion = {count:0}; migrate DB 1 -> 2 (old: {count:number; extra?:number}) { old.count += 1; }").unwrap();
    let output = build(&dir);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SELU0902"));
}

#[test]
fn adoption_persisted_context_v4_retry_and_lua_master_rollback() {
    let dir = project(
        r#"
        type Aura = {threshold:number; enabled:boolean};
        type History = {debuffFlash?:Aura; auraFlash?:Aura};
        saved global RennerToolkitDB version 10 using dbVersion legacy RennersAddonsDB = {auraFlash:{threshold:3,enabled:true}};
        migrate RennerToolkitDB 1 -> 2 (old:History) {}
        migrate RennerToolkitDB 2 -> 3 (old:History) { old.debuffFlash=null; }
        migrate RennerToolkitDB 3 -> 4 (old:History, original:History) {
            const legacy=original.debuffFlash;
            if (legacy != null) {
                const dest=old.auraFlash;
                if (dest == null) { old.auraFlash=legacy; }
                else {
                    if (dest.threshold==3 && legacy.threshold!=3) { dest.threshold=legacy.threshold; }
                    if (dest.enabled==true && legacy.enabled==false) { dest.enabled=false; }
                }
            }
        }
        migrate RennerToolkitDB 4 -> 5 (old:History) {}
        migrate RennerToolkitDB 5 -> 6 (old:History) {}
        migrate RennerToolkitDB 6 -> 7 (old:History) {}
        migrate RennerToolkitDB 7 -> 8 (old:History) {}
        migrate RennerToolkitDB 8 -> 9 (old:History) {}
        migrate RennerToolkitDB 9 -> 10 (old:History) {}
    "#,
    );
    success(&dir);
    let built = files(&dir);
    let mutation = built
        .iter()
        .map(|(n, s)| {
            (
                n.clone(),
                s.replace(
                    "function(old, original)",
                    "function(old, original) error('v4 failure');",
                ),
            )
        })
        .collect::<Vec<_>>();
    let seed = common::SavedVm::new();
    seed.eval("__selu_saved_names.RennerToolkitDB=true; __selu_saved_names.RennersAddonsDB=true")
        .unwrap();
    seed.eval(include_str!("fixtures/toolkit_saved_oracle.lua"))
        .unwrap();
    seed.eval("__selu_test_lua_master_load(); RennerToolkitDB.dbVersion=2; RennerToolkitDB.debuffFlash={threshold=9,enabled=false}; RennerToolkitDB.extension={ [0]='zero', ['0']='string' }").unwrap();
    let input = seed
        .logout_saved(&["RennerToolkitDB", "RennersAddonsDB"])
        .unwrap();
    let mut mutation = mutation;
    mutation.insert(0, ("capture.lua".into(), "local addon=...; local f=CreateFrame('Frame'); f:RegisterEvent('ADDON_LOADED'); f:SetScript('OnEvent',function(_,_,name) if name==addon then root=RennerToolkitDB end end)".into()));
    let sim = Sim::login_saved("Saved", &mutation, &input).unwrap();
    sim.eval("assert(RennerToolkitDB==root and root.dbVersion==3 and root.debuffFlash.threshold==9 and root.__selu_saved_adoption_context.original.debuffFlash.threshold==9)").unwrap();
    let snapshot = sim
        .logout_saved(&["RennerToolkitDB", "RennersAddonsDB"])
        .unwrap();
    let retry = Sim::login_saved("Saved", &built, &snapshot).unwrap();
    retry.eval("assert(RennerToolkitDB.dbVersion==10 and RennerToolkitDB.auraFlash.threshold==9 and RennerToolkitDB.auraFlash.enabled==false and RennerToolkitDB.__selu_saved_adoption_context==nil)").unwrap();
    let baseline = Sim::login_saved("LuaMaster", &[], &input).unwrap();
    baseline
        .eval(include_str!("fixtures/toolkit_saved_oracle.lua"))
        .unwrap();
    baseline.eval("__selu_test_lua_master_load()").unwrap();
    let baseline_snapshot = baseline
        .logout_saved(&["RennerToolkitDB", "RennersAddonsDB"])
        .unwrap();
    assert_eq!(
        retry
            .logout_saved(&["RennerToolkitDB", "RennersAddonsDB"])
            .unwrap(),
        baseline_snapshot
    );
    let rollback = Sim::login_saved("LuaMaster", &[], &snapshot).unwrap();
    rollback
        .eval(include_str!("fixtures/toolkit_saved_oracle.lua"))
        .unwrap();
    rollback.eval("assert(RennerToolkitDB.dbVersion==3 and RennerToolkitDB.debuffFlash.threshold==9); assert(__selu_test_lua_master_load()==RennerToolkitDB); assert(RennerToolkitDB.dbVersion==10); assert(RennerToolkitDB.__selu_saved_adoption_context.original.debuffFlash.threshold==9); assert(RennerToolkitDB.auraFlash.threshold==9 and RennerToolkitDB.auraFlash.enabled==false)").unwrap();
    let rolled = rollback
        .logout_saved(&["RennerToolkitDB", "RennersAddonsDB"])
        .unwrap();
    let cleanup = Sim::login_saved("Saved", &built, &rolled).unwrap();
    assert_eq!(
        cleanup
            .logout_saved(&["RennerToolkitDB", "RennersAddonsDB"])
            .unwrap(),
        baseline_snapshot
    );
}

#[test]
fn adoption_current_toolkit_v10_is_unchanged_after_logout() {
    // A synthetic save produced by the pinned real toolkit loader/defaults.
    let seed = Sim::new().unwrap();
    seed.eval("__selu_saved_names.RennerToolkitDB=true; __selu_saved_names.RennersAddonsDB=true")
        .unwrap();
    seed.eval(include_str!("fixtures/toolkit_saved_oracle.lua"))
        .unwrap();
    seed.eval("__selu_test_lua_master_load(); RennerToolkitDB.auraFlash.threshold=9; RennerToolkitDB.auraFlash.enabled=false; RennerToolkitDB.cooldownText.characters.Alice={ownedValue='1',restoreValue='0'}").unwrap();
    let before = seed
        .logout_saved(&["RennerToolkitDB", "RennersAddonsDB"])
        .unwrap();
    let mut source = String::from(
        "saved global RennerToolkitDB version 10 using dbVersion legacy RennersAddonsDB = {auraFlash:{threshold:3,enabled:true},cooldownText:{enabled:true,characters:new Map<string,{ownedValue?:string;restoreValue?:string}>()}};",
    );
    for v in 1..10 {
        source.push_str(&format!(
            "migrate RennerToolkitDB {v} -> {} (old:{{}}) {{}}",
            v + 1
        ));
    }
    source.push_str("on PLAYER_LOGIN() { print(RennerToolkitDB.status); }");
    let dir = project(&source);
    success(&dir);
    let sim = common::SavedVm::new();
    sim.eval("__selu_saved_names.RennerToolkitDB=true; __selu_saved_names.RennersAddonsDB=true")
        .unwrap();
    sim.eval(include_str!("fixtures/toolkit_saved_oracle.lua"))
        .unwrap();
    sim.eval("__selu_test_lua_master_load(); RennerToolkitDB.auraFlash.threshold=9; RennerToolkitDB.auraFlash.enabled=false; RennerToolkitDB.cooldownText.characters.Alice={ownedValue='1',restoreValue='0'}; root=RennerToolkitDB; aura=root.auraFlash; cooldown=root.cooldownText; characters=cooldown.characters").unwrap();
    sim.load_addon("Saved", &files(&dir)).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.eval("assert(RennerToolkitDB==root and root.auraFlash==aura and root.cooldownText==cooldown and cooldown.characters==characters and characters.Alice.restoreValue=='0')").unwrap();
    assert_eq!(sim.prints(), ["ready"]);
    assert_eq!(
        serde_json::to_vec(
            &sim.logout_saved(&["RennerToolkitDB", "RennersAddonsDB"])
                .unwrap()
                .0
        )
        .unwrap(),
        serde_json::to_vec(&before.0).unwrap()
    );
}

#[test]
fn adoption_repair_union_history_expands_named_table_shapes() {
    let source = "type Entry={ownedValue?:string}; type Characters=Map<string,Entry>; saved global DB version 2 using dbVersion={count:0}; migrate DB 1 -> 2 (old:{characters?:SavedRepair<Characters>}) {}";
    let dir = project(source);
    success(&dir);
    let before = std::fs::read(dir.join("selu.saved.toml")).unwrap();
    std::fs::write(
        dir.join("src/main.selu"),
        source.replace("ownedValue?:string", "ownedValue?:number"),
    )
    .unwrap();
    let out = build(&dir);
    assert!(
        !out.status.success(),
        "historical repair arm changed without rejection"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0902"));
    assert_eq!(std::fs::read(dir.join("selu.saved.toml")).unwrap(), before);
}

#[test]
fn adoption_deferred_inputs_are_general_nested_and_transactional() {
    let source = r#"
        type Historical={ input:{ inner?:{choice:number} }; result:number; flag?:boolean };
        saved global DB version 3 using dbVersion = { input:{}, result:0 };
        migrate DB 1 -> 2 (old:Historical) { old.input.inner=null; old.flag=null; }
        migrate DB 2 -> 3 (old:Historical, original:Historical) {
            const inner=original.input.inner;
            if(inner!=null) { old.result+=inner.choice; }
        }
        on PLAYER_LOGIN() { print(DB.status); }
    "#;
    let dir = project(source);
    success(&dir);
    let built = files(&dir);
    let failing = built
        .iter()
        .map(|(n, s)| {
            (
                n.clone(),
                s.replace(
                    "function(old, original)",
                    "function(old, original) old.input.inner.choice=99; error('failure');",
                ),
            )
        })
        .collect::<Vec<_>>();
    let sim = common::SavedVm::new();
    sim.eval("DB={dbVersion=1,input={inner={choice=7}},result=1,flag=false}; root=DB; input=root.input; inner=input.inner").unwrap();
    sim.load_addon("Saved", &failing).unwrap();
    sim.eval("assert(DB==root and DB.dbVersion==2 and DB.input==input and input.inner==inner and inner.choice==7 and DB.result==1 and DB.flag==false)").unwrap();
    let snapshot = sim.logout_saved(&["DB"]).unwrap();
    let retry = Sim::login_saved("Saved", &built, &snapshot).unwrap();
    retry.eval("assert(DB.dbVersion==3 and DB.input.inner==nil and DB.result==8 and DB.flag==nil and DB.__selu_saved_adoption_context==nil)").unwrap();
    let conflicting = built
        .iter()
        .map(|(n, s)| {
            (
                n.clone(),
                s.replace(
                    "function(old, original)",
                    "function(old, original) old.input.inner={choice=8};",
                ),
            )
        })
        .collect::<Vec<_>>();
    let conflict = Sim::login_saved("Saved", &conflicting, &snapshot).unwrap();
    conflict.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_adoption_status(&conflict, "failed");
    assert_eq!(conflict.logout_saved(&["DB"]).unwrap(), snapshot);
    for bad in [
        "false",
        "{format=1,original={},removals=false}",
        "{format=1,original={dbVersion=false},removals={}}",
    ] {
        let sim = common::SavedVm::new();
        sim.eval(&format!(
            "DB={{dbVersion=3,input={{}},result=1,__selu_saved_adoption_context={bad}}}"
        ))
        .unwrap();
        let before = sim.logout_saved(&["DB"]).unwrap();
        sim.load_addon("Saved", &built).unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert_adoption_status(&sim, "corrupt");
        assert_eq!(sim.logout_saved(&["DB"]).unwrap(), before);
    }
}

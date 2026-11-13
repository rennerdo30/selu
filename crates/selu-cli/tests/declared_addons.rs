//! Declared other addons (docs/lang/addons.md, level 2): `addons/<Name>.selu`
//! declares another addon's API; `import { X } from addon "Name"` reads it
//! live, every crossing value checked against its declaration.
use std::path::Path;
use std::process::Command;

fn build(label: &str, addon_files: &[(&str, &str)], files: &[(&str, &str)]) -> (std::path::PathBuf, std::process::Output) {
    let dir = std::env::temp_dir().join(format!("selu-declared-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("addons")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"Consumer\"\ntargets = [\"wowf@1.60.1.70790\"]\n",
    )
    .unwrap();
    for (name, source) in addon_files {
        std::fs::write(dir.join("addons").join(name), source).unwrap();
    }
    for (name, source) in files {
        std::fs::write(dir.join("src").join(name), source).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .output()
        .unwrap();
    (dir, output)
}

fn run(label: &str, addon_files: &[(&str, &str)], files: &[(&str, &str)], setup: &str) -> Vec<String> {
    let (dir, output) = build(label, addon_files, files);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let dist = dir.join("dist/Consumer");
    let toc = std::fs::read_to_string(dist.join("Consumer.toc")).unwrap();
    assert!(toc.contains("## OptionalDeps: FakeLib"), "{toc}");
    let files: Vec<_> = toc
        .lines()
        .filter(|s| s.ends_with(".lua"))
        .map(|n| (n.to_owned(), std::fs::read_to_string(dist.join(n)).unwrap()))
        .collect();
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval(setup).unwrap();
    sim.load_addon("Consumer", &files)
        .unwrap_or_else(|e| panic!("{e}\n{files:#?}"));
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.prints()
}

const DECLARATION: &str = r#"
addon "FakeLib" {
    type Entry = { name: string; level?: number };
    global FakeLib: {
        version: number;
        Lookup: (id: number) => Entry?;
        Names: () => string[];
        Each: (visit: (name: string, level: number) => void) => void;
        Broken: () => number;
    };
}
"#;

const MAIN: &str = r#"
import { FakeLib } from addon "FakeLib";
on PLAYER_LOGIN() {
    const lib = FakeLib;
    if (lib == null) { print("absent"); return; }
    print(lib.version ?? -1);
    const entry = lib.Lookup(7);
    if (entry != null) { print(entry.name ?? "?"); print(entry.level ?? 0); }
    const missing = lib.Lookup(8);
    print(missing == null);
    const names = lib.Names();
    print(names != null ? names.length : -1);
    lib.Each((name, level) => { print(`${name ?? "?"}:${level ?? 0}`); });
    print(lib.Broken() ?? -2);
}
"#;

const LIB: &str = "C_AddOns = C_AddOns or {}; C_AddOns.IsAddOnLoaded = function(name) return name == 'FakeLib' end
FakeLib = {
  version = 3,
  Lookup = function(id) if id == 7 then return { name = 'Seven', level = 'high' } end return 'not a table' end,
  Names = function() return { 'a', 'b', 3 } end,
  Each = function(visit) visit('ok', 5); visit(42, 6) end,
  Broken = function() error('boom') end,
}";

#[test]
fn crossing_values_are_checked_and_calls_are_protected() {
    let prints = run("checks", &[("FakeLib.selu", DECLARATION)], &[("main.selu", MAIN)], LIB);
    let warnings: Vec<_> = prints.iter().filter(|p| p.contains("it is ignored")).cloned().collect();
    let values: Vec<_> = prints.iter().filter(|p| !p.contains("it is ignored")).cloned().collect();
    // version; the entry's name and its wrong level (nil -> 0); a non-table
    // result is null; an array with a number among strings is null; the
    // callback's wrong first argument is null; a raising call is null.
    assert_eq!(values, ["3", "Seven", "0", "true", "-1", "ok:5", "?:6", "-2"], "{prints:#?}");
    // One warning per place.
    assert!(warnings.iter().any(|w| w.contains("FakeLib lib.Broken") && w.contains("raised an error")), "{warnings:#?}");
    assert!(warnings.iter().any(|w| w.contains("does not match its declaration")), "{warnings:#?}");
}

#[test]
fn an_unloaded_addon_reads_as_null() {
    let setup = "C_AddOns = C_AddOns or {}; C_AddOns.IsAddOnLoaded = function() return false end; FakeLib = { version = 1 }";
    let prints = run("absent", &[("FakeLib.selu", DECLARATION)], &[("main.selu", MAIN)], setup);
    assert_eq!(prints, ["absent"]);
}

#[test]
fn foreign_values_stay_foreign_and_declarations_stay_closed() {
    // A foreign table never becomes a Selu type of the same shape.
    let (_, out) = build(
        "assign",
        &[("FakeLib.selu", DECLARATION)],
        &[("main.selu", r#"import { FakeLib } from addon "FakeLib";
type Plain = { version: number };
const lib = FakeLib;
if (lib != null) { const plain: Plain = lib; print(plain.version); }"#)],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU03"), "{}", String::from_utf8_lossy(&out.stderr));
    // An undeclared field is an error.
    let (_, out) = build(
        "undeclared",
        &[("FakeLib.selu", DECLARATION)],
        &[("main.selu", r#"import { FakeLib } from addon "FakeLib";
const lib = FakeLib;
if (lib != null) { print(lib.secret); }"#)],
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0712"), "{}", String::from_utf8_lossy(&out.stderr));
    // Another addon's table is never written.
    let (_, out) = build(
        "write",
        &[("FakeLib.selu", DECLARATION)],
        &[("main.selu", r#"import { FakeLib } from addon "FakeLib";
const lib = FakeLib;
if (lib != null) { lib.version = 5; }"#)],
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0713"), "{}", String::from_utf8_lossy(&out.stderr));
    // A game name cannot be declared.
    let (_, out) = build(
        "game",
        &[("FakeLib.selu", r#"addon "FakeLib" { global GetMoney: () => number; }"#)],
        &[("main.selu", r#"import { GetMoney } from addon "FakeLib"; print(GetMoney != null);"#)],
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0702"), "{}", String::from_utf8_lossy(&out.stderr));
    // A missing declaration file is reported.
    let (_, out) = build("missing", &[], &[("main.selu", r#"import { X } from addon "Nowhere"; print(X != null);"#)]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0710"), "{}", String::from_utf8_lossy(&out.stderr));
}

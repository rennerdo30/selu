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

#[test]
fn maps_and_arrays_cross_as_checked_copies() {
    let declaration = r#"
addon "FakeLib" {
    type Spot = { x: number; y: number };
    global FakeLib: {
        Spawns: (id: number) => Map<number, number[][]>;
        Names: () => Map<string, string>;
        Ids: () => number[];
        Bad: () => Map<number, string>;
        Raising: () => number[];
        Points: () => Map<number, Spot>;
    };
}
"#;
    let main = r#"
import { FakeLib } from addon "FakeLib";
on PLAYER_LOGIN() {
    const lib = FakeLib;
    if (lib == null) { return; }
    const spawns = lib.Spawns(1);
    if (spawns != null) {
        const keys: number[] = [];
        for (const [area, list] of spawns) { keys.push(area * 100 + list.length); }
        keys.sort();
        print(keys.join(","));
        const first = spawns.get(12);
        if (first != null) { print(first[0][0]); }
        // A copy: writing it never reaches the other addon's table.
        spawns.set(99, []);
    }
    print(lib.Spawns(2) == null);
    const names = lib.Names();
    if (names != null) { print(names.get("a") ?? "?"); print(names.count()); }
    const ids = lib.Ids();
    if (ids != null) { ids.push(4); print(ids.length); }
    print(lib.Bad() == null);
    const raising = lib.Raising();
    print(raising != null ? raising.length : -1);
    const points = lib.Points();
    if (points != null) {
        const p = points.get(5);
        if (p != null) { print(p.x ?? -1); print(p.y ?? -1); }
    }
}
"#;
    let setup = "C_AddOns = C_AddOns or {}; C_AddOns.IsAddOnLoaded = function(name) return name == 'FakeLib' end
local spawns = { [12] = { { 1, 2 }, { 3, 4 } }, [7] = { { 5, 6 } } }
local ids = { 1, 2, 3 }
FakeLib = {
  Spawns = function(id) if id == 1 then return spawns end return { [1] = { { 'x', 2 } } } end,
  Names = function() return { a = 'A', b = 'B' } end,
  Ids = function() return ids end,
  Bad = function() return { [1] = 'one', two = 'two' } end,
  Raising = function() return setmetatable({ 1, 2 }, { __index = function() error('boom') end }) end,
  Points = function() return { [5] = { x = 0.5, y = 'north' } } end,
}
function FakeLibProbe() return spawns[99] == nil and #ids == 3 end";
    let (dir, output) = build("maps", &[("FakeLib.selu", declaration)], &[("main.selu", main)]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let dist = dir.join("dist/Consumer");
    let toc = std::fs::read_to_string(dist.join("Consumer.toc")).unwrap();
    let files: Vec<_> = toc
        .lines()
        .filter(|s| s.ends_with(".lua"))
        .map(|n| (n.to_owned(), std::fs::read_to_string(dist.join(n)).unwrap()))
        .collect();
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval(setup).unwrap();
    sim.load_addon("Consumer", &files).unwrap_or_else(|e| panic!("{e}\n{files:#?}"));
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.eval("print(FakeLibProbe())").unwrap();
    let prints = sim.prints();
    let values: Vec<_> = prints.iter().filter(|p| !p.contains("it is ignored")).cloned().collect();
    // Keys 7 (one spot) and 12 (two); the first coordinate; a wrong element
    // makes the whole map null; a string map; the copy grows, the source not;
    // a string key in a number map is null; a table whose __index raises is
    // copied from its own elements, never raising; a table value stays
    // foreign with checked fields (y is wrong).
    assert_eq!(
        values,
        ["701,1202", "1", "true", "A", "2", "4", "true", "2", "0.5", "-1", "true"],
        "{prints:#?}"
    );
    // Only number or string keys.
    let (_, out) = build(
        "mapkeys",
        &[("FakeLib.selu", r#"addon "FakeLib" { global FakeLib: { M: () => Map<boolean, number> }; }"#)],
        &[("main.selu", r#"import { FakeLib } from addon "FakeLib"; print(FakeLib != null);"#)],
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0711"), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn declared_types_are_importable_and_stay_foreign() {
    let declaration = r#"
addon "FakeLib" {
    type Entry = { name: string; level?: number };
    global FakeLib: { Lookup: (id: number) => Entry?; All: () => Entry[] };
}
"#;
    let main = r#"
import { FakeLib, Entry } from addon "FakeLib";
function label(entry: Entry): string { return `${entry.name ?? "?"}/${entry.level ?? 0}`; }
on PLAYER_LOGIN() {
    const lib = FakeLib;
    if (lib == null) { return; }
    const one = lib.Lookup(1);
    if (one != null) { print(label(one)); }
    const all = lib.All();
    if (all != null) { for (const e of all) { print(label(e)); } }
}
"#;
    let setup = "C_AddOns = C_AddOns or {}; C_AddOns.IsAddOnLoaded = function(name) return name == 'FakeLib' end
FakeLib = {
  Lookup = function() return { name = 'One', level = 3 } end,
  All = function() return { { name = 'A' }, { name = 5, level = 2 } } end,
}";
    let (dir, output) = build("types", &[("FakeLib.selu", declaration)], &[("main.selu", main)]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let dist = dir.join("dist/Consumer");
    let toc = std::fs::read_to_string(dist.join("Consumer.toc")).unwrap();
    let files: Vec<_> = toc
        .lines()
        .filter(|s| s.ends_with(".lua"))
        .map(|n| (n.to_owned(), std::fs::read_to_string(dist.join(n)).unwrap()))
        .collect();
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval(setup).unwrap();
    sim.load_addon("Consumer", &files).unwrap_or_else(|e| panic!("{e}\n{files:#?}"));
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    let values: Vec<_> = sim.prints().into_iter().filter(|p| !p.contains("it is ignored")).collect();
    // Field reads through the imported type stay checked: a number name is null.
    assert_eq!(values, ["One/3", "A/0", "?/2"]);
    // The other addon's value never becomes a Selu type of the same shape.
    let (_, out) = build(
        "typesplain",
        &[("FakeLib.selu", declaration)],
        &[("main.selu", r#"import { FakeLib } from addon "FakeLib";
type Plain = { name: string; level?: number };
const lib = FakeLib;
if (lib != null) { const e = lib.Lookup(1); if (e != null) { const p: Plain = e; print(p.name); } }"#)],
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU03"), "{}", String::from_utf8_lossy(&out.stderr));
}

// ---- Selu addons using each other (level 4) -------------------------------

fn project(dir: &Path, name: &str, toml_extra: &str, files: &[(&str, &str)]) -> std::process::Output {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        format!("[addon]\nname = \"{name}\"\ntargets = [\"wowf@1.60.1.70790\"]\n{toml_extra}"),
    )
    .unwrap();
    for (file, source) in files {
        std::fs::write(dir.join("src").join(file), source).unwrap();
    }
    Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(dir)
        .arg("--manifest-dir")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .output()
        .unwrap()
}

fn addon_files(dir: &Path, name: &str) -> Vec<(String, String)> {
    let dist = dir.join("dist").join(name);
    let toc = std::fs::read_to_string(dist.join(format!("{name}.toc"))).unwrap();
    toc.lines()
        .filter(|s| s.ends_with(".lua"))
        .map(|n| (n.to_owned(), std::fs::read_to_string(dist.join(n)).unwrap()))
        .collect()
}

const PROVIDER: &str = r#"
type Quest = { id: number; title: string };
const version = 2;
function quest(id: number): Quest? { return id == 1 ? { id: 1, title: "First" } : null; }
function each(visit: (title: string) => void) { visit("a"); visit("b"); }
export addon { version, quest as findQuest, each };
"#;

const CONSUMER: &str = r#"
import { version, findQuest, each } from addon "Provider";
on PLAYER_LOGIN() {
    print(version ?? -1);
    const find = findQuest;
    if (find != null) {
        const q = find(1);
        if (q != null) { print(q.title ?? "?"); }
        print(find(2) == null);
    }
    const visit = each;
    if (visit != null) { visit((title) => { print(title ?? "?"); }); }
}
"#;

fn two_addons(label: &str, tamper: Option<&str>) -> Vec<String> {
    let root = std::env::temp_dir().join(format!("selu-api-{label}-{}", std::process::id()));
    let provider = root.join("provider");
    let out = project(&provider, "Provider", "", &[("main.selu", PROVIDER)]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let api_path = provider.join("dist/Provider.selu-api");
    let mut api = std::fs::read_to_string(&api_path).unwrap();
    assert!(api.contains("addon \"Provider\" fingerprint"), "{api}");
    assert!(api.contains("global findQuest: (p1: number) => { id: number; title: string }?;"), "{api}");
    if let Some(fake) = tamper {
        // The consumer was built against another version of the API.
        let start = api.find("fingerprint \"").unwrap() + 13;
        api.replace_range(start..start + 16, fake);
        std::fs::write(&api_path, &api).unwrap();
    }
    let consumer = root.join("consumer");
    let out = project(
        &consumer,
        "Consumer",
        &format!("[addons]\nProvider = {:?}\n", api_path.to_string_lossy()),
        &[("main.selu", CONSUMER)],
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("C_AddOns = C_AddOns or {}; C_AddOns.IsAddOnLoaded = function(name) return name == 'Provider' or name == 'Consumer' end").unwrap();
    sim.load_addon("Provider", &addon_files(&provider, "Provider")).unwrap();
    sim.load_addon("Consumer", &addon_files(&consumer, "Consumer")).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.prints()
}

#[test]
fn a_selu_addon_uses_another_through_its_published_api() {
    assert_eq!(two_addons("match", None), ["2", "First", "true", "a", "b"]);
}

#[test]
fn a_different_api_version_is_checked_per_value() {
    // Same values, so the per-value checks pass them; nothing raises.
    assert_eq!(two_addons("stale", Some("0000000000000000")), ["2", "First", "true", "a", "b"]);
}

#[test]
fn only_plain_data_and_functions_are_published() {
    let dir = std::env::temp_dir().join(format!("selu-api-bad-{}", std::process::id()));
    let out = project(
        &dir,
        "Provider",
        "",
        &[("main.selu", "class Box { n = 1; }\nconst box = new Box();\nlet counter = 1;\nexport addon { box, counter };")],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("SELU0711") && stderr.contains("SELU0714"), "{stderr}");
}

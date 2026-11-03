//! Behaviour of generated Lua, checked by running it in the simulator.
//! Each test compiles Selu source with the library crates, loads the output
//! into `selu-sim`, fires PLAYER_LOGIN and compares what the code printed.

use selu_sim::Sim;
use std::path::Path;

fn run(src: &str) -> Vec<String> {
    let manifest = selu_manifest::load_manifest(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json"),
    )
    .unwrap();
    let program = selu_syntax::parse("t.selu", src).unwrap_or_else(|d| panic!("{}", d.render(src)));
    let checked = selu_check::check_program(&program, &manifest);
    assert!(
        checked.diagnostics.is_empty(),
        "{}",
        checked
            .diagnostics
            .iter()
            .map(|d| d.render(src))
            .collect::<String>()
    );
    let lua = selu_lower::lower(&program, &manifest, &checked.types).lua;
    let sim = Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua.clone())])
        .unwrap_or_else(|e| panic!("{e}\n--- generated Lua ---\n{lua}"));
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.prints()
}

#[test]
fn coalesce_keeps_false_and_evaluates_each_side_once() {
    let out = run(r#"
let calls = 0;
function no(): boolean? { calls = calls + 1; return false; }
function yes() { calls = calls + 1; return true; }
function seven() { calls = calls + 1; return 7; }
function none() { calls = calls + 1; return null; }
function pick() { return none() ?? seven(); }
on PLAYER_LOGIN() {
  const a = no() ?? yes();
  print(a);
  print(calls);
  const b = none() ?? none() ?? seven();
  print(b);
  print(calls);
  print(pick());
  let c = 1;
  c = none() ?? 2;
  print(c);
  const t: number? = 5;
  print(t ?? 0);
}
"#);
    assert_eq!(
        out,
        ["false", "1", "7", "4", "7", "2", "5"],
        "`false` is a value, not a missing one, and no side runs twice"
    );
}

#[test]
fn control_characters_in_strings_survive() {
    let out = run("on PLAYER_LOGIN() {\n  print(\"tab\\there\");\n  print(\"zero\\0x\");\n}\n");
    assert_eq!(out, ["tab\there", "zero\0x"]);
}

#[test]
fn handlers_see_functions_and_values_declared_later() {
    // Without forward declarations Lua would read globals named `later` and
    // `limit` here, which are nil: the call fails at login.
    let out = run(r#"
on PLAYER_LOGIN() {
  print(later());
  print(limit);
}
function later() { return limit + 1; }
const limit = 3;
"#);
    assert_eq!(out, ["4", "3"]);
}

#[test]
fn inner_blocks_shadow_and_outer_value_survives() {
    let out = run(r#"
on PLAYER_LOGIN() {
  let x = 1;
  if (x == 1) {
    let x = 2;
    x = x + 10;
    print(x);
  }
  {
    const x = 5;
    print(x);
  }
  x = x + 1;
  print(x);
}
"#);
    assert_eq!(out, ["12", "5", "2"]);
}

#[test]
fn global_declaration_is_a_real_lua_global() {
    let src =
        "global MyAddon_Count = 1;\non PLAYER_LOGIN() { MyAddon_Count = MyAddon_Count + 1; }\n";
    let manifest = selu_manifest::load_manifest(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json"),
    )
    .unwrap();
    let program = selu_syntax::parse("t.selu", src).unwrap();
    let checked = selu_check::check_program(&program, &manifest);
    assert!(checked.diagnostics.is_empty());
    let lua = selu_lower::lower(&program, &manifest, &checked.types).lua;
    let sim = Sim::new().unwrap();
    // A second, plain Lua file reads the global the way another addon would.
    sim.load_files(&[
        ("t.lua".into(), lua),
        ("other.lua".into(), "print(MyAddon_Count)".into()),
    ])
    .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.load_files(&[("again.lua".into(), "print(MyAddon_Count)".into())])
        .unwrap();
    assert_eq!(sim.prints(), ["1", "2"]);
}

#[test]
fn plus_joins_strings_and_adds_numbers() {
    let out = run(r#"
const prefix = "hp: ";
function label(name: string, value: string) { return name + ": " + value; }
on PLAYER_LOGIN() {
  print(prefix + "full");
  print(label("a", "b"));
  print(1 + 2);
  const t = GetTime() + 1;
  print(t > 0);
}
"#);
    assert_eq!(out, ["hp: full", "a: b", "3", "true"]);
}

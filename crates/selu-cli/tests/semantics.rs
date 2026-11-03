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
    let diags = selu_check::check(&program, &manifest);
    assert!(
        diags.is_empty(),
        "{}",
        diags.iter().map(|d| d.render(src)).collect::<String>()
    );
    let lua = selu_lower::lower(&program, &manifest).lua;
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
function no() { calls = calls + 1; return false; }
function seven() { calls = calls + 1; return 7; }
function none() { calls = calls + 1; return null; }
function pick() { return none() ?? seven(); }
on PLAYER_LOGIN() {
  const a = no() ?? seven();
  print(a);
  print(calls);
  const b = none() ?? none() ?? seven();
  print(b);
  print(calls);
  print(pick());
  let c = 1;
  c = none() ?? 2;
  print(c);
  const t = 5;
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

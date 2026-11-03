//! Shared helpers for the behaviour tests: compile Selu source, run the Lua
//! in the simulator, and return what it printed.

#![allow(dead_code)]

use selu_manifest::Manifest;
use selu_sim::Sim;
use std::path::Path;

pub fn manifest() -> Manifest {
    selu_manifest::load_manifest(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json"),
    )
    .unwrap()
}

/// The generated Lua for `src`; panics with the rendered diagnostics if the
/// checker reports anything.
pub fn compile(src: &str) -> String {
    let manifest = manifest();
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
    selu_lower::lower(&program, &manifest, &checked.types).lua
}

/// Compiles `src`, loads it, fires PLAYER_LOGIN and returns the printed
/// lines.
pub fn run(src: &str) -> Vec<String> {
    let lua = compile(src);
    let sim = Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua.clone())])
        .unwrap_or_else(|e| panic!("{e}\n--- generated Lua ---\n{lua}"));
    sim.fire("PLAYER_LOGIN", vec![])
        .unwrap_or_else(|e| panic!("{e}\n--- generated Lua ---\n{lua}"));
    sim.prints()
}

/// The diagnostic codes the checker reports for `src`.
pub fn codes(src: &str) -> Vec<String> {
    let program = selu_syntax::parse("t.selu", src).unwrap_or_else(|d| panic!("{}", d.render(src)));
    selu_check::check(&program, &manifest())
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

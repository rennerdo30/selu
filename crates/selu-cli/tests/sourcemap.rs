use selu_sourcemap::{SourceMap, map_lua_error};

fn compile(source: &str) -> (String, SourceMap) {
    let program = selu_syntax::parse("main.selu", source).unwrap();
    let manifest = selu_manifest::load_manifest(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json"),
    )
    .unwrap();
    let checked = selu_check::check_program(&program, &manifest);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let lowered = selu_lower::lower(&program, &manifest, &checked.types);
    let map = lowered.source_map("main.lua", "src/main.selu", source);
    (lowered.lua, SourceMap::from_json(&map.to_json()).unwrap())
}

#[test]
fn callback_statement_positions_and_runtime_error() {
    let source = "global fail = () => {};\non PLAYER_LOGIN() {\n  const a = 1;\n  const f = () => {\n    const b = 2;\n    const n = 1;\n    fail();\n  };\n  f();\n}\n";
    let (lua, map) = compile(source);
    for (line, code) in [
        (3, "local a = 1"),
        (5, "local b = 2"),
        (7, "fail()"),
        (9, "f()"),
    ] {
        let generated = map.generated_lines("src/main.selu", line);
        assert!(!generated.is_empty(), "missing source line {line}");
        assert!(
            generated
                .iter()
                .any(|g| lua.lines().nth(*g as usize - 1).unwrap().contains(code)),
            "{line}: {lua}"
        );
        for g in generated {
            assert_eq!(map.lookup(g).unwrap().line, line);
        }
    }
    assert_eq!(map.lookup(1), None, "event runtime is unmapped");
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("main.lua".into(), lua)]).unwrap();
    sim.eval("fail = 3").unwrap();
    let error = sim.fire("PLAYER_LOGIN", vec![]).unwrap_err().to_string();
    let mapped = map_lua_error(&error, &[map]);
    assert!(mapped.contains("src/main.selu:7:"), "{mapped}");
}

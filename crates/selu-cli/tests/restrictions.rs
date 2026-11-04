mod common;
use selu_sim::Sim;
fn compile(src: &str) -> (String, Vec<String>) {
    let mut m = selu_manifest::load_manifest(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/retail/12.1.5.70391.json"),
    )
    .unwrap();
    let mut f = m
        .functions
        .iter()
        .find(|f| f.name == "GetTime")
        .unwrap()
        .clone();
    f.name = "Restricted".into();
    let d = f.signature.as_mut().unwrap();
    d.name = "Restricted".into();
    d.arguments.clear();
    d.returns.clear();
    d.attributes
        .insert("IsProtectedFunction".into(), true.into());
    let mut free = f.clone();
    free.name = "Free".into();
    free.signature.as_mut().unwrap().name = "Free".into();
    free.signature
        .as_mut()
        .unwrap()
        .attributes
        .insert("IsProtectedFunction".into(), false.into());
    m.functions.push(free);
    m.functions.push(f);
    let p = selu_syntax::parse("test.selu", src).unwrap();
    let c = selu_check::check_program(&p, &m);
    let codes = c.diagnostics.iter().map(|d| d.code.to_string()).collect();
    (selu_lower::lower(&p, &m, &c.types).lua, codes)
}
#[test]
fn defers_in_order_and_runs_directly_after_combat() {
    let (lua, codes) = compile("Restricted(); Restricted();");
    assert!(codes.is_empty(), "{codes:?}");
    let sim = Sim::new().unwrap();
    sim.eval("calls = 0; function Restricted() calls = calls + 1; print(calls) end")
        .unwrap();
    sim.set_combat(true).unwrap();
    sim.load_files(&[("test.lua".into(), lua.clone())]).unwrap();
    assert!(sim.prints().is_empty());
    sim.set_combat(false).unwrap();
    assert_eq!(sim.prints(), ["1", "2"]);
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["1", "2", "3", "4"]);
}
#[test]
fn protected_value_is_an_error() {
    assert!(
        compile("const x = Restricted();")
            .1
            .contains(&"SELU0600".into())
    );
}
#[test]
fn secret_read_becomes_null_before_arithmetic() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../manifests/retail/12.1.5.70391.json");
    let m = selu_manifest::load_manifest(&path).unwrap();
    let src = "const health = UnitHealth(\"player\") ?? 0; print(health + 1);";
    let p = selu_syntax::parse("test.selu", src).unwrap();
    let c = selu_check::check_program(&p, &m);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let lua = selu_lower::lower(&p, &m, &c.types).lua;
    let sim = Sim::new().unwrap();
    sim.set_secret_global("hidden").unwrap();
    sim.eval("function UnitHealth() return hidden end").unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["1"]);
}

#[test]
fn per_result_secret_policy_preserves_public_positions() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../manifests/retail/12.1.5.70391.json");
    let m = selu_manifest::load_manifest(&path).unwrap();
    let src = "const [name, filename, id] = UnitClass(\"player\"); const publicName: string = filename ?? \"\"; print(name ?? \"hidden\", publicName, id);";
    let p = selu_syntax::parse("test.selu", src).unwrap();
    let c = selu_check::check_program(&p, &m);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    assert!(
        selu_lower::lower(&p, &m, &c.types)
            .lua
            .contains("__selu_read_svv")
    );
    let sim = Sim::new().unwrap();
    sim.set_secret_global("hidden").unwrap();
    sim.eval("function UnitClass() return hidden, 'WARRIOR', 1 end")
        .unwrap();
    sim.load_files(&[("test.lua".into(), selu_lower::lower(&p, &m, &c.types).lua)])
        .unwrap();
    assert_eq!(sim.prints(), ["hidden\tWARRIOR\t1"]);
}
#[test]
fn protected_result_contexts_and_aliases_are_rejected() {
    for src in [
        "if (Restricted()) {}",
        "const [a] = Restricted();",
        "const f = Restricted; const x = f();",
    ] {
        assert!(compile(src).1.contains(&"SELU0600".into()), "{src}");
    }
}

#[test]
fn draining_pauses_if_combat_starts_again() {
    let (lua, _) = compile("Restricted(); Restricted();");
    let sim = Sim::new().unwrap();
    sim.eval("calls = 0; function Restricted() calls = calls + 1; print(calls); if calls == 1 then __selu_sim_combat = true end end").unwrap();
    sim.set_combat(true).unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    sim.set_combat(false).unwrap();
    assert_eq!(sim.prints(), ["1"]);
    sim.set_combat(false).unwrap();
    assert_eq!(sim.prints(), ["1", "2"]);
}

#[test]
fn conditional_function_values_keep_the_stricter_policy() {
    let (lua, codes) = compile("const f = true ? Free : Restricted; f();");
    assert!(codes.is_empty(), "{codes:?}");
    let sim = Sim::new().unwrap();
    sim.eval("function Free() print('free') end; function Restricted() print('restricted') end")
        .unwrap();
    sim.set_combat(true).unwrap();
    sim.load_files(&[("test.lua".into(), lua)]).unwrap();
    assert!(sim.prints().is_empty());
    sim.set_combat(false).unwrap();
    assert_eq!(sim.prints(), ["free"]);
}

#[test]
fn several_targets_keep_secret_guards_regardless_of_primary_order() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests");
    let retail = selu_manifest::load_manifest(&root.join("retail/12.1.5.70391.json")).unwrap();
    let classic =
        selu_manifest::load_manifest(&root.join("classic-era/1.15.9.70576.json")).unwrap();
    let targets = [classic, retail];
    for primary in &targets {
        let p = selu_syntax::parse("test.selu", "print(UnitHealth(\"player\") ?? 0);").unwrap();
        let (c, _) = selu_check::check_module_targets(&p, primary, Default::default(), &targets);
        assert!(!c.has_errors(), "{:?}", c.diagnostics);
        let universe = selu_check::targets::with_fallbacks(primary, &targets);
        let lua = selu_lower::lower(&p, &universe, &c.types).lua;
        let sim = Sim::new().unwrap();
        sim.set_secret_global("hidden").unwrap();
        sim.eval("function UnitHealth() return hidden end").unwrap();
        sim.load_files(&[("test.lua".into(), lua)]).unwrap();
        assert_eq!(sim.prints(), ["0"]);
        assert!(c.diagnostics.is_empty(), "{:?}", c.diagnostics);
    }
}

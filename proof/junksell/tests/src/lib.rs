#[cfg(test)]
use selu_sim::Sim;
#[test]
fn merchant_visit() {
    build_proof();
    let sim = Sim::new().unwrap();
    sim.eval(include_str!("../merchant.lua")).unwrap();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../dist/JunkSellProof/main.lua");
    let mut lua = std::fs::read_to_string(path).unwrap();
    if std::env::var_os("JUNKSELL_BREAK").is_some() {
        lua = lua.replace("count = count + 1", "count = count + 0");
    }
    sim.load_addon("JunkSellProof", &[("main.lua".into(), lua)])
        .unwrap();
    sim.eval("JunkSellProof_settings.locks[103] = true")
        .unwrap();
    sim.fire("MERCHANT_SHOW", vec![]).unwrap();
    sim.advance_time(2.0);
    sim.eval("assert(#sold == 1 and sold[1] == 101); assert(repaired == 1 and not guildUsed)")
        .unwrap();
    assert!(
        sim.prints()
            .iter()
            .any(|s| s == "Sold 1 junk item(s) for 20c."),
        "{:?}",
        sim.prints()
    );
    sim.eval("buyback = {'item:103', 'item:103'}").unwrap();
    sim.fire("MERCHANT_UPDATE", vec![]).unwrap();
    sim.advance_time(2.0);
    sim.eval("assert(#bought == 2 and bought[1] == 2 and bought[2] == 1)")
        .unwrap();
    sim.fire("MERCHANT_CLOSED", vec![]).unwrap();
    sim.eval("assert(#sold == 1)").unwrap();
}

#[test]
fn gap_reproductions() {
    // Each gap from docs/proof/junksell.md with its current status. A closed
    // gap must keep compiling on every real flavour; an open one must still
    // fail, so the document can't drift from the compiler.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let manifests = selu_manifest::load_manifest_dir(&root.join("manifests")).unwrap();
    // (source, open on these flavours; empty = closed everywhere)
    let cases: [(&str, &[&str]); 7] = [
        // Gap 1, open: `try` doesn't parse yet.
        ("try { print(1); } catch (e) { print(2); }", &["*"]),
        // Gaps 2 and 3, closed by package L.
        ("type Item = { id: number }; const x: Item = {id: 1};", &[]),
        ("function safe(x) { return typeof x == \"number\"; }", &[]),
        // The workaround the port uses.
        ("const [ok, value] = pcall(GetMoney);", &[]),
        // Gap 4, open: no string patterns yet.
        ("const x = \"item:123\".match(\"item:(%d+)\");", &["*"]),
        // Gap 5, closed by package M3.
        ("const x = MerchantFrame;", &[]),
        // Gap 6: closed on retail and forever by M3; the classic branches
        // only reference the constant, so it stays missing there.
        (
            "const y = NUM_TOTAL_EQUIPPED_BAG_SLOTS;",
            &[
                "classic-era",
                "classic-anniversary",
                "classic-titan",
                "classic",
            ],
        ),
    ];
    for (source, open_on) in cases {
        println!("REPRO: {source}");
        let program = match selu_syntax::parse("gap.selu", source) {
            Ok(p) => p,
            Err(d) => {
                assert_eq!(
                    open_on,
                    ["*"],
                    "{source} no longer parses:\n{}",
                    d.render(source)
                );
                println!("{}", d.render(source));
                continue;
            }
        };
        for m in &manifests {
            if m.fixture {
                continue;
            }
            let checked = selu_check::check_program(&program, m);
            let errors = checked.has_errors();
            let expected = open_on == ["*"] || open_on.contains(&m.flavour.as_str());
            assert_eq!(
                errors,
                expected,
                "{source} on {}: {}",
                m.target_string(),
                checked
                    .diagnostics
                    .iter()
                    .map(|d| d.render(source))
                    .collect::<String>()
            );
        }
    }
}

// Build the proof source in-process. No child processes or prior CLI build.
#[cfg(test)]
fn build_proof() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let project = root.join("proof/junksell");
    let source = std::fs::read_to_string(project.join("src/main.selu")).unwrap();
    // This proof is one source module; fail if its structure changes.
    assert_eq!(std::fs::read_dir(project.join("src")).unwrap().count(), 1);
    let manifests = selu_manifest::load_manifest_dir(&root.join("manifests")).unwrap();
    let manifest = selu_manifest::resolve(
        &selu_manifest::TargetSpec::parse("retail").unwrap(),
        &manifests,
    )
    .unwrap();
    let program = selu_syntax::parse("main.selu", &source).unwrap();
    let checked = selu_check::check_program(&program, manifest);
    assert!(!checked.has_errors(), "{:?}", checked.diagnostics);
    let compiled =
        selu_lower::lower_with_addon(&program, manifest, &checked.types, "JunkSellProof");
    let output = project.join("dist/JunkSellProof");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("main.lua"), compiled.lua).unwrap();
}

#[test]
fn gap_7_simulator_timers() {
    let sim = Sim::new().unwrap();
    sim.eval("C_Timer.After(0.2, function() print('sold') end)")
        .unwrap();
    assert!(sim.prints().is_empty());
    sim.advance_time(0.2);
    assert_eq!(sim.prints(), ["sold"]);
}

#[test]
fn review_build_has_no_processes() {
    assert!(!include_str!("lib.rs").contains(&["std::process", "::Command"].concat()));
}

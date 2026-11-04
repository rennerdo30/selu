#[cfg(test)]
use selu_sim::Sim;
#[test]
fn merchant_visit() {
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
    sim.eval(
        "drain(); assert(#sold == 1 and sold[1] == 101); assert(repaired == 1 and not guildUsed)",
    )
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
    sim.eval("drain(); assert(#bought == 2 and bought[1] == 2 and bought[2] == 1)")
        .unwrap();
    sim.fire("MERCHANT_CLOSED", vec![]).unwrap();
    sim.eval("assert(#sold == 1)").unwrap();
}

#[test]
fn gap_reproductions() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let manifests = selu_manifest::load_manifest_dir(&root.join("manifests")).unwrap();
    for source in [
        "try { print(1); } catch (e) { print(2); }",
        "type Item = { id: number }; const x: Item = {id: 1};",
        "function safe(x) { return typeof x == \"number\"; }",
        "const [ok, value] = pcall(GetMoney);",
        "const x = \"item:123\".match(\"item:(%d+)\");",
        "const x = MerchantFrame; const y = NUM_TOTAL_EQUIPPED_BAG_SLOTS;",
    ] {
        println!("REPRO: {source}");
        match selu_syntax::parse("gap.selu", source) {
            Err(d) => {
                assert!(
                    source.starts_with("try ")
                        || source.starts_with("type ")
                        || source.contains("typeof")
                );
                println!("{}", d.render(source));
            }
            Ok(p) => {
                for m in &manifests {
                    if m.fixture {
                        continue;
                    }
                    let checked = selu_check::check_program(&p, m);
                    assert_eq!(checked.diagnostics.is_empty(), source.contains("pcall"));
                    println!(
                        "{}: {}",
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
    }
}

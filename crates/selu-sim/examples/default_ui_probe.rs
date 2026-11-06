//! Bounded local-source startup probe. Never opens WTF or launches a client.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new("E:/Games/Battle.net/World of Warcraft");
    match selu_gamedata::Storage::open(root, "wowf", None) {
        Ok(storage) => {
            for path in [
                "Interface/ui-toc-list.txt",
                "Interface/AddOns/Blizzard_Minimap/Blizzard_Minimap.toc",
            ] {
                match storage.read_path(path) {
                    Ok(bytes) => println!("CASC {path}: {} bytes", bytes.len()),
                    Err(e) => println!("CASC {path}: {e}"),
                }
            }
        }
        Err(e) => println!("CASC unavailable: {e}"),
    }
    let mut session = selu_sim::Session::new(selu_sim::SessionConfig {
        target: Some("wowf@1.60.1.70790".into()),
        ..Default::default()
    })?;
    let report = session.load_default_ui(
        std::path::Path::new("E:/Development/wow-ui-source"),
        |name| println!("loading {name}"),
    )?;
    println!(
        "addons={} errors={} elapsed={:?}",
        report.addons_loaded,
        report.errors.len(),
        report.elapsed
    );
    for e in report.errors {
        println!("ERROR {e}");
    }
    for n in report.notes {
        println!("NOTE {n}");
    }
    println!("regions={}", session.snapshot()?.regions.len());
    Ok(())
}

//! Bounded local-source startup probe. Never opens WTF or launches a client.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|s| s == "--ui-data") {
        let storage = selu_gamedata::Storage::open(
            std::path::Path::new("E:/Games/Battle.net/World of Warcraft"),
            "wowf",
            None,
        )?;
        for (name, id) in [("GlobalColor", 4328468), ("GlobalStrings", 1394440)] {
            let bytes = storage.read(id)?;
            let u32_at = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            println!(
                "{name}: flags={:08X} layout={:08X} records={} fields={} storage={} common={} pallet={}",
                u32_at(172),
                u32_at(156),
                u32_at(136),
                u32_at(140),
                u32_at(188),
                u32_at(192),
                u32_at(196)
            );
            println!(
                "section rows={} ids={} relation={} offset_ids={} copies={}",
                u32_at(216),
                u32_at(228),
                u32_at(232),
                u32_at(236),
                u32_at(240)
            );
            for field in 0..u32_at(140) as usize {
                let at = 244 + u32_at(140) as usize * 4 + field * 24;
                println!("storage {field}: {:?}", &bytes[at..at + 24]);
            }
            match selu_gamedata::db2::Table::parse(&bytes) {
                Ok(table) => {
                    println!(
                        "first row: {:?}",
                        (0..10)
                            .map_while(|column| table.value(0, column).ok())
                            .collect::<Vec<_>>()
                    );
                    for row in (0..table.count())
                        .filter(|row| {
                            table.text(*row, 0).is_ok_and(|name| {
                                name == "NORMAL_FONT_COLOR"
                                    || name == "HIGHLIGHT_FONT_COLOR"
                                    || name == "CHAT_MSG_SAY"
                            })
                        })
                        .take(4)
                    {
                        println!(
                            "{} {:?}",
                            table.text(row, 0)?,
                            (1..10)
                                .map_while(|column| table.value(row, column).ok())
                                .collect::<Vec<_>>()
                        );
                    }
                }
                Err(error) => println!("{name} parse: {error}"),
            }
        }
        return Ok(());
    }
    if std::env::args().any(|s| s == "--atlas") {
        let storage = selu_gamedata::Storage::open(
            std::path::Path::new("E:/Games/Battle.net/World of Warcraft"),
            "wowf",
            None,
        )?;
        println!("client build {}", storage.build.version);
        println!(
            "action bar atlas: {:?}",
            storage.atlas("UI-HUD-ActionBar-Frame")?
        );
        for (name, id) in [
            ("UiTextureAtlas", 897470),
            ("UiTextureAtlasMember", 897532),
            ("UiTextureAtlasElement", 1989276),
        ] {
            let bytes = storage.read(id)?;
            let table = selu_gamedata::db2::Table::parse(&bytes)?;
            println!(
                "{name}: layout={:08X} rows={} fields={:?}",
                table.layout,
                table.count(),
                (0..20)
                    .map_while(|column| table.value(0, column).ok())
                    .collect::<Vec<_>>()
            );
            if name != "UiTextureAtlas" {
                for row in (0..table.count())
                    .filter(|row| {
                        table
                            .text(*row, 0)
                            .is_ok_and(|name| name.contains("UI-HUD-ActionBar-Frame"))
                    })
                    .take(4)
                {
                    println!(
                        "{}: {:?}",
                        table.text(row, 0)?,
                        (1..20)
                            .map_while(|column| table.value(row, column).ok())
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
        return Ok(());
    }
    if std::env::args().any(|s| s == "--types") {
        let session = selu_sim::Session::new(selu_sim::SessionConfig {
            target: Some("wowf@1.60.1.70790".into()),
            ..Default::default()
        })?;
        let manifest = session.config().manifest.as_ref().ok_or("no manifest")?;
        for schema in manifest.tables.iter().chain(&manifest.enums).filter(|s| {
            s.name.contains("LFG_ROLE")
                || s.name == "GameMode"
                || s.namespace.as_deref() == Some("Constants")
        }) {
            println!("constant schema: {schema:?}");
        }
        for global in manifest
            .data_globals
            .iter()
            .filter(|s| s.name.contains("LFG_ROLE") || s.name == "Constants")
        {
            println!("constant global: {global:?}");
        }
        for name in [
            "C_ClassColor.GetClassColor",
            "C_CurveUtil.CreateCurve",
            "C_UnitAuras.GetAuraSlots",
        ] {
            if let Some(function) = manifest.function(name) {
                if let Some(signature) = &function.signature {
                    println!("{name}: {:?}", signature.returns);
                    for ty in signature.returns.iter().filter_map(|r| r.ty.as_deref()) {
                        for schema in manifest
                            .tables
                            .iter()
                            .chain(&manifest.widgets)
                            .filter(|s| s.name == ty)
                        {
                            println!(
                                "schema {}: fields={:?} methods={:?} inherits={:?}",
                                schema.name,
                                schema.fields.iter().map(|f| &f.name).collect::<Vec<_>>(),
                                schema.methods.iter().map(|f| &f.name).collect::<Vec<_>>(),
                                schema.inherits
                            );
                        }
                    }
                }
            }
        }
        return Ok(());
    }
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
        client_install: Some(root.into()),
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
    session.login()?;
    for _ in 0..4 {
        session.advance_time(1.0 / 60.0);
    }
    let snapshot_start = std::time::Instant::now();
    let snapshot = session.snapshot()?;
    println!(
        "regions={} snapshot={:?}",
        snapshot.regions.len(),
        snapshot_start.elapsed()
    );
    for name in [
        "PlayerFrame",
        "Minimap",
        "MinimapCluster",
        "MainActionBar",
        "MainMenuBar",
        "MicroMenuContainer",
        "MicroMenu",
        "BagsBar",
        "ChatFrame1",
        "DefaultUiAnchorButton",
    ] {
        if let Some(frame) = snapshot
            .regions
            .iter()
            .find(|frame| frame.name.as_deref() == Some(name))
        {
            println!(
                "HUD {name}: shown={} visible={} rect={:?}",
                frame.shown, frame.visible, frame.rect
            );
        } else {
            println!("HUD {name}: absent");
        }
    }
    selu_sim::profile::report();
    Ok(())
}

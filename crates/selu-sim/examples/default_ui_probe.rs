//! Bounded local-source startup probe. Never opens WTF or launches a client.
fn painted(r: &selu_sim::RegionSnapshot) -> bool {
    r.visible
        && r.alpha > 0.0
        && r.widget != "MaskTexture"
        && ((r.text.as_ref().is_some_and(|s| !s.is_empty()) && r.visual.color[3] > 0.0)
            || (r.visual.tint[3] > 0.0
                && r.texture.as_ref().is_some_and(|t| match t {
                    selu_sim::TextureFill::Color(c) => c[3] > 0.0,
                    selu_sim::TextureFill::Gradient { min, max, .. } => {
                        min[3] > 0.0 || max[3] > 0.0
                    }
                    _ => true,
                })))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|s| s == "--micro-art") {
        let storage = selu_gamedata::Storage::open(client().as_path(), "wowf", None)?;
        let ab = storage.read(897470)?;
        let mb = storage.read(897532)?;
        let eb = storage.read(1989276)?;
        let a = selu_gamedata::db2::Table::parse(&ab)?;
        let m = selu_gamedata::db2::Table::parse(&mb)?;
        let e = selu_gamedata::db2::Table::parse(&eb)?;
        let mut files = std::collections::HashMap::<u32, String>::new();
        for row in 0..e.count() {
            let name = e.text(row, 0)?;
            if !name.starts_with("UI-HUD-MicroMenu-") {
                continue;
            }
            let id = e.value(row, 1)?;
            for member in 0..m.count() {
                if m.value(member, 9)? != id {
                    continue;
                }
                for sheet in 0..a.count() {
                    if a.value(sheet, 0)? != m.value(member, 2)? {
                        continue;
                    }
                    let file = a.value(sheet, 1)?;
                    let info = files.entry(file).or_insert_with(|| {
                        match storage
                            .read(file)
                            .and_then(|b| selu_gamedata::blp::decode(&b).map_err(Into::into))
                        {
                            Ok(image) => format!("installed {}x{}", image.width(), image.height()),
                            Err(error) => format!("missing {error}"),
                        }
                    });
                    println!(
                        "MICRO_ART {name} member={} sheet={} set={} file={file} bounds={},{},{},{} declared={}x{} {info}",
                        m.id(member)?,
                        a.value(sheet, 0)?,
                        a.value(sheet, 2)?,
                        m.value(member, 5)?,
                        m.value(member, 6)?,
                        m.value(member, 7)?,
                        m.value(member, 8)?,
                        a.value(sheet, 3)?,
                        a.value(sheet, 4)?
                    );
                }
            }
            println!(
                "MICRO_SELECTED {name} {:?}",
                storage.atlas(name).map(|a| a.file_data_id)
            );
        }
        return Ok(());
    }
    if std::env::args().any(|s| s == "--ui-data") {
        let storage = selu_gamedata::Storage::open(client().as_path(), "wowf", None)?;
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
        let storage = selu_gamedata::Storage::open(client().as_path(), "wowf", None)?;
        println!("client build {}", storage.build.version);
        if let Some(folder) = std::env::var_os("SELU_SIM_DUMP_ART") {
            for (name, file) in [
                (
                    "PlayerFrame.xml",
                    "Interface/AddOns/Blizzard_UnitFrame/Mainline/PlayerFrame.xml",
                ),
                (
                    "MainMenuBarMicroButtons.xml",
                    "Interface/AddOns/Blizzard_MicroMenu/Mainline/MainMenuBarMicroButtons.xml",
                ),
            ] {
                match storage.read_path(file) {
                    Ok(bytes) => std::fs::write(
                        std::path::Path::new(&folder).join(format!("client-{name}")),
                        bytes,
                    )?,
                    Err(error) => println!("UI file {file}: {error}"),
                }
            }
        }
        for name in ["classicon-WARRIOR", "UI-HUD-UnitFrame-Player-PortraitOn"] {
            let atlas = storage.atlas(name)?;
            let bytes = storage.read(atlas.file_data_id)?;
            let pixels = selu_gamedata::blp::decode(&bytes)?;
            println!(
                "ART METADATA {atlas:?}; actual {}x{}",
                pixels.width(),
                pixels.height()
            );
            if let Some(folder) = std::env::var_os("SELU_SIM_DUMP_ART") {
                pixels.save(
                    std::path::Path::new(&folder).join(format!("{}.png", atlas.file_data_id)),
                )?;
            }
        }
        println!(
            "action bar atlas: {:?}",
            storage.atlas("UI-HUD-ActionBar-Frame")?
        );
        for name in [
            "UI-HUD-MicroMenu-Professions-Up",
            "UI-HUD-MicroMenu-Professions-Disabled",
            "UI-HUD-MicroMenu-Collections-Disabled",
        ] {
            let atlas = storage.atlas(name)?;
            println!("HUD ATLAS {name}: {atlas:?}");
            {
                println!(
                    "HUD FILE {}: {:?}",
                    atlas.file_data_id,
                    storage.read(atlas.file_data_id).map(|bytes| bytes.len())
                );
            }
        }
        match storage
            .read_path("DBFilesClient/UiCanvas.db2")
            .or_else(|_| storage.read(1989363))
        {
            Ok(bytes) => {
                let table = match selu_gamedata::db2::Table::parse(&bytes) {
                    Ok(table) => table,
                    Err(error) => {
                        println!(
                            "CANVAS parse {error} header {:?}",
                            (136..=200)
                                .step_by(4)
                                .map(|at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()))
                                .collect::<Vec<_>>()
                        );
                        let fields =
                            u32::from_le_bytes(bytes[140..144].try_into().unwrap()) as usize;
                        for i in 0..fields {
                            let at = 244 + fields * 4 + i * 24;
                            println!("CANVAS FIELD {:?}", &bytes[at..at + 24]);
                        }
                        return Ok(());
                    }
                };
                println!("CANVAS layout={:08x}", table.layout);
                for row in 0..table.count() {
                    println!(
                        "CANVAS {:?}",
                        (0..8)
                            .map_while(|column| table.value(row, column).ok())
                            .collect::<Vec<_>>()
                    );
                }
            }
            Err(error) => println!("CANVAS unavailable: {error}"),
        }
        let atlas_bytes = storage.read(897470)?;
        let member_bytes = storage.read(897532)?;
        let element_bytes = storage.read(1989276)?;
        let atlases = selu_gamedata::db2::Table::parse(&atlas_bytes)?;
        let members = selu_gamedata::db2::Table::parse(&member_bytes)?;
        let elements = selu_gamedata::db2::Table::parse(&element_bytes)?;
        for element in 0..elements.count() {
            if [
                "UI-HUD-MicroMenu-Professions-Up",
                "UI-HUD-UnitFrame-SmallCircle",
            ]
            .contains(&elements.text(element, 0)?)
            {
                let id = elements.value(element, 1)?;
                let mut count = 0;
                for member in 0..members.count() {
                    if members.value(member, 9)? == id {
                        for sheet in 0..atlases.count() {
                            if atlases.value(sheet, 0)? == members.value(member, 2)? {
                                println!(
                                    "CANDIDATE {} member={:?} sheet={:?}",
                                    elements.text(element, 0)?,
                                    (0..14)
                                        .map_while(|column| members.value(member, column).ok())
                                        .collect::<Vec<_>>(),
                                    (0..6)
                                        .map_while(|column| atlases.value(sheet, column).ok())
                                        .collect::<Vec<_>>()
                                );
                                count += 1;
                                let file = atlases.value(sheet, 1)?;
                                println!(
                                    "MICRO CANDIDATE sheet={} set={} file={} installed={}",
                                    atlases.value(sheet, 0)?,
                                    atlases.value(sheet, 2)?,
                                    file,
                                    storage.read(file).is_ok()
                                );
                            }
                        }
                    }
                }
                println!("MICRO CANDIDATES {count}");
            }
        }
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
            if let Some(function) = manifest.function(name)
                && let Some(signature) = &function.signature
            {
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
        return Ok(());
    }
    let client_root = client();
    let root = client_root.as_path();
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
    let report = session.load_default_ui(source().as_path(), |name| println!("loading {name}"))?;
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
    session.lua().globals().raw_set(
        "__probe_meta",
        session
            .lua()
            .create_function(|_, values: mlua::MultiValue| {
                println!(
                    "HUDMETA {}",
                    values
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect::<Vec<_>>()
                        .join("\t")
                );
                Ok(())
            })?,
    )?;
    if std::env::var_os("SELU_SIM_PROFILE_PCALL").is_some() {
        session.eval("__probe_original_pcall=pcall; local original=pcall; local log=__probe_meta; pcall=function(...) local function finish(ok,...) if not ok then log('CAUGHT',(...)) end; return ok,... end; return finish(original(...)) end")?;
    }
    session.login()?;
    for line in session.logs() {
        if line.contains("error") {
            eprintln!("STARTUP {line}");
        }
    }
    if let Err(error)=session.eval(r#"local f=MainActionBar; local m=EditModeManagerFrame; local a=m:GetActiveLayoutInfo(); __probe_meta('layout',a and a.interfaceStyle,Enum.InputDeviceInterfaceType.Mkb,Enum.InputDeviceInterfaceType.Gamepad,#EditModeUtil:GetBottomActionBars(),m.layoutApplyInProgress,f:IsInDefaultPosition()); __probe_meta('children',#f:GetLayoutChildren()); __probe_meta('fields',f.system,f.systemIndex,f.numButtonsShowable,f.noSpacers,f.actionButtons and #f.actionButtons,f.shownButtonContainers and #f.shownButtonContainers,f.systemInfo~=nil,#m.registeredSystemFrames,m.layoutInfo and m.layoutInfo.activeLayout); if m.GetActiveLayoutSystemInfo then local s=m:GetActiveLayoutSystemInfo(f.system,f.systemIndex); __probe_meta('system',s and s.system,s and s.systemIndex,s and #s.settings) end; for i=1,f:GetNumPoints() do local p,r,rp,x,y=f:GetPoint(i); __probe_meta('point',p,r and r:GetName(),rp,x,y) end"#) {eprintln!("HUD diagnostic: {error}");}
    if std::env::var_os("SELU_SIM_PROFILE_PCALL").is_some() {
        session.eval("pcall=__probe_original_pcall; __probe_original_pcall=nil")?;
    }
    for _ in 0..4 {
        session.advance_time(1.0 / 60.0);
    }
    session.advance_time(1.0 / 60.0);
    for error in session.default_ui_startup_errors() {
        println!("STARTUP {error}");
    }
    session.eval("local f=ObjectiveTrackerFrame; __probe_meta('TRACKER',f:IsShown(),f.isCollapsed,f.needsSorting,f.isDirty,EditModeManagerFrame:IsEditModeActive(),#f.modules); for _,m in ipairs(f.modules) do __probe_meta('MODULE',m:GetName(),m:GetContentsHeight()) end")?;
    let snapshot_start = std::time::Instant::now();
    let snapshot = session.snapshot()?;
    println!(
        "regions={} snapshot={:?}",
        snapshot.regions.len(),
        snapshot_start.elapsed()
    );
    session.eval("__probe_meta('PORTRAITSTATE',PlayerFrame.unit,PlayerFrame.portrait and PlayerFrame.portrait:GetAtlas(),PlayerFrame.portrait and PlayerFrame.portrait:GetTexture(),PlayerFrame.portrait and PlayerFrame.portrait:GetDrawLayer(),GetClassAtlas(select(2,UnitClass('player'))))")?;
    let portrait: mlua::Table = session
        .lua()
        .globals()
        .raw_get::<mlua::Table>("PlayerFrame")?
        .raw_get("portrait")?;
    let portrait = selu_sim::frames::arena::id(&portrait)?;
    for region in &snapshot.regions {
        let parent = region
            .parent
            .and_then(|id| snapshot.regions.get(id.index as usize));
        if region.id.index as usize == portrait
            || parent.is_some_and(|parent| parent.name.as_deref() == Some("CharacterMicroButton"))
            || (region.texture.is_some()
                && parent
                    .is_some_and(|parent| parent.name.as_deref() == Some("PlayerFrameContainer")))
        {
            println!(
                "ARTSTATE {} {:?} {:?} {:?} {:?} {:?}",
                region.id.index,
                parent.map(|r| &r.name),
                region.rect,
                region.texture,
                region.layer,
                region.visual.uv
            );
            println!(
                "ART FLAGS {} kind={} visible={} shown={} alpha={} tint={:?}",
                region.id.index,
                region.widget,
                region.visible,
                region.shown,
                region.alpha,
                region.visual.tint
            );
        }
    }
    {
        let store = selu_sim::frames::arena::store(session.lua());
        let arena = store.borrow();
        for region in snapshot.regions.iter().filter(|r| {
            r.visible
                && r.texture.is_some()
                && r.rect.is_some_and(|q| {
                    q.left < 515.0
                        && q.left + q.width > 450.0
                        && q.bottom < 330.0
                        && q.bottom + q.height > 270.0
                })
        }) {
            let native = &arena.regions[region.id.index as usize];
            let table = native.table.get();
            let keys = native.parent.map(|parent| {
                arena.regions[parent]
                    .table
                    .get()
                    .pairs::<mlua::Value, mlua::Value>()
                    .filter_map(|entry| {
                        let (key, value) = entry.ok()?;
                        if value == mlua::Value::Table(table.clone()) {
                            Some(format!("{key:?}"))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            });
            println!(
                "PORTRAIT OVERLAP {} {} {:?} {:?} {:?} {:?} {:?} {:?}",
                region.id.index,
                region.widget,
                region.layer,
                region.rect,
                region.texture,
                table.raw_get::<Option<String>>("__selu_atlas")?,
                keys,
                region.visual.uv
            );
        }
    }

    let unplaced = snapshot
        .regions
        .iter()
        .filter(|r| r.rect.is_none())
        .collect::<Vec<_>>();
    println!(
        "UNPLACED total={} visible={} named={}",
        unplaced.len(),
        unplaced.iter().filter(|r| r.visible).count(),
        unplaced.iter().filter(|r| r.name.is_some()).count()
    );
    println!(
        "UNPLACED painted-visible={}",
        unplaced.iter().filter(|r| painted(r)).count()
    );
    {
        let arena = selu_sim::frames::arena::store(session.lua());
        let arena = arena.borrow();
        for r in unplaced.iter().filter(|r| painted(r)) {
            let native = &arena.regions[r.id.index as usize];
            let mut parent = native.parent;
            let mut lineage = Vec::new();
            for _ in 0..6 {
                let Some(index) = parent else { break };
                let p = &arena.regions[index];
                lineage.push(format!(
                    "{}:{}:{:?}",
                    index,
                    p.name.as_deref().unwrap_or(&p.kind),
                    p.rect
                ));
                parent = p.parent;
            }
            println!(
                "PAINT-UNPLACED {} {:?} text={:?} texture={:?} anchors={:?} source={:?} parents={}",
                r.id.index,
                r.name,
                r.text,
                r.texture,
                native
                    .points
                    .iter()
                    .map(|p| (&p.point, p.relative, &p.anchor, p.x, p.y))
                    .collect::<Vec<_>>(),
                native.created_at,
                lineage.join(" / ")
            );
        }
    }
    for r in snapshot
        .regions
        .iter()
        .filter(|r| r.visible && r.rect.is_some() && r.texture.is_some())
    {
        let q = r.rect.unwrap();
        if q.left > 1450.0 && q.bottom > 700.0 && q.bottom < 850.0 && q.width > 250.0 {
            println!(
                "GOLD-CANDIDATE {} {:?} {:?} {:?} parent={:?}",
                r.id.index,
                r.name,
                r.texture,
                r.rect,
                r.parent
                    .and_then(|id| snapshot.regions.get(id.index as usize))
                    .map(|r| (&r.name, &r.widget))
            );
        }
    }
    for r in unplaced
        .iter()
        .filter(|r| r.visible && r.name.is_some())
        .take(30)
    {
        println!(
            "UNPLACED {} {} {:?}",
            r.widget,
            r.name.as_deref().unwrap_or(""),
            r.size
        );
    }
    for region in snapshot.regions.iter().filter(|r| {
        r.visible
            && r.rect.is_some()
            && r.name.is_some()
            && !matches!(r.widget.as_str(), "Texture" | "FontString" | "MaskTexture")
    }) {
        let rect = region.rect.unwrap();
        if rect.bottom > 600.0 && rect.width > 150.0 && rect.height > 20.0 && rect.height < 200.0 {
            println!(
                "PANEL {} {} {:?}",
                region.name.as_deref().unwrap(),
                region.widget,
                rect
            );
        }
    }
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
    for line in session
        .prints()
        .iter()
        .filter(|line| line.contains("CAUGHT") || line.contains("HUDMETA"))
    {
        println!("{line}");
    }
    {
        let arena = selu_sim::frames::arena::store(session.lua());
        let a = arena.borrow();
        for (index, r) in a.regions.iter().enumerate().filter(|(_, r)| {
            r.visible
                && r.rect.is_some_and(|q| {
                    q.left > 1450. && q.bottom > 700. && q.bottom < 850. && q.width > 250.
                })
                && r.texture.is_some()
        }) {
            let mut at = Some(index);
            let mut lineage = Vec::new();
            for _ in 0..12 {
                let Some(i) = at else { break };
                let r = &a.regions[i];
                lineage.push(format!("{}:{}", i, r.name.as_deref().unwrap_or(&r.kind)));
                at = r.parent;
            }
            println!(
                "GOLD-LINEAGE {} {:?} {}",
                index,
                r.texture,
                lineage.join(" / ")
            );
        }
    }
    selu_sim::profile::report();
    Ok(())
}

/// The value after `flag` on the command line; the probe has no built-in
/// paths, so every machine passes its own.
fn path_arg(flag: &str) -> std::path::PathBuf {
    let args: Vec<String> = std::env::args().collect();
    let index = args
        .iter()
        .position(|a| a == flag)
        .unwrap_or_else(|| panic!("pass {flag} <path>"));
    std::path::PathBuf::from(
        args.get(index + 1)
            .unwrap_or_else(|| panic!("{flag} needs a path")),
    )
}
/// The game installation (`--client`).
fn client() -> std::path::PathBuf {
    path_arg("--client")
}
/// The wow-ui-source clone (`--source`).
fn source() -> std::path::PathBuf {
    path_arg("--source")
}

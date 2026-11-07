//! Read-only local client check for the lean real Blizzard environment.
#[test]
#[ignore = "requires SELU_CLIENT_INSTALL; optional SELU_DEFAULT_UI_SOURCE override"]
fn core_loads_real_common_frames_and_records_startup() {
    let start = std::time::Instant::now();
    let mut s = selu_sim::Session::new(selu_sim::SessionConfig {
        target: Some(
            std::env::var("SELU_CORE_TARGET").unwrap_or_else(|_| "wowf@1.60.1.70790".into()),
        ),
        client_install: Some(std::env::var_os("SELU_CLIENT_INSTALL").unwrap().into()),
        ..Default::default()
    })
    .unwrap();
    let mut roots = selu_sim::loader::CORE_UI_ADDONS
        .iter()
        .map(|s| (*s).into())
        .collect::<Vec<_>>();
    // Optional dependency-cost probe, using addon roots rather than UI objects.
    if let Ok(extra) = std::env::var("SELU_CORE_EXTRA_ADDONS") {
        roots.extend(
            extra
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        );
    }
    let report = s
        .load_blizzard_ui(
            std::path::Path::new(
                &std::env::var_os("SELU_DEFAULT_UI_SOURCE")
                    .unwrap_or_else(|| std::env::var_os("SELU_CLIENT_INSTALL").unwrap()),
            ),
            Some(&roots),
            |_| {},
        )
        .unwrap();
    s.eval("assert(C_GameRules.GetActiveGameMode()==Enum.GameMode.Standard)")
        .unwrap();
    // Optional diagnostic of a read-only real addon; only its fresh in-memory
    // error log is inspected, never a saved-variable file or player account.
    let toolkit = std::env::var_os("SELU_CORE_TOOLKIT");
    if let Some(path) = &toolkit {
        s.load_addon_dir(std::path::Path::new(path)).unwrap();
    }
    s.login().unwrap();
    for _ in 0..4 {
        s.advance_time(1. / 60.);
    }
    selu_sim::profile::report();
    eprintln!(
        "Core startup {:?}: {} addons; {} load errors; {} startup errors",
        start.elapsed(),
        report.addons_loaded,
        report.errors.len(),
        s.default_ui_startup_errors().len()
    );
    for error in s.default_ui_startup_errors() {
        eprintln!("CORE STARTUP {error}");
    }
    if let Some(dir) = std::env::var_os("SELU_CORE_AUDIT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::write(
            dir.join("snapshot.json"),
            serde_json::to_vec(&s.snapshot().unwrap()).unwrap(),
        )
        .unwrap();
        let mut keys = Vec::new();
        for root in ["Enum", "Constants"] {
            let table: mlua::Table = s.lua().globals().raw_get(root).unwrap();
            for pair in table.pairs::<String, mlua::Table>() {
                let (name, fields) = pair.unwrap();
                for pair in fields.pairs::<String, mlua::Value>() {
                    let (key, value) = pair.unwrap();
                    if !value.is_nil() {
                        keys.push(format!("{root}.{name}.{key}"));
                    }
                }
            }
        }
        keys.sort();
        std::fs::write(dir.join("documented-keys.txt"), keys.join("\n")).unwrap();
        if let Some(file) = std::env::var_os("SELU_CORE_DUMP_TEXTURE") {
            let id = file.to_string_lossy().parse::<u32>().unwrap();
            let storage = selu_sim::frames::arena::store(s.lua())
                .borrow()
                .fonts
                .assets
                .client_storage()
                .unwrap();
            selu_gamedata::blp::decode(&storage.read(id).unwrap())
                .unwrap()
                .save(dir.join("texture.png"))
                .unwrap();
        }
    }
    for error in &report.errors {
        eprintln!("CORE ERROR {error}");
    }
    assert!(!report.elapsed.is_zero());
    if toolkit.is_some() {
        let ns = s.addon_namespace("RennersAddons").unwrap();
        let errors: mlua::Table = ns.raw_get("ErrorLog").unwrap();
        for error in errors.sequence_values::<mlua::Table>() {
            let message: String = error.unwrap().raw_get("message").unwrap();
            eprintln!("TOOLKIT INTERNAL ERROR {message}");
            assert!(!message.contains("global 'Minimap' (a nil value)"));
        }
    }
    s.eval("assert(IsAddOnLoaded('Blizzard_Minimap'));assert(Minimap and DEFAULT_CHAT_FRAME and GameTooltip);assert(Minimap:GetObjectType()=='Minimap');assert(DEFAULT_CHAT_FRAME.AddMessage and GameTooltip.SetOwner)").unwrap();
}

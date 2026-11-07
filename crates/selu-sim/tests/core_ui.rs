//! Read-only local client check for the lean real Blizzard environment.
#[test]
#[ignore = "requires SELU_DEFAULT_UI_SOURCE and SELU_CLIENT_INSTALL"]
fn core_loads_real_common_frames_and_records_startup() {
    let start = std::time::Instant::now();
    let mut s = selu_sim::Session::new(selu_sim::SessionConfig {
        target: Some("wowf@1.60.1.70790".into()),
        client_install: Some(std::env::var_os("SELU_CLIENT_INSTALL").unwrap().into()),
        ..Default::default()
    })
    .unwrap();
    let roots = selu_sim::loader::CORE_UI_ADDONS
        .iter()
        .map(|s| (*s).into())
        .collect::<Vec<_>>();
    let report = s
        .load_blizzard_ui(
            std::path::Path::new(&std::env::var_os("SELU_DEFAULT_UI_SOURCE").unwrap()),
            Some(&roots),
            |_| {},
        )
        .unwrap();
    s.login().unwrap();
    for _ in 0..4 {
        s.advance_time(1. / 60.);
    }
    eprintln!(
        "Core startup {:?}: {} addons; {} load errors; startup {:?}",
        start.elapsed(),
        report.addons_loaded,
        report.errors.len(),
        s.default_ui_startup_errors()
    );
    for error in &report.errors {
        eprintln!("CORE ERROR {error}");
    }
    assert!(!report.elapsed.is_zero());
    s.eval("assert(IsAddOnLoaded('Blizzard_Minimap'));assert(Minimap and DEFAULT_CHAT_FRAME and GameTooltip);assert(Minimap:GetObjectType()=='Minimap');assert(DEFAULT_CHAT_FRAME.AddMessage and GameTooltip.SetOwner)").unwrap();
}

use selu_sim::{
    Session, SessionConfig,
    world::{Scenario, bag_slot_defaults},
};

fn fixture(tag: &str, documentation: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("selu-bag-slots-{tag}-{}", std::process::id()));
    let addon = root.join("Interface/AddOns/Fixture");
    let api = root.join("Interface/AddOns/Blizzard_APIDocumentationGenerated");
    std::fs::create_dir_all(&addon).unwrap();
    std::fs::create_dir_all(&api).unwrap();
    std::fs::write(
        root.join("Interface/ui-toc-list.txt"),
        "Interface/AddOns/Fixture/Fixture.toc\n",
    )
    .unwrap();
    std::fs::write(addon.join("Fixture.toc"), "Fixture.lua\n").unwrap();
    std::fs::write(addon.join("Fixture.lua"),"FixtureCounts={Constants.InventoryConstants.NumBagSlots,Constants.InventoryConstants.NumReagentBagSlots}").unwrap();
    std::fs::write(
        api.join("Blizzard_APIDocumentationGenerated.toc"),
        "Fixture.lua\n",
    )
    .unwrap();
    std::fs::write(api.join("Fixture.lua"), documentation).unwrap();
    root
}
fn session(target: &str) -> Session {
    Session::new(SessionConfig {
        target: Some(format!("{target}@fixture")),
        ..Default::default()
    })
    .unwrap()
}
#[test]
fn product_defaults_cover_channels_and_unknown_products_make_no_claim() {
    for product in ["wow", "wowf", "wowt", "wowxptr", "wow_beta", "wowf_ptr"] {
        let slots = bag_slot_defaults(Some(product));
        assert_eq!((slots.ordinary, slots.reagent), (4, 1), "{product}");
    }
    for product in [
        "wow_classic",
        "wow_classic_era",
        "wow_anniversary",
        "wow_classic_titan",
        "wow_classic_ptr",
        "wow_classic_beta",
        "wow_classic_era_ptr",
        "wow_anniversary_ptr",
        "wow_classic_titan_beta",
    ] {
        let slots = bag_slot_defaults(Some(product));
        assert_eq!((slots.ordinary, slots.reagent), (4, 0), "{product}");
    }
    assert_eq!(bag_slot_defaults(None), Default::default());
    assert_eq!(bag_slot_defaults(Some("unrecognized")), Default::default());
}
#[test]
fn classic_fixture_and_scenario_toml_share_live_counts() {
    let root = fixture("classic", "local selfWritten={Tables={}}");
    let mut s = session("wow_classic_era");
    s.load_default_ui(&root, |_| {}).unwrap();
    s.eval("assert(FixtureCounts[1]==4 and FixtureCounts[2]==0)")
        .unwrap();
    let mut state = Scenario::capture(s.sim()).unwrap();
    state.bag_slots = Some(2);
    state.reagent_bag_slots = Some(1);
    let path = root.join("scenario.toml");
    state.save(&path).unwrap();
    let state = Scenario::load(&path).unwrap();
    state.apply(s.sim()).unwrap();
    s.eval("assert(Constants.InventoryConstants.NumBagSlots==2 and Constants.InventoryConstants.NumReagentBagSlots==1);assert(type(Constants.UndocumentedFixture)=='table');assert(Constants.UndocumentedFixture.Value==nil);assert(Constants.UndocumentedFixture.Value==nil)").unwrap();
    assert_eq!(s.logs().iter().filter(|l|l.contains("undocumented client value: Constants.UndocumentedFixture.Value")).count(),1);
    s.reload().unwrap();
    s.eval("assert(FixtureCounts[1]==2 and FixtureCounts[2]==1)")
        .unwrap();
    assert_eq!(Scenario::capture(s.sim()).unwrap().bag_slots, Some(2));
}
#[test]
fn documented_counts_override_scenario_and_native_world_counts() {
    let root = fixture(
        "documented",
        r#"local selfWritten={Tables={{Name="InventoryConstants",Type="Constants",Values={{Name="NumBagSlots",Value=3},{Name="NumReagentBagSlots",Value=2}}}}}"#,
    );
    let mut s = session("wow");
    s.load_default_ui(&root, |_| {}).unwrap();
    let state = Scenario {
        bag_slots: Some(1),
        reagent_bag_slots: Some(0),
        ..Default::default()
    };
    state.apply(s.sim()).unwrap();
    s.eval("assert(Constants.InventoryConstants.NumBagSlots==3 and Constants.InventoryConstants.NumReagentBagSlots==2)").unwrap();
    let slots = s.sim().bag_slot_counts().unwrap();
    assert_eq!((slots.ordinary, slots.reagent), (3, 2));
}
#[test]
#[ignore = "requires read-only SELU_CLIENT_INSTALL; optional SELU_DEFAULT_UI_SOURCE"]
fn scenario_counts_reach_blizzard_container_startup_and_reload() {
    let install = std::env::var_os("SELU_CLIENT_INSTALL").unwrap();
    let source = std::env::var_os("SELU_DEFAULT_UI_SOURCE").unwrap_or_else(|| install.clone());
    let mut s = Session::new(SessionConfig {
        target: Some("wowf@1.60.1.70790".into()),
        client_install: Some(install.into()),
        ..Default::default()
    })
    .unwrap();
    let mut state = Scenario::capture(s.sim()).unwrap();
    state.bag_slots = Some(2);
    state.reagent_bag_slots = Some(0);
    state.apply(s.sim()).unwrap();
    s.load_default_ui(std::path::Path::new(&source), |_| {})
        .unwrap();
    s.login().unwrap();
    s.eval("assert(NUM_BAG_FRAMES==2 and NUM_REAGENTBAG_FRAMES==0 and NUM_CONTAINER_FRAMES==3)")
        .unwrap();
    state.bag_slots = Some(3);
    state.reagent_bag_slots = Some(1);
    state.apply(s.sim()).unwrap();
    s.eval("assert(Constants.InventoryConstants.NumBagSlots==3 and Constants.InventoryConstants.NumReagentBagSlots==1)").unwrap();
    s.reload().unwrap();
    s.eval("assert(NUM_BAG_FRAMES==3 and NUM_REAGENTBAG_FRAMES==1 and NUM_CONTAINER_FRAMES==5)")
        .unwrap();
}

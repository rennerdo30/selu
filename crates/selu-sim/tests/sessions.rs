use selu_sim::world::Unit;
use selu_sim::{Session, SessionConfig, Sim, TraceKind};
#[test]
fn migration_world_trace_and_isolation() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/addons/Migration");
    let mut s = Session::new(SessionConfig {
        trace: true,
        ..Default::default()
    })
    .unwrap();
    s.load_addon_dir(&dir).unwrap();
    s.login().unwrap();
    s.set_unit(
        "player",
        Unit {
            name: "Test".into(),
            class: "MAGE".into(),
            level: 80,
            health: 42,
            max_health: 100,
            power: 10,
            max_power: 100,
            auras: vec![],
            combat: false,
            ..Default::default()
        },
    )
    .unwrap();
    s.eval("assert(MigrationDB.health==42); MigrationDB.version=0; MigrationDB.migrated=false")
        .unwrap();
    s.reload().unwrap();
    s.eval("assert(MigrationDB.health==42 and MigrationDB.migrated and MigrationDB.version==1)")
        .unwrap();
    assert!(
        s.trace_since(0)
            .iter()
            .any(|r| r.kind == TraceKind::Event && r.target == "ADDON_LOADED")
    );
    let mut other = Session::new(Default::default()).unwrap();
    other.load_addon_dir(&dir).unwrap();
    other.eval("assert(MigrationDB.health==100)").unwrap();
    assert!(
        s.host_commands()
            .contains(&selu_sim::session::HostCommand::Reload)
    );
}
#[test]
fn bounded_idle_calendar_and_coroutines() {
    let s = Session::new(Default::default()).unwrap();
    s.eval("assert(time()==946684800); assert(date('!%Y-%m-%d')=='2000-01-01'); C_Timer.After(.1,function() print('done') end)").unwrap();
    assert!(s.run_until_idle(1.0, 100).unwrap());
    assert_eq!(s.prints(), ["done"]);
    s.eval("C_Timer.NewTicker(.01,function() end)").unwrap();
    assert!(!s.run_until_idle(0.1, 100).unwrap());
    let s = Sim::new().unwrap();
    s.ctx().set_budget(1);
    s.eval("local f=coroutine.wrap(function() local n=0; for i=1,100000 do n=n+i end end); f()")
        .unwrap_err();
    s.eval("print('alive')").unwrap();
    assert_eq!(s.prints(), ["alive"]);
}

#[test]
fn flavour_dependencies_and_load_on_demand() {
    let root = std::env::temp_dir().join(format!("selu-s2-loader-{}", std::process::id()));
    for name in ["Main", "Dep", "Lazy"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    std::fs::write(root.join("Dep/Dep.toc"), "main.lua").unwrap();
    std::fs::write(root.join("Dep/main.lua"), "load_order='dep'").unwrap();
    std::fs::write(
        root.join("Main/Main_Mainline.toc"),
        "## Dependencies: Dep\nmain.lua",
    )
    .unwrap();
    std::fs::write(
        root.join("Main/main.lua"),
        "assert(load_order=='dep'); print('mainline')",
    )
    .unwrap();
    std::fs::write(root.join("Lazy/Lazy.toc"), "## LoadOnDemand: 1\nmain.lua").unwrap();
    std::fs::write(root.join("Lazy/main.lua"), "print('lazy')").unwrap();
    let mut s = Session::new(SessionConfig {
        target: Some("retail@12.0".into()),
        ..Default::default()
    })
    .unwrap();
    s.load_addon_dir(&root.join("Main")).unwrap();
    s.login().unwrap();
    s.eval("assert(C_AddOns.LoadAddOn('Lazy')); assert(C_AddOns.LoadAddOn('Lazy'))")
        .unwrap();
    assert_eq!(s.prints(), ["mainline", "lazy"]);
    s.reload().unwrap();
    s.eval("assert(C_AddOns.LoadAddOn('Lazy'))").unwrap();
    assert_eq!(s.prints(), ["mainline", "lazy"]);
    std::fs::remove_dir_all(root).unwrap();
}

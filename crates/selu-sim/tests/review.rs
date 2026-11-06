use selu_sim::world::Unit;
use selu_sim::{Session, SessionConfig, Sim, SimValue, TraceKind};
use std::path::{Path, PathBuf};
fn session() -> Session {
    Session::new(SessionConfig {
        trace: true,
        ..Default::default()
    })
    .unwrap()
}
fn addon(root: &Path, name: &str, toc: &str, lua: &str) -> PathBuf {
    let d = root.join(name);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(format!("{name}.toc")), toc).unwrap();
    std::fs::write(d.join("main.lua"), lua).unwrap();
    d
}
fn root(name: &str) -> PathBuf {
    let d = std::env::temp_dir()
        .join(format!("selu-s2-review-{}", std::process::id()))
        .join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn unit() -> Unit {
    Unit {
        name: "Alice".into(),
        class: "MAGE".into(),
        level: 80,
        health: 50,
        max_health: 100,
        power: 20,
        max_power: 100,
        auras: vec![],
        combat: false,
        ..Default::default()
    }
}
#[test]
fn budget_after_create() {
    let s = session();
    s.ctx().set_budget(1);
    let r =
        s.eval("local c=coroutine.create(function() end); local n=0; for i=1,100000 do n=n+i end");
    assert!(r.is_err(), "creating a coroutine disarmed main budget");
}
#[test]
fn budget_after_resume() {
    let s = session();
    s.ctx().set_budget(1);
    let r=s.eval("local c=coroutine.create(function() end); assert(coroutine.resume(c)); local n=0; for i=1,100000 do n=n+i end");
    assert!(r.is_err(), "resuming coroutine disarmed main budget");
}
#[test]
fn addon_loaded_payload() {
    let s = session();
    s.eval("local f=CreateFrame('Frame');f:RegisterEvent('ADDON_LOADED'); f:SetScript('OnEvent',function(_,_,name,bindings) assert(name=='Test');assert(bindings==false, 'containsBindings missing') end)").unwrap();
    s.load_addon("Test", &[]).unwrap();
}
#[test]
fn load_on_demand_at_top_level() {
    let r = root("top-level");
    addon(&r, "Lazy", "## LoadOnDemand: 1\nmain.lua", "print('lazy')");
    let main = addon(&r, "Main", "main.lua", "assert(C_AddOns.LoadAddOn('Lazy'))");
    session().load_addon_dir(&main).unwrap();
}
#[test]
fn missing_lod_returns_failure() {
    let r = root("missing-lod");
    let main = addon(&r, "Main", "main.lua", "");
    let mut s = session();
    s.load_addon_dir(&main).unwrap();
    s.eval("local ok,loaded,reason=pcall(C_AddOns.LoadAddOn,'Missing');assert(ok and loaded==nil and type(reason)=='string', 'missing addon throws')").unwrap();
}
#[test]
fn dynamic_then_explicit_load_once() {
    let r = root("duplicate");
    let lazy = addon(&r, "Lazy", "## LoadOnDemand: 1\nmain.lua", "print('lazy')");
    let main = addon(&r, "Main", "main.lua", "");
    let mut s = session();
    s.load_addon_dir(&main).unwrap();
    s.eval("C_AddOns.LoadAddOn('Lazy')").unwrap();
    s.load_addon_dir(&lazy).unwrap();
    assert_eq!(s.prints(), ["lazy"]);
}
#[test]
fn reload_does_not_reset_sv_before_files() {
    let r = root("saved-timing");
    let main = addon(
        &r,
        "Main",
        "## SavedVariables: DB\nmain.lua",
        "DB={value=1}; local f=CreateFrame('Frame'); f:RegisterEvent('ADDON_LOADED');f:SetScript('OnEvent',function(_,_,name) if name=='Main' then print(DB.value) end end)",
    );
    let mut s = session();
    s.load_addon_dir(&main).unwrap();
    s.eval("DB.value=42").unwrap();
    s.reload().unwrap();
    assert_eq!(s.prints(), ["42"]);
}
#[test]
fn dynamic_reload_preserves_registration_order() {
    let r = root("reload-order");
    addon(&r, "Lazy", "## LoadOnDemand: 1\nmain.lua", "print('lazy')");
    let main = addon(
        &r,
        "Main",
        "main.lua",
        "print('main'); local f=CreateFrame('Frame');f:RegisterEvent('PLAYER_LOGIN');f:SetScript('OnEvent',function() assert(C_AddOns.LoadAddOn('Lazy')) end)",
    );
    let mut s = session();
    s.load_addon_dir(&main).unwrap();
    s.login().unwrap();
    s.reload().unwrap();
    assert_eq!(s.prints(), ["main", "lazy"]);
}
#[test]
fn world_edits_are_replayable() {
    let s = session();
    s.set_unit("player", unit()).unwrap();
    s.set_money(123).unwrap();
    s.set_bag(0, &[1]).unwrap();
    assert_eq!(
        s.host_commands().len(),
        3,
        "all world edits omitted from replay"
    );
}
#[test]
fn host_fire_retains_types() {
    let s = session();
    s.fire(
        "X",
        vec![
            SimValue::Boolean(true),
            SimValue::Number(3.0),
            SimValue::Nil,
        ],
    )
    .unwrap();
    println!("replay={:?}", s.host_commands());
    assert!(
        !format!("{:?}", s.host_commands()).contains("\"true\""),
        "bool serialized as a string"
    );
}
#[test]
fn onupdate_is_script_trace() {
    let s = session();
    s.eval("CreateFrame('Frame'):SetScript('OnUpdate',function()end)")
        .unwrap();
    s.advance_time(1.0 / 60.0);
    let records = s.trace_since(0);
    assert!(
        records
            .iter()
            .any(|r| r.kind == TraceKind::Script && r.target == "OnUpdate"),
        "{records:?}"
    );
}
#[test]
fn unit_class_contract() {
    let s = session();
    s.set_unit("player", unit()).unwrap();
    s.eval("local name,file,id=UnitClass('player');assert(name=='Mage' and file=='MAGE' and id==8, 'UnitClass returns wrong values')").unwrap();
}
#[test]
fn os_is_not_a_wall_clock_escape() {
    let s = session();
    s.eval("assert(type(os)~='table' or os.time==time, 'real os.time exposed')")
        .unwrap();
}
#[test]
fn table_print_is_deterministic() {
    let s = session();
    s.eval("print(tostring({}))").unwrap();
    assert_eq!(s.prints(), ["table"]);
}
#[test]
fn reload_preserves_world_and_generation() {
    let mut s = session();
    s.set_unit("player", unit()).unwrap();
    s.set_money(123).unwrap();
    s.set_bag(0, &[42]).unwrap();
    s.reload().unwrap();
    s.eval("assert(UnitHealth('player')==50 and GetMoney()==123 and C_Container.GetContainerItemID(0,1)==42)").unwrap();
    assert_eq!(s.generation().0, 1);
}
#[test]
fn coroutine_status_yield_and_error_identity() {
    let s = session();
    s.eval("local parent=coroutine.running();local c=coroutine.create(function() assert(coroutine.status(coroutine.running())=='running');coroutine.yield(1,nil,3);return 4 end);assert(coroutine.status(c)=='suspended');local ok,a,b,d=coroutine.resume(c);assert(ok and a==1 and b==nil and d==3);assert(coroutine.status(c)=='suspended');assert(coroutine.resume(c));assert(coroutine.status(c)=='dead');local e={};local w=coroutine.wrap(function()error(e)end);local ok,err=pcall(w);assert(not ok and err==e)").unwrap();
}
#[test]
fn sim_inversion_existing_surface() {
    let s = Sim::new().unwrap();
    s.load_files(&[("a.lua".into(), "print('file')".into())])
        .unwrap();
    s.load_addon("X", &[]).unwrap();
    s.fire("PLAYER_LOGIN", vec![]).unwrap();
    s.eval("assert(GetTime()==0)").unwrap();
    let _ = s.lua();
    let _ = s.ctx();
    let _ = s.snapshot().unwrap();
    assert_eq!(s.prints(), ["file"]);
}
#[test]
fn random_interval_edge() {
    let s = Session::new(SessionConfig {
        seed: 739806647,
        ..Default::default()
    })
    .unwrap();
    s.eval("local n=math.random();assert(n<1, 'random returned one');local n=math.random(1,6);assert(n>=1 and n<=6)").unwrap();
}
#[test]
fn random_integer_interval_edge() {
    let s = Session::new(SessionConfig {
        seed: 739806647,
        ..Default::default()
    })
    .unwrap();
    s.eval("assert(math.random(1,6)<=6, 'd6 rolled seven')")
        .unwrap();
}
#[test]
fn long_timer_not_spent_by_scheduler() {
    let s = session();
    s.ctx().set_budget(1);
    s.eval("C_Timer.After(10,function() print('done') end)")
        .unwrap();
    s.advance_time(10.0);
    assert_eq!(s.prints(), ["done"], "now={} logs={:?}", s.now(), s.logs());
}
#[test]
fn classic_era_flavour_toc() {
    let r = root("era");
    let d = r.join("EraOnly");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("EraOnly_Vanilla.toc"), "main.lua").unwrap();
    std::fs::write(d.join("main.lua"), "print('era')").unwrap();
    let mut s = Session::new(SessionConfig {
        target: Some("classic-era@1.15.9".into()),
        ..Default::default()
    })
    .unwrap();
    s.load_addon_dir(&d).unwrap();
}
#[test]
fn reload_clock_is_monotonic() {
    let mut s = session();
    s.advance_time(10.0);
    s.reload().unwrap();
    assert_eq!(s.now(), 10.0);
}
#[test]
fn target_change_event() {
    let s = session();
    s.eval("local f=CreateFrame('Frame');f:RegisterEvent('PLAYER_TARGET_CHANGED');f:SetScript('OnEvent',function() print('target changed')end)").unwrap();
    s.set_unit("target", unit()).unwrap();
    assert_eq!(s.prints(), ["target changed"]);
}
#[test]
fn unchanged_world_emits_no_aura_update() {
    let s = session();
    s.set_unit("player", unit()).unwrap();
    s.eval("local f=CreateFrame('Frame');f:RegisterEvent('UNIT_AURA');f:SetScript('OnEvent',function() error('spurious aura') end)").unwrap();
    s.set_unit("player", unit()).unwrap();
}
#[test]
fn login_payloads_and_once() {
    let mut s = session();
    s.eval("local f=CreateFrame('Frame');f:RegisterEvent('PLAYER_ENTERING_WORLD'); f:SetScript('OnEvent',function(_,_,initial,reload) assert(type(initial)=='boolean' and type(reload)=='boolean');assert(initial~=reload);print(initial,reload)end)").unwrap();
    s.login().unwrap();
    s.login().unwrap();
    assert_eq!(s.prints(), ["true\tfalse"]);
}
#[test]
fn scheduler_advances_get_fresh_budget() {
    let s = session();
    s.ctx().set_budget(1);
    s.eval("C_Timer.After(10,function()print('done')end)")
        .unwrap();
    for _ in 0..600 {
        s.advance_time(1.0 / 60.0);
    }
    assert_eq!(s.prints(), ["done"], "now={} logs={:?}", s.now(), s.logs());
}
#[test]
fn two_sessions_have_equal_pointer_text() {
    let a = session();
    let b = session();
    a.eval("print(tostring({}))").unwrap();
    b.eval("print(tostring({}))").unwrap();
    assert_eq!(a.prints(), b.prints());
}
#[test]
fn invalid_idle_next_step_cannot_run_unbudgeted() {
    let s = session();
    s.ctx().set_budget(1);
    s.eval("CreateFrame('Frame'):SetScript('OnUpdate',function()end)")
        .unwrap();
    let result = s.run_until_idle(0.0, 1).unwrap();
    assert!(!result);
}
#[test]
fn saved_data_and_logout_are_isolated() {
    let r = root("isolation");
    let d = addon(
        &r,
        "Saved",
        "## SavedVariables: DB\n## SavedVariablesPerCharacter: CharDB\nmain.lua",
        "local f=CreateFrame('Frame');f:RegisterEvent('ADDON_LOADED');f:RegisterEvent('PLAYER_LOGOUT');f:SetScript('OnEvent',function(_,event) if event=='ADDON_LOADED' then DB=DB or {n=1};CharDB=CharDB or {n=2} else DB.n=DB.n+1;CharDB.n=CharDB.n+1 end end)",
    );
    let mut a = session();
    let mut b = session();
    a.load_addon_dir(&d).unwrap();
    b.load_addon_dir(&d).unwrap();
    a.reload().unwrap();
    a.eval("assert(DB.n==2 and CharDB.n==3)").unwrap();
    b.eval("assert(DB.n==1 and CharDB.n==2)").unwrap();
}

#[test]
fn onupdate_aggregates_per_second() {
    let s = session();
    s.eval("CreateFrame('Frame'):SetScript('OnUpdate',function()end)")
        .unwrap();
    s.advance_time(0.9);
    let r = s.trace_since(0);
    assert_eq!(
        r.iter().filter(|r| r.target.contains("OnUpdate")).count(),
        1
    );
}
#[test]
fn all_flavour_tocs() {
    for (flavour, suffix) in [
        ("retail", "Mainline"),
        ("forever", "Vanilla"),
        ("classic-era", "Vanilla"),
        ("classic-anniversary", "TBC"),
        ("classic-titan", "Wrath"),
        ("classic", "Mists"),
    ] {
        let r = root(flavour);
        let d = r.join("Flavour");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(format!("Flavour_{suffix}.toc")), "main.lua").unwrap();
        std::fs::write(d.join("main.lua"), "").unwrap();
        let mut s = Session::new(SessionConfig {
            target: Some(format!("{flavour}@test")),
            ..Default::default()
        })
        .unwrap();
        s.load_addon_dir(&d).unwrap();
    }
}

#[test]
fn event_snapshots_are_bounded_and_immutable() {
    let s = session();
    s.eval("f=CreateFrame('Frame');f:SetSize(10,20)").unwrap();
    s.fire("MARK", vec![]).unwrap();
    let seq = s
        .trace_since(0)
        .into_iter()
        .find(|r| r.target == "MARK")
        .unwrap()
        .seq;
    let old = s.snapshot_at(seq).expect("event snapshot missing");
    s.eval("f:SetSize(50,60)").unwrap();
    assert_eq!(s.snapshot_at(seq).unwrap(), old);
    for _ in 0..130 {
        s.fire("MARK", vec![]).unwrap();
    }
    assert!(s.snapshot_at(seq).is_none());
}

#[test]
fn coroutine_resumes_share_one_dispatch_budget() {
    let s = session();
    s.ctx().set_budget(1);
    assert!(s.eval("local a=coroutine.create(function() for i=1,20000 do coroutine.yield(i) end end); local b=coroutine.create(function() for i=1,20000 do coroutine.yield(i) end end); for i=1,20000 do coroutine.resume(a);coroutine.resume(b) end").is_err());
}

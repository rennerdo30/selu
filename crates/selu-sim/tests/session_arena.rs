use selu_sim::world::Unit;
use selu_sim::{Generation, Rect, Session, SessionConfig, TextureFill, TraceKind};
#[test]
fn toc_world_migration_trace_and_real_arena_survive_reload() {
    let mut s = Session::new(SessionConfig {
        trace: true,
        ui_size: (800., 600.),
        ..Default::default()
    })
    .unwrap();
    s.load_addon_dir(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/addons/Migration"),
    )
    .unwrap();
    s.login().unwrap();
    let tree = s.snapshot().unwrap();
    assert_eq!(
        selu_sim::frames::arena::store(s.lua())
            .borrow()
            .regions
            .len(),
        tree.regions.len()
    );
    let panel = tree
        .regions
        .iter()
        .find(|r| r.name.as_deref() == Some("MigrationPanel"))
        .unwrap();
    assert_eq!(
        panel.rect,
        Some(Rect {
            left: 300.,
            bottom: 260.,
            width: 200.,
            height: 80.
        })
    );
    assert_eq!(panel.parent, Some(tree.regions[0].id));
    let fill = tree
        .regions
        .iter()
        .find(|r| r.name.as_deref() == Some("MigrationFill"))
        .unwrap();
    assert_eq!(fill.parent, Some(panel.id));
    assert_eq!(fill.rect, panel.rect);
    assert_eq!(fill.texture, Some(TextureFill::Color([0.1, 0.2, 0.3, 1.])));
    s.set_unit(
        "player",
        Unit {
            name: "Tester".into(),
            class: "MAGE".into(),
            level: 80,
            health: 42,
            max_health: 100,
            power: 20,
            max_power: 100,
            auras: vec![],
            combat: false,
        },
    )
    .unwrap();
    let event = s
        .trace_since(0)
        .into_iter()
        .find(|r| r.kind == TraceKind::Event && r.target == "UNIT_HEALTH")
        .unwrap();
    assert_eq!(event.args, ["player"]);
    let mark = s.snapshot_at(event.seq).unwrap();
    assert!(mark.regions.iter().any(|r| r.widget == "FontString"
        && r.text.as_deref() == Some("health: 42")
        && r.rect.is_some()));
    s.eval("MigrationDB.version=0;MigrationDB.migrated=false")
        .unwrap();
    s.reload().unwrap();
    s.eval("assert(MigrationDB.version==1 and MigrationDB.migrated and MigrationDB.health==42);assert(UIParent:GetWidth()==800 and UIParent:GetHeight()==600)").unwrap();
    let tree = s.snapshot().unwrap();
    assert_eq!(tree.generation, Generation(1));
    assert!(
        tree.regions
            .iter()
            .any(|r| r.name.as_deref() == Some("MigrationPanel")
                && r.rect
                    == Some(Rect {
                        left: 300.,
                        bottom: 260.,
                        width: 200.,
                        height: 80.
                    })
                && r.id.generation == 1)
    );
    assert!(
        tree.regions
            .iter()
            .any(|r| r.text.as_deref() == Some("health: 42"))
    );
    assert!(s.trace_since(0).iter().any(|r| r.kind == TraceKind::Event
        && r.target == "ADDON_LOADED"
        && r.args == ["Migration", "false"]));
}
#[test]
fn coroutine_and_arena_callbacks_share_one_outer_instruction_budget() {
    let s = Session::new(Default::default()).unwrap();
    s.ctx().set_budget(1);
    let result=s.eval("local co=coroutine.create(function()local n=0;for i=1,3000 do n=n+i end end);assert(coroutine.resume(co));local f=CreateFrame('Frame');f:SetScript('OnHide',function()local n=0;for i=1,3000 do n=n+i end end);f:Hide()");
    assert!(result.is_err());
    assert!(s.logs().iter().any(|s| s.contains("script ran too long")));
    s.eval("assert(2+2==4)").unwrap();
}
#[test]
fn combat_world_and_lockdown_share_state_across_reload() {
    let mut s = Session::new(Default::default()).unwrap();
    s.set_unit(
        "player",
        Unit {
            name: "Tester".into(),
            class: "MAGE".into(),
            level: 80,
            health: 42,
            max_health: 100,
            power: 20,
            max_power: 100,
            auras: vec![],
            combat: true,
        },
    )
    .unwrap();
    s.eval("assert(InCombatLockdown() and UnitAffectingCombat('player'))")
        .unwrap();
    s.reload().unwrap();
    s.eval("assert(InCombatLockdown() and UnitAffectingCombat('player'))")
        .unwrap();
    s.set_combat(false).unwrap();
    s.eval("assert(not InCombatLockdown() and not UnitAffectingCombat('player'))")
        .unwrap();
}

#[test]
fn arena_callback_errors_keep_deterministic_host_rendering() {
    let a = Session::new(SessionConfig {
        trace: true,
        ..Default::default()
    })
    .unwrap();
    let b = Session::new(SessionConfig {
        trace: true,
        ..Default::default()
    })
    .unwrap();
    let source =
        "local f=CreateFrame('Frame');f:SetScript('OnHide',function()error({})end);f:Hide()";
    let x = a.eval(source).unwrap_err().to_string();
    let y = b.eval(source).unwrap_err().to_string();
    assert_eq!(x, y);
    assert_eq!(a.trace_since(0), b.trace_since(0));
}

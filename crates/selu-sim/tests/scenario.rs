use selu_sim::world::{Scenario, Unit, Vignette};
use selu_sim::{Session, SessionConfig, TraceKind};
fn session() -> Session {
    Session::new(SessionConfig {
        trace: true,
        ..Default::default()
    })
    .unwrap()
}
#[test]
fn player_form_is_scenario_state_and_emits_only_on_transition() {
    let s = session();
    s.eval("assert(GetShapeshiftForm()==0); formChanges=0; local f=CreateFrame('Frame'); f:RegisterEvent('UPDATE_SHAPESHIFT_FORM'); f:SetScript('OnEvent',function() formChanges=formChanges+1 end)").unwrap();
    let mut state = Scenario::default();
    let player = state.units.entry("player".into()).or_default();
    player.class = "DRUID".into();
    player.form = 2;
    state.apply(s.sim()).unwrap();
    s.eval("assert(GetShapeshiftForm()==2 and formChanges==1)")
        .unwrap();
    state.apply(s.sim()).unwrap();
    s.eval("assert(GetShapeshiftForm()==2 and formChanges==1)")
        .unwrap();
    state.units.get_mut("player").unwrap().form = 0;
    state.apply(s.sim()).unwrap();
    s.eval("assert(GetShapeshiftForm()==0 and formChanges==2)")
        .unwrap();
}
#[test]
fn scenario_units_events_and_apis() {
    let s = session();
    let mut state = Scenario::default();
    state.units.insert(
        "target".into(),
        Unit {
            classification: "rare".into(),
            ..Default::default()
        },
    );
    state.units.insert("nameplate1".into(), Unit::default());
    state.apply(s.sim()).unwrap();
    s.eval("assert(UnitClassification('target')=='rare'); assert(UnitHealth('target')==100); assert(UnitExists('target')); assert(UnitGUID('target')=='Creature-0-0-0-0-1-1'); assert(UnitCreatureType('target')=='Beast'); assert(UnitReaction('target','player')==3); assert(UnitCanAttack('player','target')); assert(not UnitIsDead('target')); assert(C_NamePlate.GetNamePlateForUnit('nameplate1'):IsShown()); assert(#C_NamePlate.GetNamePlates()==1)").unwrap();
    for event in [
        "PLAYER_TARGET_CHANGED",
        "UNIT_CLASSIFICATION_CHANGED",
        "UNIT_HEALTH",
        "NAME_PLATE_UNIT_ADDED",
    ] {
        assert!(
            s.trace_since(0)
                .iter()
                .any(|r| r.kind == TraceKind::Event && r.target == event),
            "{event}"
        );
    }
    state.units.get_mut("target").unwrap().health = 0;
    state.units.get_mut("target").unwrap().dead = true;
    state.apply(s.sim()).unwrap();
    s.eval("assert(UnitIsDead('target')); assert(UnitHealth('target')==0)")
        .unwrap();
    state.units.clear();
    state.apply(s.sim()).unwrap();
    s.eval("assert(not UnitExists('target')); assert(C_NamePlate.GetNamePlateForUnit('nameplate1')==nil)").unwrap();
    assert!(
        s.trace_since(0)
            .iter()
            .any(|r| r.target == "NAME_PLATE_UNIT_REMOVED")
    );
}
#[test]
fn vignette_payload_and_removal() {
    let s = session();
    s.eval("events={}; local f=CreateFrame('Frame'); f:RegisterEvent('VIGNETTE_MINIMAP_UPDATED'); f:RegisterEvent('VIGNETTES_UPDATED'); f:SetScript('OnEvent',function(_,e,...) events[#events+1]={e,...} end)").unwrap();
    let mut state = Scenario::default();
    state.vignettes.push(Vignette {
        guid: "Vignette-1".into(),
        object_guid: "Creature-1".into(),
        name: "Rare".into(),
        map: 1,
        x: 0.2,
        y: 0.3,
    });
    state.apply(s.sim()).unwrap();
    s.eval("assert(events[1][1]=='VIGNETTE_MINIMAP_UPDATED' and events[1][2]=='Vignette-1' and events[1][3]==true); assert(events[2][1]=='VIGNETTES_UPDATED' and #events[2]==1); assert(C_VignetteInfo.GetVignettes()[1]=='Vignette-1'); local v=C_VignetteInfo.GetVignetteInfo('Vignette-1'); assert(v.name=='Rare' and v.objectGUID=='Creature-1'); local p=C_VignetteInfo.GetVignettePosition('Vignette-1',1); assert(p.x==0.2 and p.y==0.3); assert(C_VignetteInfo.GetVignettePosition('Vignette-1',2)==nil)").unwrap();
    state.apply(s.sim()).unwrap();
    s.eval("assert(#events==2)").unwrap();
    state.vignettes.clear();
    state.apply(s.sim()).unwrap();
    s.eval("assert(events[3][2]=='Vignette-1' and events[3][3]==false); assert(C_VignetteInfo.GetVignetteInfo('Vignette-1')==nil)").unwrap();
}
#[test]
fn other_state_event_payloads() {
    let s = session();
    s.eval("events={};local f=CreateFrame('Frame'); for _,e in ipairs({'PLAYER_REGEN_DISABLED','PLAYER_REGEN_ENABLED','PLAYER_MONEY','BAG_UPDATE','BAG_UPDATE_DELAYED','ZONE_CHANGED','ZONE_CHANGED_NEW_AREA','ZONE_CHANGED_INDOORS'}) do f:RegisterEvent(e) end; f:SetScript('OnEvent',function(_,e,...) events[e]={...} end)").unwrap();
    let mut state = Scenario {
        combat: true,
        money: 123,
        zone: "Test zone".into(),
        subzone: "Test subzone".into(),
        map: 42,
        ..Default::default()
    };
    state.bags.insert(0, vec![6948]);
    state.apply(s.sim()).unwrap();
    s.eval("assert(InCombatLockdown()); assert(GetMoney()==123); assert(C_Container.GetContainerItemID(0,1)==6948); assert(C_Container.GetContainerNumSlots(0)==1); assert(GetZoneText()=='Test zone' and GetSubZoneText()=='Test subzone'); assert(C_Map.GetBestMapForUnit('player')==42); assert(events.BAG_UPDATE[1]==0); for _,e in ipairs({'PLAYER_REGEN_DISABLED','PLAYER_MONEY','BAG_UPDATE_DELAYED','ZONE_CHANGED','ZONE_CHANGED_NEW_AREA','ZONE_CHANGED_INDOORS'}) do assert(events[e] and #events[e]==0,e) end").unwrap();
    state.combat = false;
    state.apply(s.sim()).unwrap();
    s.eval("assert(not InCombatLockdown()); assert(events.PLAYER_REGEN_ENABLED)")
        .unwrap();
}

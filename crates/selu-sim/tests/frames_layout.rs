use selu_sim::Sim;
#[test]
fn hand_worked_layout_and_input() {
    let sim = Sim::new().unwrap();
    sim.eval(include_str!("addons/Layout/Layout.lua")).unwrap();
    let snap = sim.snapshot().unwrap();
    let r = snap.regions[1].rect.unwrap();
    assert_eq!(
        (r.left, r.bottom, r.width, r.height),
        (10.0, 20.0, 200.0, 100.0)
    );
    assert_eq!(snap.regions[2].rect, Some(r));
    sim.click(20.0, 30.0, "LeftButton").unwrap();
    assert_eq!(sim.prints(), ["clicked"]);
    sim.eval("LayoutFrame:SetScale(2); LayoutFrame:SetAlpha(.5); LayoutFrame:Hide()")
        .unwrap();
    let snap = sim.snapshot().unwrap();
    let r = snap.regions[2].rect.unwrap();
    assert_eq!(
        (r.left, r.bottom, r.width, r.height),
        (20.0, 40.0, 400.0, 200.0)
    );
    assert!(!snap.regions[2].visible);
    assert_eq!(snap.regions[2].alpha, 0.5);
}
#[test]
fn stretch_cycle_fields_and_focus() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
 local f=CreateFrame('Frame','Stretch');f:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',10,20);f:SetPoint('TOPRIGHT',UIParent,'TOPRIGHT',-30,-40)
 assert(f.missing==nil);f.myField=7;assert(f.myField==7)
 local e=CreateFrame('EditBox','Editor',f);e:SetFocus()
 local a=CreateFrame('Frame','CycleA');local b=CreateFrame('Frame','CycleB');a:SetPoint('CENTER',b);b:SetPoint('CENTER',a)
 "#).unwrap();
    let snap = sim.snapshot().unwrap();
    let r = snap.regions[1].rect.unwrap();
    assert_eq!(
        (r.left, r.bottom, r.width, r.height),
        (10.0, 20.0, 1880.0, 1020.0)
    );
    assert!(snap.regions[3].rect.is_none());
    assert!(sim.logs().iter().any(|s| s.contains("cycle")));
    sim.type_text("hello").unwrap();
    sim.eval("assert(Editor:GetText()=='hello')").unwrap();
}
#[test]
fn callbacks_are_deferred_and_parent_visibility_propagates() {
    let sim = Sim::new().unwrap();
    sim.eval(
        r#"
 local p=CreateFrame('Frame','Parent');local c=CreateFrame('Frame',nil,p)
 c:SetScript('OnHide',function() print('hide') end)
 p:Hide();print('after')
 "#,
    )
    .unwrap();
    assert_eq!(sim.prints(), ["after", "hide"]);
}
#[test]
fn unanchored_size_change_is_deferred() {
    let sim = Sim::new().unwrap();
    sim.eval("local f=CreateFrame('Frame'); f:SetScript('OnSizeChanged',function(_,w,h) print(w,h) end); f:SetSize(80,30);print('after')").unwrap();
    assert_eq!(sim.prints(), ["after", "80\t30"]);
}
#[test]
fn anchor_cycles_do_not_change_shown_visibility() {
    let sim = Sim::new().unwrap();
    sim.eval("local a=CreateFrame('Frame','VisibleCycleA'); local b=CreateFrame('Frame','VisibleCycleB');a:SetPoint('CENTER',b);b:SetPoint('CENTER',a);assert(a:IsShown());assert(a:IsVisible())").unwrap();
    assert!(sim.snapshot().unwrap().regions[1].rect.is_none());
}

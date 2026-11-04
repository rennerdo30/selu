use selu_sim::{Session, SessionConfig, Sim};

fn lua(code: &str) {
    let sim = Sim::new().unwrap();
    sim.eval(code).unwrap();
}

#[test]
fn correct_identity_fields_override_and_scale() {
    lua(r#"
local p=CreateFrame('Frame','Parent');p:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',10,20);p:SetSize(100,50);p:SetScale(2);p:SetAlpha(.5)
local c=CreateFrame('Frame','$parentChild',p);c:SetAllPoints(p);c:SetAlpha(.5)
assert(ParentChild==c and c:GetParent()==p and select(2,c:GetPoint())==p)
c.field=42;c.GetName=function() return 'override' end;assert(c:GetName()=='override' and c.field==42 and c.absent==nil)
assert(c:GetEffectiveScale()==2 and c:GetEffectiveAlpha()==.25)
local x,y,w,h=c:GetRect();assert(x==10 and y==20 and w==100 and h==50)
p:Hide();assert(c:IsShown() and not c:IsVisible());p:Show();assert(c:IsVisible())
c:ClearAllPoints();assert(c:GetRect()==nil)
"#);
}

#[test]
fn parent_can_anchor_to_independently_anchored_child() {
    lua(r#"
local p=CreateFrame('Frame');local c=CreateFrame('Frame',nil,p)
c:SetSize(10,10);c:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',100,100)
p:SetSize(20,20);p:SetPoint('BOTTOMLEFT',c,'BOTTOMLEFT')
local x,y,w,h=p:GetRect();assert(x==100 and y==100 and w==20 and h==20, 'valid parent/child anchors incorrectly unresolved')
"#);
}

#[test]
fn intrinsic_fontstring_dimensions() {
    lua(r#"
local f=UIParent:CreateFontString();f:SetFont('Fonts/FRIZQT__.TTF',12);f:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',0,0);f:SetText('abcd')
local x,y,w,h=f:GetRect();assert(w==24 and h==12,'expected metric 24x12, got '..w..'x'..h)
"#);
}

#[test]
fn wrapping_uses_resolved_anchor_width() {
    lua(r#"
local f=UIParent:CreateFontString();f:SetFont('Fonts/FRIZQT__.TTF',12);f:SetText('abcdefgh')
f:SetPoint('TOPLEFT',UIParent,'TOPLEFT',0,0);f:SetPoint('TOPRIGHT',UIParent,'TOPLEFT',24,0)
assert(f:GetWidth()==24);assert(f:GetStringHeight()==24,'expected two lines, got '..f:GetStringHeight())
"#);
}

#[test]
fn timer_callbacks_flush_before_next_timer() {
    let sim = Sim::new().unwrap();
    sim.eval(
        r#"
local f=CreateFrame('Frame');local sized=false
f:SetScript('OnSizeChanged',function() sized=true end)
C_Timer.After(.01,function() f:SetSize(80,30) end)
C_Timer.After(.02,function() print(sized) end)
"#,
    )
    .unwrap();
    sim.advance_time(0.03);
    assert_eq!(sim.prints(), ["true"]);
}

#[test]
fn scale_does_not_report_unchanged_logical_size() {
    let sim = Sim::new().unwrap();
    sim.eval("f=CreateFrame('Frame');f:SetPoint('CENTER');f:SetSize(20,30)")
        .unwrap();
    sim.eval("f:SetScript('OnSizeChanged',function(_,w,h) print(w,h) end);f:SetScale(2)")
        .unwrap();
    assert!(sim.prints().is_empty(), "{:?}", sim.prints());
}

#[test]
fn snapshot_does_not_run_addon_code() {
    let sim = Sim::new().unwrap();
    sim.eval("local f=CreateFrame('Frame');f:SetSize(10,20);f:SetPoint('CENTER');f.GetRect=function() error('override called') end;f.GetScript=function() error('override called') end").unwrap();
    assert!(sim.snapshot().unwrap().regions[1].rect.is_some());
    assert!(sim.prints().is_empty());
}

#[test]
fn input_stops_after_mousedown_hides_target() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
local b=CreateFrame('Button');b:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',0,0);b:SetSize(100,100)
b:SetScript('OnMouseDown',function(self) self:Hide() end);b:SetScript('OnClick',function() print('clicked hidden target') end)
"#).unwrap();
    sim.click(10., 10., "LeftButton").unwrap();
    assert!(sim.prints().is_empty(), "{:?}", sim.prints());
}

#[test]
fn drag_start_observes_start_cursor_and_button() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
local b=CreateFrame('Button');b:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',0,0);b:SetSize(100,100);b:RegisterForDrag('LeftButton')
b:SetScript('OnDragStart',function(self,button) local x,y=GetCursorPosition();print(x,y,button);self:StartMoving() end)
b:SetScript('OnDragStop',function(self) self:StopMovingOrSizing() end)
"#).unwrap();
    sim.drag((10., 20.), (30., 40.)).unwrap();
    assert_eq!(sim.prints(), ["10\t20\tLeftButton"]);
}

#[test]
fn engine_onupdate_does_not_call_overridden_isvisible() {
    let sim = Sim::new().unwrap();
    sim.eval("local f=CreateFrame('Frame');f.IsVisible=function() return false end;f:SetScript('OnUpdate',function() print('update') end)").unwrap();
    sim.advance_time(1. / 60.);
    assert_eq!(sim.prints(), ["update"]);
}

#[test]
fn checkbutton_inherits_button_object_type() {
    lua("assert(CreateFrame('CheckButton'):IsObjectType('Button'))");
}

#[test]
fn slider_emits_value_change() {
    let sim = Sim::new().unwrap();
    sim.eval("local s=CreateFrame('Slider');s:SetMinMaxValues(0,10);s:SetScript('OnValueChanged',function(_,v) print(v) end);s:SetValue(5)").unwrap();
    assert_eq!(sim.prints(), ["5"]);
}

#[test]
fn settexture_nil_clears_instead_of_setting_file_zero() {
    lua(
        "local t=UIParent:CreateTexture();t:SetTexture('icon');t:SetTexture(nil);assert(t:GetTexture()==nil,'got '..tostring(t:GetTexture()))",
    );
}

#[test]
fn session_ui_size_controls_geometry() {
    let s = Session::new(SessionConfig {
        ui_size: (800., 600.),
        ..SessionConfig::default()
    })
    .unwrap();
    let snap = s.snapshot().unwrap();
    let r = snap.regions[0].rect.unwrap();
    assert_eq!((r.width, r.height), (snap.ui_width, snap.ui_height));
}

#[test]
fn regions_track_parent_draw_order() {
    let sim = Sim::new().unwrap();
    sim.eval("local p=CreateFrame('Frame');local t=p:CreateTexture();p:SetFrameStrata('HIGH');p:SetFrameLevel(10)").unwrap();
    let s = sim.snapshot().unwrap();
    assert_eq!(s.regions[2].strata, s.regions[1].strata);
    assert_eq!(s.regions[2].level, s.regions[1].level);
}

#[test]
fn protected_flag_is_honored_for_package_g() {
    lua("local f=CreateFrame('Frame');f.__protected=true;assert(f:IsProtected())");
}

#[test]
fn nonstring_addon_field_reads_nil() {
    lua("local f=CreateFrame('Frame');assert(f[123]==nil)");
}

#[test]
fn titlebar_drag_moves_parent_marked_by_startmoving() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
local p=CreateFrame('Frame','Panel');p:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',10,20);p:SetSize(100,100)
local bar=CreateFrame('Button',nil,p);bar:SetAllPoints(p);bar:RegisterForDrag('LeftButton')
bar:SetScript('OnDragStart',function(self) self:GetParent():StartMoving() end)
bar:SetScript('OnDragStop',function(self) self:GetParent():StopMovingOrSizing() end)
"#).unwrap();
    sim.drag((20., 30.), (40., 50.)).unwrap();
    let s = sim.snapshot().unwrap();
    let r = s.regions[1].rect.unwrap();
    assert_eq!((r.left, r.bottom), (30., 40.));
}

#[test]
fn stretched_scale_change_reports_logical_resize() {
    let sim = Sim::new().unwrap();
    sim.eval("f=CreateFrame('Frame');f:SetAllPoints(UIParent)")
        .unwrap();
    sim.eval("f:SetScript('OnSizeChanged',function(_,w,h) print(w,h) end);f:SetScale(2)")
        .unwrap();
    assert_eq!(sim.prints(), ["960\t540"]);
}

#[test]
fn edit_focus_events_and_hide_release_focus() {
    let sim = Sim::new().unwrap();
    sim.eval(
        r#"
local e=CreateFrame('EditBox','Editor')
e:SetScript('OnEditFocusGained',function() print('focus') end)
e:SetScript('OnEditFocusLost',function() print('blur') end)
e:SetFocus();e:Hide()
"#,
    )
    .unwrap();
    sim.type_text("typed while hidden").unwrap();
    assert_eq!(sim.prints(), ["focus", "blur"]);
    sim.eval("assert(not Editor:HasFocus())").unwrap();
}

#[test]
fn widget_has_script_uses_documented_types() {
    lua(
        "local f=CreateFrame('Frame');local b=CreateFrame('Button');assert(not f:HasScript('OnClick'));assert(b:HasScript('PreClick'))",
    );
}

#[test]
fn modelled_setters_return_documented_success() {
    lua(
        "local t=UIParent:CreateTexture();assert(t:SetTexture('icon')==true);local f=UIParent:CreateFontString();assert(f:SetFont('Fonts/FRIZQT__.TTF',12)==true)",
    );
}
#[test]
fn callback_chain_is_bounded_as_one_dispatch() {
    let s = Sim::new().unwrap();
    s.ctx().set_budget(10);
    s.eval("local f=CreateFrame('Frame');local count=0;f:SetScript('OnSizeChanged',function() count=count+1;if count<20000 then f:SetWidth(count%2+1) end end);f:SetWidth(1)").unwrap();
    assert!(
        s.logs()
            .iter()
            .any(|l| l.contains("budget") || l.contains("script ran too long")),
        "{:?}",
        s.logs()
    );
}
#[test]
fn font_roundtrips_and_texture_layer_sublevel() {
    lua(
        "local f=UIParent:CreateFontString();f:SetFont('custom',15,'OUTLINE');local p,s,o=f:GetFont();assert(p=='custom' and s==15 and o=='OUTLINE');local t=UIParent:CreateTexture(nil,'OVERLAY',nil,3);local l,n=t:GetDrawLayer();assert(l=='OVERLAY' and n==3)",
    );
}
#[test]
fn checkbutton_toggles_after_down() {
    let s = Sim::new().unwrap();
    s.eval("local b=CreateFrame('CheckButton');b:SetAllPoints(UIParent);b:SetScript('OnMouseDown',function(self) print(self:GetChecked()) end);b:SetScript('OnClick',function(self) print(self:GetChecked()) end)").unwrap();
    s.click(1., 1., "LeftButton").unwrap();
    assert_eq!(s.prints(), ["false", "true"]);
}

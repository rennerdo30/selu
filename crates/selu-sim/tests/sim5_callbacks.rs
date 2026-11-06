use selu_sim::Sim;
#[test]
fn api_callbacks_finish_before_return() {
    let sim = Sim::new().unwrap();
    sim.eval(
        r#"local s=CreateFrame('Slider'); local count=0
 s:SetScript('OnValueChanged',function() count=count+1 end)
 s:SetValue(.5); assert(count==1, 'SetValue callback deferred')
 local p=CreateFrame('Frame');local c=CreateFrame('Frame',nil,p)
 p:Hide();local shown=false;c:SetScript('OnShow',function() shown=true end)
 p:Show();assert(shown, 'child OnShow deferred')
 local e=CreateFrame('EditBox'); local changed=false
 e:SetScript('OnTextChanged',function() changed=true end)
 e:SetText('hello');assert(changed, 'OnTextChanged deferred')
 local gained=false;e:SetScript('OnEditFocusGained',function() gained=true end)
 e:SetFocus();assert(gained, 'focus deferred')"#,
    )
    .unwrap();
}
#[test]
fn unbounded_width_ignores_constraints() {
    let sim = Sim::new().unwrap();
    sim.eval(
        r#"local f=UIParent:CreateFontString(); f:SetText('a long string for measurement')
 local w=f:GetUnboundedStringWidth();assert(type(w)=='number' and w>20)
 f:SetWidth(20);f:SetMaxLines(1);assert(f:GetUnboundedStringWidth()==w)"#,
    )
    .unwrap();
}

#[test]
fn moving_flags_and_clamped_drag() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"w=CreateFrame('Frame');w:SetSize(100,100);w:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',0,0);w:EnableMouse(true)
 w:SetMovable(true);w:SetClampedToScreen(true);assert(w:IsMovable() and w:IsClampedToScreen())
 w:RegisterForDrag('LeftButton');w:SetScript('OnDragStart',function(self) self:StartMoving() end)
 w:SetScript('OnDragStop',function(self) self:StopMovingOrSizing() end)"#).unwrap();
    sim.drag((20., 20.), (-200., -200.)).unwrap();
    sim.eval("assert(w:GetLeft()==0 and w:GetBottom()==0)")
        .unwrap();
}
#[test]
fn edit_selection_replaces_text() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"e=CreateFrame('EditBox');e:SetAutoFocus(false);assert(not e:IsAutoFocus());e:SetText('abcdef');e:SetFocus();e:SetCursorPosition(3);assert(e:GetCursorPosition()==3);e:HighlightText(1,4)"#).unwrap();
    sim.type_text("X").unwrap();
    sim.eval("assert(e:GetText()=='aXef' and e:GetCursorPosition()==2)")
        .unwrap();
}
#[test]
fn disabled_hover_is_opt_in() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"b=CreateFrame('Button');b:SetSize(100,100);b:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',0,0);b:Disable();n=0;b:SetScript('OnEnter',function() n=n+1 end)"#).unwrap();
    sim.hover(10., 10.).unwrap();
    sim.eval("assert(n==0);b:SetMotionScriptsWhileDisabled(true)")
        .unwrap();
    sim.hover(10., 10.).unwrap();
    sim.eval("assert(n==1)").unwrap();
}
#[test]
fn tooltip_lines_are_real_font_strings() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"local t=CreateFrame('GameTooltip','SIM5Tooltip');t:SetOwner(UIParent,'ANCHOR_RIGHT');t:SetText('Title',.2,.3,.4);t:AddLine('Wrapped line',1,1,1,true);t:AddDoubleLine('Left','Right');assert(t:NumLines()==3);assert(t:GetText()=='Title');assert(SIM5TooltipTextRight3:GetText()=='Right');t:Show();assert(t:IsVisible());t:ClearLines();assert(t:NumLines()==0);t:Hide()"#).unwrap();
}

#[test]
fn drag_requires_registration_and_movable() {
    // Blizzard_SharedXML/Mainline/SharedUIPanelTemplates.lua:1526 registers drag buttons.
    // SimpleFrameAPIDocumentation.lua declares SetMovable; StartMoving requires the flag.
    let sim = Sim::new().unwrap();
    sim.eval("w=CreateFrame('Frame');w:SetSize(100,100);w:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',0,0);w:EnableMouse(true);n=0;w:SetScript('OnDragStart',function(self) n=n+1;self:StartMoving() end)").unwrap();
    sim.drag((10., 10.), (20., 20.)).unwrap();
    sim.eval("assert(n==0 and w:GetLeft()==0);w:RegisterForDrag('LeftButton')")
        .unwrap();
    sim.drag((10., 10.), (20., 20.)).unwrap();
    sim.eval("assert(n==1 and w:GetLeft()==0)").unwrap();
}

#[test]
fn synchronous_recursion_reports_and_recovers() {
    let sim = Sim::new().unwrap();
    let result=sim.eval("s=CreateFrame('Slider');s:SetScript('OnValueChanged',function(self,v) self:SetValue(1-v) end);s:SetValue(.25)");
    assert!(result.is_err());
    assert!(sim.logs().iter().any(|s| s.contains("nested too deeply")));
    assert!(!sim.sim().ctx().runtime_failures().is_empty());
    sim.eval("s:SetScript('OnValueChanged',nil);s:RegisterEvent('RECOVER');s:SetScript('OnEvent',function() recovered=true end)").unwrap();
    sim.fire("RECOVER", vec![]).unwrap();
    sim.eval("assert(recovered)").unwrap();
}
#[test]
fn autofocus_policy_and_edit_snapshot() {
    let sim = Sim::new().unwrap();
    sim.eval("e=CreateFrame('EditBox');e:Hide();e:SetAutoFocus(false);e:Show();assert(not e:HasFocus());e:Hide();e:SetAutoFocus(true);e:Show();assert(e:HasFocus());e:SetText('hello');e:SetCursorPosition(3);e:HighlightText(1,3)").unwrap();
    let snap = sim.snapshot().unwrap();
    let e = snap.regions.iter().find(|r| r.widget == "EditBox").unwrap();
    assert_eq!(e.visual.edit_range, Some((1, 3)));
    assert!(e.visual.edit_caret.is_some());
    assert_eq!(e.visual.edit_selection.len(), 1);
}

#[test]
fn multiline_selection_uses_shared_layout_rows() {
    let sim = Sim::new().unwrap();
    sim.eval("e=CreateFrame('EditBox');e:SetSize(35,100);e:SetPoint('CENTER');e:SetMultiLine(true);e:SetText('first line and another line');e:SetFocus();e:HighlightText(0,-1)").unwrap();
    let snap = sim.snapshot().unwrap();
    let e = snap.regions.iter().find(|r| r.widget == "EditBox").unwrap();
    assert!(
        e.visual.edit_selection.len() > 1,
        "selection must cover wrapped rows"
    );
    assert!(
        e.visual
            .edit_selection
            .windows(2)
            .all(|p| p[1][1] > p[0][1])
    );
}

#[test]
fn api_scripts_do_not_drain_deferred_host_work() {
    let sim = Sim::new().unwrap();
    sim.sim()
        .ctx()
        .queue(Box::new(|lua| lua.globals().set("deferred", true)));
    sim.eval("local s=CreateFrame('Slider');s:SetValue(.5);assert(deferred==nil)")
        .unwrap();
    sim.eval("assert(deferred==true)").unwrap();
}
#[test]
fn enabled_text_and_focus_transitions_complete_inside_the_call() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"local b=CreateFrame('Button');local n=0
 b:SetScript('OnDisable',function() n=n+1 end);b:Disable();assert(n==1);b:Disable();assert(n==1)
 b:SetScript('OnEnable',function() n=n+1 end);b:Enable();assert(n==2)
 local e=CreateFrame('EditBox');local set=false;local changed=false;local lost=false
 e:SetScript('OnTextSet',function() set=true end);e:SetScript('OnTextChanged',function() assert(set);changed=true end)
 e:SetText('new');assert(set and changed)
 e:SetFocus();e:SetScript('OnEditFocusLost',function() lost=true end);e:ClearFocus();assert(lost)
 local s=CreateFrame('Slider');local range=false;s:SetScript('OnMinMaxChanged',function(_,lo,hi) range=lo==2 and hi==4 end)
 s:SetMinMaxValues(2,4);assert(range)"#).unwrap();
}

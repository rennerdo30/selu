use selu_sim::Sim;
#[test]
fn native_map_widgets_keep_configuration_without_painting_fake_map_data() {
    let sim = Sim::new().unwrap();
    sim.eval("local blob=CreateFrame('QuestPOIFrame');blob:SetMapID(37);assert(blob:GetMapID()==37);blob:SetFillTexture('self-written-fixture');blob:DrawNone();local frame=CreateFrame('Frame');assert(frame.GetMapID==nil);local units=CreateFrame('UnitPositionFrame');units:SetUiMapID(37);assert(units:GetUiMapID()==37);units:SetPlayerPingScale(2);assert(units:GetPlayerPingScale()==2);units:SetPlayerPingTexture(0,'self-written-fixture',16,16);units:ClearUnits();units:AddUnit('player','self-written-fixture',16,16);units:FinalizeUnits();assert(units:GetMouseOverUnits()==nil)").unwrap();
}
#[test]
fn masks_attach_once_remove_and_reach_the_renderer() {
    let sim = Sim::new().unwrap();
    sim.eval("local t=UIParent:CreateTexture('MaskedPaint');local m=UIParent:CreateMaskTexture('PaintMask'); t:AddMaskTexture(m);t:AddMaskTexture(m)").unwrap();
    let snapshot = sim.snapshot().unwrap();
    let texture = snapshot
        .regions
        .iter()
        .find(|r| r.name.as_deref() == Some("MaskedPaint"))
        .unwrap();
    let mask = snapshot
        .regions
        .iter()
        .find(|r| r.name.as_deref() == Some("PaintMask"))
        .unwrap();
    assert_eq!(texture.visual.masks, vec![mask.id.index as usize]);
    sim.eval("MaskedPaint:RemoveMaskTexture(PaintMask)")
        .unwrap();
    assert!(
        sim.snapshot()
            .unwrap()
            .regions
            .iter()
            .find(|r| r.name.as_deref() == Some("MaskedPaint"))
            .unwrap()
            .visual
            .masks
            .is_empty()
    );
}
#[test]
fn colour_getters_and_inherited_font() {
    let sim = Sim::new().unwrap();
    sim.eval(
        r#"local f=CreateFont('SIM4Font'); f:SetTextColor(.38,.22,.06,.75)
 local t=UIParent:CreateFontString();t:SetFontObject(f)
 local r,g,b,a=t:GetTextColor(); assert(r==.38 and g==.22 and b==.06 and a==.75)
 t:SetShadowColor(.1,.2,.3,.5); r,g,b,a=t:GetShadowColor();assert(math.abs(r-.1)<1e-6 and a==.5)
 t:SetVertexColor(.2,.3,.4,.5);r,g,b,a=t:GetVertexColor();assert(math.abs(b-.4)<1e-6 and a==.5)
 t:SetAlpha(.25);assert(t:GetAlpha()==.25)"#,
    )
    .unwrap();
}
#[test]
fn keyboard_and_wheel_state() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"local f=CreateFrame('Button');f:EnableKeyboard(true);assert(f:IsKeyboardEnabled());f:SetPropagateKeyboardInput(true);assert(f:GetPropagateKeyboardInput());f:EnableKeyboard(false);assert(not f:IsKeyboardEnabled());f:EnableMouseWheel(true);assert(f:IsMouseWheelEnabled());f:EnableMouseWheel(false);assert(not f:IsMouseWheelEnabled())"#).unwrap();
}
#[test]
fn slider_configuration() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"local s=CreateFrame('Slider');s:SetMinMaxValues(0,10);s:SetValue(20);assert(s:GetValue()==10);s:SetMinMaxValues(0,5);assert(s:GetValue()==5);s:SetOrientation('HORIZONTAL');assert(s:GetOrientation()=='HORIZONTAL');s:SetValueStep(2);assert(s:GetValueStep()==2);s:SetObeyStepOnDrag(true);assert(s:GetObeyStepOnDrag())"#).unwrap();
}
#[test]
fn thumb_and_tiling_state() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"local s=CreateFrame('Slider');s:SetSize(100,20);s:SetPoint('CENTER');s:SetOrientation('HORIZONTAL');s:SetThumbTexture('Interface/Buttons/WHITE8X8');assert(s:GetThumbTexture():GetObjectType()=='Texture');local t=UIParent:CreateTexture();t:SetTexture('missing','MIRROR','REPEAT');t:SetHorizTile(true);t:SetVertTile(true);assert(t:GetHorizTile() and t:GetVertTile())"#).unwrap();
    let snapshot = sim.snapshot().unwrap();
    let texture = snapshot.regions.last().unwrap();
    assert_eq!(texture.visual.tile, [true, true]);
    assert_eq!(texture.visual.wrap_modes, ["MIRROR", "REPEAT"]);
}
#[test]
fn slider_drag_changes_value_as_user() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"s=CreateFrame('Slider');s:SetSize(100,20);s:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',0,0);s:SetOrientation('HORIZONTAL');s:SetMinMaxValues(0,10);s:SetValueStep(2);s:SetObeyStepOnDrag(true);s:SetScript('OnValueChanged',function(_,v,user) lastUser=user end)"#).unwrap();
    sim.drag((10., 10.), (75., 10.)).unwrap();
    sim.eval("assert(s:GetValue()==8 and lastUser==true)")
        .unwrap();
    sim.eval("s:Disable()").unwrap();
    sim.drag((10., 10.), (20., 10.)).unwrap();
    sim.eval("assert(s:GetValue()==8)").unwrap();
}
#[test]
fn wheel_policy_and_keyboard_disable() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"s=CreateFrame('Slider');s:SetSize(100,20);s:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',0,0);s:SetMinMaxValues(0,10);s:EnableMouseWheel(true);s:SetScript('OnMouseWheel',function(self,d) self:SetValue(self:GetValue()+d,true) end);s:EnableKeyboard(true);s:SetScript('OnKeyDown',function() keys=(keys or 0)+1 end)"#).unwrap();
    sim.wheel(10., 10., 1.).unwrap();
    sim.key("A", true).unwrap();
    sim.eval("assert(s:GetValue()==1 and keys==1);s:Disable()")
        .unwrap();
    sim.wheel(10., 10., 1.).unwrap();
    sim.key("A", true).unwrap();
    sim.eval("assert(s:GetValue()==1 and keys==1)").unwrap();
}
#[test]
fn vertical_thumb_minimum_is_at_top() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"local s=CreateFrame('Slider');s:SetSize(20,100);s:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',0,0);local t=s:CreateTexture();t:SetSize(20,20);s:SetThumbTexture(t);s:SetMinMaxValues(0,10);s:SetValue(0);assert(t:GetBottom()==80);s:SetValue(10);assert(t:GetBottom()==0)"#).unwrap();
}

#[test]
fn idle_and_paused_animation_groups_do_not_receive_ticks() {
    let sim = Sim::new().unwrap();
    sim.eval("f=CreateFrame('Frame');g=f:CreateAnimationGroup();a=g:CreateAnimation('Alpha');a:SetDuration(1);a:SetFromAlpha(0);a:SetToAlpha(1);before=__selu_sim_counts.updates").unwrap();
    sim.advance_time(1.0 / 60.0);
    sim.eval("assert(__selu_sim_counts.updates==before);g:Play()")
        .unwrap();
    sim.advance_time(1.0 / 60.0);
    sim.eval(
        "assert(__selu_sim_counts.updates==before+1);g:Pause();before=__selu_sim_counts.updates",
    )
    .unwrap();
    sim.advance_time(1.0 / 60.0);
    sim.eval("assert(__selu_sim_counts.updates==before);g:Play()")
        .unwrap();
    sim.advance_time(1.0 / 60.0);
    sim.eval(
        "assert(__selu_sim_counts.updates==before+1);g:Stop();before=__selu_sim_counts.updates",
    )
    .unwrap();
    sim.advance_time(1.0 / 60.0);
    sim.eval("assert(__selu_sim_counts.updates==before)")
        .unwrap();
}

#[test]
fn click_runs_the_click_handlers_synchronously_while_enabled() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
        order = {}
        b = CreateFrame("Button"); b:Hide()
        b:SetScript("PreClick", function(_, button, down) order[#order + 1] = "pre " .. button .. " " .. tostring(down) end)
        b:SetScript("OnClick", function(_, button) order[#order + 1] = "click " .. button end)
        b:SetScript("PostClick", function() order[#order + 1] = "post" end)
        b:Click()
        assert(table.concat(order, ",") == "pre LeftButton false,click LeftButton,post", table.concat(order, ","))
        b:Disable(); b:Click("RightButton"); assert(#order == 3)
        b:Enable(); b:Click("RightButton"); assert(order[5] == "click RightButton")
        c = CreateFrame("CheckButton"); c:Click(); assert(c:GetChecked() == true)
        f = CreateFrame("Frame"); assert(f.Click == nil)
    "#).unwrap();
}

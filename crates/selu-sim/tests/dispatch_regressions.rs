use selu_sim::Sim;
use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn visibility_pingpong_is_bounded_in_child() {
    if std::env::var_os("SELU_CALLBACK_CHILD").is_some() {
        let s = Sim::new().unwrap();
        s.ctx().set_budget(100000);
        let result=s.eval("local f=CreateFrame('Frame');local count=0;f:SetScript('OnShow',function(self) count=count+1;if count<500 then self:Hide() end end);f:SetScript('OnHide',function(self) count=count+1;if count<500 then self:Show() end end);f:Hide()");
        assert!(result.is_err());
        assert!(
            s.logs()
                .iter()
                .any(|l| l.contains("error: script callbacks nested too deeply")),
            "{:?}",
            s.logs()
        );
        s.eval("assert(1+1==2)").unwrap();
        return;
    }
    let receipt = std::env::temp_dir().join("selu-S1-visibility-child-pids.log");
    let stderr = std::env::temp_dir().join(format!(
        "selu-S1-visibility-child-{}.err",
        std::process::id()
    ));
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "visibility_pingpong_is_bounded_in_child",
            "--nocapture",
        ])
        .env("SELU_CALLBACK_CHILD", "1")
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    eprintln!("callback test child PID {}", child.id());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(receipt)
    {
        let _ = writeln!(
            f,
            "parent {}, child {}, stderr {}",
            std::process::id(),
            child.id(),
            stderr.display()
        );
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "child exit {status}: {}",
                std::fs::read_to_string(stderr).unwrap()
            );
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("callback child timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn flat_visibility_callbacks_share_outer_budget() {
    let s = Sim::new().unwrap();
    s.ctx().set_budget(100000);
    let result=s.eval("f=CreateFrame('Frame');count=0;f:SetScript('OnShow',function() count=count+1 end);f:SetScript('OnHide',function() count=count+1 end);for i=1,6000 do f:Hide();f:Show() end");
    let count: i64 = s.lua().globals().get("count").unwrap();
    assert!(
        result.is_err() && count <= 10000 && s.logs().iter().any(|l| l.contains("budget")),
        "count={count}, logs={:?}",
        s.logs()
    );
    s.eval("assert(1+1==2)").unwrap();
}
#[test]
fn event_subscribers_flush_between_handlers() {
    let s = Sim::new().unwrap();
    s.eval("sized=false;a=CreateFrame('Frame');a:SetPoint('CENTER');a:SetSize(10,10);b=CreateFrame('Frame');a:RegisterEvent('TEST');b:RegisterEvent('TEST');a:SetScript('OnSizeChanged',function() sized=true end);a:SetScript('OnEvent',function() a:SetWidth(20);print('first') end);b:SetScript('OnEvent',function() print('second',sized) end)").unwrap();
    s.eval("sized=false").unwrap();
    s.fire("TEST", vec![]).unwrap();
    assert_eq!(s.prints(), ["first", "second\ttrue"]);
}
#[test]
fn hook_continuations_flush_between_handlers() {
    let s = Sim::new().unwrap();
    s.eval("sized=false;a=CreateFrame('Frame');a:SetPoint('CENTER');a:SetSize(10,10);a:RegisterEvent('TEST');a:SetScript('OnSizeChanged',function() sized=true end);a:SetScript('OnEvent',function() a:SetWidth(20) end);a:HookScript('OnEvent',function() print(sized) end)").unwrap();
    s.eval("sized=false").unwrap();
    s.fire("TEST", vec![]).unwrap();
    assert_eq!(s.prints(), ["true"]);
}

use selu_sim::{Session, SessionConfig};
#[test]
fn timer_then_update_flush_and_override_visibility() {
    let s = Sim::new().unwrap();
    s.eval(
        "p=CreateFrame('Frame');f=CreateFrame('Frame',nil,p);f:SetPoint('CENTER');f:SetSize(10,10)",
    )
    .unwrap();
    s.eval("sized=false;f:SetScript('OnSizeChanged',function() sized=true end);f:SetScript('OnUpdate',function(self) self:SetScript('OnUpdate',nil); print(sized) end);f.IsVisible=function() error('override') end;C_Timer.After(.01,function() f:SetWidth(20) end)").unwrap();
    s.advance_time(1. / 60.);
    assert_eq!(s.prints(), ["true"]);
    assert!(s.logs().is_empty(), "{:?}", s.logs());
}

#[test]
fn mutually_anchored_size_callbacks_are_bounded() {
    let s = Sim::new().unwrap();
    s.ctx().set_budget(100000);
    s.eval("local a=CreateFrame('Frame');local b=CreateFrame('Frame');a:SetPoint('LEFT',b,'RIGHT');b:SetPoint('LEFT',a,'RIGHT');local n=0;a:SetScript('OnSizeChanged',function() n=n+1;if n<20000 then b:SetWidth(n%3+1) end end);b:SetScript('OnSizeChanged',function() n=n+1;if n<20000 then a:SetWidth(n%3+1) end end);a:SetWidth(1)").unwrap();
    assert!(
        s.logs().iter().any(|x| x.contains("budget exhausted")),
        "{:?}",
        s.logs()
    );
    s.eval("assert(1+1==2)").unwrap();
}

#[test]
fn anchored_two_frame_size_chain_shares_instructions() {
    let s = Sim::new().unwrap();
    s.ctx().set_budget(10);
    s.eval("local a=CreateFrame('Frame');local b=CreateFrame('Frame');a:SetPoint('CENTER');b:SetPoint('LEFT',a,'RIGHT');local n=0;a:SetScript('OnSizeChanged',function() n=n+1;if n<20000 then b:SetWidth(n%3+1) end end);b:SetScript('OnSizeChanged',function() n=n+1;if n<20000 then a:SetWidth(n%3+1) end end);a:SetWidth(1)").unwrap();
    assert!(
        s.logs().iter().any(|x| x.contains("script ran too long")),
        "{:?}",
        s.logs()
    );
    s.eval("assert(1+1==2)").unwrap();
}

#[test]
fn callbacks_create_frames_without_borrow_panics_and_share_instructions() {
    let s = Sim::new().unwrap();
    s.ctx().set_budget(1);
    let result=s.eval("local count=0;local function spawn() count=count+1;local f=CreateFrame('Frame');f:SetScript('OnSizeChanged',function() if count<1500 then spawn() end end);f:SetWidth(1) end;spawn()");
    assert!(
        s.logs().iter().any(|x| x.contains("script ran too long")),
        "result={result:?}, logs={:?}",
        s.logs()
    );
    s.eval("assert(1+1==2)").unwrap();
}

#[test]
fn font_multiline_wrap_scaled_parent_and_reparent_draw_order() {
    let s = Sim::new().unwrap();
    s.eval(r#"
local p=CreateFrame('Frame');p:SetScale(2);p:SetPoint('CENTER');p:SetSize(100,100)
local f=p:CreateFontString();f:SetFont('custom',12,'OUTLINE');f:SetText('abcdefgh\nxy');f:SetPoint('TOPLEFT',p,'TOPLEFT');f:SetPoint('TOPRIGHT',p,'TOPLEFT',24,0)
assert(f:GetWidth()==24 and f:GetHeight()==36 and f:GetStringHeight()==36)
local q=CreateFrame('Frame');q:SetFrameStrata('TOOLTIP');q:SetFrameLevel(20)
local t=p:CreateTexture(nil,'OVERLAY',nil,-3);t:SetParent(q)
assert(t:GetFrameStrata()=='TOOLTIP' and t:GetFrameLevel()==20)
local l,n=t:GetDrawLayer();assert(l=='OVERLAY' and n==-3)
"#).unwrap();
}

#[test]
fn click_rechecks_after_each_phase_and_delivers_order() {
    let s = Sim::new().unwrap();
    s.eval("b=CreateFrame('CheckButton');b:SetAllPoints(UIParent);for _,phase in ipairs({'OnMouseDown','OnMouseUp','PreClick','OnClick','PostClick'}) do local name=phase;b:SetScript(name,function(self) print(name,self:GetChecked()) end) end").unwrap();
    s.click(1., 1., "LeftButton").unwrap();
    assert_eq!(
        s.prints(),
        [
            "OnMouseDown\tfalse",
            "OnMouseUp\tfalse",
            "PreClick\ttrue",
            "OnClick\ttrue",
            "PostClick\ttrue"
        ]
    );
    s.eval("b:SetScript('PreClick',function(self) self:EnableMouse(false) end)")
        .unwrap();
    let n = s.prints().len();
    s.click(1., 1., "LeftButton").unwrap();
    assert_eq!(&s.prints()[n..], ["OnMouseDown\ttrue", "OnMouseUp\ttrue"]);
}

#[test]
fn scaled_parent_drag_cursor_and_stop_button() {
    let s = Sim::new().unwrap();
    s.eval("local p=CreateFrame('Frame','Panel');p:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',10,20);p:SetSize(100,100);p:SetScale(2);local b=CreateFrame('Button',nil,p);b:SetAllPoints(p);b:SetScript('OnDragStart',function(self,button) local x,y=GetCursorPosition();assert(x==30 and y==50 and button=='LeftButton');p:StartMoving() end);b:SetScript('OnDragStop',function(self,button) local x,y=GetCursorPosition();assert(x==50 and y==80 and button=='LeftButton');p:StopMovingOrSizing() end)").unwrap();
    s.drag((30., 50.), (50., 80.)).unwrap();
    s.eval("local x,y=Panel:GetRect();assert(x==20 and y==35)")
        .unwrap();
}

#[test]
fn focus_parent_hide_and_transition_order() {
    let s = Sim::new().unwrap();
    s.eval("local p=CreateFrame('Frame');local a=CreateFrame('EditBox',nil,p);local b=CreateFrame('EditBox',nil,p);a:SetScript('OnEditFocusGained',function() print('a+') end);a:SetScript('OnEditFocusLost',function() print('a-') end);b:SetScript('OnEditFocusGained',function() print('b+') end);b:SetScript('OnEditFocusLost',function() print('b-') end);a:SetFocus();b:SetFocus();p:Hide();assert(not a:HasFocus() and not b:HasFocus())").unwrap();
    assert_eq!(s.prints(), ["a+", "a-", "b+", "b-"]);
    s.type_text("ignored").unwrap();
}

#[test]
fn slider_unchanged_and_clamped_values_only_emit_changes() {
    let s = Sim::new().unwrap();
    s.eval("local s=CreateFrame('Slider');s:SetMinMaxValues(0,10);s:SetScript('OnValueChanged',function(_,v,user) print(v,user) end);s:SetValue(5);s:SetValue(5);s:SetValue(11);s:SetValue(15)").unwrap();
    assert_eq!(s.prints(), ["5\tfalse", "10\tfalse"]);
}

#[test]
fn studio_demo_snapshot_fields_and_timer_render_data() {
    let mut session = Session::new(SessionConfig::default()).unwrap();
    session
        .load_addon_dir(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/addons/StudioDemo"),
        )
        .unwrap();
    session.login().unwrap();
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.regions.len(), 10);
    let panel = &snapshot.regions[1];
    assert_eq!(panel.name.as_deref(), Some("StudioDemoPanel"));
    assert_eq!(panel.widget, "Frame");
    assert!(panel.rect.is_some() && panel.visible && panel.alpha == 1.);
    let title = snapshot
        .regions
        .iter()
        .find(|r| r.text.as_deref() == Some("Studio Demo"))
        .unwrap();
    assert_eq!(title.widget, "FontString");
    assert!(title.rect.unwrap().width > 0. && title.visible && title.alpha == 1.);
    assert_eq!(
        snapshot
            .regions
            .iter()
            .filter(|r| r.texture.is_some())
            .count(),
        5
    );
    assert!(
        snapshot
            .regions
            .iter()
            .filter(|r| r.texture.is_some())
            .all(|r| r.rect.is_some() && r.visible && r.alpha == 1.)
    );
    session.advance_time(1.);
    let snapshot = session.snapshot().unwrap();
    assert!(
        snapshot
            .regions
            .iter()
            .any(|r| r.text.as_deref() == Some("ticks: 2"))
    );
    assert!(snapshot.regions.iter().any(|r| r.texture
        == Some(selu_sim::TextureFill::Color([0.2, 0.8, 0.3, 1.]))
        && r.rect.unwrap().width == 16.));
    session.slash("/demo hide").unwrap();
    let snapshot = session.snapshot().unwrap();
    assert!(snapshot.regions[1..9].iter().all(|r| !r.visible));
    session.slash("/demo show").unwrap();
    session.reload().unwrap();
    session.eval("assert(UIParent:GetWidth()==1920)").unwrap();
}

#[test]
fn session_custom_size_survives_reload() {
    let mut s = Session::new(SessionConfig {
        ui_size: (800., 600.),
        ..SessionConfig::default()
    })
    .unwrap();
    s.reload().unwrap();
    s.eval("assert(UIParent:GetWidth()==800 and UIParent:GetHeight()==600)")
        .unwrap();
}

#[test]
fn g_helpers_share_arena_state_and_secret_snapshot_markers() {
    let s = Sim::new().unwrap();
    s.set_secret_global("SECRET").unwrap();
    s.eval("events={};local f=CreateFrame('Frame');f:RegisterEvent('PLAYER_REGEN_DISABLED');f:RegisterEvent('PLAYER_REGEN_ENABLED');f:SetScript('OnEvent',function(_,e) events[#events+1]=e end);f.__protected=true;assert(f:IsProtected());assert(not CreateFrame('Frame'):IsProtected());assert(issecretvalue(SECRET) and not canaccessvalue(SECRET));local text=f:CreateFontString('SecretLabel');text:SetPoint('CENTER',UIParent,'CENTER');text:SetText(SECRET);assert(issecretvalue(text:GetText()));assert(type(geterrorhandler())=='function')").unwrap();
    s.set_combat(true).unwrap();
    s.set_combat(true).unwrap();
    s.set_combat(false).unwrap();
    s.eval("assert(not InCombatLockdown());assert(#events==2 and events[1]=='PLAYER_REGEN_DISABLED' and events[2]=='PLAYER_REGEN_ENABLED')").unwrap();
    let snapshot = s.snapshot().unwrap();
    let label = snapshot
        .regions
        .iter()
        .find(|r| r.name.as_deref() == Some("SecretLabel"))
        .unwrap();
    assert_eq!(label.text.as_deref(), Some("<secret>"));
    assert!(label.rect.unwrap().width > 0.0);
}
#[test]
fn timer_accounting_hooks_are_private() {
    let s = Sim::new().unwrap();
    s.eval("assert(rawget(_G,'__selu_sim_begin_callback')==nil);assert(rawget(_G,'__selu_sim_end_callback')==nil)").unwrap();
}

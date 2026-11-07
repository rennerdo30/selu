//! Local client-backed checks; never starts a client or reads account data.
use selu_sim::{Session, SessionConfig};
use std::sync::{OnceLock, mpsc};

struct Case {
    run: Box<dyn FnOnce(&mut Session) + Send>,
    reply: mpsc::Sender<Result<(), String>>,
}
fn with_session(run: impl FnOnce(&mut Session) + Send + 'static) {
    static WORKER: OnceLock<mpsc::Sender<Case>> = OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (send, receive) = mpsc::channel::<Case>();
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            let mut session = loaded();
            eprintln!(
                "client interaction fixture loaded once in {:?}",
                start.elapsed()
            );
            let mut poisoned = None;
            for case in receive {
                if poisoned.take().is_some() {
                    eprintln!("recreating failed client fixture before next case");
                    session = loaded();
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    (case.run)(&mut session);
                    reset(&session);
                }))
                .map_err(|error| {
                    error
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| error.downcast_ref::<&str>().map(|s| (*s).into()))
                        .unwrap_or_else(|| "client case panicked".into())
                });
                if let Err(error) = &result {
                    poisoned = Some(error.clone());
                }
                let _ = case.reply.send(result);
            }
        });
        send
    });
    let (reply, result) = mpsc::channel();
    worker
        .send(Case {
            run: Box::new(run),
            reply,
        })
        .unwrap();
    result.recv().expect("client fixture worker ended").unwrap();
}
fn reset(s: &Session) {
    s.hover(-1., -1.).unwrap();
    let keys = selu_sim::frames::arena::store(s.lua())
        .borrow()
        .keys
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    for key in keys {
        s.key(&key, false).unwrap();
    }
    s.eval("CloseAllWindows();CloseAllBags(); local f=GetCurrentKeyBoardFocus(); if f then f:ClearFocus() end; ChatFrame1:ScrollToBottom();assert(not ContainerFrame1:IsShown() and not CharacterFrame:IsShown())").unwrap();
    let bags: mlua::Table = s.lua().globals().raw_get("__selu_world_bags").unwrap();
    if bags
        .raw_get::<Option<mlua::Table>>(0)
        .unwrap()
        .is_some_and(|bag| bag.raw_len() > 0)
    {
        s.set_bag(0, &[]).unwrap();
    }
}
fn loaded() -> Session {
    let source = std::path::PathBuf::from(
        std::env::var_os("SELU_DEFAULT_UI_SOURCE").expect("set read-only UI source"),
    );
    let install = std::path::PathBuf::from(
        std::env::var_os("SELU_CLIENT_INSTALL").expect("set read-only client install"),
    );
    let mut s = Session::new(SessionConfig {
        target: Some("wowf@1.60.1.70790".into()),
        client_install: Some(install),
        ..Default::default()
    })
    .unwrap();
    s.load_default_ui(&source, |_| {}).unwrap();
    s.login().unwrap();
    for _ in 0..4 {
        s.advance_time(1.0 / 60.0);
    }
    s
}
fn center(s: &Session, name: &str) -> (f64, f64) {
    let snapshot = s.snapshot().unwrap();
    let r = snapshot
        .regions
        .iter()
        .find(|r| r.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("missing {name}"));
    assert!(r.visible, "{name} hidden");
    let q = r.rect.unwrap_or_else(|| panic!("unplaced {name}"));
    (q.left + q.width / 2., q.bottom + q.height / 2.)
}
fn click(s: &Session, name: &str) {
    let (x, y) = center(s, name);
    let hit = s.click(x, y, "LeftButton").unwrap_or_else(|error| {
        panic!(
            "{name}: {error}; logs={:?}",
            s.logs().iter().rev().take(8).collect::<Vec<_>>()
        )
    });
    let snapshot = s.snapshot().unwrap();
    let hit = hit.and_then(|id| snapshot.regions.get(id.index as usize));
    assert_eq!(
        hit.and_then(|r| r.name.as_deref()),
        Some(name),
        "hit={:?}",
        hit.map(|r| (&r.widget, r.rect, r.parent))
    );
}
fn shown(s: &Session, name: &str) -> bool {
    s.snapshot()
        .unwrap()
        .regions
        .iter()
        .find(|r| r.name.as_deref() == Some(name))
        .is_some_and(|r| r.visible)
}
fn verify_default_layout_and_bag_art(s: &Session) {
    // Mainline/EditModePresetLayouts.lua:248-261 is also loaded by Camelot.
    // Compare the applied native anchors with Blizzard's selected preset data,
    // rather than inventing a supposedly more appropriate player position.
    s.eval(r#"
        local m=EditModeManagerFrame
        local layout=m:GetActiveLayoutInfo()
        assert(layout.layoutIndex==Enum.EditModePresetLayouts.Modern)
        local expected=EDIT_MODE_MODERN_SYSTEM_MAP[PlayerFrame.system][PlayerFrame.systemIndex].anchorInfo
        local point,relative,relativePoint,x,y=PlayerFrame:GetPoint(1)
        assert(point==expected.point and relative==_G[expected.relativeTo] and relativePoint==expected.relativePoint)
        assert(x==expected.offsetX and y==expected.offsetY)
        __bag_art_ids={}
        for _,f in ipairs({CharacterBag0Slot,CharacterBag1Slot,CharacterBag2Slot,CharacterBag3Slot,CharacterReagentBag0Slot}) do
            local normal=f:GetNormalTexture()
            local atlas=f:GetSlotAtlases()
            assert(normal:IsVisible() and normal:GetAtlas()==atlas)
            __bag_art_ids[#__bag_art_ids+1]=assert(tonumber(normal:GetTexture()))
        end
    "#).unwrap();
    let install = std::env::var_os("SELU_CLIENT_INSTALL").unwrap();
    let storage =
        selu_gamedata::Storage::open(std::path::Path::new(&install), "wowf", None).unwrap();
    let ids: mlua::Table = s.lua().globals().raw_get("__bag_art_ids").unwrap();
    for id in ids.sequence_values::<u32>() {
        let id = id.unwrap();
        let bytes = storage
            .read(id)
            .unwrap_or_else(|e| panic!("bag art {id}: {e}"));
        selu_gamedata::blp::decode(&bytes).unwrap_or_else(|e| panic!("bag art {id}: {e}"));
    }
    s.lua()
        .globals()
        .raw_set("__bag_art_ids", mlua::Value::Nil)
        .unwrap();
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn backpack_button_opens_backpack() {
    with_session(|s| {
        assert!(!shown(s, "ContainerFrame1"));
        click(s, "MainMenuBarBackpackButton");
        assert!(
            shown(s, "ContainerFrame1"),
            "{:?}",
            s.logs().iter().rev().take(8).collect::<Vec<_>>()
        );
    });
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn character_micro_button_opens_character_frame() {
    with_session(|s| {
        assert!(!shown(s, "CharacterFrame"));
        click(s, "CharacterMicroButton");
        assert!(
            shown(s, "CharacterFrame"),
            "{:?}",
            s.logs().iter().rev().take(8).collect::<Vec<_>>()
        );
    });
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn escape_closes_backpack_and_character_frame() {
    with_session(|s| {
        click(s, "MainMenuBarBackpackButton");
        click(s, "CharacterMicroButton");
        assert!(
            shown(s, "ContainerFrame1") && shown(s, "CharacterFrame"),
            "bag={} char={} logs={:?}",
            shown(s, "ContainerFrame1"),
            shown(s, "CharacterFrame"),
            s.logs().iter().rev().take(12).collect::<Vec<_>>()
        );
        s.key("ESCAPE", true).unwrap();
        assert!(
            !shown(s, "ContainerFrame1") && !shown(s, "CharacterFrame"),
            "prints={:?}; logs={:?}; bag={} char={}",
            s.prints(),
            s.logs().iter().rev().take(12).collect::<Vec<_>>(),
            shown(s, "ContainerFrame1"),
            shown(s, "CharacterFrame")
        );
    });
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn action_button_has_pressed_texture_until_release() {
    with_session(|s| {
        let (x, y) = center(s, "ActionButton1");
        s.mouse_down(x, y, "LeftButton").unwrap();
        s.eval("assert(ActionButton1:GetButtonState()=='PUSHED');assert(ActionButton1:GetPushedTexture():IsVisible())").unwrap();
        s.mouse_up(x, y, "LeftButton").unwrap();
        s.eval("assert(ActionButton1:GetButtonState()=='NORMAL');assert(not ActionButton1:GetPushedTexture():IsVisible())").unwrap();
    });
}

#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn fresh_login_has_no_absent_unit_frames_or_empty_objective_tracker() {
    with_session(|s| {
        verify_default_layout_and_bag_art(s);
        s.eval("assert(not UnitExists('target') and not UnitExists('focus'))")
            .unwrap();
        for name in ["TargetFrame", "FocusFrame", "ObjectiveTrackerFrame"] {
            assert!(
            !shown(s, name),
            "{name} is visible; {:?}",
            s.logs()
                .iter()
                .filter(|line| line.starts_with("error:") || line.starts_with("callback error:"))
                .rev()
                .take(12)
                .collect::<Vec<_>>()
        );
        }
    });
}

#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn hover_and_leave_run_source_tooltip_handlers() {
    with_session(|s| {
        let (x, y) = center(s, "CharacterMicroButton");
        s.hover(x, y).unwrap();
        s.eval("assert(GameTooltip:IsVisible(),'tooltip hidden'); assert(GameTooltip:GetOwner()==CharacterMicroButton,'tooltip owner');assert(GameTooltip:NumLines()>0,'tooltip empty');assert(GameTooltip:GetText() and GameTooltip:GetText()~='','tooltip title empty')").unwrap();
        s.hover(-1., -1.).unwrap();
        s.eval("assert(not GameTooltip:IsShown())").unwrap();
    });
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn keyboard_focus_and_modifiers_follow_host_input() {
    with_session(|s| {
        s.eval("ChatFrameUtil.OpenChat('',ChatFrame1);assert(GetCurrentKeyBoardFocus()==ChatFrame1EditBox)").unwrap();
        s.key("LSHIFT", true).unwrap();
        s.eval("assert(IsShiftKeyDown() and IsLeftShiftKeyDown() and IsModifierKeyDown())")
            .unwrap();
        s.key("LSHIFT", false).unwrap();
        s.key("LCTRL", true).unwrap();
        s.eval("assert(IsControlKeyDown() and IsLeftControlKeyDown())")
            .unwrap();
        s.key("LCTRL", false).unwrap();
        s.key("RALT", true).unwrap();
        s.eval("assert(IsAltKeyDown() and IsRightAltKeyDown())")
            .unwrap();
        s.key("RALT", false).unwrap();
        s.type_text("focus probe").unwrap();
        s.eval("assert(not IsShiftKeyDown());assert(ChatFrame1EditBox:GetText()=='focus probe');ChatFrame1EditBox:ClearFocus();ChatFrame1EditBox:Hide()").unwrap();
    });
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn movable_addon_frame_drags_in_loaded_ui() {
    with_session(|s| {
        s.eval("local f=CreateFrame('Frame','InputDragProbe',UIParent);f:SetSize(60,40);f:SetPoint('CENTER');f:EnableMouse(true);f:SetMovable(true);f:RegisterForDrag('LeftButton');f:SetScript('OnDragStart',function(self) self:StartMoving() end);f:SetScript('OnDragStop',function(self) self:StopMovingOrSizing() end)").unwrap();
        let (x, y) = center(s, "InputDragProbe");
        s.mouse_down(x, y, "LeftButton").unwrap();
        s.hover(x + 10., y + 4.).unwrap();
        let (mx, my) = center(s, "InputDragProbe");
        assert!((mx - x - 10.).abs() < 0.001 && (my - y - 4.).abs() < 0.001);
        s.hover(x + 37., y + 12.).unwrap();
        let (mx, my) = center(s, "InputDragProbe");
        assert!((mx - x - 37.).abs() < 0.001 && (my - y - 12.).abs() < 0.001);
        s.mouse_up(x + 37., y + 12., "LeftButton").unwrap();
        let (nx, ny) = center(s, "InputDragProbe");
        assert!((nx - x - 37.).abs() < 0.001 && (ny - y - 12.).abs() < 0.001);
        s.eval("InputDragProbe:Hide()").unwrap();
    });
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn wheel_scrolls_chat_through_source_handler() {
    with_session(|s| {
        s.eval("for i=1,40 do print('scroll probe '..i) end;ChatFrame1:ScrollToBottom();assert(ChatFrame1:GetScrollOffset()==0)").unwrap();
        let (x, y) = center(s, "ChatFrame1");
        s.wheel(x, y, 1.).unwrap();
        s.eval("assert(ChatFrame1:IsMouseWheelEnabled(),'wheel disabled');assert(ChatFrame1:GetScript('OnMouseWheel'),'no wheel handler');assert(ChatFrame1:GetMaxScrollRange()>0,'no messages');assert(ChatFrame1:GetScrollOffset()>0,'scroll unchanged')").unwrap();
        s.wheel(x, y, -1.).unwrap();
        s.eval("assert(ChatFrame1:GetScrollOffset()==0)").unwrap();
    });
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn scenario_items_reach_source_container_buttons() {
    with_session(|s| {
        let mut scenario = selu_sim::world::Scenario::capture(s.sim()).unwrap();
        scenario.bags.insert(0, vec![117, 6948]);
        scenario.apply(s.sim()).unwrap();
        click(s, "MainMenuBarBackpackButton");
        s.eval("assert(C_Container.GetContainerItemInfo(0,1).itemID==117);local count=0;for _,button in ContainerFrame1:EnumerateValidItems() do if button:GetBagID()==0 then local id=C_Container.GetContainerItemID(0,button:GetID());if id then assert(button.hasItem,'hasItem');assert(button.icon:GetTexture()~=nil,'icon texture');assert(button.icon:GetRect()~=nil,'icon rect');count=count+1 end end end;assert(count==2,'item count='..count..' size='..tostring(ContainerFrame1.size))").unwrap_or_else(|error|panic!("{error}; logs={:?}",s.logs().iter().rev().take(12).collect::<Vec<_>>()));
    });
}

#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn active_micro_buttons_have_layout_and_absent_navigation_has_no_frame() {
    with_session(|s| {
        s.eval("assert(C_Navigation.GetFrame()==nil and not SuperTrackedFrame:IsShown());for _,info in ipairs(MicroMenu:GenerateButtonInfos()) do local button=info.button;if button and button:GetParent()==MicroMenu and button:IsVisible() then assert(button:GetRect(),button:GetName()) end end").unwrap();
    });
}

//! Local client-backed checks; never starts a client or reads account data.
use selu_sim::{Session, SessionConfig};
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
    s.click(x, y, "LeftButton")
        .unwrap_or_else(|error| panic!("{name}: {error}"));
}
fn shown(s: &Session, name: &str) -> bool {
    s.snapshot()
        .unwrap()
        .regions
        .iter()
        .find(|r| r.name.as_deref() == Some(name))
        .is_some_and(|r| r.visible)
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn backpack_button_opens_backpack() {
    let s = loaded();
    assert!(!shown(&s, "ContainerFrame1"));
    click(&s, "MainMenuBarBackpackButton");
    assert!(
        shown(&s, "ContainerFrame1"),
        "{:?}",
        s.logs().iter().rev().take(8).collect::<Vec<_>>()
    );
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn character_micro_button_opens_character_frame() {
    let s = loaded();
    assert!(!shown(&s, "CharacterFrame"));
    click(&s, "CharacterMicroButton");
    assert!(
        shown(&s, "CharacterFrame"),
        "{:?}",
        s.logs().iter().rev().take(8).collect::<Vec<_>>()
    );
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn escape_closes_backpack_and_character_frame() {
    let s = loaded();
    click(&s, "MainMenuBarBackpackButton");
    click(&s, "CharacterMicroButton");
    assert!(shown(&s, "ContainerFrame1") && shown(&s, "CharacterFrame"));
    s.key("ESCAPE", true).unwrap();
    assert!(
        !shown(&s, "ContainerFrame1") && !shown(&s, "CharacterFrame"),
        "prints={:?}; logs={:?}; bag={} char={}",
        s.prints(),
        s.logs().iter().rev().take(12).collect::<Vec<_>>(),
        shown(&s, "ContainerFrame1"),
        shown(&s, "CharacterFrame")
    );
}
#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn action_button_has_pressed_texture_until_release() {
    let s = loaded();
    let (x, y) = center(&s, "ActionButton1");
    s.mouse_down(x, y, "LeftButton").unwrap();
    s.eval("assert(ActionButton1:GetButtonState()=='PUSHED');assert(ActionButton1:GetPushedTexture():IsVisible())").unwrap();
    s.mouse_up(x, y, "LeftButton").unwrap();
    s.eval("assert(ActionButton1:GetButtonState()=='NORMAL');assert(not ActionButton1:GetPushedTexture():IsVisible())").unwrap();
}

#[test]
#[ignore = "requires read-only local game art and matching Blizzard UI sources"]
fn fresh_login_has_no_absent_unit_frames_or_empty_objective_tracker() {
    let s = loaded();
    s.eval("assert(not UnitExists('target') and not UnitExists('focus'))")
        .unwrap();
    for name in ["TargetFrame", "FocusFrame", "ObjectiveTrackerFrame"] {
        assert!(
            !shown(&s, name),
            "{name} is visible; {:?}",
            s.logs()
                .iter()
                .filter(|line| line.starts_with("error:") || line.starts_with("callback error:"))
                .rev()
                .take(12)
                .collect::<Vec<_>>()
        );
    }
}

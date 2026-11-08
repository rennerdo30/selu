//! Execution taint (SL-35). The "Blizzard" side is self-written secure code
//! loaded without an owner; the addon side is a fixture addon folder. The
//! cases mirror real incidents: a hook running inside Blizzard's call, an
//! addon closure called directly by Blizzard layout code, and the client's
//! secure-call isolation.
use selu_sim::{Session, SessionConfig};

/// Secure code: a protected bar and helpers Blizzard-style code would have.
const SECURE: &str = r#"
SecureBar = CreateFrame("Frame", "SecureBar")
rawset(SecureBar, "__protected", true)
SecureView = {}
function Secure_ShowPanel() end
function Secure_Layout() Secure_ShowPanel(); SecureBar:SetPoint("CENTER") end
function Secure_Refresh()
    if SecureView.calc then SecureView.calc() end
    SecureBar:SetPoint("CENTER")
end
SecureDriver = CreateFrame("Frame")
SecureDriver:RegisterEvent("SELU_LAYOUT")
SecureDriver:RegisterEvent("SELU_REFRESH")
SecureDriver:SetScript("OnEvent", function(_, event)
    if event == "SELU_LAYOUT" then Secure_Layout() else Secure_Refresh() end
end)
"#;

fn session(addon_source: Option<&str>) -> Session {
    let mut s = Session::new(SessionConfig::default()).unwrap();
    s.sim()
        .load_files(&[("Blizzard_Fixture.lua".into(), SECURE.into())])
        .unwrap();
    if let Some(source) = addon_source {
        // Tests run in parallel: each needs its own folder (a timestamp can
        // repeat across threads).
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "selu-taint-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let dir = root.join("TaintFixture");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("TaintFixture.toc"), "## Interface: 1\naddon.lua\n").unwrap();
        std::fs::write(dir.join("addon.lua"), source).unwrap();
        s.load_addon_dir(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }
    s.login().unwrap();
    s
}

fn blocked(s: &Session) -> Vec<(String, String)> {
    s.taint_log()
        .into_iter()
        .filter(|r| r.event == "ADDON_ACTION_BLOCKED")
        .map(|r| (r.addon, r.action))
        .collect()
}

#[test]
fn a_hook_inside_a_secure_call_blocks_protected_layout_in_combat() {
    let s = session(Some(
        r#"hooksecurefunc("Secure_ShowPanel", function() end)"#,
    ));
    s.sim().set_combat(true).unwrap();
    s.fire("SELU_LAYOUT", vec![]).unwrap();
    assert_eq!(
        blocked(&s),
        [(
            "TaintFixture".to_string(),
            "SecureBar:SetPoint()".to_string()
        )]
    );
    assert!(
        s.logs()
            .iter()
            .any(|l| l == "taint: ADDON_ACTION_BLOCKED TaintFixture SecureBar:SetPoint()"),
        "{:?}",
        s.logs()
    );
}

#[test]
fn an_addon_closure_called_by_secure_code_taints_the_rest_of_the_call() {
    let s = session(Some(r#"SecureView.calc = function() return 1 end"#));
    s.sim().set_combat(true).unwrap();
    s.fire("SELU_REFRESH", vec![]).unwrap();
    assert_eq!(blocked(&s).len(), 1, "{:?}", s.taint_log());
}

#[test]
fn clean_secure_calls_and_out_of_combat_calls_are_not_blocked() {
    // No addon involvement: secure all the way.
    let clean = session(None);
    clean.sim().set_combat(true).unwrap();
    clean.fire("SELU_LAYOUT", vec![]).unwrap();
    clean.fire("SELU_REFRESH", vec![]).unwrap();
    assert!(clean.taint_log().is_empty());
    // Tainted, but out of combat: frame actions are allowed.
    let s = session(Some(
        r#"hooksecurefunc("Secure_ShowPanel", function() end)"#,
    ));
    s.fire("SELU_LAYOUT", vec![]).unwrap();
    assert!(s.taint_log().is_empty());
}

#[test]
fn every_engine_entry_starts_secure() {
    // The addon's own event handler is tainted; the next secure handler is not.
    let s = session(Some(
        r#"local f = CreateFrame("Frame"); f:RegisterEvent("SELU_ADDON");
           f:SetScript("OnEvent", function() AddonSawSecure = issecure() end)"#,
    ));
    s.sim().set_combat(true).unwrap();
    s.fire("SELU_ADDON", vec![]).unwrap();
    s.fire("SELU_LAYOUT", vec![]).unwrap();
    assert!(s.taint_log().is_empty(), "{:?}", s.taint_log());
    s.eval("assert(AddonSawSecure == false)").unwrap();
}

#[test]
fn issecure_and_forceinsecure_follow_the_execution() {
    let s = session(None);
    s.sim()
        .load_files(&[(
            "Blizzard_Probe.lua".into(),
            r#"function Secure_Probe() local before = issecure(); forceinsecure(); return before, issecure() end"#
                .into(),
        )])
        .unwrap();
    s.eval("local before, after = Secure_Probe(); assert(before == true and after == false)")
        .unwrap();
}

#[test]
fn xml_protected_attribute_marks_frames_protected() {
    let root = std::env::temp_dir().join(format!("selu-taint-xml-{}", std::process::id()));
    let dir = root.join("XmlFixture");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("XmlFixture.toc"), "## Interface: 1\nframes.xml\n").unwrap();
    std::fs::write(
        dir.join("frames.xml"),
        r#"<Ui><Frame name="FixtureProtectedTemplate" protected="true" virtual="true"/>
           <Frame name="FixtureProtected" inherits="FixtureProtectedTemplate"/>
           <Frame name="FixturePlain"/></Ui>"#,
    )
    .unwrap();
    let mut s = Session::new(SessionConfig::default()).unwrap();
    s.load_addon_dir(&dir).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    s.eval("assert(FixtureProtected:IsProtected() == true); assert(FixturePlain:IsProtected() == false)")
        .unwrap();
}
#[test]
fn issecure_reports_taint_from_a_called_addon_closure() {
    let s = session(Some(r#"SecureView.calc = function() return 1 end"#));
    s.sim()
        .load_files(&[(
            "Blizzard_Probe2.lua".into(),
            r#"function Secure_P() local a = issecure(); SecureView.calc(); return a, issecure() end"#
                .into(),
        )])
        .unwrap();
    s.eval("local before, after = Secure_P(); assert(before == true and after == false)")
        .unwrap();
}

#[test]
fn attribute_handlers_run_with_their_own_security() {
    let s = session(Some(
        r#"local f = CreateFrame("Frame"); f:RegisterEvent("SELU_ADDON");
           f:SetScript("OnEvent", function() SecureAttr:SetAttribute("invoke", 1) end)"#,
    ));
    s.sim()
        .load_files(&[(
            "Blizzard_Attr.lua".into(),
            r#"SecureAttr = CreateFrame("Frame")
               SecureAttr:SetScript("OnAttributeChanged", function() AttrSecure = issecure() end)
               SecureShowFrame = CreateFrame("Frame"); SecureShowFrame:Hide()
               SecureShowFrame:SetScript("OnShow", function() ShowSecure = issecure() end)"#
                .into(),
        )])
        .unwrap();
    s.fire("SELU_ADDON", vec![]).unwrap();
    s.eval("assert(AttrSecure == true)").unwrap();
    // OnShow inherits the caller's taint.
    s.eval("forceinsecure(); SecureShowFrame:Show()").unwrap();
    s.eval("assert(ShowSecure == false)").unwrap();
}

#[test]
fn a_reinstalled_hook_still_stops_a_runaway_loop() {
    // Every addon load re-installs the native hook (call events for taint).
    // A re-install must keep delegating to mlua's hook, not to itself: a
    // runaway loop then fails with the budget error instead of overflowing
    // the native stack (found through `sim.reload()` with Blizzard's UI).
    let mut s = session(Some("SeluFirst = true"));
    let root = std::env::temp_dir().join(format!("selu-taint-second-{}", std::process::id()));
    let dir = root.join("SecondFixture");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SecondFixture.toc"),
        "## Interface: 1\nsecond.lua\n",
    )
    .unwrap();
    std::fs::write(dir.join("second.lua"), "SeluSecond = true").unwrap();
    s.load_addon_dir(&dir).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    let error = s.eval("while true do end").unwrap_err().to_string();
    assert!(error.contains("script ran too long"), "{error}");
}

#[test]
fn callback_counts_include_only_addon_callbacks_once_an_addon_is_loaded() {
    // Blizzard's own UI schedules timers and OnUpdates; sim.callbackCounts
    // must still show what the addon under test does.
    let s = session(Some(
        r#"local f = CreateFrame("Frame"); f:SetScript("OnUpdate", function() end)
           C_Timer.After(0.05, function() end)"#,
    ));
    s.sim()
        .load_files(&[(
            "Blizzard_Ticker.lua".into(),
            r#"SecureTicker = C_Timer.NewTicker(0.05, function() end)
               local f = CreateFrame("Frame"); f:SetScript("OnUpdate", function() end)"#
                .into(),
        )])
        .unwrap();
    s.eval("__selu_sim_counts.timers = 0; __selu_sim_counts.updates = 0")
        .unwrap();
    s.sim().advance_time(0.5);
    s.eval("assert(__selu_sim_counts.timers == 1, 'timers ' .. __selu_sim_counts.timers)")
        .unwrap();
    s.eval("assert(__selu_sim_counts.updates > 0 and __selu_sim_counts.updates <= 31, 'updates ' .. __selu_sim_counts.updates)")
        .unwrap();
}

#[test]
fn break_up_large_numbers_groups_digits_like_the_client() {
    let s = session(None);
    s.eval("assert(BreakUpLargeNumbers(0) == '0'); assert(BreakUpLargeNumbers(999) == '999'); assert(BreakUpLargeNumbers(1234567) == '1,234,567'); assert(BreakUpLargeNumbers(-1000) == '-1,000')").unwrap();
    s.eval("LARGE_NUMBER_SEPERATOR = '.'; assert(BreakUpLargeNumbers(1234567) == '1.234.567')")
        .unwrap();
}

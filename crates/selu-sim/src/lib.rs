//! Headless WoW simulator core (DESIGN.md section 7): the compiled Lua
//! runs in a Lua 5.1 VM with a simulated clock, captured `print`, a minimal
//! `CreateFrame`, and event firing from Rust. Anything the sim does not
//! model logs `not simulated: X` once; it never fails silently.
//!
//! Timers use exact positive deadlines plus a persistent 60 Hz OnUpdate grid.
//! Zero/negative delays wait 1/60 second; advance_time(0) runs nothing.
//! Timers precede OnUpdate on ties and callback errors go to `Sim::logs`.
//! Historical ticker semantics follow wow-ui-source
//! `1.13.2:SharedXML/C_TimerAugment.lua`: callback errors stop tickers and
//! iteration counts may be zero, negative or fractional. Natural-completion
//! IsCancelled and the hybrid frame/deadline model remain unverified against
//! current native clients. Advances have a 100,000 step/callback budget;
//! exhaustion is logged, leaving the clock at its last dispatch point.
//! Endpoint comparisons allow an absolute 1 ns rounding tolerance.

use mlua::Value;
use std::collections::HashSet;

// Module ownership for round 8 (docs/WORK-PACKAGES.md): `core` is S0's
// contract; `frames` and `regions` belong to S1; `session`, `loader`,
// `world`, `trace` and `snapshot` to S2. This file is S0's wiring.
mod client_ui;
pub mod core;
pub mod fonts;
pub mod frames;
mod host_lua;
pub mod loader;
mod native_defaults;
mod native_stdlib;
pub mod profile;
mod regions;
pub mod saved;
pub mod session;
mod sim_globals;
pub mod snapshot;
pub mod text;
mod timers;
mod trace;
pub mod world;
mod wow_api;
mod xml;

pub use core::{
    Ctx, DrawLayer, Generation, Rect, RegionId, RegionModel, RegionSnapshot, Snapshot, SourcePos,
    Strata, TextureFill, TraceKind, TraceRecord,
};
pub use mlua::Value as SimValue;
pub use session::{Session, SessionConfig};
pub type SimResult<T> = Result<T, mlua::Error>;

use crate::core::Shared;

pub(crate) fn value_string(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
        Value::Table(_) => "table".into(),
        Value::Function(_) => "function".into(),
        Value::Thread(_) => "thread".into(),
        _ => "userdata".into(),
    }
}

pub(crate) fn log_once(logs: &Shared<Vec<String>>, warned: &Shared<HashSet<String>>, what: &str) {
    let mut w = warned.lock().unwrap();
    if w.insert(what.to_string()) {
        logs.lock().unwrap().push(format!("not simulated: {what}"));
    }
}

pub struct Sim {
    session: Session,
}
impl Sim {
    pub fn new() -> SimResult<Self> {
        Ok(Self {
            session: Session::new(SessionConfig::default())?,
        })
    }
}
impl std::ops::Deref for Sim {
    type Target = Session;
    fn deref(&self) -> &Session {
        &self.session
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_event_roundtrip_and_print_capture() {
        let sim = Sim::new().unwrap();
        sim.eval(
            r#"
            local f = CreateFrame("Frame")
            f:RegisterEvent("PLAYER_LOGIN")
            f:SetScript("OnEvent", function(self, event) print("got " .. event) end)
            "#,
        )
        .unwrap();
        let delivered = sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert_eq!(delivered, 1);
        assert_eq!(sim.prints(), vec!["got PLAYER_LOGIN".to_string()]);
        sim.fire("UNIT_HEALTH", vec![]).unwrap();
        assert_eq!(sim.prints().len(), 1, "unregistered event must not fire");
    }

    #[test]
    fn unregistered_methods_and_globals_log_once() {
        let sim = Sim::new().unwrap();
        sim.eval(
            r#"
            local f = CreateFrame("Frame")
            f:SetCooldown(1, 2)
            f:SetCooldown(1, 2)
            assert(f.missingAddonField == nil)
            assert(TotallyMadeUp == nil)
            assert(TotallyMadeUp == nil)
            "#,
        )
        .unwrap();
        let logs = sim.logs();
        assert_eq!(
            logs.iter()
                .filter(|l| l.contains("Frame:SetCooldown"))
                .count(),
            1,
            "{logs:?}"
        );
        assert_eq!(
            logs.iter()
                .filter(|l| l.contains("not simulated: TotallyMadeUp"))
                .count(),
            1,
            "{logs:?}"
        );
    }

    #[test]
    fn addon_files_share_name_and_table() {
        let sim = Sim::new().unwrap();
        sim.load_addon(
            "Demo",
            &[
                (
                    "a.lua".into(),
                    "local name, ns = ...; ns.x = 1; print(name)".into(),
                ),
                ("b.lua".into(), "local _, ns = ...; print(ns.x + 1)".into()),
            ],
        )
        .unwrap();
        assert_eq!(sim.prints(), ["Demo", "2"]);
    }

    #[test]
    fn simulated_clock_advances() {
        let sim = Sim::new().unwrap();
        sim.advance_time(2.5);
        sim.eval("print(GetTime())").unwrap();
        assert_eq!(sim.prints(), vec!["2.5".to_string()]);
        assert_eq!(sim.now(), 2.5);
    }

    #[test]
    fn runaway_handler_is_a_logged_error_not_a_hang() {
        let sim = Sim::new().unwrap();
        sim.ctx().set_budget(50);
        sim.eval(
            r#"
            local f = CreateFrame("Frame")
            f:RegisterEvent("PLAYER_LOGIN")
            f:SetScript("OnEvent", function() while true do end end)
            "#,
        )
        .unwrap();
        let err = sim.fire("PLAYER_LOGIN", vec![]).unwrap_err();
        assert!(err.to_string().contains(core::BUDGET_ERROR), "{err}");
        assert!(
            sim.logs().iter().any(|l| l.contains(core::BUDGET_ERROR)),
            "{:?}",
            sim.logs()
        );
        // The session keeps working afterwards.
        sim.eval("print('still alive')").unwrap();
        assert_eq!(sim.prints(), ["still alive"]);
    }

    #[test]
    fn unregister_stops_delivery() {
        let sim = Sim::new().unwrap();
        sim.eval(
            r#"
            local f = CreateFrame("Frame")
            f:RegisterEvent("PLAYER_LOGIN")
            f:UnregisterEvent("PLAYER_LOGIN")
            f:SetScript("OnEvent", function() print("should not run") end)
            "#,
        )
        .unwrap();
        assert_eq!(sim.fire("PLAYER_LOGIN", vec![]).unwrap(), 0);
        assert!(sim.prints().is_empty());
    }
}

mod snapshot_cache;

pub mod assets;

pub mod wtf;

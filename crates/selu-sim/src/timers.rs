//! Deterministic hybrid steps: exact positive timer deadlines, plus a 60 Hz
//! OnUpdate grid anchored at time zero. Zero/negative delays wait 1/60 second;
//! advance_time(0) does no work. Partial advances preserve the frame grid.
//! Timers precede OnUpdate on ties; equal deadlines retain insertion order.
//! Nested due timers run in the same advance. Tickers reschedule from their
//! origin plus tick index times period, so advancing a large interval does not lose ticks.
//!
//! Client evidence: wow-ui-source `1.13.2:SharedXML/C_TimerAugment.lua`:
//! callback errors stop tickers; counts decrement after the first call, so
//! 0/negative counts run once and positive fractions run ceil(count) times.
//! Current native natural-completion IsCancelled and hybrid frame/deadline
//! scheduling remain unverified. Explicit cancellation is idempotent.
//! Endpoints use an absolute 1 ns tolerance. Each advance permits at most
//! 100,000 scheduler steps and callbacks, reporting exhaustion in Sim::logs
//! and leaving time at the last dispatched point. Nonprogressing periods
//! fall back to a frame (or the smallest safe scale at very large clocks).
use crate::{Shared, SimResult};
use mlua::{Function, Lua};

pub(crate) fn install(lua: &Lua, logs: Shared<Vec<String>>) -> SimResult<()> {
    lua.globals().set(
        "__selu_sim_error",
        lua.create_function(move |_, error: String| {
            logs.lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("callback error: {error}"));
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "__selu_sim_flush",
        lua.create_function(|lua, ()| {
            let ctx = lua.app_data_ref::<crate::Ctx>().unwrap().clone();
            crate::core::flush(lua, &ctx);
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "__selu_sim_begin_callback",
        lua.create_function(|lua, ()| {
            let ctx = lua.app_data_ref::<crate::Ctx>().unwrap().clone();
            crate::core::begin_callback(lua, &ctx)
        })?,
    )?;
    lua.globals().set(
        "__selu_sim_end_callback",
        lua.create_function(|lua, ()| {
            let ctx = lua.app_data_ref::<crate::Ctx>().unwrap().clone();
            crate::core::end_callback(&ctx);
            Ok(())
        })?,
    )?;
    lua.load(include_str!("timers.lua")).exec()?;
    // Timers retain private upvalues; addons must not be able to decrement
    // the host's synchronous callback nesting counter.
    lua.globals()
        .raw_set("__selu_sim_begin_callback", mlua::Value::Nil)?;
    lua.globals()
        .raw_set("__selu_sim_end_callback", mlua::Value::Nil)?;
    Ok(())
}

// Admission tolerance is one nanosecond, absolute, only at the advance endpoint.
// Deadlines themselves remain exact f64 origin + index * period values.
pub(crate) fn advance(lua: &Lua, clock: &Shared<f64>, logs: &Shared<Vec<String>>, seconds: f64) {
    advance_bounded(lua, clock, logs, seconds, 100_000);
}

fn advance_bounded(
    lua: &Lua,
    clock: &Shared<f64>,
    logs: &Shared<Vec<String>>,
    seconds: f64,
    limit: usize,
) {
    let result = (|| -> SimResult<()> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(mlua::Error::runtime("invalid time advance"));
        }
        let start = *clock.lock().unwrap_or_else(|e| e.into_inner());
        let target = start + seconds;
        if !target.is_finite() {
            return Err(mlua::Error::runtime("clock overflow"));
        }
        if seconds == 0.0 {
            return Ok(());
        }
        lua.globals().set("__selu_sim_budget", limit)?;
        let next: Function = lua.globals().get("__selu_sim_next_step")?;
        let step: Function = lua.globals().get("__selu_sim_step")?;
        let mut steps = 0;
        loop {
            let time: f64 = next.call(())?;
            if time > target && time - target > 1e-9 {
                break;
            }
            if !time.is_finite() || time < *clock.lock().unwrap_or_else(|e| e.into_inner()) {
                return Err(mlua::Error::runtime("scheduler deadline did not progress"));
            }
            steps += 1;
            if steps > limit {
                return Err(mlua::Error::runtime("dispatch budget exceeded"));
            }
            *clock.lock().unwrap_or_else(|e| e.into_inner()) = time;
            step.call::<()>(())?;
        }
        let mut now = clock.lock().unwrap_or_else(|e| e.into_inner());
        *now = now.max(target);
        Ok(())
    })();
    if let Err(error) = result {
        logs.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(format!("scheduler error: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use crate::Sim;
    #[test]
    fn review_partitioned_after() {
        let s = Sim::new().unwrap();
        s.eval("C_Timer.After(1, function() print(GetTime()) end)")
            .unwrap();
        for _ in 0..10 {
            s.advance_time(0.1);
        }
        assert_eq!(s.prints(), ["1"]);
    }
    #[test]
    fn review_error_stops_ticker() {
        let s = Sim::new().unwrap();
        s.eval("C_Timer.NewTicker(.1, function() error('stop') end)")
            .unwrap();
        s.advance_time(0.5);
        assert_eq!(s.logs().len(), 1);
    }
    #[test]
    fn review_dispatch_budget() {
        let s = Sim::new().unwrap();
        s.eval("local n=0; local function again() n=n+1; if n<100 then C_Timer.After(0, again) end end; C_Timer.After(0, again)").unwrap();
        super::advance_bounded(s.lua(), &s.clock, &s.logs, 10.0, 5);
        assert!(s.logs().iter().any(|l| l.contains("dispatch budget")));
    }
    #[test]
    fn review_tiny_period_progress() {
        let s = Sim::new().unwrap();
        s.advance_time(1.0);
        s.eval("local n=0; C_Timer.NewTicker(1e-20, function(t) n=n+1; print(GetTime()); if n==3 then t:Cancel() end end)").unwrap();
        s.advance_time(0.1);
        assert!(s.prints().iter().all(|t| t.parse::<f64>().unwrap() > 1.0));
    }
    #[test]
    fn review_rounding_and_partitioning() {
        let s = Sim::new().unwrap();
        s.eval("n=0; C_Timer.NewTicker(.1, function() n=n+1 end, 10000)")
            .unwrap();
        s.advance_time(1000.0);
        s.eval("assert(n==10000)").unwrap();
        let s = Sim::new().unwrap();
        s.eval("C_Timer.After(1, function() print(GetTime()) end)")
            .unwrap();
        for _ in 0..10 {
            s.advance_time(0.1);
        }
        assert_eq!(s.prints(), ["1"]);
    }
    #[test]
    fn review_hidden_ancestors() {
        let s = Sim::new().unwrap();
        s.eval("p=CreateFrame('Frame'); c=CreateFrame('Frame', nil, p); c:SetScript('OnUpdate', function() print('child') end); p:Hide()").unwrap();
        s.advance_time(1.0 / 60.0);
        assert!(s.prints().is_empty());
        s.eval("p:Show()").unwrap();
        s.advance_time(1.0 / 60.0);
        assert_eq!(s.prints(), ["child"]);
        s.eval("C_Timer.After(.001, function() p:Hide() end)")
            .unwrap();
        s.advance_time(0.1);
        assert_eq!(s.prints(), ["child"]);
    }
    #[test]
    fn review_error_formatting_and_ticker_failure() {
        let s = Sim::new().unwrap();
        s.eval("C_Timer.NewTicker(.1, function() error(setmetatable({}, {__tostring=function() error('format failed') end})) end); C_Timer.After(.1, function() print('survived') end)").unwrap();
        s.advance_time(0.5);
        assert_eq!(s.prints(), ["survived"]);
        assert_eq!(s.logs().len(), 1);
    }
    #[test]
    fn review_historical_iterations() {
        let s = Sim::new().unwrap();
        s.eval("for _, count in ipairs({0, -2, 2.5}) do C_Timer.NewTicker(.1, function() print(count) end, count) end").unwrap();
        s.advance_time(0.5);
        assert_eq!(s.prints(), ["0", "-2", "2.5", "2.5", "2.5"]);
    }
    #[test]
    fn timers_order_nested_cancel_and_errors() {
        let s = Sim::new().unwrap();
        s.eval(r#"
local cancelled = C_Timer.NewTimer(.1, function() error('cancelled fired') end)
cancelled:Cancel(); assert(cancelled:IsCancelled())
C_Timer.After(.2, function() print(GetTime()); C_Timer.After(.1, function() print('nested') end) end)
C_Timer.After(.1, function() error('expected timer error') end)
C_Timer.After(.2, function() print('tie') end)
local done = C_Timer.NewTimer(.1, function(self) assert(not self:IsCancelled()); print('timer') end)
C_Timer.After(.3, function() done:Cancel(); done:Cancel(); assert(done:IsCancelled()) end)
"#).unwrap();
        s.advance_time(0.5);
        assert_eq!(s.prints(), ["timer", "0.2", "tie", "nested"]);
        assert_eq!(s.logs().len(), 1);
        assert!(s.logs()[0].contains("expected timer error"));
    }
    #[test]
    fn ticker_and_zero_delay_and_onupdate() {
        let s = Sim::new().unwrap();
        s.eval(r#"
local count, elapsed = 0, 0
local f = CreateFrame('Frame')
f:SetScript('OnUpdate', function(self, dt)
 elapsed = elapsed + dt
 if elapsed >= .09 then print('update'); self:Hide() end
end)
C_Timer.After(0, function() print('next frame') end)
local ticker
 ticker = C_Timer.NewTicker(.04, function(self) assert(self == ticker); count=count+1; print(count) end, 2)
local infinite = C_Timer.NewTicker(.03, function(self) self:Cancel(); print('cancel') end)
C_Timer.After(.2, function() ticker:Cancel(); assert(infinite:IsCancelled()) end)
"#).unwrap();
        s.advance_time(0.0);
        assert!(s.prints().is_empty());
        s.advance_time(0.01);
        assert!(s.prints().is_empty());
        s.advance_time(0.29);
        assert_eq!(s.prints(), ["next frame", "cancel", "1", "2", "update"]);
    }
}

//! Deterministic hybrid steps: exact positive timer deadlines, plus a 60 Hz
//! OnUpdate grid anchored at time zero. Zero/negative delays wait 1/60 second;
//! advance_time(0) does no work. Partial advances preserve the frame grid.
//! Timers precede OnUpdate on ties; equal deadlines retain insertion order.
//! Nested due timers run in the same advance. Tickers reschedule from their
//! previous deadline, so advancing a large interval does not lose ticks.
//!
//! Checked read-only wow-ui-source HEAD UITimerDocumentation.lua: After has
//! no callback arguments; NewTimer/NewTicker supply the callback object.
//! Native scheduling and cancellation internals are not in that document.
//! Assumptions: table handles, explicit cancellation only in IsCancelled,
//! finished Cancel harmless, nonpositive delay clamped to one frame, positive
//! integer iterations required. This is not a renderer or native frame model.
use crate::{Shared, SimResult};
use mlua::{Function, Lua};

pub(crate) fn install(lua: &Lua, logs: Shared<Vec<String>>) -> SimResult<()> {
    lua.globals().set(
        "__selu_sim_error",
        lua.create_function(move |_, error: String| {
            logs.lock()
                .unwrap()
                .push(format!("callback error: {error}"));
            Ok(())
        })?,
    )?;
    lua.load(include_str!("timers.lua")).exec()
}

pub(crate) fn advance(lua: &Lua, clock: &Shared<f64>, seconds: f64) {
    assert!(
        seconds.is_finite() && seconds >= 0.0,
        "invalid time advance"
    );
    let target = *clock.lock().unwrap() + seconds;
    assert!(target.is_finite(), "clock overflow");
    let next: Function = lua.globals().get("__selu_sim_next_step").unwrap();
    let step: Function = lua.globals().get("__selu_sim_step").unwrap();
    loop {
        let time: f64 = next.call(()).unwrap();
        if time > target {
            break;
        }
        *clock.lock().unwrap() = time;
        step.call::<()>(()).unwrap();
    }
    *clock.lock().unwrap() = target;
}

#[cfg(test)]
mod tests {
    use crate::Sim;
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

//! Native and host events share ordered subscriptions and callback budgets.
use crate::{Ctx, SimResult, TraceKind};
use mlua::{Lua, MultiValue, Table, Value};
pub(crate) fn deliver(
    lua: &Lua,
    ctx: &Ctx,
    event: &str,
    args: &[Value],
    tolerant: bool,
) -> SimResult<usize> {
    let _profile = crate::profile::Span::new("events.dispatch");
    let arena = crate::frames::arena::store(lua);
    let mut delivered = 0;
    let mut next_index = 0;
    loop {
        // Query the live ordered subscription index after each callback:
        // unregistering a later frame still suppresses that delivery.
        let next = {
            let arena = arena.borrow();
            arena
                .event_subscribers
                .get(event)
                .and_then(|indices| indices.range(next_index..).next())
                .map(|&index| (index, arena.regions[index].table.get()))
        };
        let Some((index, frame)) = next else { break };
        next_index = index + 1;
        let registered: Value = frame.get::<Table>("__events")?.get(event)?;
        if matches!(registered, Value::Nil) {
            continue;
        }
        let on_event: Value = frame.get::<Table>("__scripts")?.get("OnEvent")?;
        if let Value::Function(func) = on_event {
            let mut call = MultiValue::new();
            call.push_back(Value::Table(frame));
            call.push_back(Value::String(lua.create_string(event)?));
            for a in args {
                call.push_back(a.clone());
            }
            ctx.record(TraceKind::Script, "OnEvent", vec![event.into()]);
            let result = crate::core::script_callback(lua, ctx, || {
                crate::core::call_lua::<()>(lua, func, call)
            });
            crate::core::flush(lua, ctx);
            if let Err(error) = result {
                if tolerant {
                    ctx.log(format!("error: {error}"));
                } else {
                    return Err(error);
                }
            }
            delivered += 1;
        }
    }
    Ok(delivered)
}

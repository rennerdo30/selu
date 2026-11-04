//! Callback tracing and coroutine budget propagation without changing S0.
use crate::{Ctx, SimResult, TraceKind};
use mlua::{Function, Lua, MultiValue};
#[derive(Clone, Default)]
struct Aggregates(std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<i64, (u64, u64)>>>);
pub(crate) fn since(lua: &Lua, ctx: &Ctx, from: u64) -> Vec<crate::TraceRecord> {
    let mut records = crate::core::lock(&ctx.trace).since(from);
    if let Some(aggregate) = lua.app_data_ref::<Aggregates>() {
        let map = crate::core::lock(&aggregate.0);
        for record in &mut records {
            if record.target == "OnUpdate"
                && let Some((_, count)) = map.values().find(|(seq, _)| *seq == record.seq)
            {
                record.args = vec![count.to_string()];
            }
        }
    }
    records
}
pub(crate) fn install(lua: &Lua, ctx: &Ctx) -> SimResult<()> {
    let hook_context = ctx.clone();
    lua.globals().set(
        "__selu_trace_hook",
        lua.create_function(move |_, ()| {
            hook_context.record(TraceKind::Hook, "hooksecurefunc", vec![]);
            Ok(())
        })?,
    )?;
    let context = ctx.clone();
    let aggregates = Aggregates::default();
    lua.set_app_data(aggregates.clone());
    lua.globals().set(
        "__selu_trace_callback",
        lua.create_function(
            move |lua, (name, callback, args): (String, Function, MultiValue)| {
                let record = if name == "OnUpdate" && crate::core::lock(&context.trace).enabled {
                    let second = crate::core::lock(&context.clock).floor() as i64;
                    let mut map = crate::core::lock(&aggregates.0);
                    if let Some((_, count)) = map.get_mut(&second) {
                        *count += 1;
                        false
                    } else {
                        let seq = crate::core::lock(&context.trace).count();
                        if map.len() == 10000 {
                            map.pop_first();
                        }
                        map.insert(second, (seq, 1));
                        true
                    }
                } else {
                    true
                };
                if record {
                    context.record(
                        if name.starts_with("timer") {
                            TraceKind::Timer
                        } else {
                            TraceKind::Script
                        },
                        &name,
                        vec![],
                    );
                }
                crate::core::callback_dispatch(lua, &context, || {
                    crate::core::call_lua::<MultiValue>(lua, callback, args)
                })
            },
        )?,
    )?;
    let context = ctx.clone();
    lua.globals().set(
        "__selu_hook_thread",
        lua.create_function(move |lua, thread: Option<mlua::Thread>| {
            crate::core::target_thread(lua, &context, thread);
            Ok(())
        })?,
    )?;
    lua.load(
        r#"
local resume = coroutine.resume
coroutine.resume = function(thread, ...)
 local parent = coroutine.running()
 __selu_hook_thread(thread)
 local function restore(...) __selu_hook_thread(parent); return ... end
 return restore(resume(thread, ...))
end
coroutine.wrap = function(fn)
 local thread = coroutine.create(fn)
 return function(...)
  local function result(ok, ...) if not ok then error((...), 2) end; return ... end
  return result(coroutine.resume(thread, ...))
 end
end
"#,
    )
    .exec()
}

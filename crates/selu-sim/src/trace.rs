//! Callback tracing and coroutine budget propagation without changing S0.
use crate::{Ctx, SimResult, TraceKind};
use mlua::{Function, Lua, MultiValue};
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
    lua.globals().set(
        "__selu_trace_callback",
        lua.create_function(
            move |lua, (name, callback, args): (String, Function, MultiValue)| {
                context.record(
                    if name.starts_with("timer") {
                        TraceKind::Timer
                    } else {
                        TraceKind::Script
                    },
                    &name,
                    vec![],
                );
                crate::core::dispatch(lua, &context, || callback.call::<MultiValue>(args))
            },
        )?,
    )?;
    let context = ctx.clone();
    lua.globals().set(
        "__selu_hook_thread",
        lua.create_function(move |_, thread: mlua::Thread| {
            let limit = crate::core::lock(&context.budget).limit;
            let used = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
            thread.set_hook(
                mlua::HookTriggers::new().every_nth_instruction(1),
                move |_, _| {
                    if used.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                        >= limit.saturating_mul(u64::from(crate::core::HOOK_INTERVAL))
                    {
                        Err(mlua::Error::runtime(crate::core::BUDGET_ERROR))
                    } else {
                        Ok(mlua::VmState::Continue)
                    }
                },
            );
            Ok(())
        })?,
    )?;
    lua.load(
        r#"
local create, resume = coroutine.create, coroutine.resume
coroutine.create = function(fn)
 local thread = create(fn); __selu_hook_thread(thread); return thread
end
coroutine.resume = function(thread, ...)
 __selu_hook_thread(thread); return resume(thread, ...)
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

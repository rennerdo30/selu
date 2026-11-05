//! The core contract shared by the simulator modules (docs/SIMULATOR.md
//! decisions 5, 6, 8, 15, 16 and 17). Owner: package S0 (round 8). S1
//! (`frames/`) and S2 (`session.rs`, `loader.rs`, `world.rs`, `trace.rs`,
//! `snapshot.rs`) build on these types; a change here goes to the parent
//! first, because both depend on it.
//!
//! What lives here:
//! - [`RegionId`], the generational id of a frame, texture or font string.
//! - [`Ctx`], the install context every module receives: logging, the
//!   clock, the trace, the instruction budget and the flush queue.
//! - [`dispatch`], the single entry point for host-initiated Lua calls,
//!   which resets the budget and flushes queued callbacks (decision 16).
//! - [`install_hooks`], the always-on count hook (decision 17).
//! - The immutable snapshot view ([`Snapshot`], [`RegionSnapshot`]) and the
//!   [`RegionModel`] trait the frames module implements to produce it.
//! - The trace record types ([`TraceRecord`], [`TraceKind`], [`Trace`]).
//! - [`Generation`], the session generation number.

use mlua::{HookTriggers, Lua, VmState};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

pub(crate) type Shared<T> = Arc<Mutex<T>>;

pub(crate) fn shared<T>(value: T) -> Shared<T> {
    Arc::new(Mutex::new(value))
}

/// Lock without propagating poisoning: a panic inside one callback must not
/// make the whole session unusable.
pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A region (frame, texture, font string, ...) in the session. `index` is
/// the arena slot, `generation` tells a reused slot from its earlier
/// occupant. Regions can't be destroyed in the client, so slots are never
/// reused within one VM today; the generation still makes ids from before a
/// `reload()` invalid in a new VM (it is the session [`Generation`] low bits
/// until S1's arena assigns its own).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegionId {
    pub index: u32,
    pub generation: u32,
}

/// Session generation: changes on every `reload()`, so a front end can drop
/// selections and snapshots that belong to a VM that no longer exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Generation(pub u64);

impl Generation {
    pub fn next(self) -> Generation {
        Generation(self.0 + 1)
    }
}

/// A position in loaded code, as the Lua VM reports it (`chunk` is the
/// generated file name). Front ends map it through `selu-sourcemap`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePos {
    pub chunk: String,
    pub line: u32,
}

// ---------------------------------------------------------------------------
// Snapshot view (decision 8). Plain data; Studio draws from this only.
// ---------------------------------------------------------------------------

/// A rectangle in UIParent coordinates, origin at the bottom left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub left: f64,
    pub bottom: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Strata {
    World,
    Background,
    Low,
    Medium,
    High,
    Dialog,
    Fullscreen,
    FullscreenDialog,
    Tooltip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DrawLayer {
    Background,
    Border,
    Artwork,
    Overlay,
    Highlight,
}

/// What a texture shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureFill {
    /// A file path or file id, as the addon passed it.
    File(String),
    Atlas(String),
    /// RGBA, each 0..=1.
    Color([f32; 4]),
}

/// Visual state shared by layout, snapshots and Studio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualStyle {
    pub font: String,
    pub size: f64,
    pub flags: String,
    pub color: [f32; 4],
    pub shadow: [f32; 4],
    pub shadow_offset: (f64, f64),
    pub justify_h: String,
    pub justify_v: String,
    pub wrap: bool,
    pub max_lines: usize,
    #[serde(default = "default_true")]
    pub non_space_wrap: bool,
    #[serde(default = "default_true")]
    pub ellipsis: bool,
    #[serde(default)]
    pub spacing: f64,
    pub uv: [f32; 4],
    pub tint: [f32; 4],
    pub blend: String,
}
fn default_true() -> bool {
    true
}
impl Default for VisualStyle {
    fn default() -> Self {
        Self {
            font: String::new(),
            size: 12.0,
            flags: String::new(),
            color: [1.0; 4],
            shadow: [0.0; 4],
            shadow_offset: (0.0, 0.0),
            justify_h: "LEFT".into(),
            justify_v: "TOP".into(),
            wrap: true,
            max_lines: 0,
            non_space_wrap: true,
            ellipsis: true,
            spacing: 0.0,
            uv: [0.0, 1.0, 0.0, 1.0],
            tint: [1.0; 4],
            blend: "BLEND".into(),
        }
    }
}
/// One region as the front ends see it. Fields that the current model does
/// not know yet are `None`; S1 fills them from the arena.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionSnapshot {
    pub id: RegionId,
    pub parent: Option<RegionId>,
    /// Widget type name as the manifest spells it: `Frame`, `Button`,
    /// `Texture`, `FontString`, ...
    pub widget: String,
    pub name: Option<String>,
    /// Resolved rectangle; `None` when layout can't place it (no points, a
    /// cycle, an unresolvable anchor) or isn't modelled yet.
    pub rect: Option<Rect>,
    /// Explicit size, as `SetSize` stored it.
    pub size: Option<(f64, f64)>,
    /// `IsShown`: the region's own flag.
    pub shown: bool,
    /// `IsVisible`: shown and every ancestor shown.
    pub visible: bool,
    pub strata: Option<Strata>,
    pub level: Option<i32>,
    pub layer: Option<(DrawLayer, i8)>,
    /// Effective alpha (own alpha times the ancestors').
    pub alpha: f64,
    #[serde(default)]
    pub visual: VisualStyle,
    #[serde(default)]
    pub clip: Option<Rect>,
    pub texture: Option<TextureFill>,
    pub text: Option<String>,
    #[serde(default)]
    pub text_layout: Option<crate::text::TextLayout>,
    /// Script handlers set on the region, sorted (`OnClick`, `OnEvent`, ...).
    pub scripts: Vec<String>,
    /// Where the region was created, when known.
    pub created_at: Option<SourcePos>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub generation: Generation,
    /// Simulated `GetTime()` when the snapshot was taken.
    pub time: f64,
    pub ui_width: f64,
    pub ui_height: f64,
    /// Every region, in creation order; parents come before children.
    pub regions: Vec<RegionSnapshot>,
}

/// Implemented by the frames module (S1). Neither method may run addon
/// code (decision 16): layout callbacks it discovers go into
/// [`Ctx::queue`] and run at the next flush point.
pub trait RegionModel {
    /// Resolve layout for every dirty region. The `Lua` handle is there for
    /// the round-8 Lua-table model; S1's arena shouldn't need it.
    fn resolve_layout(&self, lua: &Lua, ctx: &Ctx) -> mlua::Result<()>;
    /// The current region tree. Resolves layout first.
    fn snapshot_regions(&self, lua: &Lua, ctx: &Ctx) -> mlua::Result<Vec<RegionSnapshot>>;
}

// ---------------------------------------------------------------------------
// Trace (decision 7).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceKind {
    /// A host command (`fire`, `slash`, `click`, ...).
    Host,
    Event,
    Script,
    Timer,
    Hook,
    ProtectedCall,
    Deferred,
    Error,
    Print,
    NotSimulated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceRecord {
    /// Monotonic within a session generation.
    pub seq: u64,
    pub time: f64,
    pub kind: TraceKind,
    /// Event name, `Frame:OnClick`, timer id, ...
    pub target: String,
    /// Arguments rendered to short strings.
    pub args: Vec<String>,
    pub source: Option<SourcePos>,
}

/// Bounded ring buffer. When disabled only the counter moves, so `selu
/// test` pays almost nothing; Studio enables it.
#[derive(Debug, Clone)]
pub struct Trace {
    pub enabled: bool,
    pub capacity: usize,
    records: VecDeque<TraceRecord>,
    next_seq: u64,
}

impl Default for Trace {
    fn default() -> Self {
        Trace {
            enabled: false,
            capacity: 10_000,
            records: VecDeque::new(),
            next_seq: 0,
        }
    }
}

impl Trace {
    pub fn record(
        &mut self,
        time: f64,
        kind: TraceKind,
        target: &str,
        args: Vec<String>,
        source: Option<SourcePos>,
    ) {
        let seq = self.next_seq;
        self.next_seq += 1;
        if !self.enabled || self.capacity == 0 {
            return;
        }
        if self.records.len() == self.capacity {
            self.records.pop_front();
        }
        self.records.push_back(TraceRecord {
            seq,
            time,
            kind,
            target: target.to_string(),
            args,
            source,
        });
    }
    /// How many records were produced, kept or not.
    pub fn count(&self) -> u64 {
        self.next_seq
    }
    pub fn records(&self) -> impl Iterator<Item = &TraceRecord> {
        self.records.iter()
    }
    /// Records with `seq >= from`, for incremental delivery to a front end.
    pub fn since(&self, from: u64) -> Vec<TraceRecord> {
        self.records
            .iter()
            .filter(|r| r.seq >= from)
            .cloned()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Instruction budget (decision 17).
// ---------------------------------------------------------------------------

/// Instructions between two count-hook calls.
pub const HOOK_INTERVAL: u32 = 10_000;

/// Default budget per dispatch, in hook calls (here 50 million
/// instructions; a fraction of a second on a desktop).
pub const DEFAULT_BUDGET: u64 = 5_000;

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    /// Hook calls allowed per dispatch.
    pub limit: u64,
    used: u64,
    callbacks: usize,
    pending_tasks: usize,
    callback_depth: usize,
    failure: Option<&'static str>,
    /// The limit was hit in this dispatch; the hook now fails on every
    /// instruction.
    exhausted: bool,
}

pub const BUDGET_ERROR: &str = "script ran too long";

// ---------------------------------------------------------------------------
// Install context and dispatch (decision 16).
// ---------------------------------------------------------------------------

/// A callback that layout (or another host-side change) caused and that must
/// run outside the call that discovered it: `OnSizeChanged`, `OnShow` from a
/// parent being shown, ...
pub type Pending = Box<dyn FnOnce(&Lua) -> mlua::Result<()>>;

/// The shared state every module gets at install time. Cheap to clone (all
/// handles). Rust code must not hold one of these locks across a call into
/// Lua (decision 15).
#[derive(Debug, Clone)]
pub struct RuntimeFailure {
    pub kind: String,
    pub message: String,
}

#[derive(Clone)]
pub struct Ctx {
    pub failures: Shared<Vec<RuntimeFailure>>,
    pub stale_sources: Shared<HashSet<String>>,
    pub prints: Shared<Vec<String>>,
    pub logs: Shared<Vec<String>>,
    pub(crate) warned: Shared<HashSet<String>>,
    pub clock: Shared<f64>,
    pub trace: Shared<Trace>,
    pub(crate) budget: Shared<Budget>,
    pub(crate) pending: Shared<VecDeque<Pending>>,
    /// Nesting depth of [`dispatch`]; only the outermost resets and flushes.
    pub(crate) depth: Shared<u32>,
}

impl Ctx {
    pub fn new() -> Ctx {
        Ctx {
            failures: shared(Vec::new()),
            stale_sources: shared(HashSet::new()),
            prints: shared(Vec::new()),
            logs: shared(Vec::new()),
            warned: shared(HashSet::new()),
            clock: shared(0.0),
            trace: shared(Trace::default()),
            budget: shared(Budget {
                limit: DEFAULT_BUDGET,
                used: 0,
                callbacks: 0,
                pending_tasks: 0,
                callback_depth: 0,
                failure: None,
                exhausted: false,
            }),
            pending: shared(VecDeque::new()),
            depth: shared(0),
        }
    }

    /// Start a front-end observation interval without clearing prints or world state.
    pub fn begin_observation(&self) {
        lock(&self.failures).clear();
        lock(&self.warned).clear();
        lock(&self.logs).clear();
    }

    pub fn now(&self) -> f64 {
        *lock(&self.clock)
    }

    pub fn log(&self, line: impl Into<String>) {
        lock(&self.logs).push(line.into());
    }
    pub fn runtime_failure(&self, kind: &str, message: impl Into<String>) {
        lock(&self.failures).push(RuntimeFailure {
            kind: kind.into(),
            message: message.into(),
        });
    }

    pub fn runtime_failures(&self) -> Vec<RuntimeFailure> {
        lock(&self.failures).clone()
    }

    /// Log `not simulated: what` the first time only (SPEC 8).
    pub fn not_simulated(&self, what: &str) {
        if lock(&self.warned).insert(what.to_string()) {
            self.log(format!("not simulated: {what}"));
            self.record(TraceKind::NotSimulated, what, Vec::new());
        }
    }

    pub fn record(&self, kind: TraceKind, target: &str, args: Vec<String>) {
        let now = self.now();
        lock(&self.trace).record(now, kind, target, args, None);
    }

    /// Queue a callback for the next flush point.
    pub fn queue(&self, callback: Pending) {
        lock(&self.pending).push_back(callback);
    }

    /// Hook calls allowed per dispatch (see [`HOOK_INTERVAL`]).
    pub fn set_budget(&self, hook_calls: u64) {
        lock(&self.budget).limit = hook_calls.max(1);
    }
}

impl Default for Ctx {
    fn default() -> Self {
        Ctx::new()
    }
}

/// Install the always-on count hook. Call once per VM. Line hooks for the
/// debugger (round 10) join this hook here, in one place, because Lua has a
/// single hook per thread.
///
/// Coroutine resume wrappers coordinate the VM's single mlua hook target;
/// instruction usage remains shared with synchronous and queued scripts.
pub fn install_hooks(lua: &Lua, ctx: &Ctx) {
    set_count_hook(lua, ctx.budget.clone(), HOOK_INTERVAL);
}

/// Once the budget is exhausted the hook fires on *every* instruction and
/// fails each time, until the next dispatch restores the normal interval.
/// That way the error surfaces in every frame up the stack: a `pcall`
/// around a runaway loop catches it, but its caller's next instruction
/// fails again, so `while true do pcall(f) end` can't keep running either.
fn budget_instruction(budget: &Shared<Budget>) -> mlua::Result<VmState> {
    let mut b = lock(budget);
    b.used = b.used.saturating_add(1);
    if b.exhausted || b.used > b.limit.saturating_mul(u64::from(HOOK_INTERVAL)) {
        b.exhausted = true;
        return Err(mlua::Error::runtime(b.failure.unwrap_or(BUDGET_ERROR)));
    }
    Ok(VmState::Continue)
}

// mlua 0.10 has one hook target per VM. Switch that target only at native
// resume boundaries, restoring the caller before executing more Lua. Count
// every instruction so repeated short coroutine resumes cannot lose credit.
fn set_count_hook(lua: &Lua, budget: Shared<Budget>, _interval: u32) {
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(1),
        move |lua, debug| {
            reject_stale(lua, debug.source().source.as_deref())?;
            budget_instruction(&budget)
        },
    );
}

pub(crate) fn target_thread(lua: &Lua, ctx: &Ctx, thread: Option<mlua::Thread>) {
    let budget = ctx.budget.clone();
    if let Some(thread) = thread {
        thread.set_hook(
            HookTriggers::new().every_nth_instruction(1),
            move |lua, debug| {
                reject_stale(lua, debug.source().source.as_deref())?;
                budget_instruction(&budget)
            },
        );
    } else {
        set_count_hook(lua, budget, 1);
    }
}

fn reject_stale(lua: &Lua, source: Option<&str>) -> mlua::Result<()> {
    if let Some(ctx) = lua.app_data_ref::<Ctx>()
        && source.is_some_and(|s| lock(&ctx.stale_sources).contains(s.trim_start_matches('@')))
    {
        return Err(mlua::Error::runtime(
            "SELU0623: stale addon reference after sim.reload(); use sim helpers to access the current Session",
        ));
    }
    Ok(())
}

/// Reset the budget for a new dispatch, restoring the normal hook interval
/// if the previous dispatch exhausted it.
fn reset_budget(lua: &Lua, ctx: &Ctx) {
    let mut b = lock(&ctx.budget);
    b.used = 0;
    b.callbacks = 0;
    b.pending_tasks = 0;
    b.callback_depth = 0;
    b.failure = None;
    if b.exhausted {
        b.exhausted = false;
        drop(b);
        set_count_hook(lua, ctx.budget.clone(), HOOK_INTERVAL);
    }
}

#[derive(Clone, Copy)]
struct CallbackInstructions {
    limit: u64,
    used: u64,
}
/// One outer timer dispatch retains S1 callback/nesting/task accounting.
/// Scheduler instructions have their own allowance; all addon callbacks and
/// their queued continuations share the configured instruction allowance.
pub(crate) fn scheduler_dispatch<R>(
    lua: &Lua,
    ctx: &Ctx,
    call: impl FnOnce() -> mlua::Result<R>,
) -> mlua::Result<R> {
    let limit = lock(&ctx.budget).limit;
    let previous = lua.remove_app_data::<CallbackInstructions>();
    lua.set_app_data(CallbackInstructions { limit, used: 0 });
    lock(&ctx.budget).limit = DEFAULT_BUDGET;
    let result = dispatch(lua, ctx, call);
    let instructions = lua.remove_app_data::<CallbackInstructions>().unwrap();
    {
        let mut b = lock(&ctx.budget);
        b.limit = limit;
        b.used = instructions.used;
    }
    if let Some(previous) = previous {
        lua.set_app_data(previous);
    }
    result
}
pub(crate) fn callback_dispatch<R>(
    lua: &Lua,
    ctx: &Ctx,
    call: impl FnOnce() -> mlua::Result<R>,
) -> mlua::Result<R> {
    let Some(instructions) = lua.app_data_ref::<CallbackInstructions>().map(|v| *v) else {
        return call();
    };
    let (limit, used) = {
        let mut b = lock(&ctx.budget);
        let saved = (b.limit, b.used);
        b.limit = instructions.limit;
        b.used = instructions.used;
        saved
    };
    let result = call();
    flush(lua, ctx);
    let callback_used = {
        let mut b = lock(&ctx.budget);
        let callback_used = b.used;
        b.limit = limit;
        b.used = used;
        callback_used
    };
    lua.app_data_mut::<CallbackInstructions>().unwrap().used = callback_used;
    result
}

#[derive(Debug)]
struct NativeBoundaryFailure(String);
impl std::fmt::Display for NativeBoundaryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for NativeBoundaryFailure {}
pub(crate) fn boundary_was_reported(error: &mlua::Error) -> bool {
    error.downcast_ref::<NativeBoundaryFailure>().is_some()
        || error.parent().is_some_and(boundary_was_reported)
}
fn render_boundary_error(lua: &Lua, error: mlua::Value) -> mlua::Result<String> {
    if let mlua::Value::String(text) = error {
        return Ok(text.to_string_lossy());
    }
    let fallback = error.type_name().to_string();
    let mut text = lua
        .globals()
        .get::<mlua::Function>("tostring")
        .and_then(|format| format.call::<mlua::Value>(error))
        .ok()
        .and_then(|v| {
            if let mlua::Value::String(s) = v {
                Some(s.to_string_lossy())
            } else {
                None
            }
        })
        .unwrap_or(fallback);
    // xpcall invokes its handler before unwinding the failing stack.
    text.push_str("\nstack traceback:");
    for level in 0..128 {
        let Some(frame) = lua.inspect_stack(level) else {
            break;
        };
        let source = frame.source();
        if source.what == "C" {
            continue;
        }
        let name = source.short_src.as_deref().unwrap_or("?");
        text.push_str(&format!("\n\t{name}:{}", frame.curr_line()));
    }
    Ok(text)
}

/// Protect Lua calls *before* mlua stringifies non-string error objects.
/// The handler is only at the host boundary: addon pcall/coroutine code
/// continues to receive the original error value unchanged.
pub(crate) fn call_lua<R: mlua::FromLuaMulti>(
    lua: &Lua,
    function: mlua::Function,
    args: impl mlua::IntoLuaMulti,
) -> mlua::Result<R> {
    const KEY: &str = "selu.host_boundary";
    let protected = match lua.named_registry_value::<Option<mlua::Function>>(KEY)? {
        Some(function) => function,
        None => {
            let render = lua.create_function(|lua, error: mlua::Value| {
                if let mlua::Value::Error(ref cause) = error
                    && boundary_was_reported(cause)
                {
                    return Ok(error);
                }
                let notified = crate::world::notify_native_error(lua, error.clone())?;
                let message = render_boundary_error(lua, error)?;
                if notified {
                    lua.app_data_ref::<Ctx>()
                        .unwrap()
                        .runtime_failure("native boundary", message.clone());
                    Ok(mlua::Value::Error(Box::new(mlua::Error::external(
                        NativeBoundaryFailure(message),
                    ))))
                } else {
                    Ok(mlua::Value::String(lua.create_string(message)?))
                }
            })?;
            let factory: mlua::Function = lua
                .load(
                    r#"
local xpcall, error, unpack, select = xpcall, error, unpack, select
return function(render)
 return function(fn, ...)
  local args, count = {...}, select('#', ...)
  local function result(ok, ...) if not ok then error((...),0) end; return ... end
  return result(xpcall(function() return fn(unpack(args,1,count)) end,render))
 end
end
"#,
                )
                .set_name("=selu host boundary")
                .eval()?;
            let function: mlua::Function = factory.call(render)?;
            lua.set_named_registry_value(KEY, function.clone())?;
            function
        }
    };
    let mut args = args.into_lua_multi(lua)?;
    args.push_front(mlua::Value::Function(function));
    protected.call(args)
}

/// Run one host-initiated call (an event, a timer advance, a script call, an
/// input action, an `eval`). The outermost call resets the instruction
/// budget, then flushes queued callbacks before control returns to the host.
/// The call's own error is returned, as before; a budget error is also
/// logged as `error: ...`, so a runaway loop is visible even when the host
/// ignores the result. Errors from flushed callbacks are logged.
pub fn dispatch<R>(
    lua: &Lua,
    ctx: &Ctx,
    call: impl FnOnce() -> mlua::Result<R>,
) -> mlua::Result<R> {
    let outermost = {
        let mut depth = lock(&ctx.depth);
        *depth += 1;
        *depth == 1
    };
    if outermost {
        reset_budget(lua, ctx);
    }
    let result = call();
    if outermost {
        if let Err(e) = &result {
            let text = e.to_string();
            if text.contains(BUDGET_ERROR) {
                ctx.log(format!("error: {text}"));
            }
            ctx.record(TraceKind::Error, "dispatch", vec![text]);
        }
        flush(lua, ctx);
    }
    *lock(&ctx.depth) -= 1;
    result
}

/// Every script entry, including synchronous visibility and Lua-side timers,
/// shares these limits. A small depth cap protects the native Rust/C stack.
pub(crate) fn begin_callback(lua: &Lua, ctx: &Ctx) -> mlua::Result<()> {
    let (reason, new_failure) = {
        let mut b = lock(&ctx.budget);
        let reason = if b.exhausted {
            Some(b.failure.unwrap_or(BUDGET_ERROR))
        } else if b.callbacks >= 10_000 {
            Some("dispatch callback budget exhausted")
        } else if b.callback_depth >= 32 {
            Some("script callbacks nested too deeply")
        } else {
            None
        };
        if let Some(reason) = reason {
            let fresh = !b.exhausted;
            b.exhausted = true;
            b.failure = Some(reason);
            (Some(reason), fresh)
        } else {
            b.callbacks += 1;
            b.callback_depth += 1;
            (None, false)
        }
    };
    if let Some(reason) = reason {
        if new_failure {
            ctx.log(format!("error: {reason}"));
            ctx.record(TraceKind::Error, "callback", vec![reason.into()]);
            set_count_hook(lua, ctx.budget.clone(), 1);
        }
        return Err(mlua::Error::runtime(reason));
    }
    Ok(())
}
pub(crate) fn end_callback(ctx: &Ctx) {
    let mut b = lock(&ctx.budget);
    b.callback_depth = b.callback_depth.saturating_sub(1);
}
struct CallbackGuard<'a>(&'a Ctx);
impl Drop for CallbackGuard<'_> {
    fn drop(&mut self) {
        end_callback(self.0);
    }
}
/// Account before entering Lua; unwind the nesting count even on an error.
pub(crate) fn script_callback<R>(
    lua: &Lua,
    ctx: &Ctx,
    call: impl FnOnce() -> mlua::Result<R>,
) -> mlua::Result<R> {
    begin_callback(lua, ctx)?;
    let _guard = CallbackGuard(ctx);
    call()
}

/// Run queued callbacks until the queue is empty. Callbacks share the outer dispatch instruction and callback budgets;
/// callbacks a callback queues run in the same flush.
pub(crate) fn flush(lua: &Lua, ctx: &Ctx) {
    loop {
        let Some(next) = lock(&ctx.pending).pop_front() else {
            break;
        };
        let stop = {
            let mut b = lock(&ctx.budget);
            b.pending_tasks += 1;
            b.exhausted || b.pending_tasks > 10_000
        };
        if stop {
            ctx.runtime_failure("flush", "dispatch callback/instruction budget exhausted");
            lock(&ctx.pending).clear();
            ctx.log("error: dispatch callback/instruction budget exhausted");
            ctx.record(
                TraceKind::Error,
                "flush",
                vec!["dispatch budget exhausted".into()],
            );
            break;
        }
        if let Err(e) = next(lua) {
            let text = e.to_string();
            if !boundary_was_reported(&e) {
                ctx.runtime_failure("flush", text.clone());
            }
            ctx.log(format!("error: {text}"));
            ctx.record(TraceKind::Error, "flush", vec![text]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_is_bounded_and_counts_when_disabled() {
        let mut t = Trace::default();
        t.record(0.0, TraceKind::Event, "A", vec![], None);
        assert_eq!(t.count(), 1);
        assert_eq!(t.records().count(), 0, "disabled keeps nothing");
        t.enabled = true;
        t.capacity = 2;
        for name in ["B", "C", "D"] {
            t.record(1.0, TraceKind::Event, name, vec![], None);
        }
        let kept: Vec<_> = t.records().map(|r| r.target.as_str()).collect();
        assert_eq!(kept, ["C", "D"]);
        assert_eq!(t.since(3).len(), 1);
        assert_eq!(t.count(), 4);
    }

    #[test]
    fn runaway_loop_hits_the_budget_instead_of_hanging() {
        let lua = Lua::new();
        let ctx = Ctx::new();
        ctx.set_budget(20);
        install_hooks(&lua, &ctx);
        let err = dispatch(&lua, &ctx, || lua.load("while true do end").exec()).unwrap_err();
        assert!(err.to_string().contains(BUDGET_ERROR), "{err}");
        // `pcall` can't keep a runaway loop alive.
        let err = dispatch(&lua, &ctx, || {
            lua.load("while true do pcall(function() while true do end end) end")
                .exec()
        })
        .unwrap_err();
        assert!(err.to_string().contains(BUDGET_ERROR), "{err}");
        // A fresh dispatch gets a fresh budget.
        dispatch(&lua, &ctx, || {
            lua.load("local x = 0 for i = 1, 1000 do x = x + i end")
                .exec()
        })
        .unwrap();
        let logs = lock(&ctx.logs).clone();
        assert_eq!(logs.len(), 2, "{logs:?}");
        assert!(logs[0].starts_with("error: "), "{logs:?}");
    }

    #[test]
    fn queued_callbacks_flush_after_the_outermost_dispatch() {
        let lua = Lua::new();
        let ctx = Ctx::new();
        install_hooks(&lua, &ctx);
        lua.globals()
            .set("order", lua.create_table().unwrap())
            .unwrap();
        let inner = ctx.clone();
        dispatch(&lua, &ctx, || {
            inner.queue(Box::new(|lua| {
                lua.load("order[#order + 1] = 'queued'").exec()
            }));
            // A nested dispatch must not flush early.
            dispatch(&lua, &inner, || {
                lua.load("order[#order + 1] = 'nested'").exec()
            })?;
            lua.load("order[#order + 1] = 'call'").exec()
        })
        .unwrap();
        let order: Vec<String> = lua
            .load("return table.concat(order, ',')")
            .eval::<String>()
            .unwrap()
            .split(',')
            .map(String::from)
            .collect();
        assert_eq!(order, ["nested", "call", "queued"]);
    }

    #[test]
    fn not_simulated_logs_once() {
        let ctx = Ctx::new();
        ctx.not_simulated("Frame:Wiggle");
        ctx.not_simulated("Frame:Wiggle");
        assert_eq!(lock(&ctx.logs).as_slice(), ["not simulated: Frame:Wiggle"]);
    }

    #[test]
    fn snapshot_serialises() {
        let snap = Snapshot {
            generation: Generation(2),
            time: 1.5,
            ui_width: 1920.0,
            ui_height: 1080.0,
            regions: vec![RegionSnapshot {
                id: RegionId {
                    index: 0,
                    generation: 2,
                },
                parent: None,
                widget: "Frame".into(),
                name: Some("DemoFrame".into()),
                rect: None,
                size: Some((100.0, 20.0)),
                shown: true,
                visible: true,
                strata: Some(Strata::FullscreenDialog),
                level: Some(1),
                layer: Some((DrawLayer::Artwork, 0)),
                alpha: 1.0,
                visual: VisualStyle::default(),
                clip: None,
                texture: Some(TextureFill::Color([1.0, 0.0, 0.0, 1.0])),
                text: None,
                text_layout: None,
                scripts: vec!["OnEvent".into()],
                created_at: None,
            }],
        };
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"FULLSCREEN_DIALOG\""), "{json}");
        let back: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, snap);
    }
}

/// Font-backed measurement using the same fallback face as region layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMeasurement {
    pub width: f64,
    pub height: f64,
}
pub fn measure_text(text: &str, font_size: f64) -> TextMeasurement {
    let style = VisualStyle {
        size: font_size,
        ..Default::default()
    };
    let layout = crate::fonts::FontBook::default().layout(text, &style, None, None);
    TextMeasurement {
        width: layout.width,
        height: layout.height,
    }
}
#[cfg(test)]
mod proportional_falsifier {
    #[test]
    fn measured_advances_come_from_the_font() {
        assert!(
            super::measure_text("WWW", 18.0).width > super::measure_text("iii", 18.0).width * 2.0
        );
    }
}

#[cfg(test)]
mod text_tests {
    #[test]
    fn unicode_and_markup() {
        let m = super::measure_text("|cffffffff\u{732b}\u{e9}|r\n|Hitem:1|hX|h|Ticon:12|t", 12.0);
        let (first, h) = crate::text::expected_fallback("\u{732b}\u{e9}", 12.0);
        let second = crate::text::expected_fallback("X", 12.0).0 + 12.0;
        assert!((m.width - first.max(second)).abs() < 1e-5);
        assert!((m.height - 2.0 * h).abs() < 1e-5);
    }
}

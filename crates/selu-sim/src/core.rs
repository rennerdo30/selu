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
    pub texture: Option<TextureFill>,
    pub text: Option<String>,
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
#[derive(Clone)]
pub struct Ctx {
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
            prints: shared(Vec::new()),
            logs: shared(Vec::new()),
            warned: shared(HashSet::new()),
            clock: shared(0.0),
            trace: shared(Trace::default()),
            budget: shared(Budget {
                limit: DEFAULT_BUDGET,
                used: 0,
                exhausted: false,
            }),
            pending: shared(VecDeque::new()),
            depth: shared(0),
        }
    }

    pub fn now(&self) -> f64 {
        *lock(&self.clock)
    }

    pub fn log(&self, line: impl Into<String>) {
        lock(&self.logs).push(line.into());
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
/// Limitation: Lua 5.1 hooks are per coroutine; code running inside a
/// coroutine the addon created is not budgeted yet (S2 wraps
/// `coroutine.create` to install the hook on new threads).
pub fn install_hooks(lua: &Lua, ctx: &Ctx) {
    set_count_hook(lua, ctx.budget.clone(), HOOK_INTERVAL);
}

/// Once the budget is exhausted the hook fires on *every* instruction and
/// fails each time, until the next dispatch restores the normal interval.
/// That way the error surfaces in every frame up the stack: a `pcall`
/// around a runaway loop catches it, but its caller's next instruction
/// fails again, so `while true do pcall(f) end` can't keep running either.
fn set_count_hook(lua: &Lua, budget: Shared<Budget>, interval: u32) {
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(interval),
        move |lua, _| {
            let mut b = lock(&budget);
            if !b.exhausted {
                b.used += 1;
                if b.used <= b.limit {
                    return Ok(VmState::Continue);
                }
                b.exhausted = true;
                drop(b);
                // Safe inside a hook: mlua keeps the running callback alive
                // and `lua_sethook` may be called at any time.
                set_count_hook(lua, budget.clone(), 1);
                b = lock(&budget);
            }
            let limit = b.limit * u64::from(HOOK_INTERVAL);
            Err(mlua::Error::RuntimeError(format!(
                "{BUDGET_ERROR} (more than {limit} instructions in one dispatch)"
            )))
        },
    );
}

/// Reset the budget for a new dispatch, restoring the normal hook interval
/// if the previous dispatch exhausted it.
fn reset_budget(lua: &Lua, ctx: &Ctx) {
    let mut b = lock(&ctx.budget);
    b.used = 0;
    if b.exhausted {
        b.exhausted = false;
        drop(b);
        set_count_hook(lua, ctx.budget.clone(), HOOK_INTERVAL);
    }
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

/// Run queued callbacks until the queue is empty. Each gets a fresh budget;
/// callbacks a callback queues run in the same flush.
fn flush(lua: &Lua, ctx: &Ctx) {
    loop {
        let Some(next) = lock(&ctx.pending).pop_front() else {
            break;
        };
        reset_budget(lua, ctx);
        if let Err(e) = next(lua) {
            let text = e.to_string();
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
                texture: Some(TextureFill::Color([1.0, 0.0, 0.0, 1.0])),
                text: None,
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

/// Fixed-width metric shared with Studio: Unicode characters have width
/// `0.5 * font_size`, lines have height `font_size`. WoW markup is ignored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMeasurement {
    pub width: f64,
    pub height: f64,
}
pub fn measure_text(text: &str, font_size: f64) -> TextMeasurement {
    let mut chars = text.chars().peekable();
    let (mut width, mut longest, mut lines) = (0usize, 0usize, 1usize);
    while let Some(c) = chars.next() {
        if c == '|' {
            match chars.peek().copied() {
                Some('c') => {
                    chars.next();
                    for _ in 0..8 {
                        chars.next();
                    }
                    continue;
                }
                Some('r') | Some('h') => {
                    chars.next();
                    continue;
                }
                Some('T') | Some('H') => {
                    let end = if chars.next() == Some('T') { 't' } else { 'h' };
                    while let Some(c) = chars.next() {
                        if c == '|' && chars.peek() == Some(&end) {
                            chars.next();
                            break;
                        }
                    }
                    continue;
                }
                Some('|') => {
                    chars.next();
                }
                _ => {}
            }
        }
        if c == '\n' {
            longest = longest.max(width);
            width = 0;
            lines += 1;
        } else {
            width += 1;
        }
    }
    TextMeasurement {
        width: longest.max(width) as f64 * 0.5 * font_size,
        height: lines as f64 * font_size,
    }
}
#[cfg(test)]
mod text_tests {
    #[test]
    fn unicode_and_markup() {
        let m = super::measure_text("|cffffffff猫é|r\n|Hitem:1|hX|h|Ticon:12|t", 12.0);
        assert_eq!((m.width, m.height), (12.0, 24.0));
    }
}

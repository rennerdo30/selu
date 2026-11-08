//! Execution taint (SL-35).
//!
//! The client runs Blizzard's code "secure" and addon code "insecure". Once
//! insecure code runs inside a call, the rest of that call is tainted by that
//! addon, and protected actions in it are refused: `ADDON_ACTION_BLOCKED`
//! (combat-restricted frame actions) or `ADDON_ACTION_FORBIDDEN` (protected
//! functions). Here every function is attributed by the chunk it was loaded
//! from: chunks loaded for a user addon belong to that addon; Blizzard's and
//! the simulator's own chunks are secure. The native call hook
//! (`fast_hook`) taints execution when a call enters an addon function, so
//! direct Lua-to-Lua calls (a Blizzard layout calling an addon closure) count
//! too. A host-level entry starts secure; `securecall` and
//! `secureexecuterange` give each callback its own taint and restore the
//! caller's afterwards, as the client does.
//!
//! Not modeled: per-variable and per-value taint (`issecurevariable` and
//! `issecurevalue` report every value secure, with a once-only note), taint spreading through table writes, and
//! protection inherited through anchoring.
use mlua::{Lua, Value};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// `forceinsecure()` taints with this name, as the client does.
pub const FORCED: &str = "*** ForceTaint_Strong ***";

#[derive(Debug, Clone, PartialEq)]
pub struct TaintRecord {
    pub time: f64,
    /// `ADDON_ACTION_BLOCKED` or `ADDON_ACTION_FORBIDDEN`.
    pub event: &'static str,
    pub addon: String,
    /// `Frame:Method()` or `Function()`.
    pub action: String,
}

#[derive(Default)]
pub struct Taint {
    /// Chunk name -> owning addon. Only insecure (addon) chunks are listed.
    owners: HashMap<String, Rc<str>>,
    /// Chunk names Blizzard (secure) code also uses; they stay secure.
    secure_names: std::collections::HashSet<String>,
    /// Interned chunk-name pointer -> owner, filled by the call hook.
    pub(crate) by_source: HashMap<usize, Option<Rc<str>>>,
    pub(crate) current: Option<Rc<str>>,
    /// Nesting of host calls into Lua; 0 means the engine is calling.
    pub(crate) lua_depth: u32,
    pub(crate) log: Vec<TaintRecord>,
}
pub type SharedTaint = Rc<RefCell<Taint>>;

impl Taint {
    /// Attribute a chunk to an addon. A name Blizzard code also loads stays
    /// secure: a wrongly tainted Blizzard function would block real UI.
    pub fn register(&mut self, chunk: &str, addon: &str) {
        if !self.secure_names.contains(chunk) {
            self.owners.insert(chunk.to_owned(), Rc::from(addon));
            self.by_source.clear();
        }
    }
    pub fn register_secure(&mut self, chunk: &str) {
        self.secure_names.insert(chunk.to_owned());
        if self.owners.remove(chunk).is_some() {
            self.by_source.clear();
        }
    }
    pub fn tracking(&self) -> bool {
        !self.owners.is_empty()
    }
    /// Owner of a function's chunk, from its Lua source string.
    pub(crate) fn owner_of_source(&self, source: &str) -> Option<Rc<str>> {
        self.owners.get(source).cloned()
    }
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }
    pub fn log(&self) -> &[TaintRecord] {
        &self.log
    }
}

/// Lua 5.1 keeps `[string "name"]` sources as the bare chunk name.
pub(crate) fn source_name(source: &str) -> &str {
    source
        .strip_prefix('=')
        .or_else(|| source.strip_prefix('@'))
        .unwrap_or(source)
}

/// Taint the current call by `function`'s owner, if it is an addon's.
pub(crate) fn enter_function(taint: &SharedTaint, function: &mlua::Function) {
    let mut t = taint.borrow_mut();
    if t.current.is_some() || !t.tracking() {
        return;
    }
    if let Some(source) = function.info().source {
        t.current = t.owner_of_source(source_name(&source));
    }
}

/// Record a refused protected action and queue the client's event.
pub(crate) fn refuse(lua: &Lua, ctx: &crate::Ctx, event: &'static str, action: String) {
    let Some(addon) = ctx.taint.borrow().current.clone() else {
        return;
    };
    let record = TaintRecord {
        time: ctx.now(),
        event,
        addon: addon.to_string(),
        action: action.clone(),
    };
    ctx.log(format!("taint: {event} {addon} {action}"));
    ctx.record(
        crate::core::TraceKind::Error,
        event,
        vec![addon.to_string(), action.clone()],
    );
    ctx.taint.borrow_mut().log.push(record);
    let context = ctx.clone();
    let _ = lua;
    ctx.queue(Box::new(move |lua| {
        let tolerant = *crate::core::lock(&context.tolerant_ui);
        let args = vec![
            Value::String(lua.create_string(&*addon)?),
            Value::String(lua.create_string(&action)?),
        ];
        crate::events::deliver(lua, &context, event, &args, tolerant)
            .map(|_| ())
            .map_err(|e| mlua::Error::runtime(e.to_string()))
    }));
}

/// Frame methods the client refuses on a protected frame in combat.
pub(crate) fn combat_protected_method(method: &str) -> bool {
    matches!(
        method,
        "Show"
            | "Hide"
            | "SetShown"
            | "SetPoint"
            | "SetAllPoints"
            | "ClearAllPoints"
            | "SetParent"
            | "SetSize"
            | "SetWidth"
            | "SetHeight"
            | "SetScale"
            | "SetAttribute"
            | "SetFrameStrata"
            | "SetFrameLevel"
            | "Raise"
            | "Lower"
            | "EnableMouse"
            | "RegisterForClicks"
    )
}

/// `issecure`, `forceinsecure` and `issecurevariable`.
pub(crate) fn install(lua: &Lua, ctx: &crate::Ctx) -> mlua::Result<()> {
    let globals = lua.globals();
    let taint = ctx.taint.clone();
    globals.raw_set(
        "issecure",
        lua.create_function(move |_, ()| Ok(taint.borrow().current.is_none()))?,
    )?;
    let taint = ctx.taint.clone();
    globals.raw_set(
        "forceinsecure",
        lua.create_function(move |_, ()| {
            let mut t = taint.borrow_mut();
            if t.current.is_none() {
                t.current = Some(Rc::from(FORCED));
            }
            Ok(())
        })?,
    )?;
    // Whether a timer or OnUpdate callback counts for `sim.callbackCounts`:
    // once an addon is loaded only addon code counts, so Blizzard's own UI
    // does not hide what the addon under test schedules.
    let taint = ctx.taint.clone();
    globals.raw_set(
        "__selu_counts_callback",
        lua.create_function(move |_, f: Value| {
            let t = taint.borrow();
            let Value::Function(f) = f else {
                return Ok(!t.tracking());
            };
            if !t.tracking() {
                return Ok(true);
            }
            Ok(f.info()
                .source
                .is_some_and(|source| t.owner_of_source(source_name(&source)).is_some()))
        })?,
    )?;
    let context = ctx.clone();
    globals.raw_set(
        "issecurevalue",
        lua.create_function(move |_, _: mlua::MultiValue| {
            context.not_simulated("issecurevalue (per-value taint)");
            Ok(true)
        })?,
    )?;
    let context = ctx.clone();
    globals.raw_set(
        "issecurevariable",
        lua.create_function(move |_, _: mlua::MultiValue| {
            context.not_simulated("issecurevariable (per-variable taint)");
            Ok((true, Value::Nil))
        })?,
    )?;
    Ok(())
}

/// Run `call` with its own taint, restoring the caller's afterwards: the
/// client's `securecall` keeps a callee's taint from reaching the caller.
pub(crate) fn isolated<R>(taint: &SharedTaint, call: impl FnOnce() -> R) -> R {
    let saved = taint.borrow().current.clone();
    let result = call();
    taint.borrow_mut().current = saved;
    result
}

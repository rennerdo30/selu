//! The Session API, the contract between the core and its front ends
//! (`selu test`, Selu Studio, Rust tests). docs/SIMULATOR.md decisions 5,
//! 13, 18, 20 and 22. Owner: package S2.
//!
//! Session owns VM setup and host state; Sim forwards to this API.

use crate::core::{Generation, Snapshot, TraceRecord, lock};
use crate::core::{Shared, dispatch};
use crate::frames::LuaTableModel;
use crate::{Ctx, TraceKind, core, frames, log_once, regions, timers, value_string, wow_api};
use crate::{SimResult, loader, snapshot};
use mlua::Value;
use mlua::{Function, Lua, MultiValue, Table};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// VM-independent host commands retained across reload (decision 22).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum HostCommand {
    Fire { event: String, args: Vec<String> },
    Slash { line: String },
    AdvanceTime { seconds: f64 },
    Reload,
    Eval { code: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    /// `flavour@version`, selecting flavour-specific `.toc` files.
    pub target: Option<String>,
    /// UIParent size in pixels at scale 1 (decision 4).
    pub ui_size: (f64, f64),
    /// Seeds `math.random` (decision 13).
    pub seed: u32,
    /// Keep trace records (Studio) or only count them (`selu test`).
    pub trace: bool,
    pub account: String,
    pub realm: String,
    pub character: String,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            target: None,
            ui_size: snapshot::DEFAULT_UI_SIZE,
            seed: 1,
            trace: false,
            account: "Default".into(),
            realm: "Realm".into(),
            character: "Player".into(),
        }
    }
}

pub struct Session {
    config: SessionConfig,
    sim: Vm,
    generation: Generation,
    addons: Vec<PathBuf>,
    logged_in: bool,
    saved: crate::saved::SavedSnapshot,
    loading: HashSet<PathBuf>,
    commands: std::cell::RefCell<Vec<HostCommand>>,
}

fn load_dynamic(
    vm: &Vm,
    dir: &Path,
    config: &SessionConfig,
    saved: &crate::saved::SavedSnapshot,
    visiting: &mut HashSet<PathBuf>,
) -> SimResult<()> {
    let dir = dir.canonicalize().map_err(|e| runtime(e.to_string()))?;
    let loaded: Table = vm.lua.globals().get("__selu_loaded_dirs")?;
    if loaded
        .raw_get::<Option<bool>>(dir.to_string_lossy().as_ref())?
        .unwrap_or(false)
    {
        return Ok(());
    }
    if !visiting.insert(dir.clone()) {
        return Err(runtime("cyclic addon dependencies".into()));
    }
    let toc = loader::read_toc_target(&dir, config.target.as_deref()).map_err(runtime)?;
    for dep in toc
        .list("Dependencies")
        .into_iter()
        .chain(toc.list("RequiredDeps"))
    {
        load_dynamic(
            vm,
            &dir.parent().unwrap().join(dep),
            config,
            saved,
            visiting,
        )?;
    }
    Session::load_into(vm, &dir, config, saved)?;
    loaded.set(dir.to_string_lossy().as_ref(), true)?;
    loaded.raw_set(loaded.raw_len() + 1, dir.to_string_lossy().as_ref())?;
    visiting.remove(&dir);
    Ok(())
}

fn saved_key(config: &SessionConfig, toc: &loader::Toc, name: &str) -> String {
    let character = if toc
        .list("SavedVariablesPerCharacter")
        .contains(&name.to_string())
    {
        format!("{}:{}", config.realm, config.character)
    } else {
        String::new()
    };
    format!(
        "{:?}/{:?}/{:?}/{:?}/{name}",
        config.target, config.account, character, toc.name
    )
}

fn runtime(message: String) -> mlua::Error {
    mlua::Error::RuntimeError(message)
}

impl Session {
    pub fn new(config: SessionConfig) -> SimResult<Session> {
        let sim = Self::start_vm(&config)?;
        Ok(Session {
            config,
            sim,
            generation: Generation::default(),
            addons: Vec::new(),
            logged_in: false,
            saved: Default::default(),
            loading: HashSet::new(),
            commands: Default::default(),
        })
    }

    fn start_vm(config: &SessionConfig) -> SimResult<Vm> {
        let sim = Vm::new()?;
        lock(&sim.ctx().trace).enabled = config.trace;
        sim.lua()
            .load(format!("math.randomseed({})", config.seed))
            .exec()?;
        Ok(sim)
    }

    /// Advance until no timers or visible OnUpdate callbacks remain, bounded
    /// by simulated seconds and host steps. Returns whether idle was reached.
    pub fn run_until_idle(&self, max_seconds: f64, max_steps: usize) -> SimResult<bool> {
        if !max_seconds.is_finite() || max_seconds < 0.0 {
            return Err(runtime("invalid idle bound".into()));
        }
        let end = self.now() + max_seconds;
        for _ in 0..max_steps {
            let pending: bool = self
                .sim
                .lua
                .globals()
                .get::<Function>("__selu_sim_pending")?
                .call(())?;
            if !pending {
                return Ok(true);
            }
            let next: f64 = self
                .sim
                .lua
                .globals()
                .get::<Function>("__selu_sim_next_step")?
                .call(())?;
            if next > end {
                return Ok(false);
            }
            self.advance_time((next - self.now()).max(0.0));
        }
        Ok(!self
            .sim
            .lua
            .globals()
            .get::<Function>("__selu_sim_pending")?
            .call::<bool>(())?)
    }

    pub fn host_commands(&self) -> Vec<HostCommand> {
        self.commands.borrow().clone()
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }
    pub fn generation(&self) -> Generation {
        self.generation
    }
    /// The underlying VM, for tests and for what the Session API doesn't
    /// cover yet.
    pub fn sim(&self) -> &Vm {
        &self.sim
    }

    /// Load an addon folder through its `.toc` and fire `ADDON_LOADED`.
    /// Returns the addon name.
    pub fn load_addon_dir(&mut self, dir: &Path) -> SimResult<String> {
        let dir = dir.canonicalize().map_err(|e| runtime(e.to_string()))?;
        let toc = loader::read_toc_target(&dir, self.config.target.as_deref()).map_err(runtime)?;
        if self.addons.contains(&dir) {
            return Ok(toc.name);
        }
        if !self.loading.insert(dir.clone()) {
            return Err(runtime("cyclic addon dependencies".into()));
        }
        let result = (|| {
            for dep in toc
                .list("Dependencies")
                .into_iter()
                .chain(toc.list("RequiredDeps"))
            {
                self.load_addon_dir(&dir.parent().unwrap().join(dep))?;
            }
            Self::load_into(&self.sim, &dir, &self.config, &self.saved)?;
            self.addons.push(dir.clone());
            Ok(toc.name)
        })();
        self.loading.remove(&dir);
        if result.is_ok() {
            self.sim
                .lua
                .globals()
                .get::<Table>("__selu_loaded_dirs")?
                .set(dir.to_string_lossy().as_ref(), true)?;
            self.install_lod(dir.parent().unwrap())?;
        }
        result
    }

    fn install_lod(&self, root: &Path) -> SimResult<()> {
        let root = root.to_path_buf();
        let ctx = self.sim.ctx.clone();
        let config = self.config.clone();
        let saved = self.saved.clone();
        let api: Table = self.sim.lua.create_table()?;
        api.set(
            "LoadAddOn",
            self.sim.lua.create_function(move |lua, name: String| {
                let vm = Vm {
                    lua: lua.clone(),
                    ctx: ctx.clone(),
                    prints: ctx.prints.clone(),
                    logs: ctx.logs.clone(),
                    clock: ctx.clock.clone(),
                };
                if name.contains(['/', '\\']) || name == ".." {
                    return Err(runtime("invalid addon name".into()));
                }
                load_dynamic(&vm, &root.join(name), &config, &saved, &mut HashSet::new())?;
                Ok(true)
            })?,
        )?;
        self.sim.lua.globals().set("C_AddOns", api)
    }

    fn load_into(
        sim: &Vm,
        dir: &Path,
        config: &SessionConfig,
        saved: &crate::saved::SavedSnapshot,
    ) -> SimResult<String> {
        let toc = loader::read_toc_target(dir, config.target.as_deref()).map_err(runtime)?;
        for name in toc
            .list("SavedVariables")
            .into_iter()
            .chain(toc.list("SavedVariablesPerCharacter"))
        {
            sim.lua
                .globals()
                .get::<Table>("__selu_saved_names")?
                .raw_set(name.as_str(), true)?;
            if let Some(value) = saved.0.get(&saved_key(config, &toc, &name)) {
                sim.lua
                    .globals()
                    .raw_set(name, crate::saved::restore(&sim.lua, value)?)?;
            }
        }
        let files = loader::read_files(&toc).map_err(runtime)?;
        sim.load_addon(&toc.name, &files)?;
        Ok(toc.name)
    }

    /// The once-per-session login sequence, after every initially loaded
    /// addon (decision 18).
    pub fn login(&mut self) -> SimResult<()> {
        if self.logged_in {
            return Ok(());
        }
        self.logged_in = true;
        self.sim.fire("PLAYER_LOGIN", vec![])?;
        self.sim.fire(
            "PLAYER_ENTERING_WORLD",
            vec![
                Value::Boolean(self.generation.0 == 0),
                Value::Boolean(self.generation.0 != 0),
            ],
        )?;
        Ok(())
    }

    /// Drop the VM, start a new one, load every addon again and log in again
    /// if the session had. The generation changes. Saved data is restored before ADDON_LOADED.
    pub fn reload(&mut self) -> SimResult<()> {
        self.commands.borrow_mut().push(HostCommand::Reload);
        let loaded: Table = self.sim.lua.globals().get("__selu_loaded_dirs")?;
        for entry in loaded.sequence_values::<String>() {
            let path = PathBuf::from(entry?);
            if !self.addons.contains(&path) {
                self.addons.push(path);
            }
        }
        self.sim.fire("PLAYER_LOGOUT", vec![])?;
        let mut saved = self.saved.clone();
        for dir in &self.addons {
            let toc =
                loader::read_toc_target(dir, self.config.target.as_deref()).map_err(runtime)?;
            for name in toc
                .list("SavedVariables")
                .into_iter()
                .chain(toc.list("SavedVariablesPerCharacter"))
            {
                saved.0.insert(
                    saved_key(&self.config, &toc, &name),
                    crate::saved::capture(
                        self.sim.lua.globals().raw_get(name)?,
                        &mut HashSet::new(),
                    )?,
                );
            }
        }
        let mut world = Vec::new();
        for name in [
            "__selu_world_units",
            "__selu_world_money",
            "__selu_world_bags",
        ] {
            world.push((
                name,
                crate::saved::capture(self.sim.lua.globals().raw_get(name)?, &mut HashSet::new())?,
            ));
        }
        let replacement = Self::start_vm(&self.config)?;
        for (name, value) in world {
            let restored = crate::saved::restore(&replacement.lua, &value)?;
            // Unit API closures hold the original units table.
            if name == "__selu_world_units" {
                let table: Table = replacement.lua.globals().raw_get(name)?;
                if let Value::Table(values) = restored {
                    for pair in values.pairs::<Value, Value>() {
                        let (k, v) = pair?;
                        table.raw_set(k, v)?;
                    }
                }
            } else {
                replacement.lua.globals().raw_set(name, restored)?;
            }
        }
        self.saved = saved;
        self.sim = replacement;
        if let Some(dir) = self.addons.first() {
            self.install_lod(dir.parent().unwrap())?;
        }
        self.generation = self.generation.next();
        for dir in &self.addons {
            Self::load_into(&self.sim, dir, &self.config, &self.saved)?;
            let loaded: Table = self.sim.lua.globals().get("__selu_loaded_dirs")?;
            loaded.set(dir.to_string_lossy().as_ref(), true)?;
        }
        if self.logged_in {
            self.logged_in = false;
            self.login()?;
        }
        Ok(())
    }

    pub fn fire(&self, event: &str, args: Vec<Value>) -> SimResult<usize> {
        self.commands.borrow_mut().push(HostCommand::Fire {
            event: event.into(),
            args: args.iter().map(value_string).collect(),
        });
        self.sim.fire(event, args)
    }
    pub fn slash(&self, line: &str) -> SimResult<()> {
        self.commands
            .borrow_mut()
            .push(HostCommand::Slash { line: line.into() });
        self.sim.slash(line)
    }
    pub fn eval(&self, source: &str) -> SimResult<()> {
        self.commands.borrow_mut().push(HostCommand::Eval {
            code: source.into(),
        });
        self.sim.eval(source)
    }
    pub fn advance_time(&self, seconds: f64) {
        self.commands
            .borrow_mut()
            .push(HostCommand::AdvanceTime { seconds });
        self.sim.advance_time(seconds)
    }
    pub fn now(&self) -> f64 {
        self.sim.now()
    }
    pub fn prints(&self) -> Vec<String> {
        self.sim.prints()
    }
    pub fn logs(&self) -> Vec<String> {
        self.sim.logs()
    }

    pub fn snapshot(&self) -> SimResult<Snapshot> {
        let model = LuaTableModel {
            generation: self.generation.0 as u32,
        };
        snapshot::take(
            self.sim.lua(),
            self.sim.ctx(),
            &model,
            self.generation,
            self.config.ui_size,
        )
    }

    /// Trace records with `seq >= from` (empty unless `config.trace`).
    pub fn trace_since(&self, from: u64) -> Vec<TraceRecord> {
        lock(&self.sim.ctx().trace).since(from)
    }
}

#[derive(Clone)]
pub struct Vm {
    pub(crate) lua: Lua,
    pub(crate) ctx: Ctx,
    pub(crate) prints: Shared<Vec<String>>,
    pub(crate) logs: Shared<Vec<String>>,
    pub(crate) clock: Shared<f64>,
}

impl Vm {
    pub fn new() -> SimResult<Vm> {
        let lua = Lua::new();
        let ctx = Ctx::new();
        core::install_hooks(&lua, &ctx);
        regions::install(&lua)?;
        let prints = ctx.prints.clone();
        let logs = ctx.logs.clone();
        let warned = ctx.warned.clone();
        let clock = ctx.clock.clone();
        let globals = lua.globals();
        globals.set("__selu_saved_names", lua.create_table()?)?;

        {
            let ctx = ctx.clone();
            globals.set(
                "print",
                lua.create_function(move |_, args: MultiValue| {
                    let parts: Vec<String> = args.iter().map(value_string).collect();
                    let line = parts.join("\t");
                    ctx.record(TraceKind::Print, "print", vec![line.clone()]);
                    lock(&ctx.prints).push(line);
                    Ok(())
                })?,
            )?;
        }
        {
            let clock = clock.clone();
            globals.set(
                "GetTime",
                lua.create_function(move |_, ()| Ok(*clock.lock().unwrap()))?,
            )?;
        }

        crate::install_frames(&lua, &logs, &warned)?;
        // Unknown globals: log once, hand back a callable no-op so the
        // addon keeps running and the gap is visible.
        {
            let mt = lua.create_table()?;
            let logs = logs.clone();
            let warned = warned.clone();
            mt.set(
                "__index",
                lua.create_function(move |lua, (_t, key): (Value, String)| {
                    let names: Table = lua.globals().raw_get("__selu_saved_names")?;
                    if names
                        .raw_get::<Option<bool>>(key.as_str())?
                        .unwrap_or(false)
                    {
                        return Ok(Value::Nil);
                    }
                    log_once(&logs, &warned, &key);
                    let noop = lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?;
                    Ok(Value::Function(noop))
                })?,
            )?;
            globals.set_metatable(Some(mt));
        }
        wow_api::install(&lua)?;
        timers::install(&lua, logs.clone())?;
        lua.globals()
            .set("__selu_loaded_dirs", lua.create_table()?)?;
        crate::world::install(&lua)?;
        crate::trace::install(&lua, &ctx)?;

        Ok(Vm {
            lua,
            ctx,
            prints,
            logs,
            clock,
        })
    }

    /// Load generated Lua files in TOC order: `(file name, source)` pairs.
    pub fn load_files(&self, files: &[(String, String)]) -> SimResult<()> {
        for (name, source) in files {
            dispatch(&self.lua, &self.ctx, || {
                self.lua.load(source).set_name(name).exec()
            })?;
        }
        Ok(())
    }

    /// Load an addon's files in TOC order the way the client does: every
    /// file receives `...` = the addon's name and one table shared by all of
    /// its files.
    pub fn load_addon(&self, addon: &str, files: &[(String, String)]) -> SimResult<()> {
        let ns = self.lua.create_table()?;
        for (name, source) in files {
            dispatch(&self.lua, &self.ctx, || {
                self.lua
                    .load(source)
                    .set_name(name)
                    .call::<()>((addon, ns.clone()))
            })?;
        }
        self.fire(
            "ADDON_LOADED",
            vec![Value::String(self.lua.create_string(addon)?)],
        )?;
        Ok(())
    }

    pub fn slash(&self, line: &str) -> SimResult<()> {
        self.ctx
            .record(TraceKind::Host, "slash", vec![line.to_string()]);
        dispatch(&self.lua, &self.ctx, || {
            self.lua
                .globals()
                .get::<Function>("__selu_sim_slash")?
                .call(line)
        })
    }

    pub fn eval(&self, source: &str) -> SimResult<()> {
        dispatch(&self.lua, &self.ctx, || self.lua.load(source).exec())
    }

    /// Fire an event at every frame that registered for it.
    pub fn fire(&self, event: &str, args: Vec<Value>) -> SimResult<usize> {
        let rendered = args.iter().map(value_string).collect();
        self.ctx.record(TraceKind::Event, event, rendered);
        self.fire_now(event, &args)
    }

    fn fire_now(&self, event: &str, args: &[Value]) -> SimResult<usize> {
        let registry = self.lua.globals().get::<Table>("__selu_sim_frames")?;
        let mut delivered = 0;
        for pair in registry.pairs::<i64, Table>() {
            let (_, frame) = pair?;
            let registered: Value = frame.get::<Table>("__events")?.get(event)?;
            if matches!(registered, Value::Nil) {
                continue;
            }
            let on_event: Value = frame.get::<Table>("__scripts")?.get("OnEvent")?;
            if let Value::Function(func) = on_event {
                let mut call = MultiValue::new();
                call.push_back(Value::Table(frame));
                call.push_back(Value::String(self.lua.create_string(event)?));
                for a in args {
                    call.push_back(a.clone());
                }
                self.ctx
                    .record(TraceKind::Script, "OnEvent", vec![event.into()]);
                dispatch(&self.lua, &self.ctx, || func.call::<()>(call))?;
                delivered += 1;
            }
        }
        Ok(delivered)
    }

    pub fn prints(&self) -> Vec<String> {
        self.prints.lock().unwrap().clone()
    }
    pub fn logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().clone()
    }
    pub fn now(&self) -> f64 {
        *self.clock.lock().unwrap()
    }
    pub fn advance_time(&self, seconds: f64) {
        timers::advance(&self.lua, &self.clock, &self.logs, seconds);
    }
    pub fn lua(&self) -> &Lua {
        &self.lua
    }
    /// The install context: logs, clock, trace, budget and flush queue.
    pub fn ctx(&self) -> &Ctx {
        &self.ctx
    }
    /// The region tree through the core's snapshot view (generation 0; a
    /// [`Session`] stamps its own).
    pub fn snapshot(&self) -> SimResult<Snapshot> {
        let model = frames::LuaTableModel { generation: 0 };
        snapshot::take(
            &self.lua,
            &self.ctx,
            &model,
            Generation::default(),
            snapshot::DEFAULT_UI_SIZE,
        )
    }
}

impl std::ops::Deref for Session {
    type Target = Vm;
    fn deref(&self) -> &Vm {
        &self.sim
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_through_the_toc_logs_in_and_reloads() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/addons/Hello");
        let mut s = Session::new(SessionConfig {
            trace: true,
            ..SessionConfig::default()
        })
        .unwrap();
        assert_eq!(s.load_addon_dir(&dir).unwrap(), "Hello");
        s.login().unwrap();
        assert_eq!(
            s.prints(),
            ["Hello loaded", "Hello: login", "Hello: entering world true"]
        );
        let first = s.snapshot().unwrap();
        assert_eq!(first.generation, Generation(0));
        assert!(first.regions.iter().any(|r| r.scripts == ["OnEvent"]));
        let events: Vec<_> = s
            .trace_since(0)
            .into_iter()
            .filter(|r| r.kind == crate::TraceKind::Event)
            .map(|r| r.target)
            .collect();
        assert_eq!(
            events,
            ["ADDON_LOADED", "PLAYER_LOGIN", "PLAYER_ENTERING_WORLD"]
        );

        s.reload().unwrap();
        assert_eq!(s.generation(), Generation(1));
        assert_eq!(s.prints().len(), 3, "a reload starts a fresh VM");
        assert_eq!(s.snapshot().unwrap().regions[0].id.generation, 1);
    }

    #[test]
    fn xml_files_are_a_visible_error() {
        let dir = std::env::temp_dir().join(format!("selu-sim-xml-{}", std::process::id()));
        let addon = dir.join("WithXml");
        std::fs::create_dir_all(&addon).unwrap();
        std::fs::write(addon.join("WithXml.toc"), "main.lua\nframes.xml\n").unwrap();
        std::fs::write(addon.join("main.lua"), "print('x')").unwrap();
        let mut s = Session::new(SessionConfig::default()).unwrap();
        let err = s.load_addon_dir(&addon).unwrap_err().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(err.contains("XML files are not supported"), "{err}");
    }

    #[test]
    fn math_random_is_seeded() {
        let roll = |seed| {
            let s = Session::new(SessionConfig {
                seed,
                ..SessionConfig::default()
            })
            .unwrap();
            s.eval("print(math.random(1, 1000000))").unwrap();
            s.prints()
        };
        assert_eq!(roll(7), roll(7));
    }
}

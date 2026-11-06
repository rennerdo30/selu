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
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// VM-independent host commands retained across reload (decision 22).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum HostCommand {
    Fire {
        event: String,
        args: Vec<HostValue>,
    },
    SetUnit {
        token: String,
        unit: crate::world::Unit,
    },
    SetCombat {
        combat: bool,
    },
    SetMoney {
        copper: u64,
    },
    SetBag {
        bag: i32,
        items: Vec<u32>,
    },
    Slash {
        line: String,
    },
    AdvanceTime {
        seconds: f64,
    },
    Reload,
    SwitchCharacter {
        character: String,
    },
    Eval {
        code: String,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum HostValue {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
    Bytes { bytes: Vec<u8> },
    Table(Vec<(HostValue, HostValue)>),
}
impl HostValue {
    /// Reconstruct the original Lua value, including binary strings and keys.
    pub fn to_lua(&self, lua: &Lua) -> SimResult<Value> {
        Ok(match self {
            Self::Nil => Value::Nil,
            Self::Bool(v) => Value::Boolean(*v),
            Self::Number(v) => Value::Number(*v),
            Self::String(v) => Value::String(lua.create_string(v)?),
            Self::Bytes { bytes } => Value::String(lua.create_string(bytes)?),
            Self::Table(pairs) => {
                let table = lua.create_table()?;
                for (key, value) in pairs {
                    table.raw_set(key.to_lua(lua)?, value.to_lua(lua)?)?;
                }
                Value::Table(table)
            }
        })
    }
    pub fn capture(v: &Value) -> SimResult<Self> {
        fn convert(v: crate::saved::SavedValue) -> HostValue {
            use crate::saved::SavedValue as S;
            match v {
                S::Nil => HostValue::Nil,
                S::Boolean(v) => HostValue::Bool(v),
                S::Number(v) => HostValue::Number(v),
                S::String(v) => match String::from_utf8(v) {
                    Ok(text) => HostValue::String(text),
                    Err(error) => HostValue::Bytes {
                        bytes: error.into_bytes(),
                    },
                },
                S::Table(v) => HostValue::Table(
                    v.into_iter()
                        .map(|(k, v)| (convert(k), convert(v)))
                        .collect(),
                ),
            }
        }
        Ok(convert(crate::saved::capture(
            v.clone(),
            &mut HashSet::new(),
        )?))
    }
}
#[derive(Default)]
struct VmState {
    tolerant_ui: bool,
    blizzard_loaded: HashSet<String>,
    loaded: Vec<PathBuf>,
    loading: HashSet<PathBuf>,
    commands: Vec<HostCommand>,
    marks: std::collections::VecDeque<(u64, Snapshot)>,
    generation: Generation,
    ui_size: Option<(f64, f64)>,
    frame_families: std::collections::BTreeSet<(String, String)>,
    checked_manifests: std::collections::BTreeMap<PathBuf, selu_manifest::Manifest>,
    manual_frame_families: std::collections::BTreeSet<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    /// The checked target manifest. Front ends may pass it directly; otherwise
    /// sessions resolve `target` from the nearest manifests directory.
    pub manifest: Option<selu_manifest::Manifest>,
    /// Explicit read-only client install; None keeps deterministic offline sessions.
    pub client_install: Option<PathBuf>,
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
            manifest: None,
            client_install: None,
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

/// Open the configured client's textures and fonts for the session's
/// product (from its manifest, else its target). Without a product there is
/// nothing to open yet.
fn open_client_assets(sim: &Vm, config: &SessionConfig) -> SimResult<()> {
    let Some(root) = &config.client_install else {
        return Ok(());
    };
    let target = config
        .manifest
        .as_ref()
        .map(selu_manifest::Manifest::target_string)
        .or_else(|| config.target.clone());
    if let Some(target) = target {
        let product = target.split('@').next().unwrap_or_default();
        crate::frames::arena::store(sim.lua())
            .borrow_mut()
            .fonts
            .open_client(root, product)
            .map_err(mlua::Error::runtime)?;
    }
    Ok(())
}

/// A saved fixture failed validation; retain its global for source diagnostics.
#[derive(Debug)]
pub struct SavedFixtureError {
    pub global: String,
    cause: mlua::Error,
}
impl std::fmt::Display for SavedFixtureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}
impl std::error::Error for SavedFixtureError {}

pub struct Session {
    default_ui_source: Option<PathBuf>,
    default_ui_report: Option<loader::DefaultUiReport>,
    default_ui_log_end: usize,
    config: SessionConfig,
    sim: Vm,
    generation: Generation,
    logged_in: bool,
    saved: crate::saved::SavedSnapshot,
}

type CheckedManifests = std::collections::BTreeMap<PathBuf, selu_manifest::Manifest>;
fn snapshot_target(config: &SessionConfig, checked: &CheckedManifests) -> Option<String> {
    config.target.clone().or_else(|| {
        checked
            .values()
            .next()
            .map(selu_manifest::Manifest::target_string)
    })
}
fn validate_snapshots(checked: &CheckedManifests) -> SimResult<()> {
    let mut target = None;
    let mut families = std::collections::BTreeMap::new();
    for (path, m) in checked {
        let current = m.target_string();
        if target.as_ref().is_some_and(|t| t != &current) {
            return Err(runtime(format!(
                "incompatible checked snapshot target `{current}` for {}",
                path.display()
            )));
        }
        target = Some(current);
        for family in &m.frame_families {
            let key = (&family.prefix, &family.suffix);
            if let Some(previous) = families.insert(key, family)
                && (previous.widget != family.widget
                    || previous.min_index != family.min_index
                    || previous.max_index != family.max_index)
            {
                return Err(runtime(format!(
                    "incompatible checked frame family `{}...{}` in {}",
                    family.prefix,
                    family.suffix,
                    path.display()
                )));
            }
        }
    }
    Ok(())
}
fn install_snapshots(vm: &Vm, checked: CheckedManifests) -> SimResult<()> {
    validate_snapshots(&checked)?;
    let manual = lock(&vm.state).manual_frame_families.clone();
    let mut families = manual;
    for m in checked.values() {
        crate::sim_globals::install_manifest(vm.lua(), m, &vm.ctx)?;
        families.extend(
            m.frame_families
                .iter()
                .map(|f| (f.prefix.clone(), f.suffix.clone())),
        );
    }
    let table = vm.lua.create_table()?;
    for (prefix, suffix) in &families {
        table.raw_set(
            table.raw_len() + 1,
            vm.lua
                .create_sequence_from([prefix.as_str(), suffix.as_str()])?,
        )?;
    }
    vm.lua.globals().raw_set("__selu_frame_families", table)?;
    let mut state = lock(&vm.state);
    state.checked_manifests = checked;
    state.frame_families = families;
    Ok(())
}
fn register_addon_snapshot(vm: &Vm, dir: &Path, config: &SessionConfig) -> SimResult<()> {
    // Reload installs the complete prevalidated set before executing any addon.
    if lock(&vm.state).checked_manifests.contains_key(dir) {
        return Ok(());
    }
    let mut checked = lock(&vm.state).checked_manifests.clone();
    let target = snapshot_target(config, &checked);
    match crate::snapshot_cache::compiled_manifest_for_target(dir, target.as_deref())
        .map_err(runtime)?
    {
        Some(m) => {
            checked.insert(dir.to_path_buf(), m);
            install_snapshots(vm, checked)?;
        }
        None if checked.contains_key(dir) => {
            return Err(runtime(format!(
                "checked snapshot missing for {}",
                dir.display()
            )));
        }
        None => {}
    }
    Ok(())
}
fn load_dynamic(
    vm: &Vm,
    dir: &Path,
    config: &SessionConfig,
    saved: &crate::saved::SavedSnapshot,
) -> SimResult<String> {
    let dir = dir.canonicalize().map_err(|e| runtime(e.to_string()))?;
    let toc = loader::read_toc_target(&dir, config.target.as_deref()).map_err(runtime)?;
    {
        let mut state = lock(&vm.state);
        if state.loading.contains(&dir) {
            return Err(runtime("cyclic addon dependencies".into()));
        }
        if state.loaded.contains(&dir) {
            return Ok(toc.name);
        }
        state.loading.insert(dir.clone());
    }
    let result = (|| {
        for dep in toc
            .list("Dependencies")
            .into_iter()
            .chain(toc.list("RequiredDeps"))
        {
            load_dynamic(vm, &dir.parent().unwrap().join(dep), config, saved)?;
        }
        register_addon_snapshot(vm, &dir, config)?;
        lock(&vm.state).loaded.push(dir.clone());
        Session::load_into(vm, &dir, config, saved)?;
        lock(&vm.state).loading.remove(&dir);
        vm.fire(
            "ADDON_LOADED",
            vec![
                Value::String(vm.lua.create_string(&toc.name)?),
                Value::Boolean(false),
            ],
        )?;
        Ok(toc.name)
    })();
    {
        let mut state = lock(&vm.state);
        let unfinished = state.loading.remove(&dir);
        if result.is_err() && unfinished {
            state.loaded.retain(|path| path != &dir);
        }
    }
    if result.is_err() {
        let mut checked = lock(&vm.state).checked_manifests.clone();
        if checked.remove(&dir).is_some() {
            let empty = checked.is_empty();
            install_snapshots(vm, checked)?;
            if empty && let Some(m) = &config.manifest {
                for f in &m.frame_families {
                    register_family(vm, &f.prefix, &f.suffix)?;
                }
            }
        }
    }
    result
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

fn register_family(vm: &Vm, prefix: &str, suffix: &str) -> SimResult<()> {
    if lock(&vm.state)
        .frame_families
        .insert((prefix.into(), suffix.into()))
    {
        let families: Table = vm.lua.globals().raw_get("__selu_frame_families")?;
        families.raw_set(
            families.raw_len() + 1,
            vm.lua.create_sequence_from([prefix, suffix])?,
        )?;
    }
    Ok(())
}
fn discover_manifest(
    config: &SessionConfig,
    from: &Path,
) -> SimResult<Option<selu_manifest::Manifest>> {
    let inferred = from.ancestors().find_map(|dir| {
        let text = std::fs::read_to_string(dir.join("selu.toml")).ok()?;
        let project: toml::Value = toml::from_str(&text).ok()?;
        let targets = project.get("addon")?.get("targets")?.as_array()?;
        if targets.len() == 1 {
            return targets[0].as_str().map(str::to_owned);
        }
        let addon = from.file_name()?.to_str()?;
        targets
            .iter()
            .filter_map(|v| v.as_str())
            .find(|t| addon.ends_with(&format!("-{}", t.split('@').next().unwrap_or(t))))
            .map(str::to_owned)
    });
    let Some(target) = config.target.as_ref().or(inferred.as_ref()) else {
        return Ok(None);
    };
    // Session targets also select TOC flavours for plain Lua/synthetic addons,
    // whose labels (for example retail@test) need not be manifest versions.
    let Ok(spec) = selu_manifest::TargetSpec::parse(target) else {
        return Ok(None);
    };
    if let Ok(all) =
        selu_manifest::resolution::discover(None, from, Some(std::slice::from_ref(target)))
    {
        return Ok(selu_manifest::resolve(&spec, &all).ok().cloned());
    }
    Ok(None)
}

fn runtime(message: String) -> mlua::Error {
    mlua::Error::RuntimeError(message)
}

impl Session {
    pub fn default_ui_report(&self) -> Option<&loader::DefaultUiReport> {
        self.default_ui_report.as_ref()
    }
    /// Manifest native stand-ins ranked by calls in this default UI session.
    pub fn missing_natives(&self) -> Vec<(String, u64)> {
        crate::native_defaults::counts(&self.sim.lua)
    }
    pub fn default_ui_startup_errors(&self) -> Vec<String> {
        lock(&self.sim.logs)
            .iter()
            .skip(self.default_ui_log_end)
            .filter(|s| s.starts_with("error:") || s.starts_with("callback error"))
            .cloned()
            .collect()
    }
    /// Load external Blizzard sources before the user's addon. Each source
    /// chunk has its own error boundary; failures remain visible in the receipt.
    /// This method performs no saved-variable reads or writes.
    pub fn load_default_ui(
        &mut self,
        source: &Path,
        mut progress: impl FnMut(&str),
    ) -> SimResult<crate::loader::DefaultUiReport> {
        let _profile = crate::profile::Span::new("startup.unattributed");
        let _layout_batch = crate::frames::arena::LayoutBatch::new(&self.sim.lua);
        let _bootstrap_budget = core::BlizzardBootstrapBudget::new(&self.sim.lua, &self.sim.ctx);
        let gc_pause = crate::profile::GcPause::new(&self.sim.lua);
        eprintln!("Default UI layout transaction: active; resolves after source construction");
        let start = std::time::Instant::now();
        let log_start = lock(&self.sim.logs).len();
        lock(&self.sim.state).tolerant_ui = true;
        *lock(&self.sim.ctx.tolerant_ui) = true;
        self.sim
            .lua
            .set_app_data(crate::native_defaults::CallCounts::default());
        crate::native_stdlib::install(&self.sim.lua)?;
        crate::client_ui::install(&self.sim.lua, &self.sim.ctx)?;
        let mut events: HashSet<String> = self
            .config
            .manifest
            .as_ref()
            .map(|m| m.events.iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default();
        events.extend(
            [
                "VARIABLES_LOADED",
                "ADDON_LOADED",
                "PLAYER_LOGIN",
                "PLAYER_ENTERING_WORLD",
                "PLAYER_LEAVING_WORLD",
                "PLAYER_LOGOUT",
                "CVAR_UPDATE",
                "CVAR_LOADED",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        let event_api = self
            .sim
            .lua
            .globals()
            .raw_get::<Option<Table>>("C_EventUtils")?
            .unwrap_or(self.sim.lua.create_table()?);
        event_api.raw_set(
            "IsEventValid",
            self.sim
                .lua
                .create_function(move |_, name: String| Ok(events.contains(&name)))?,
        )?;
        self.sim.lua.globals().raw_set("C_EventUtils", event_api)?;
        self.sim.ctx.not_simulated("native taint tracking");
        self.sim
            .ctx
            .not_simulated("localized SecondsFormatter rendering (configuration only)");
        let ctx = self.sim.ctx.clone();
        self.sim.lua.globals().raw_set(
            "__selu_secure_invoke",
            self.sim.lua.create_function(
                move |lua, (function, args): (Function, MultiValue)| match core::script_callback(
                    lua,
                    &ctx,
                    || core::call_lua::<MultiValue>(lua, function, args),
                ) {
                    Ok(values) => Ok(values),
                    Err(error) => {
                        ctx.log(format!("error: {error}"));
                        core::check_budget(&ctx)?;
                        Ok(MultiValue::new())
                    }
                },
            )?,
        )?;
        let build = self
            .config
            .manifest
            .as_ref()
            .map(|m| (&m.build, m.interface));
        let chunk = crate::host_lua::load(&self.sim.lua, include_str!("default_world.lua"));
        if let Some((b, interface)) = build {
            chunk.call::<()>((
                format!("{}.{}.{}", b.major, b.minor, b.patch),
                b.build,
                interface,
            ))?;
        } else {
            chunk.call::<()>(())?;
        }
        crate::host_lua::load(&self.sim.lua, include_str!("default_curves.lua")).exec()?;
        let source_root = source.canonicalize().map_err(|e| runtime(e.to_string()))?;
        let mut source_notes = Vec::new();
        let cached_storage = crate::frames::arena::store(&self.sim.lua)
            .borrow()
            .fonts
            .assets
            .client_storage();
        let storage = if cached_storage.is_some() {
            cached_storage
        } else if let (Some(install), Some(target)) =
            (&self.config.client_install, &self.config.target)
        {
            match selu_gamedata::Storage::open(
                install,
                target.split('@').next().unwrap_or(""),
                None,
            ) {
                Ok(storage) => Some(std::sync::Arc::new(storage)),
                Err(e) => {
                    source_notes.push(format!("CASC UI unavailable; using source checkout: {e}"));
                    None
                }
            }
        } else {
            None
        };
        let _text_cache = crate::ui_source_cache::Scope::new(
            &source_root,
            storage
                .as_ref()
                .map(|storage| storage.build.build_key.as_str()),
        );
        let client = storage.as_deref().map(|s| (s, source_root.as_path()));
        let (addons, mut report) = loader::default_ui_order_client(
            &source_root,
            self.config.target.as_deref(),
            storage.as_deref(),
        )
        .map_err(runtime)?;
        report.notes.extend(source_notes);
        let saved_names: Table = self.sim.lua.globals().raw_get("__selu_saved_names")?;
        for toc in &addons {
            for name in toc
                .list("SavedVariables")
                .into_iter()
                .chain(toc.list("SavedVariablesPerCharacter"))
            {
                saved_names.raw_set(name, true)?;
            }
        }
        self.default_ui_source = Some(source.to_path_buf());
        let api = self
            .sim
            .lua
            .globals()
            .raw_get::<Option<Table>>("C_AddOns")?
            .unwrap_or(self.sim.lua.create_table()?);
        let state = self.sim.state.clone();
        let loaded = self.sim.lua.create_function(move |_, name: String| {
            let state = lock(&state);
            let loaded = state.blizzard_loaded.contains(&name)
                || state
                    .loaded
                    .iter()
                    .any(|p| p.file_name().is_some_and(|n| n == name.as_str()));
            Ok((loaded, loaded))
        })?;
        api.raw_set("IsAddOnLoaded", loaded.clone())?;
        self.sim.lua.globals().raw_set("IsAddOnLoaded", loaded)?;
        self.sim.lua.globals().raw_set("C_AddOns", api)?;
        for toc in addons {
            progress(&toc.name);
            // A bad input file must not discard every successfully read chunk
            // in the addon. The native error handler allows later files to load.
            let mut files = Vec::new();
            for file in &toc.files {
                let single = loader::Toc {
                    name: toc.name.clone(),
                    dir: toc.dir.clone(),
                    metadata: Vec::new(),
                    files: vec![file.clone()],
                };
                match loader::read_files_client(&single, &mut |s| report.notes.push(s), client) {
                    Ok(chunks) => files.extend(chunks),
                    Err(error) => report.errors.push(error),
                }
            }
            crate::fonts::register_addon_root(self.sim.lua(), &toc.dir)?;
            let ns = self.sim.lua.create_table()?;
            self.sim
                .lua
                .set_named_registry_value(&format!("selu:addon:{}", toc.name), ns.clone())?;
            for (name, source) in files {
                if std::env::var_os("SELU_SIM_UI_LOAD_TRACE").is_some() {
                    eprintln!("Default UI source: {}: {name}", toc.name);
                }
                let result = dispatch(&self.sim.lua, &self.sim.ctx, || {
                    core::call_lua::<()>(
                        &self.sim.lua,
                        if name == "@selu/xml-runtime" {
                            crate::host_lua::load(&self.sim.lua, &source)
                                .set_name(&name)
                                .into_function()?
                        } else {
                            self.sim.lua.load(&source).set_name(&name).into_function()?
                        },
                        (toc.name.as_str(), ns.clone()),
                    )
                });
                if let Err(error) = result {
                    report.errors.push(format!("{}: {error}", toc.name));
                }
            }
            report.addons_loaded += 1;
            lock(&self.sim.state)
                .blizzard_loaded
                .insert(toc.name.clone());
            if let Err(error) = self.sim.fire(
                "ADDON_LOADED",
                vec![Value::String(self.sim.lua.create_string(&toc.name)?)],
            ) {
                report.errors.push(error.to_string());
            }
        }
        drop(gc_pause);
        crate::profile::report_hooks("Blizzard sources complete");
        report.elapsed = start.elapsed();
        for line in lock(&self.sim.logs).iter().skip(log_start) {
            if line.starts_with("error:") || line.starts_with("callback error") {
                report.errors.push(line.clone());
            } else if line.starts_with("not simulated:") {
                report.notes.push(line.clone());
            }
        }
        report.notes.sort();
        report.notes.dedup();
        self.default_ui_log_end = lock(&self.sim.logs).len();
        self.default_ui_report = Some(report.clone());
        Ok(report)
    }

    pub fn new(mut config: SessionConfig) -> SimResult<Session> {
        let _profile = crate::profile::Span::new("session.create");
        if let Some(m) = &config.manifest {
            selu_manifest::resolution::select_checked_manifest(
                std::slice::from_ref(m),
                config.target.as_deref(),
            )
            .map_err(|e| runtime(e.to_string()))?;
            if config.target.is_none() {
                config.target = Some(m.target_string());
            }
        }
        if config.manifest.is_none()
            && let Ok(cwd) = std::env::current_dir()
        {
            config.manifest = discover_manifest(&config, &cwd)?;
        }
        let sim = Self::start_vm(&config)?;
        Ok(Session {
            default_ui_source: None,
            default_ui_report: None,
            default_ui_log_end: 0,
            config,
            sim,
            generation: Generation::default(),
            logged_in: false,
            saved: Default::default(),
        })
    }

    fn start_vm(config: &SessionConfig) -> SimResult<Vm> {
        let sim = Vm::new()?;
        open_client_assets(&sim, config)?;
        if let Some(manifest) = &config.manifest {
            crate::sim_globals::install_manifest(sim.lua(), manifest, &sim.ctx)?;
            for family in &manifest.frame_families {
                register_family(&sim, &family.prefix, &family.suffix)?;
            }
        }
        lock(&sim.ctx().trace).enabled = config.trace;
        lock(&sim.state).ui_size = Some(config.ui_size);
        frames::configure_ui_size(sim.lua(), config.ui_size)?;
        crate::sim_globals::configure_screen(sim.lua(), config.ui_size)?;
        let realm = config.realm.clone();
        sim.lua().globals().raw_set(
            "GetRealmName",
            sim.lua().create_function(move |_, ()| Ok(realm.clone()))?,
        )?;
        sim.lua().load("local name = ...; __selu_world_units.player = { name=name, class='WARRIOR', level=1, health=100, max_health=100, power=0, max_power=100, auras={}, combat=false }").call::<()>(config.character.as_str())?;
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

    pub fn snapshot_at(&self, seq: u64) -> Option<Snapshot> {
        lock(&self.sim.state)
            .marks
            .iter()
            .find(|(mark, _)| *mark == seq)
            .map(|(_, snapshot)| snapshot.clone())
    }
    pub fn set_unit(&self, token: &str, unit: crate::world::Unit) -> SimResult<()> {
        lock(&self.sim.state).commands.push(HostCommand::SetUnit {
            token: token.into(),
            unit: unit.clone(),
        });
        self.sim.set_unit(token, unit)
    }
    pub fn set_combat(&self, combat: bool) -> SimResult<()> {
        lock(&self.sim.state)
            .commands
            .push(HostCommand::SetCombat { combat });
        self.sim.set_combat(combat)
    }
    pub fn set_money(&self, copper: u64) -> SimResult<()> {
        lock(&self.sim.state)
            .commands
            .push(HostCommand::SetMoney { copper });
        self.sim.set_money(copper)
    }
    pub fn set_bag(&self, bag: i32, items: &[u32]) -> SimResult<()> {
        lock(&self.sim.state).commands.push(HostCommand::SetBag {
            bag,
            items: items.to_vec(),
        });
        self.sim.set_bag(bag, items)
    }
    pub fn host_commands(&self) -> Vec<HostCommand> {
        lock(&self.sim.state).commands.clone()
    }

    /// Stage declared saved fixtures off-VM before any addon loads. This does
    /// not seed globals: the ordinary loader restores them after the chunks.
    pub fn seed_saved(
        &mut self,
        dir: &Path,
        values: &BTreeMap<String, crate::saved::SavedValue>,
    ) -> SimResult<()> {
        if !lock(&self.sim.state).loaded.is_empty() {
            return Err(runtime("saved fixtures must precede addon loading".into()));
        }
        let toc = loader::read_toc_target(dir, self.config.target.as_deref()).map_err(runtime)?;
        let names: HashSet<_> = toc
            .list("SavedVariables")
            .into_iter()
            .chain(toc.list("SavedVariablesPerCharacter"))
            .collect();
        for (name, value) in values {
            let validate = || {
                if !names.contains(name) {
                    return Err(runtime(format!("undeclared saved fixture `{name}`")));
                }
                let restored = crate::saved::restore(&self.sim.lua, value)?;
                crate::saved::capture(restored, &mut HashSet::new())?;
                Ok(())
            };
            validate().map_err(|cause| {
                mlua::Error::external(SavedFixtureError {
                    global: name.clone(),
                    cause,
                })
            })?;
        }
        for (name, value) in values {
            self.saved
                .0
                .insert(saved_key(&self.config, &toc, name), value.clone());
        }
        Ok(())
    }

    /// Actual checked manifest identities, independent of the requested label.
    pub fn manifest_identities(&self) -> Vec<String> {
        let mut identities: Vec<_> = lock(&self.sim.state)
            .checked_manifests
            .values()
            .map(selu_manifest::Manifest::target_string)
            .collect();
        if let Some(m) = &self.config.manifest {
            identities.push(m.target_string());
        }
        identities.sort();
        identities.dedup();
        identities
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
        let _startup_layout = (!self.logged_in && self.default_ui_report.is_some())
            .then(|| crate::frames::arena::LayoutBatch::new(&self.sim.lua));
        if self.config.manifest.is_none() && lock(&self.sim.state).checked_manifests.is_empty() {
            self.config.manifest = discover_manifest(&self.config, dir)?;
            if let Some(m) = &self.config.manifest {
                // The product is known only now: open the client's textures
                // and fonts, which the VM could not open at start.
                if self.config.target.is_none() {
                    open_client_assets(&self.sim, &self.config)?;
                }
                crate::sim_globals::install_manifest(self.sim.lua(), m, &self.sim.ctx)?;
                for f in &m.frame_families {
                    register_family(&self.sim, &f.prefix, &f.suffix)?;
                }
            }
        }
        self.install_lod(
            dir.parent()
                .ok_or_else(|| runtime("addon has no parent".into()))?,
        )?;
        load_dynamic(&self.sim, dir, &self.config, &self.saved)
    }

    fn install_lod(&self, root: &Path) -> SimResult<()> {
        let root = root.to_path_buf();
        let ctx = self.sim.ctx.clone();
        let state = self.sim.state.clone();
        let config = self.config.clone();
        let saved = self.saved.clone();
        let api: Table = self
            .sim
            .lua
            .globals()
            .raw_get::<Option<Table>>("C_AddOns")?
            .unwrap_or(self.sim.lua.create_table()?);
        api.set(
            "LoadAddOn",
            self.sim.lua.create_function(move |lua, name: String| {
                if lock(&state).blizzard_loaded.contains(&name) {
                    return Ok((Some(true), None));
                }
                let vm = Vm {
                    lua: lua.clone(),
                    ctx: ctx.clone(),
                    prints: ctx.prints.clone(),
                    logs: ctx.logs.clone(),
                    clock: ctx.clock.clone(),
                    state: state.clone(),
                };
                if name.contains(['/', '\\']) || name == ".." {
                    return Ok((None, Some("MISSING".to_string())));
                }
                match load_dynamic(&vm, &root.join(name), &config, &saved) {
                    Ok(_) => Ok((Some(true), None)),
                    Err(error) => Ok((None, Some(error.to_string()))),
                }
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
        }
        crate::fonts::register_addon_root(sim.lua(), dir)?;
        let files =
            loader::read_files_with_diagnostics(&toc, |message| lock(&sim.logs).push(message))
                .map_err(runtime)?;
        sim.load_chunks(&toc.name, &files)?;
        for name in toc
            .list("SavedVariables")
            .into_iter()
            .chain(toc.list("SavedVariablesPerCharacter"))
        {
            if let Some(value) = saved.0.get(&saved_key(config, &toc, &name)) {
                sim.lua
                    .globals()
                    .raw_set(name, crate::saved::restore(&sim.lua, value)?)?;
            }
        }
        Ok(toc.name)
    }

    /// The once-per-session login sequence, after every initially loaded
    /// addon (decision 18).
    pub fn login(&mut self) -> SimResult<()> {
        if self.logged_in {
            return Ok(());
        }
        self.logged_in = true;
        if self.default_ui_report.is_some() {
            let _startup_history = crate::frames::arena::SnapshotBatch::new(&self.sim.lua);
            for (event, mut args) in [
                ("VARIABLES_LOADED", vec![]),
                ("UPDATE_CHAT_WINDOWS", vec![]),
                ("EDIT_MODE_LAYOUTS_UPDATED", vec![]),
                ("PLAYER_LOGIN", vec![]),
                (
                    "PLAYER_ENTERING_WORLD",
                    vec![
                        Value::Boolean(self.generation.0 == 0),
                        Value::Boolean(self.generation.0 != 0),
                    ],
                ),
            ] {
                if event == "EDIT_MODE_LAYOUTS_UPDATED" {
                    match dispatch(&self.sim.lua, &self.sim.ctx, || {
                        self.sim
                            .lua
                            .load("return C_EditMode.GetLayouts()")
                            .eval::<Value>()
                    }) {
                        Ok(layouts) => args = vec![layouts, Value::Boolean(false)],
                        Err(error) => {
                            self.sim.ctx.log(format!("error: {error}"));
                            continue;
                        }
                    }
                }
                if let Err(error) = self.sim.fire(event, args) {
                    self.sim.ctx.log(format!("error: {error}"));
                }
            }
            // Synthetic server greeting follows the normal Blizzard chat event path.
            let empty = Value::String(self.sim.lua.create_string("")?);
            let mut args = vec![empty; 18];
            for index in [6, 7, 10, 12] {
                args[index] = Value::Integer(0);
            }
            args[13] = Value::Boolean(false);
            args[15] = Value::Boolean(false);
            args[0] = Value::String(
                self.sim
                    .lua
                    .create_string("Welcome to Selu's simulated Northshire Valley.")?,
            );
            args[17] = Value::Table(self.sim.lua.create_table()?);
            if let Err(error) = self.sim.fire("CHAT_MSG_SYSTEM", args) {
                self.sim.ctx.log(format!("error: {error}"));
            }
            return Ok(());
        }
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
        self.reload_character(None)
    }

    /// Persist the old character before selecting the next character's tree.
    pub fn switch_character(&mut self, character: String) -> SimResult<()> {
        if character.is_empty() {
            return Err(runtime("character must not be empty".into()));
        }
        self.reload_character(Some(character))
    }

    fn reload_character(&mut self, character: Option<String>) -> SimResult<()> {
        lock(&self.sim.state).commands.push(match &character {
            Some(character) => HostCommand::SwitchCharacter {
                character: character.clone(),
            },
            None => HostCommand::Reload,
        });
        let addons = lock(&self.sim.state).loaded.clone();
        let old_checked = lock(&self.sim.state).checked_manifests.clone();
        let target = snapshot_target(&self.config, &old_checked);
        let mut snapshots = CheckedManifests::new();
        for dir in &addons {
            match crate::snapshot_cache::compiled_manifest_for_target(dir, target.as_deref())
                .map_err(runtime)?
            {
                Some(m) => {
                    snapshots.insert(dir.clone(), m);
                }
                None if old_checked.contains_key(dir) => {
                    return Err(runtime(format!(
                        "checked snapshot missing for {}",
                        dir.display()
                    )));
                }
                None => {}
            }
        }
        validate_snapshots(&snapshots)?;
        self.sim.fire("PLAYER_LOGOUT", vec![])?;
        let mut saved = self.saved.clone();
        for dir in &addons {
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
            "__selu_sim_combat",
        ] {
            world.push((
                name,
                crate::saved::capture(self.sim.lua.globals().raw_get(name)?, &mut HashSet::new())?,
            ));
        }
        let mut next_config = self.config.clone();
        if let Some(character) = character {
            next_config.character = character;
        }
        let replacement = Self::start_vm(&next_config)?;
        for (prefix, suffix) in &lock(&self.sim.state).manual_frame_families {
            register_family(&replacement, prefix, suffix)?;
            lock(&replacement.state)
                .manual_frame_families
                .insert((prefix.clone(), suffix.clone()));
        }
        if !snapshots.is_empty() {
            install_snapshots(&replacement, snapshots)?;
        }
        *lock(&replacement.clock) = self.now();
        {
            let mut state = lock(&replacement.state);
            state.commands = lock(&self.sim.state).commands.clone();
            state.generation = self.generation.next();
        }
        replacement
            .lua
            .globals()
            .get::<Function>("__selu_sim_set_time")?
            .call::<()>(self.now())?;
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
        self.config = next_config;
        self.saved = saved;
        self.sim = replacement;
        self.generation = self.generation.next();
        if let Some(source) = self.default_ui_source.clone() {
            self.load_default_ui(&source, |_| {})?;
        }
        for dir in &addons {
            self.load_addon_dir(dir)?;
        }
        if self.logged_in {
            self.logged_in = false;
            self.login()?;
        }
        Ok(())
    }

    pub fn fire(&self, event: &str, args: Vec<Value>) -> SimResult<usize> {
        lock(&self.sim.state).commands.push(HostCommand::Fire {
            event: event.into(),
            args: args
                .iter()
                .map(HostValue::capture)
                .collect::<SimResult<_>>()?,
        });
        self.sim.fire(event, args)
    }
    pub fn slash(&self, line: &str) -> SimResult<()> {
        lock(&self.sim.state)
            .commands
            .push(HostCommand::Slash { line: line.into() });
        self.sim.slash(line)
    }
    /// Register a manifest family whose absent members must behave as nil,
    /// rather than the simulator's fallback for unmodelled API functions.
    pub fn register_frame_family(&self, prefix: &str, suffix: &str) -> SimResult<()> {
        register_family(&self.sim, prefix, suffix)?;
        lock(&self.sim.state)
            .manual_frame_families
            .insert((prefix.into(), suffix.into()));
        Ok(())
    }

    pub fn eval(&self, source: &str) -> SimResult<()> {
        lock(&self.sim.state).commands.push(HostCommand::Eval {
            code: source.into(),
        });
        self.sim.eval(source)
    }
    pub fn advance_time(&self, seconds: f64) {
        lock(&self.sim.state)
            .commands
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
            ui_size: self.config.ui_size,
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
        crate::trace::since(&self.sim.lua, &self.sim.ctx, from)
    }
}

#[derive(Clone)]
pub struct Vm {
    pub(crate) lua: Lua,
    pub(crate) ctx: Ctx,
    pub(crate) prints: Shared<Vec<String>>,
    pub(crate) logs: Shared<Vec<String>>,
    pub(crate) clock: Shared<f64>,
    state: Shared<VmState>,
}

impl Vm {
    pub fn new() -> SimResult<Vm> {
        let lua = Lua::new();
        crate::host_lua::install(&lua)?;
        let ctx = Ctx::new();
        core::install_hooks(&lua, &ctx);
        regions::install(&lua)?;
        let prints = ctx.prints.clone();
        let logs = ctx.logs.clone();
        let warned = ctx.warned.clone();
        let clock = ctx.clock.clone();
        let globals = lua.globals();
        globals.set("__selu_saved_names", lua.create_table()?)?;
        globals.raw_set("__selu_global_shapes", lua.create_table()?)?;

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

        globals.set("__selu_sim_frames", lua.create_table()?)?;
        frames::install(&lua, &ctx)?;
        let create: Function = globals.get("CreateFrame")?;
        globals.set("UIParent", create.call::<Table>("Frame")?)?;
        let registry: Table = globals.get("__selu_sim_frames")?;
        registry.raw_set(registry.raw_len(), Value::Nil)?;
        globals.raw_set("__selu_frame_families", lua.create_table()?)?;
        // Unknown shapes stay nil; accessing them logs once and cannot invent a callable.
        {
            let mt = lua.create_table()?;
            let shape_ctx = ctx.clone();
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
                    let families: Table = lua.globals().raw_get("__selu_frame_families")?;
                    for pair in families.sequence_values::<Table>() {
                        let pair = pair?;
                        let prefix: String = pair.raw_get(1)?;
                        let suffix: String = pair.raw_get(2)?;
                        if key
                            .strip_prefix(&prefix)
                            .and_then(|s| s.strip_suffix(&suffix))
                            .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
                        {
                            return Ok(Value::Nil);
                        }
                    }
                    if let Some(value) = crate::sim_globals::lookup(lua, &key, &shape_ctx)? {
                        return Ok(value);
                    }
                    log_once(&logs, &warned, &key);
                    Ok(Value::Nil)
                })?,
            )?;
            globals.set_metatable(Some(mt));
        }
        crate::sim_globals::install(&lua, &ctx)?;
        wow_api::install(&lua)?;
        timers::install(&lua, logs.clone())?;
        lua.globals()
            .set("__selu_loaded_dirs", lua.create_table()?)?;
        crate::world::install(&lua)?;
        crate::world::install_restrictions(&lua, &ctx)?;
        crate::trace::install(&lua, &ctx)?;
        crate::host_lua::seal(&lua)?;

        Ok(Vm {
            lua,
            ctx,
            prints,
            logs,
            clock,
            state: crate::core::shared(VmState::default()),
        })
    }

    /// Load generated Lua files in TOC order: `(file name, source)` pairs.
    pub fn load_files(&self, files: &[(String, String)]) -> SimResult<()> {
        for (name, source) in files {
            dispatch(&self.lua, &self.ctx, || {
                core::call_lua::<()>(
                    &self.lua,
                    self.lua.load(source).set_name(name).into_function()?,
                    (),
                )
            })?;
        }
        Ok(())
    }

    /// Load an addon's files in TOC order the way the client does: every
    /// file receives `...` = the addon's name and one table shared by all of
    /// its files.
    fn load_chunks(&self, addon: &str, files: &[(String, String)]) -> SimResult<()> {
        let ns = self.lua.create_table()?;
        self.lua
            .set_named_registry_value(&format!("selu:addon:{addon}"), ns.clone())?;
        for (name, source) in files {
            dispatch(&self.lua, &self.ctx, || {
                core::call_lua::<()>(
                    &self.lua,
                    self.lua.load(source).set_name(name).into_function()?,
                    (addon, ns.clone()),
                )
            })?;
        }
        Ok(())
    }
    /// The private namespace passed to every chunk loaded through this addon's TOC.
    pub fn addon_namespace(&self, addon: &str) -> SimResult<Table> {
        self.lua
            .named_registry_value(&format!("selu:addon:{addon}"))
    }
    pub fn load_addon(&self, addon: &str, files: &[(String, String)]) -> SimResult<()> {
        self.load_addon_with_restore(addon, files, |_| Ok(()))
    }

    /// Execute chunks, restore synthetic SavedVariables, then fire ADDON_LOADED.
    /// Callers must never put saved globals in the VM before loading chunks.
    pub fn load_addon_with_restore(
        &self,
        addon: &str,
        files: &[(String, String)],
        restore: impl FnOnce(&Self) -> SimResult<()>,
    ) -> SimResult<()> {
        let names: Table = self.lua.globals().raw_get("__selu_saved_names")?;
        let hydrated = match self
            .lua
            .named_registry_value::<Option<Table>>("selu:saved-hydrated")?
        {
            Some(table) => table,
            None => {
                let table = self.lua.create_table()?;
                self.lua
                    .set_named_registry_value("selu:saved-hydrated", table.clone())?;
                table
            }
        };
        for pair in names.pairs::<String, bool>() {
            let (name, _) = pair?;
            if !hydrated
                .raw_get::<Option<bool>>(name.as_str())?
                .unwrap_or(false)
                && !self.lua.globals().raw_get::<Value>(name.as_str())?.is_nil()
            {
                return Err(runtime(format!(
                    "saved global `{name}` was seeded before addon chunks; restore it with load_addon_with_restore"
                )));
            }
        }
        self.load_chunks(addon, files)?;
        restore(self)?;
        for pair in names.pairs::<String, bool>() {
            let (name, _) = pair?;
            hydrated.raw_set(name, true)?;
        }
        self.fire(
            "ADDON_LOADED",
            vec![
                Value::String(self.lua.create_string(addon)?),
                Value::Boolean(false),
            ],
        )?;
        Ok(())
    }

    pub fn slash(&self, line: &str) -> SimResult<()> {
        self.ctx
            .record(TraceKind::Host, "slash", vec![line.to_string()]);
        dispatch(&self.lua, &self.ctx, || {
            core::call_lua(
                &self.lua,
                self.lua.globals().get::<Function>("__selu_sim_slash")?,
                line,
            )
        })
    }

    pub fn eval(&self, source: &str) -> SimResult<()> {
        dispatch(&self.lua, &self.ctx, || {
            core::call_lua::<()>(&self.lua, self.lua.load(source).into_function()?, ())
        })
    }

    /// Fire an event at every frame that registered for it.
    pub fn fire(&self, event: &str, args: Vec<Value>) -> SimResult<usize> {
        let rendered = args.iter().map(value_string).collect();
        let seq = lock(&self.ctx.trace).count();
        self.ctx.record(TraceKind::Event, event, rendered);
        let result = dispatch(&self.lua, &self.ctx, || self.fire_now(event, &args));
        // Startup layout transactions have no committed geometry yet. Keep
        // event records, but do not clone the whole growing UI into history
        // for every Blizzard ADDON_LOADED event.
        if lock(&self.ctx.trace).enabled
            && crate::frames::arena::store(&self.lua)
                .borrow()
                .snapshot_batches
                == 0
            && crate::frames::arena::store(&self.lua)
                .borrow()
                .layout_batches
                == 0
        {
            let snapshot = self.snapshot()?;
            let mut state = lock(&self.state);
            if state.marks.len() == 128 {
                state.marks.pop_front();
            }
            state.marks.push_back((seq, snapshot));
        }
        result
    }

    fn fire_now(&self, event: &str, args: &[Value]) -> SimResult<usize> {
        let _profile = crate::profile::Span::new("events.dispatch");
        let arena = crate::frames::arena::store(&self.lua);
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
                call.push_back(Value::String(self.lua.create_string(event)?));
                for a in args {
                    call.push_back(a.clone());
                }
                self.ctx
                    .record(TraceKind::Script, "OnEvent", vec![event.into()]);
                let result = core::script_callback(&self.lua, &self.ctx, || {
                    core::call_lua::<()>(&self.lua, func, call)
                });
                core::flush(&self.lua, &self.ctx);
                if let Err(error) = result {
                    if lock(&self.state).tolerant_ui {
                        self.ctx.log(format!("error: {error}"));
                    } else {
                        return Err(error);
                    }
                }
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
        let _ = core::scheduler_dispatch(&self.lua, &self.ctx, || {
            timers::advance(&self.lua, &self.clock, &self.logs, seconds);
            Ok(())
        });
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
        let (generation, size) = {
            let state = lock(&self.state);
            (
                state.generation,
                state.ui_size.unwrap_or(snapshot::DEFAULT_UI_SIZE),
            )
        };
        let model = frames::LuaTableModel {
            generation: generation.0 as u32,
            ui_size: size,
        };
        snapshot::take(&self.lua, &self.ctx, &model, generation, size)
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
    #[test]
    fn default_ui_native_helpers_preserve_unique_font_names() {
        let mut session = Session::new(SessionConfig::default()).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/default-ui");
        session.load_default_ui(&source, |_| {}).unwrap();
        session
            .eval(
                r#"
            local first=CreateFrame('Frame'); local second=CreateFrame('Frame')
            assert(tostring(first)~=tostring(second))
            assert(tostring({})=='table')
            CreateFont(tostring(first)); CreateFont(tostring(second))
            assert(type(table.insert)=='function')
            first:RegisterEvent('CUSTOM_EVENT')
            assert(first:IsEventRegistered('CUSTOM_EVENT'))
            first:UnregisterEvent('CUSTOM_EVENT')
            assert(not first:IsEventRegistered('CUSTOM_EVENT'))
            local shared={value=42};local original={a=shared,b=shared};original.self=original
            local copied=securecopy(original);assert(copied~=original and copied.a~=shared and copied.a==copied.b and copied.self==copied)
            local ok=pcall(securecopy,{nested={}},{maxTraversalDepth=0});assert(not ok)
            forceinsecure()
            assert(GetChatWindowMessages(1)=='SYSTEM')
            assert(C_StringUtil.EscapeDecimalNonPrintables('日本語'..string.char(1))=='日本語\\001')
        "#,
            )
            .unwrap();
    }
    #[test]
    fn blizzard_login_queries_live_geometry_and_allows_large_system_searches() {
        let mut session = Session::new(SessionConfig::default()).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/default-ui");
        session.load_default_ui(&source, |_| {}).unwrap();
        session
            .eval(
                r#"
            assert(table.contains({key='found'},'found'))
            assert(not table.contains({key='found'},'missing'))
            assert(string.join('|','HELPFUL','PLAYER')=='HELPFUL|PLAYER')
            local model=CreateFrame('PlayerModel',nil,UIParent)
            model:SetViewInsets(1,2,3,4)
            local l,r,t,b=model:GetViewInsets(); assert(l==1 and r==2 and t==3 and b==4)
            local font=UIParent:CreateFontString()
            font:SetFont('Fonts/FRIZQT__.TTF',14)
            assert(font:GetLineHeight()>0)
            local texture=UIParent:CreateTexture()
            texture:SetColorTexture(1,1,1,1); texture:SetTexture(0)
            assert(texture:GetTexture()==nil)
            local frame=CreateFrame('Frame',nil,UIParent)
            frame:RegisterEvent('EDIT_MODE_LAYOUTS_UPDATED')
            frame:SetScript('OnEvent',function(self)
                self:SetSize(562,45)
                self:SetPoint('BOTTOM',UIParent,'BOTTOM',0,10)
                local x,y,w,h=self:GetRect()
                assert(x and y==10 and w==562 and h==45,'login geometry was deferred')
                local count=0
                for i=1,10001 do securecallfunction(function() count=count+1 end) end
                assert(count==10001)
                LoginGeometryApplied=true
            end)
        "#,
            )
            .unwrap();
        session.login().unwrap();
        assert!(
            session
                .lua()
                .globals()
                .get::<bool>("LoginGeometryApplied")
                .unwrap()
        );
    }
    #[test]
    fn blizzard_transition_errors_do_not_abort_unrelated_frame_apis() {
        let mut session = Session::new(SessionConfig::default()).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/default-ui");
        session.load_default_ui(&source, |_| {}).unwrap();
        session.eval("local f=CreateFrame('Frame'); f:Hide(); f:SetScript('OnShow',function() error('synthetic Blizzard transition failure') end); f:Show(); TransitionContinued=true").unwrap();
        assert!(
            session
                .lua()
                .globals()
                .get::<bool>("TransitionContinued")
                .unwrap()
        );
        assert!(
            session
                .logs()
                .iter()
                .any(|line| line.contains("synthetic Blizzard transition failure"))
        );
    }
    #[test]
    fn default_ui_keeps_event_trace_without_partial_frame_snapshots() {
        let mut session = Session::new(SessionConfig {
            trace: true,
            ..Default::default()
        })
        .unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/default-ui");
        session.load_default_ui(&source, |_| {}).unwrap();
        session.login().unwrap();
        assert!(lock(&session.sim.state).marks.is_empty());
        assert!(
            session
                .trace_since(0)
                .iter()
                .any(|record| record.target == "ADDON_LOADED")
        );
        session.fire("AFTER_STARTUP", vec![]).unwrap();
        assert_eq!(lock(&session.sim.state).marks.len(), 1);
    }
    #[test]
    fn blizzard_chunk_errors_are_captured_and_loading_continues() {
        let mut s = Session::new(SessionConfig::default()).unwrap();
        let mut progress = Vec::new();
        let report = s
            .load_default_ui(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/default-ui"),
                |name| progress.push(name.to_owned()),
            )
            .unwrap();
        assert_eq!(report.addons_loaded, 1);
        assert_eq!(progress, ["Blizzard_Test"]);
        assert_eq!(report.errors.len(), 4);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("explicit Blizzard error handler failure"))
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("synthetic Blizzard chunk failure"))
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("unknown file type"))
        );
        s.eval("assert(AfterBrokenBlizzardChunk); assert(SyntheticOnly==nil)")
            .unwrap();
        s.login().unwrap();
        s.eval("assert(NativeLoginContinued and NativeLayoutsReceived)")
            .unwrap();
    }
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
    fn xml_files_load_in_toc_order() {
        let dir = std::env::temp_dir().join(format!("selu-sim-xml-{}", std::process::id()));
        let addon = dir.join("WithXml");
        std::fs::create_dir_all(&addon).unwrap();
        std::fs::write(addon.join("WithXml.toc"), "main.lua\nframes.xml\n").unwrap();
        std::fs::write(addon.join("main.lua"), "print('x')").unwrap();
        std::fs::write(
            addon.join("frames.xml"),
            "<Ui><Frame name=\"FromXml\"><Size x=\"90\" y=\"30\"/></Frame></Ui>",
        )
        .unwrap();
        let mut s = Session::new(SessionConfig::default()).unwrap();
        s.load_addon_dir(&addon).unwrap();
        s.eval("assert(FromXml:GetWidth()==90)").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(s.prints(), ["x"]);
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

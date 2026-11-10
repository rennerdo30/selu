use super::bridge;
use crate::*;
use mlua::{Function, Lua, Value};
use selu_sim::{Session, SessionConfig};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

fn build_error(e: impl std::fmt::Display) -> RunError {
    RunError::Build(e.to_string())
}
fn overlay(m: &mut selu_manifest::Manifest) {
    use serde_json::json;
    m.tables.push(
        serde_json::from_value(json!({"name":"SeluTestCallback","type":"CallbackType"})).unwrap(),
    );
    let mut add = |name: &str, args: serde_json::Value, returns: serde_json::Value| {
        m.functions.push(serde_json::from_value(json!({"name":name,"evidence":"verified","signature":{"name":name,"arguments":args,"returns":returns}})).unwrap());
    };
    for name in ["test", "describe"] {
        add(
            name,
            json!([{"name":"name","type":"string"},{"name":"body","type":"SeluTestCallback"}]),
            json!([]),
        );
    }
    add(
        "beforeEach",
        json!([{"name":"body","type":"SeluTestCallback"}]),
        json!([]),
    );
    add(
        "expect",
        json!([{"name":"value","type":"unknown","attributes":{"Nilable":true,"Required":true}}]),
        json!([{"name":"result","type":"SeluExpectation"}]),
    );
    for (name, args, returns) in [
        (
            "sim.fire",
            json!([{"name":"event","type":"string"},{"name":"...","type":"unknown"}]),
            json!([]),
        ),
        (
            "sim.slash",
            json!([{"name":"line","type":"string"}]),
            json!([]),
        ),
        (
            "sim.advanceTime",
            json!([{"name":"seconds","type":"number"}]),
            json!([]),
        ),
        ("sim.reload", json!([]), json!([])),
        (
            "sim.loadScenario",
            json!([{"name":"path","type":"string"}]),
            json!([]),
        ),
        (
            "sim.switchCharacter",
            json!([{"name":"character","type":"string"}]),
            json!([]),
        ),
        (
            "sim.now",
            json!([]),
            json!([{"name":"seconds","type":"number"}]),
        ),
        (
            "sim.setMoney",
            json!([{"name":"copper","type":"number"}]),
            json!([]),
        ),
        (
            "sim.setCombat",
            json!([{"name":"combat","type":"bool"}]),
            json!([]),
        ),
        (
            "sim.setUnit",
            json!([{"name":"token","type":"string"},{"name":"unit","type":"unknown"}]),
            json!([]),
        ),
        (
            "sim.setSpell",
            json!([{"name":"id","type":"number"},{"name":"spell","type":"unknown"}]),
            json!([]),
        ),
        (
            "sim.setTracking",
            json!([{"name":"entries","type":"unknown"}]),
            json!([]),
        ),
        (
            "sim.setZone",
            json!([{"name":"zone","type":"unknown"}]),
            json!([]),
        ),
        (
            "sim.setMap",
            json!([{"name":"id","type":"number"},{"name":"map","type":"unknown"}]),
            json!([]),
        ),
        (
            "sim.setPlayerPosition",
            json!([{"name":"position","type":"unknown"}]),
            json!([]),
        ),
        (
            "sim.lockCVar",
            json!([{"name":"name","type":"string"}]),
            json!([]),
        ),
        (
            "sim.setCVar",
            json!([{"name":"name","type":"string"},{"name":"value","type":"string"}]),
            json!([]),
        ),
        (
            "sim.prints",
            json!([]),
            json!([{"name":"prints","type":"table","attributes":{"InnerType":"string"}}]),
        ),
        (
            "sim.frame",
            json!([{"name":"name","type":"string"}]),
            json!([{"name":"frame","type":"Frame","attributes":{"Nilable":true}}]),
        ),
        (
            "sim.spy",
            json!([{"name":"name","type":"string"}]),
            json!([{"name":"spy","type":"unknown"}]),
        ),
        (
            "perf.allocations",
            json!([{"name":"body","type":"SeluTestCallback"}]),
            json!([{"name":"count","type":"number"}]),
        ),
    ] {
        add(name, args, returns);
    }
    m.tables.push(serde_json::from_value(json!({"name":"SeluCallbackCounts","type":"Structure","fields":[{"name":"timers","type":"number"},{"name":"updates","type":"number"}]})).unwrap());
    m.functions.push(serde_json::from_value(json!({"name":"sim.callbackCounts","evidence":"verified","signature":{"name":"callbackCounts","returns":[{"name":"counts","type":"SeluCallbackCounts"}]}})).unwrap());
    let mut fields = vec![];
    for (name, ty) in [
        ("toBe", "unknown"),
        ("toEqual", "unknown"),
        ("toContain", "unknown"),
        ("toBeLessThan", "number"),
        ("toBeGreaterThan", "number"),
        ("toHaveBeenCalledWith", "unknown"),
    ] {
        fields.push(json!({"name":name,"type":"Function","arguments":[{"name":if name == "toHaveBeenCalledWith" { "..." } else { "expected" },"type":ty,"attributes":{"Nilable":ty == "unknown","Required":true}}]}));
    }
    for name in ["toBeNull", "toThrow", "toHaveBeenCalled"] {
        fields.push(json!({"name":name,"type":"Function"}));
    }
    fields.push(json!({"name":"not","type":"SeluExpectation"}));
    m.tables.push(
        serde_json::from_value(
            json!({"name":"SeluExpectation","type":"Structure","fields":fields}),
        )
        .unwrap(),
    );
    for (name, ty) in [("sim", "SeluSim"), ("perf", "SeluPerf")] {
        let fields: Vec<_> = m
            .functions
            .iter()
            .filter_map(|f| {
                let field = f.name.strip_prefix(&format!("{name}."))?;
                let mut signature = f.signature.clone()?;
                signature.name = field.into();
                signature.ty = Some("Function".into());
                Some(signature)
            })
            .collect();
        m.tables.push(
            serde_json::from_value(json!({"name":ty,"type":"Structure","fields":fields})).unwrap(),
        );
        m.globals.push(
            serde_json::from_value(json!({"name":name,"type":ty,"evidence":"verified"})).unwrap(),
        );
    }
}

/// Phase timings and memory for `SELU_TEST_TIMINGS`.
struct Clock(Option<std::time::Instant>);
impl Clock {
    fn new() -> Self {
        Self(
            std::env::var_os("SELU_TEST_TIMINGS")
                .is_some()
                .then(std::time::Instant::now),
        )
    }
    fn mark(&mut self, label: &str) {
        if let Some(start) = self.0.as_mut() {
            let memory = private_mb()
                .map(|mb| format!(" ({mb} MB private)"))
                .unwrap_or_default();
            eprintln!("selu test timing {label}: {:?}{memory}", start.elapsed());
            *start = std::time::Instant::now();
        }
    }
}

/// How many test files run at once. Each file has its own session, and a
/// session with Blizzard's UI holds about 2 GB, so the default stays small.
fn worker_count(requested: usize, files: usize) -> usize {
    let wanted = if requested > 0 {
        requested
    } else {
        std::thread::available_parallelism()
            .map(|n| n.get() / 2)
            .unwrap_or(1)
            .clamp(1, 4)
    };
    wanted.clamp(1, files.max(1))
}

/// One compiled test file, ready for a session on any worker.
struct Job {
    index: usize,
    file: String,
    chunk: String,
    lua: String,
    maps: Vec<selu_sourcemap::SourceMap>,
}

/// What every file's session shares, read-only.
#[derive(Clone, Copy)]
struct Shared<'a> {
    options: &'a RunOptions,
    manifest: &'a selu_manifest::Manifest,
    addon: &'a std::path::Path,
    name: &'a str,
    sources: &'a [String],
    blizzard_ui: &'a Option<crate::client_assets::UiSource>,
}

type FileResult = Result<Vec<TestResult>, RunError>;

/// Build the addon with the same compiler used by `selu build`, then run each
/// file in its own session. Files run on a few worker threads at once; the
/// report keeps file order.
pub fn run(options: &RunOptions) -> Result<Report, RunError> {
    let mut clock = Clock::new();
    let files = discover(&options.project_dir).map_err(|e| RunError::Io(e.to_string()))?;
    if files.is_empty() {
        return Err(RunError::NoTests);
    }
    let config: toml::Value = toml::from_str(
        &std::fs::read_to_string(options.project_dir.join("selu.toml"))
            .map_err(|e| RunError::Io(e.to_string()))?,
    )
    .map_err(build_error)?;
    let target = options
        .target
        .clone()
        .or_else(|| {
            config
                .get("addon")?
                .get("targets")?
                .as_array()?
                .first()?
                .as_str()
                .map(String::from)
        })
        .ok_or_else(|| build_error("SELU0620: select one target"))?;
    let mut roots = Vec::new();
    for file in &files {
        let source =
            std::fs::read_to_string(options.project_dir.join(file)).map_err(build_error)?;
        let program =
            selu_syntax::parse(file, &source).map_err(|d| build_error(d.render(&source)))?;
        for stmt in &program.stmts {
            if let selu_syntax::StmtKind::Import { from, .. } = &stmt.kind {
                let key = test_import_key(file, from)
                    .ok_or_else(|| build_error("SELU0621: tests must import modules under src/"))?;
                roots.push(key);
            }
        }
    }
    project::build_tests(
        &options.project_dir,
        Some(&target),
        &roots,
        options.manifest_dir.as_deref(),
    )
    .map_err(|_| build_error("project build failed"))?;
    let name = config
        .get("addon")
        .and_then(|a| a.get("name"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| build_error("missing addon name"))?;
    let addon = options.project_dir.join("dist").join(name);
    let manifest = selu_manifest::resolution::compiled_manifest(&addon)
        .map_err(|d| build_error(&d.message))?
        .ok_or_else(|| build_error("missing checked manifest"))?;
    let modules = project::project_sources_roots(&options.project_dir.join("src"), &roots)
        .map_err(|_| build_error("module discovery failed"))?;
    clock.mark("build and discovery");
    // One prepared target for every module, as `selu build` does: preparing
    // the manifest again for each module cost more than checking them.
    let prepared = selu_check::PreparedTarget::new(&manifest, std::slice::from_ref(&manifest));
    let mut exports = BTreeMap::<String, selu_check::ModuleExports>::new();
    for module in &modules {
        let mut bindings = selu_check::ModuleBindings::default();
        for (from, key) in &module.dependencies {
            bindings.imports.insert(from.clone(), exports[key].clone());
        }
        let (_, signature) = prepared.check_module(
            &module.program,
            bindings,
            selu_check::CheckOptions::default(),
        );
        exports.insert(module.key.clone(), signature);
    }
    drop(prepared);
    clock.mark("module signatures");
    let mut test_manifest = manifest.clone();
    overlay(&mut test_manifest);
    let tests_target =
        selu_check::PreparedTarget::new(&test_manifest, std::slice::from_ref(&test_manifest));
    // Read once; every file's failures map through the same module maps.
    let mut module_maps = Vec::new();
    for module in &modules {
        let path = options
            .project_dir
            .join("dist/.selu")
            .join(name)
            .join(format!(
                "{}.lua.map",
                crate::packages::output_key(&module.key)
            ));
        if let Ok(text) = std::fs::read_to_string(path) {
            module_maps.push(selu_sourcemap::SourceMap::from_json(&text).map_err(build_error)?);
        }
    }
    let sources: Vec<String> = modules
        .iter()
        .map(|m| format!("{}.lua", crate::packages::output_key(&m.key)))
        .chain(["Selu.lua".into(), "__selu_runtime.lua".into()])
        .collect();
    drop(modules);
    let blizzard_ui = match crate::client_assets::ui_source(options, &target) {
        Ok(found) => found,
        Err(why) => {
            eprintln!("selu test: running without Blizzard's UI: {why}");
            None
        }
    };
    clock.mark("test setup");
    let shared = Shared {
        options,
        manifest: &manifest,
        addon: &addon,
        name,
        sources: &sources,
        blizzard_ui: &blizzard_ui,
    };
    let total = files.len();
    let workers = worker_count(options.jobs, total);
    let (job_tx, job_rx) = std::sync::mpsc::channel::<Job>();
    let job_rx = std::sync::Mutex::new(job_rx);
    let (done_tx, done_rx) = std::sync::mpsc::channel::<(usize, FileResult)>();
    let compiled = std::thread::scope(|scope| -> Result<(), RunError> {
        for worker in 0..workers {
            let done = done_tx.clone();
            let jobs = &job_rx;
            std::thread::Builder::new()
                .name(format!("selu-test-{worker}"))
                // Blizzard's UI nests deep Lua and native calls.
                .stack_size(16 * 1024 * 1024)
                .spawn_scoped(scope, move || {
                    loop {
                        let next = jobs.lock().unwrap_or_else(|e| e.into_inner()).recv();
                        let Ok(job) = next else { break };
                        let index = job.index;
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            run_file(shared, job)
                        }))
                        .unwrap_or_else(|panic| {
                            let why = panic
                                .downcast_ref::<String>()
                                .cloned()
                                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                                .unwrap_or_else(|| "unknown panic".into());
                            Err(build_error(format!("test session panicked: {why}")))
                        });
                        if done.send((index, result)).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|e| RunError::Io(e.to_string()))?;
        }
        // Compile here while the workers run earlier files. A compile error
        // stops handing out files; files already handed out finish.
        let mut compiled = Ok(());
        for (index, file) in files.iter().enumerate() {
            match compile_file(
                options,
                &tests_target,
                &exports,
                name,
                &module_maps,
                index,
                file,
            ) {
                Ok(job) => {
                    if job_tx.send(job).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    compiled = Err(e);
                    break;
                }
            }
        }
        drop(job_tx);
        compiled
    });
    drop(done_tx);
    let mut finished: Vec<Option<FileResult>> = (0..total).map(|_| None).collect();
    for (index, result) in done_rx {
        finished[index] = Some(result);
    }
    compiled?;
    let mut report = Report {
        target: Some(target.clone()),
        results: vec![],
    };
    for result in finished.into_iter().flatten() {
        report.results.extend(result?);
    }
    clock.mark("test files");
    Ok(report)
}

fn compile_file(
    options: &RunOptions,
    tests_target: &selu_check::PreparedTarget,
    exports: &BTreeMap<String, selu_check::ModuleExports>,
    name: &str,
    module_maps: &[selu_sourcemap::SourceMap],
    index: usize,
    file: &str,
) -> Result<Job, RunError> {
    let source = std::fs::read_to_string(options.project_dir.join(file)).map_err(build_error)?;
    let program = selu_syntax::parse(file, &source).map_err(|d| build_error(d.render(&source)))?;
    let mut imports = selu_check::ModuleBindings::default();
    let mut lower_imports = BTreeMap::new();
    for stmt in &program.stmts {
        if let selu_syntax::StmtKind::Import { from, .. } = &stmt.kind {
            let key = test_import_key(file, from).ok_or_else(|| {
                build_error("SELU0621: tests must import project modules under src/")
            })?;
            let signature = exports
                .get(&key)
                .ok_or_else(|| {
                    build_error(format!("SELU0621: module {key} is not in the addon build"))
                })?
                .clone();
            imports.imports.insert(from.clone(), signature.clone());
            lower_imports.insert(from.clone(), (key, signature));
        }
    }
    let (checked, _) = tests_target.check_module(
        &program,
        imports,
        selu_check::CheckOptions { strict_api: true },
    );
    if checked.has_errors() {
        return Err(build_error(
            checked
                .diagnostics
                .iter()
                .map(|d| d.render(&source))
                .collect::<String>(),
        ));
    }
    let mut bindings = selu_lower::ModuleBindings::new(file.to_string(), lower_imports, &program);
    bindings.addon_name = name.into();
    let lowered =
        selu_lower::lower_module(&program, tests_target.manifest(), &checked.types, bindings);
    let chunk = format!("{file}.lua");
    let mut maps = vec![lowered.source_map(&chunk, file, &source)];
    maps.extend(module_maps.iter().cloned());
    Ok(Job {
        index,
        file: file.to_string(),
        chunk,
        lua: lowered.lua,
        maps,
    })
}

/// Runs one compiled test file in a fresh session on the current thread.
fn run_file(shared: Shared, job: Job) -> FileResult {
    let Shared {
        options,
        manifest,
        addon,
        name,
        sources,
        blizzard_ui,
    } = shared;
    let Job {
        file,
        chunk,
        lua: lowered_lua,
        maps,
        ..
    } = job;
    let mut clock = Clock::new();
    let mut results = Vec::new();
    let session = Rc::new(RefCell::new(
        Session::new(SessionConfig {
            client_install: crate::client_assets::resolve(options, &manifest.target_string()),
            target: Some(manifest.target_string()),
            manifest: Some(manifest.clone()),
            ..Default::default()
        })
        .map_err(build_error)?,
    ));
    let fixture = options
        .project_dir
        .join(&file)
        .with_extension("scenario.json");
    let scenario = crate::scenario::load(&fixture)?;
    if let Some(character) = &scenario.data.character {
        session
            .borrow_mut()
            .switch_character(character.clone())
            .map_err(|e| scenario.error(e, &["character".into()]))?;
    }
    session
        .borrow_mut()
        .seed_saved(addon, &scenario.data.saved)
        .map_err(|e| {
            let mut key = vec!["saved".into()];
            if let Some(cause) = e.downcast_ref::<selu_sim::session::SavedFixtureError>() {
                key.push(cause.global.clone());
            }
            scenario.error(e, &key)
        })?;
    clock.mark(&format!("{file} session"));
    if let Some((source, roots)) = blizzard_ui {
        // Blizzard's own startup errors are the client's, not the test's:
        // they stay in the session log, as in Studio.
        if let Err(e) = session
            .borrow_mut()
            .load_blizzard_ui(source, roots.as_deref(), |_| {})
        {
            results.push(failure(
                &file,
                vec!["setup".into()],
                mlua::Error::runtime(format!("Blizzard UI did not load: {e}")),
                &maps,
                &session,
            ));
            return Ok(results);
        }
        clock.mark(&format!("{file} Blizzard UI"));
    }
    let loaded = { session.borrow_mut().load_addon_dir(addon) };
    let setup = loaded.and_then(|_| session.borrow_mut().login());
    let setup_failures = session.borrow().sim().ctx().runtime_failures();
    if let Err(error) = setup.and_then(|_| failures_result(setup_failures)) {
        results.push(failure(&file, vec!["setup".into()], error, &maps, &session));
        return Ok(results);
    }
    clock.mark(&format!("{file} addon load"));
    let lua = session.borrow().sim().lua().clone();
    let test_ctx = session.borrow().sim().ctx().clone();
    let registrations = Rc::new(RefCell::new(Registrations::default()));
    registrations.borrow_mut().sources = sources.to_vec();
    let bridge = BridgeLifetime(lua.clone(), registrations.clone());
    install(&lua, session.clone(), registrations.clone()).map_err(build_error)?;
    let namespace = session
        .borrow()
        .sim()
        .addon_namespace(name)
        .map_err(build_error)?;
    let load = selu_sim::core::dispatch(&lua, &test_ctx, || {
        lua.load(&lowered_lua)
            .set_name(&chunk)
            .call::<()>((name, namespace))
    });
    let mut registration_failures = registrations.borrow().failures.clone();
    registration_failures.extend(test_ctx.runtime_failures());
    registration_failures.extend(session.borrow().sim().ctx().runtime_failures());
    let load = load.and_then(|_| failures_result(registration_failures));
    if let Err(e) = load {
        results.push(failure(
            &file,
            vec!["registration".into()],
            e,
            &maps,
            &session,
        ));
        return Ok(results);
    }
    let boot_notes = gaps(&session);
    let tests = std::mem::take(&mut registrations.borrow_mut().tests);
    for (path, body, hooks) in tests {
        if options
            .filter
            .as_ref()
            .is_some_and(|f| !path.join(" > ").contains(f))
        {
            continue;
        }
        registrations.borrow_mut().notes.clear();
        registrations.borrow_mut().failures.clear();
        test_ctx.begin_observation();
        session.borrow().sim().ctx().begin_observation();
        // The Lua handle and Rust registrations stay alive when reload replaces Session's VM.
        let result = selu_sim::core::dispatch(&lua, &test_ctx, || {
            for scope in hooks {
                let hooks = scope.borrow().clone();
                for hook in hooks {
                    hook.call::<()>(())?;
                }
            }
            body.call::<()>(())
        });
        let mut errors = registrations.borrow().failures.clone();
        errors.extend(test_ctx.runtime_failures());
        errors.extend(session.borrow().sim().ctx().runtime_failures());
        let result = result.and_then(|_| failures_result(errors));
        match result {
            Ok(()) => results.push(TestResult {
                file: file.clone(),
                path,
                outcome: Outcome::Passed,
                not_simulated: gaps(&session),
                replay: None,
            }),
            Err(e) => results.push(failure(&file, path, e, &maps, &session)),
        }
        let mut notes = boot_notes.clone();
        notes.extend(registrations.borrow().notes.clone());
        notes.extend(
            test_ctx
                .logs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .filter(|s| s.contains("not simulated"))
                .cloned(),
        );
        notes.extend(gaps(&session));
        notes.sort();
        notes.dedup();
        results.last_mut().unwrap().not_simulated = notes;
    }
    clock.mark(&format!("{file} tests"));
    drop(bridge);
    drop(lua);
    drop(test_ctx);
    drop(session);
    clock.mark(&format!("{file} teardown"));
    Ok(results)
}

/// The process's private memory, for `SELU_TEST_TIMINGS` only.
#[cfg(windows)]
fn private_mb() -> Option<u64> {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        size: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged: usize,
        quota_paged: usize,
        quota_peak_nonpaged: usize,
        quota_nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
        private: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetCurrentProcess"]
        fn current_process() -> *mut std::ffi::c_void;
        #[link_name = "K32GetProcessMemoryInfo"]
        fn memory_info(process: *mut std::ffi::c_void, counters: *mut Counters, size: u32) -> i32;
    }
    let mut counters = Counters {
        size: std::mem::size_of::<Counters>() as u32,
        ..Default::default()
    };
    // SAFETY: the pseudo handle needs no closing; the struct matches
    // PROCESS_MEMORY_COUNTERS_EX and its size is passed.
    let ok = unsafe { memory_info(current_process(), &mut counters, counters.size) };
    (ok != 0).then_some((counters.private / (1024 * 1024)) as u64)
}
#[cfg(not(windows))]
fn private_mb() -> Option<u64> {
    None
}

type HookScope = Rc<RefCell<Vec<Function>>>;
type Registered = (Vec<String>, Function, Vec<HookScope>);
struct Registrations {
    notes: Vec<String>,
    failures: Vec<selu_sim::core::RuntimeFailure>,
    stale: Rc<std::cell::Cell<bool>>,
    sources: Vec<String>,
    path: Vec<String>,
    scopes: Vec<HookScope>,
    tests: Vec<Registered>,
}
impl Default for Registrations {
    fn default() -> Self {
        Self {
            notes: vec![],
            failures: vec![],
            stale: Default::default(),
            sources: vec![],
            path: vec![],
            scopes: vec![Default::default()],
            tests: vec![],
        }
    }
}

struct BridgeLifetime(Lua, Rc<RefCell<Registrations>>);
impl Drop for BridgeLifetime {
    fn drop(&mut self) {
        self.1.borrow_mut().scopes.clear();
        self.1.borrow_mut().tests.clear();
        for name in ["test", "describe", "beforeEach", "sim", "perf", "expect"] {
            let _ = self.0.globals().raw_set(name, Value::Nil);
        }
        // Release unreachable bridge closures before the Session's final handle.
        bridge::clear(&self.0);
        let _ = self.0.gc_collect();
    }
}
/// Simulator gaps the test reached. Notes logged while Blizzard's own UI
/// loaded, or reached only by Blizzard's secure code afterwards, are the
/// client's, not the addon's under test (Studio still shows them).
fn gaps(s: &Rc<RefCell<Session>>) -> Vec<String> {
    let session = s.borrow();
    let skip = session.default_ui_log_end();
    let client = session.sim().ctx().client_notes();
    session
        .logs()
        .into_iter()
        .skip(skip)
        .filter(|s| s.contains("not simulated") && !client.contains(s))
        .collect()
}
fn failure(
    file: &str,
    path: Vec<String>,
    e: mlua::Error,
    maps: &[selu_sourcemap::SourceMap],
    s: &Rc<RefCell<Session>>,
) -> TestResult {
    let mut message = selu_sourcemap::map_lua_error(&e.to_string(), maps);
    for source in maps.iter().flat_map(|m| &m.sources) {
        if let Some(short) = source.path.strip_prefix("src/") {
            let needle = format!("{short}:");
            if let Some(start) = message.find(&needle)
                && (start == 0 || !matches!(message.as_bytes()[start - 1], b'/' | b'\\'))
            {
                message.replace_range(start..start + short.len(), &source.path);
            }
        }
    }
    let location = maps
        .iter()
        .flat_map(|m| &m.sources)
        .filter_map(|s| {
            let prefix = format!("{}:", s.path);
            let start = message.find(&prefix)?;
            let digits: String = message[start + prefix.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            (!digits.is_empty()).then_some((start, format!("{prefix}{digits}")))
        })
        .min_by_key(|p| p.0)
        .map(|p| p.1);
    TestResult {
        file: file.into(),
        path,
        outcome: Outcome::Failed { message, location },
        not_simulated: gaps(s),
        replay: Some(Replay {
            seed: 1,
            commands: s.borrow().host_commands(),
        }),
    }
}
fn install(
    lua: &Lua,
    session: Rc<RefCell<Session>>,
    r: Rc<RefCell<Registrations>>,
) -> mlua::Result<()> {
    let reg = r.clone();
    lua.globals().set(
        "describe",
        lua.create_function(move |_, (name, body): (String, Function)| {
            {
                let mut r = reg.borrow_mut();
                r.path.push(name);
                r.scopes.push(Default::default());
            }
            let result = body.call::<()>(());
            {
                let mut r = reg.borrow_mut();
                r.path.pop();
                r.scopes.pop();
            }
            result
        })?,
    )?;
    let reg = r.clone();
    lua.globals().set(
        "beforeEach",
        lua.create_function(move |_, body: Function| {
            let r = reg.borrow();
            r.scopes.last().unwrap().borrow_mut().push(body);
            Ok(())
        })?,
    )?;
    let r_stale = r.borrow().stale.clone();
    let switch_observations = r.clone();
    let switch_stale = r.borrow().stale.clone();
    let observations = r.clone();
    let stale = r.borrow().stale.clone();
    lua.globals().set(
        "test",
        lua.create_function(move |_, (name, body): (String, Function)| {
            let mut r = r.borrow_mut();
            let mut path = r.path.clone();
            path.push(name);
            let hooks = r.scopes.clone();
            r.tests.push((path, body, hooks));
            Ok(())
        })?,
    )?;
    let sim = lua.create_table()?;
    let s = session.clone();
    sim.set(
        "now",
        lua.create_function(move |_, ()| Ok(s.borrow().now()))?,
    )?;
    let s = session.clone();
    sim.set(
        "fire",
        lua.create_function(move |_, (event, args): (String, mlua::Variadic<Value>)| {
            let session = s.borrow();
            let args = args
                .iter()
                .map(|v| selu_sim::session::HostValue::capture(v)?.to_lua(session.sim().lua()))
                .collect::<mlua::Result<Vec<_>>>()?;
            session.fire(&event, args).map(|_| ())
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "slash",
        lua.create_function(move |_, line: String| s.borrow().slash(&line))?,
    )?;
    let s = session.clone();
    sim.set(
        "advanceTime",
        lua.create_function(move |_, seconds: f64| {
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(mlua::Error::RuntimeError(
                    "SELU0622: time must be finite and nonnegative".into(),
                ));
            }
            let session = s.borrow();
            session.advance_time(seconds);
            Ok(())
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "reload",
        lua.create_function(move |_, ()| {
            let old = s.borrow().sim().ctx().clone();
            let generation = s.borrow().generation();
            let result = match s.try_borrow_mut() {
                Ok(mut session) => session.reload(),
                Err(_) => Err(mlua::Error::runtime(
                    "cannot reload during a Session callback",
                )),
            };
            // Replacement precedes addon loading/login; either may have failed.
            if s.borrow().generation() != generation {
                stale.set(true);
                old.stale_sources
                    .lock()
                    .unwrap()
                    .extend(observations.borrow().sources.clone());
            }
            // Retirement dispatch and its final flush can add failures, even on Err.
            let mut observations = observations.borrow_mut();
            observations.failures.extend(old.runtime_failures());
            // The retired session's own Blizzard UI notes are the client's.
            let client = old.client_notes();
            observations.notes.extend(
                old.logs
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .filter(|s| s.contains("not simulated") && !client.contains(*s))
                    .cloned(),
            );
            result
        })?,
    )?;
    let s = session.clone();
    let stale = switch_stale;
    let observations = switch_observations;
    sim.set(
        "switchCharacter",
        lua.create_function(move |_, character: String| {
            let old = s.borrow().sim().ctx().clone();
            let generation = s.borrow().generation();
            let result = match s.try_borrow_mut() {
                Ok(mut session) => session.switch_character(character),
                Err(_) => Err(mlua::Error::runtime(
                    "cannot reload during a Session callback",
                )),
            };
            // Replacement precedes addon loading/login; either may have failed.
            if s.borrow().generation() != generation {
                stale.set(true);
                old.stale_sources
                    .lock()
                    .unwrap()
                    .extend(observations.borrow().sources.clone());
            }
            // Retirement dispatch and its final flush can add failures, even on Err.
            let mut observations = observations.borrow_mut();
            observations.failures.extend(old.runtime_failures());
            // The retired session's own Blizzard UI notes are the client's.
            let client = old.client_notes();
            observations.notes.extend(
                old.logs
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .filter(|s| s.contains("not simulated") && !client.contains(*s))
                    .cloned(),
            );
            result
        })?,
    )?;
    let s = session.clone();
    let scenario_session = session.clone();
    sim.set(
        "loadScenario",
        lua.create_function(move |_, path: String| {
            let state = selu_sim::world::Scenario::load(std::path::Path::new(&path))
                .map_err(mlua::Error::runtime)?;
            state.apply(scenario_session.borrow().sim())
        })?,
    )?;
    sim.set(
        "setUnit",
        lua.create_function(move |_, (token, unit): (String, mlua::Table)| {
            s.borrow().set_unit(
                &token,
                selu_sim::world::Unit {
                    form: 0,
                    name: unit
                        .get::<Option<String>>("name")?
                        .unwrap_or_else(|| token.clone()),
                    class: unit
                        .get::<Option<String>>("class")?
                        .unwrap_or_else(|| "WARRIOR".into()),
                    level: unit.get::<Option<u32>>("level")?.unwrap_or(1),
                    health: unit.get::<Option<u32>>("health")?.unwrap_or(100),
                    max_health: unit.get::<Option<u32>>("maxHealth")?.unwrap_or(100),
                    power: unit.get::<Option<u32>>("power")?.unwrap_or(100),
                    max_power: unit.get::<Option<u32>>("maxPower")?.unwrap_or(100),
                    auras: vec![],
                    combat: unit.get::<Option<bool>>("combat")?.unwrap_or(false),
                    classification: unit
                        .get::<Option<String>>("classification")?
                        .unwrap_or_else(|| "normal".into()),
                    creature_type: unit
                        .get::<Option<String>>("creatureType")?
                        .unwrap_or_else(|| "Beast".into()),
                    reaction: unit.get::<Option<u8>>("reaction")?.unwrap_or(3),
                    dead: unit.get::<Option<bool>>("dead")?.unwrap_or(false),
                    guid: unit
                        .get::<Option<String>>("guid")?
                        .unwrap_or_else(|| format!("Creature-scenario-{token}")),
                },
            )?;
            let status = selu_sim::world::UnitStatus {
                xp: unit.get::<Option<u32>>("xp")?,
                max_xp: unit.get::<Option<u32>>("maxXp")?,
                distance: unit.get::<Option<f64>>("distance")?,
                distance_checked: unit.get::<Option<bool>>("distanceChecked")?,
                visible: unit.get::<Option<bool>>("visible")?,
                xp_disabled: unit.get::<Option<bool>>("xpDisabled")?,
                played: unit.get::<Option<f64>>("played")?,
                level_played: unit.get::<Option<f64>>("levelPlayed")?,
            };
            if status != selu_sim::world::UnitStatus::default() {
                s.borrow().set_unit_status(&token, status)?;
            }
            Ok(())
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "setSpell",
        lua.create_function(move |_, (id, spell): (u32, mlua::Table)| {
            s.borrow().set_spell(selu_sim::world::SpellInfo {
                id,
                name: spell.get::<Option<String>>("name")?.unwrap_or_default(),
                icon: spell.get::<Option<u32>>("icon")?.unwrap_or(134400),
                known: spell.get::<Option<bool>>("known")?.unwrap_or(true),
            })
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "setTracking",
        lua.create_function(move |_, entries: mlua::Table| {
            let mut list = Vec::new();
            for entry in entries.sequence_values::<mlua::Table>() {
                let entry = entry?;
                list.push(selu_sim::world::TrackingInfo {
                    name: entry.get::<Option<String>>("name")?.unwrap_or_default(),
                    texture: entry.get::<Option<u32>>("texture")?.unwrap_or(0),
                    active: entry.get::<Option<bool>>("active")?.unwrap_or(false),
                    kind: entry
                        .get::<Option<String>>("type")?
                        .unwrap_or_else(|| "spell".into()),
                    spell_id: entry.get::<Option<u32>>("spellID")?,
                });
            }
            s.borrow().set_tracking(list)
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "setMap",
        lua.create_function(move |_, (id, map): (u32, mlua::Table)| {
            let size = |key: &str| -> mlua::Result<f64> {
                let value = map.get::<Option<f64>>(key)?.unwrap_or(1000.0);
                if value > 0.0 {
                    Ok(value)
                } else {
                    Err(mlua::Error::runtime(format!(
                        "sim.setMap: `{key}` must be greater than 0"
                    )))
                }
            };
            s.borrow().set_map(
                id,
                selu_sim::world::MapArea {
                    name: map.get::<Option<String>>("name")?.unwrap_or_default(),
                    continent: map.get::<Option<u32>>("continent")?.unwrap_or(0),
                    north: map.get::<Option<f64>>("north")?.unwrap_or(0.0),
                    west: map.get::<Option<f64>>("west")?.unwrap_or(0.0),
                    width: size("width")?,
                    height: size("height")?,
                    parent: map.get::<Option<u32>>("parent")?.unwrap_or(0),
                    user_waypoints: map.get::<Option<bool>>("userWaypoints")?.unwrap_or(true),
                },
            )
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "setPlayerPosition",
        lua.create_function(move |_, at: mlua::Table| {
            let mut position = selu_sim::world::PlayerPosition {
                continent: at.get("continent")?,
                north: at.get("north")?,
                west: at.get("west")?,
                facing: at.get("facing")?,
                speed: at.get("speed")?,
                run_speed: at.get("runSpeed")?,
                unknown: at.get("unknown")?,
                facing_unknown: at.get("facingUnknown")?,
            };
            // Map coordinates (0..1) on a map set with sim.setMap.
            if let Some(map) = at.get::<Option<u32>>("map")? {
                let x = at.get::<Option<f64>>("x")?.unwrap_or(0.5);
                let y = at.get::<Option<f64>>("y")?.unwrap_or(0.5);
                let Some((continent, north, west)) = s.borrow().map_to_world(map, x, y)? else {
                    return Err(mlua::Error::runtime(format!(
                        "sim.setPlayerPosition: map {map} is not defined; call sim.setMap first"
                    )));
                };
                position.continent = Some(continent);
                position.north = Some(north);
                position.west = Some(west);
            }
            s.borrow().set_player_position(position)
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "setZone",
        lua.create_function(move |_, zone: mlua::Table| {
            s.borrow().set_zone(
                &zone.get::<Option<String>>("zone")?.unwrap_or_default(),
                &zone.get::<Option<String>>("subzone")?.unwrap_or_default(),
                zone.get::<Option<u32>>("map")?.unwrap_or(0),
                &zone
                    .get::<Option<String>>("instance")?
                    .unwrap_or_else(|| "none".into()),
            )
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "lockCVar",
        lua.create_function(move |_, name: String| s.borrow().lock_cvar(&name))?,
    )?;
    let s = session.clone();
    sim.set(
        "setCVar",
        lua.create_function(move |_, (name, value): (String, String)| {
            s.borrow().set_cvar(&name, &value)
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "callbackCounts",
        lua.create_function(move |lua, ()| {
            let session = s.borrow();
            let counts: mlua::Table = session.sim().lua().globals().get("__selu_sim_counts")?;
            let out = lua.create_table()?;
            out.set("timers", counts.get::<u64>("timers")?)?;
            out.set("updates", counts.get::<u64>("updates")?)?;
            Ok(out)
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "setMoney",
        lua.create_function(move |_, copper: u64| s.borrow().set_money(copper))?,
    )?;
    let s = session.clone();
    sim.set(
        "prints",
        lua.create_function(move |lua, ()| lua.create_sequence_from(s.borrow().prints()))?,
    )?;
    let s = session.clone();
    sim.set(
        "frame",
        lua.create_function(move |lua, name: String| frame_proxy(lua, s.clone(), name))?,
    )?;
    let s = session.clone();
    sim.set(
        "spy",
        lua.create_function(move |lua, name: String| {
            let session = s.borrow();
            let target = session.sim().lua();
            let original: Function = target.globals().get(name.clone())?;
            let original = target.create_registry_value(original)?;
            let calls = Rc::new(target.create_registry_value(target.create_table()?)?);
            let recorded = calls.clone();
            target.globals().set(
                name,
                target.create_function(move |lua, args: mlua::MultiValue| {
                    let calls: mlua::Table = lua.registry_value(&recorded)?;
                    let values = lua.create_table()?;
                    values.raw_set("n", args.len())?;
                    for (i, v) in args.iter().enumerate() {
                        values.raw_set(i + 1, v.clone())?;
                    }
                    calls.raw_set(calls.raw_len() + 1, values)?;
                    lua.registry_value::<Function>(&original)?
                        .call::<mlua::MultiValue>(args)
                })?,
            )?;
            let origin = target.clone();
            let cache = bridge::state(lua);
            let spy = lua.create_table()?;
            spy.set(
                "calls",
                lua.create_function(move |lua, ()| {
                    let calls: mlua::Table = origin.registry_value(&calls)?;
                    let out = lua.create_table()?;
                    for (i, call) in calls.sequence_values::<mlua::Table>().enumerate() {
                        let call = call?;
                        let n: usize = call.raw_get("n")?;
                        let values = lua.create_table()?;
                        values.raw_set("n", n)?;
                        for j in 1..=n {
                            values.raw_set(
                                j,
                                bridge::value(call.raw_get(j)?, &origin, lua, &cache)?,
                            )?;
                        }
                        out.raw_set(i + 1, values)?;
                    }
                    Ok(out)
                })?,
            )?;
            Ok(spy)
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "setCombat",
        lua.create_function(move |_, combat: bool| s.borrow().set_combat(combat))?,
    )?;
    let s = session.clone();
    let perf = lua.create_table()?;
    perf.set(
        "allocations",
        lua.create_function(move |_, _: Function| -> mlua::Result<()> {
            s.borrow().sim().ctx().not_simulated("perf.allocations");
            Err(mlua::Error::RuntimeError(
                "not simulated: perf.allocations".into(),
            ))
        })?,
    )?;
    lua.globals().set("perf", perf)?;
    lua.globals().set("sim", sim)?;
    guard_game_apis(lua, r_stale)?;
    lua.load(include_str!("matchers.lua"))
        .set_name("selu-test-matchers")
        .exec()
}

fn frame_proxy(lua: &Lua, s: Rc<RefCell<Session>>, name: String) -> mlua::Result<Value> {
    let vm = s.borrow().sim().clone();
    let value = vm.lua().globals().get::<Value>(name)?;
    bridge::value(value, vm.lua(), lua, &bridge::state(lua))
}
fn failures_result(errors: Vec<selu_sim::core::RuntimeFailure>) -> mlua::Result<()> {
    if errors.is_empty() {
        return Ok(());
    }
    let messages = errors
        .into_iter()
        .map(|e| format!("{}: {}", e.kind, e.message))
        .collect::<Vec<_>>();
    Err(mlua::Error::runtime(messages.join("\n")))
}
#[cfg(test)]
mod ownership_tests {
    use super::*;
    #[test]
    fn worker_count_follows_the_request_and_never_exceeds_the_files() {
        assert_eq!(worker_count(3, 10), 3);
        assert_eq!(worker_count(8, 2), 2);
        assert_eq!(worker_count(1, 0), 1);
        let auto = worker_count(0, 100);
        assert!((1..=4).contains(&auto));
    }

    #[test]
    fn bridge_releases_the_session_and_spy_vm() {
        let session = Rc::new(RefCell::new(
            Session::new(SessionConfig::default()).unwrap(),
        ));
        let weak = Rc::downgrade(&session);
        let lua = session.borrow().sim().lua().clone();
        let registrations = Rc::new(RefCell::new(Registrations::default()));
        {
            let _bridge = BridgeLifetime(lua.clone(), registrations.clone());
            install(&lua, session.clone(), registrations).unwrap();
            lua.load(
                "local frame = sim.frame('UIParent'); local spy = sim.spy('print'); print('owned')",
            )
            .exec()
            .unwrap();
        }
        drop(lua);
        drop(session);
        assert!(
            weak.upgrade().is_none(),
            "bridge must not retain its Session"
        );
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    #[test]
    fn guards_native_apis_only_and_survives_a_large_global_table() {
        let session = Rc::new(RefCell::new(
            Session::new(SessionConfig::default()).unwrap(),
        ));
        let lua = session.borrow().sim().lua().clone();
        // More globals than mlua has reference slots (about 8000), as with
        // Blizzard's UI loaded, plus one Blizzard-style Lua function.
        lua.load(
            "for i = 1, 9000 do rawset(_G, 'SeluFixtureGlobal' .. i, {}) end
             function SeluFixtureLua() end
             SeluFixtureLuaRef = SeluFixtureLua
             SeluFixtureNativeRef = GetTime",
        )
        .exec()
        .unwrap();
        let registrations = Rc::new(RefCell::new(Registrations::default()));
        let _bridge = BridgeLifetime(lua.clone(), registrations.clone());
        install(&lua, session, registrations).unwrap();
        lua.load(
            "assert(SeluFixtureLua == SeluFixtureLuaRef, 'Lua functions keep their identity')
             assert(GetTime ~= SeluFixtureNativeRef, 'native APIs are guarded')",
        )
        .exec()
        .unwrap();
    }
}

#[cfg(test)]
mod hook_order_tests {
    use super::*;
    #[test]
    fn hooks_declared_after_tests_apply_to_the_whole_scope() {
        let session = Rc::new(RefCell::new(
            Session::new(SessionConfig::default()).unwrap(),
        ));
        let lua = session.borrow().sim().lua().clone();
        let registrations = Rc::new(RefCell::new(Registrations::default()));
        let _bridge = BridgeLifetime(lua.clone(), registrations.clone());
        install(&lua, session, registrations.clone()).unwrap();
        lua.load("local n=0; describe('scope',function() test('late',function() assert(n==2) end); beforeEach(function() n=n+1 end) end); beforeEach(function() n=n+1 end)").exec().unwrap();
        let (_, body, hooks) = registrations.borrow_mut().tests.remove(0);
        for scope in hooks {
            for hook in scope.borrow().iter() {
                hook.call::<()>(()).unwrap();
            }
        }
        body.call::<()>(()).unwrap();
    }
}

fn guard_game_apis(lua: &Lua, stale: Rc<std::cell::Cell<bool>>) -> mlua::Result<()> {
    fn wrap(
        lua: &Lua,
        table: &mlua::Table,
        key: String,
        stale: Rc<std::cell::Cell<bool>>,
    ) -> mlua::Result<()> {
        let f: Function = table.get(key.clone())?;
        let f = lua.create_registry_value(f)?;
        table.set(key,lua.create_function(move |lua,args:mlua::MultiValue| {
            if stale.get() { return Err(mlua::Error::runtime("SELU0623: stale game API after sim.reload(); use sim helpers to access the current Session")); }
            lua.registry_value::<Function>(&f)?.call::<mlua::MultiValue>(args)
        })?)
    }
    // Only the client's native API is guarded. Blizzard's own Lua functions
    // keep their identity: wrapping them would change what hooks and secure
    // calls see. Names are collected first, without their values: with
    // Blizzard's UI loaded there are more globals than mlua has reference
    // slots.
    fn native(v: &Value) -> bool {
        matches!(v, Value::Function(f) if f.info().what == "C")
    }
    let standard = Lua::new();
    let globals = lua.globals();
    let mut functions = Vec::new();
    let mut namespaces = Vec::new();
    for pair in globals.clone().pairs::<Value, Value>() {
        let (name, v) = pair?;
        let Value::String(name) = name else { continue };
        let name = name.to_str()?.to_string();
        if name.starts_with("__")
            || ["test", "describe", "beforeEach", "expect"].contains(&name.as_str())
        {
            continue;
        }
        if native(&v) || (name == "print" && matches!(v, Value::Function(_))) {
            functions.push(name);
        } else if name.starts_with("C_") && matches!(v, Value::Table(_)) {
            namespaces.push(name);
        }
    }
    for name in functions {
        if name == "print" || standard.globals().get::<Value>(name.as_str())?.is_nil() {
            wrap(lua, &globals, name, stale.clone())?;
        }
    }
    for name in namespaces {
        let t: mlua::Table = globals.get(name.as_str())?;
        let mut methods = Vec::new();
        for pair in t.clone().pairs::<Value, Value>() {
            let (key, v) = pair?;
            if let Value::String(key) = key
                && native(&v)
            {
                methods.push(key.to_str()?.to_string());
            }
        }
        for key in methods {
            wrap(lua, &t, key, stale.clone())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod scenario_tests {
    use super::*;
    #[test]
    fn script_loads_studio_scenario() {
        let session = Rc::new(RefCell::new(Session::new(Default::default()).unwrap()));
        let lua = session.borrow().sim().lua().clone();
        let registrations = Rc::new(RefCell::new(Registrations::default()));
        let _bridge = BridgeLifetime(lua.clone(), registrations.clone());
        install(&lua, session, registrations).unwrap();
        let path =
            std::env::temp_dir().join(format!("selu-test-world-{}.toml", std::process::id()));
        let state = selu_sim::world::Scenario {
            money: 987,
            ..Default::default()
        };
        assert!(!path.exists());
        state.save(&path).unwrap();
        lua.globals()
            .set("scenario_path", path.to_string_lossy().as_ref())
            .unwrap();
        lua.load("sim.loadScenario(scenario_path); assert(GetMoney()==987)")
            .exec()
            .unwrap();
        std::fs::remove_file(path).unwrap();
    }
}

/// The module key a test file's import names: a project module under
/// `src/`, or a package entry point (`@selu/<name>`).
fn test_import_key(file: &str, from: &str) -> Option<String> {
    if from.starts_with(crate::packages::PREFIX) {
        return project::resolve_import("main", from).ok();
    }
    project::import_key(file.trim_end_matches(".selu"), from)
        .and_then(|s| s.strip_prefix("src/").map(String::from))
}

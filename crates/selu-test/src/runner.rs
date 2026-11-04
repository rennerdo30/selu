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

/// Build the addon with the same compiler used by `selu build`, then run each file.
pub fn run(options: &RunOptions) -> Result<Report, RunError> {
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
    let manifests = project::find_manifests(
        None,
        &options.project_dir,
        Some(&[target.split('@').next().unwrap().into()]),
    )
    .map_err(|_| build_error("manifest discovery failed"))?;
    let spec = selu_manifest::TargetSpec::parse(&target).map_err(|d| build_error(&d.message))?;
    let manifest =
        selu_manifest::resolve(&spec, &manifests).map_err(|d| build_error(&d.message))?;
    let mut roots = Vec::new();
    for file in &files {
        let source =
            std::fs::read_to_string(options.project_dir.join(file)).map_err(build_error)?;
        let program =
            selu_syntax::parse(file, &source).map_err(|d| build_error(d.render(&source)))?;
        for stmt in &program.stmts {
            if let selu_syntax::StmtKind::Import { from, .. } = &stmt.kind {
                let key = project::import_key(file.trim_end_matches(".selu"), from)
                    .and_then(|s| s.strip_prefix("src/").map(String::from))
                    .ok_or_else(|| build_error("SELU0621: tests must import modules under src/"))?;
                roots.push(key);
            }
        }
    }
    project::build_tests(&options.project_dir, Some(&target), &roots)
        .map_err(|_| build_error("project build failed"))?;
    let name = config
        .get("addon")
        .and_then(|a| a.get("name"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| build_error("missing addon name"))?;
    let addon = options.project_dir.join("dist").join(name);
    let modules = project::project_sources_roots(&options.project_dir.join("src"), &roots)
        .map_err(|_| build_error("module discovery failed"))?;
    let mut exports = BTreeMap::<String, selu_check::ModuleExports>::new();
    for module in &modules {
        let mut bindings = selu_check::ModuleBindings::default();
        for (from, key) in &module.dependencies {
            bindings.imports.insert(from.clone(), exports[key].clone());
        }
        let (_, signature) = selu_check::check_module(&module.program, manifest, bindings);
        exports.insert(module.key.clone(), signature);
    }
    let mut report = Report {
        target: Some(target.clone()),
        results: vec![],
    };
    for file in files {
        let source =
            std::fs::read_to_string(options.project_dir.join(&file)).map_err(build_error)?;
        let program =
            selu_syntax::parse(&file, &source).map_err(|d| build_error(d.render(&source)))?;
        let mut imports = selu_check::ModuleBindings::default();
        let mut lower_imports = BTreeMap::new();
        for stmt in &program.stmts {
            if let selu_syntax::StmtKind::Import { from, .. } = &stmt.kind {
                let key = project::import_key(file.trim_end_matches(".selu"), from)
                    .and_then(|s| s.strip_prefix("src/").map(String::from))
                    .ok_or_else(|| {
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
        let mut test_manifest = manifest.clone();
        overlay(&mut test_manifest);
        let (checked, _) = selu_check::check_module_with_options(
            &program,
            &test_manifest,
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
        let mut bindings = selu_lower::ModuleBindings::new(file.clone(), lower_imports, &program);
        bindings.addon_name = name.into();
        let lowered = selu_lower::lower_module(&program, &test_manifest, &checked.types, bindings);
        let chunk = format!("{file}.lua");
        let mut maps = vec![lowered.source_map(&chunk, &file, &source)];
        for module in &modules {
            let path = options
                .project_dir
                .join("dist/.selu")
                .join(name)
                .join(format!("{}.lua.map", module.key));
            if let Ok(text) = std::fs::read_to_string(path) {
                maps.push(selu_sourcemap::SourceMap::from_json(&text).map_err(build_error)?);
            }
        }
        let session = Rc::new(RefCell::new(
            Session::new(SessionConfig {
                target: Some(target.clone()),
                ..Default::default()
            })
            .map_err(build_error)?,
        ));
        let loaded = { session.borrow_mut().load_addon_dir(&addon) };
        let setup = loaded.and_then(|_| session.borrow_mut().login());
        let setup_failures = session.borrow().sim().ctx().runtime_failures();
        if let Err(error) = setup.and_then(|_| failures_result(setup_failures)) {
            report
                .results
                .push(failure(&file, vec!["setup".into()], error, &maps, &session));
            continue;
        }
        let lua = session.borrow().sim().lua().clone();
        let test_ctx = session.borrow().sim().ctx().clone();
        let registrations = Rc::new(RefCell::new(Registrations::default()));
        registrations.borrow_mut().sources = modules
            .iter()
            .map(|m| format!("{}.lua", m.key))
            .chain(["Selu.lua".into(), "__selu_runtime.lua".into()])
            .collect();
        let _bridge = BridgeLifetime(lua.clone(), registrations.clone());
        install(&lua, session.clone(), registrations.clone()).map_err(build_error)?;
        let namespace = session
            .borrow()
            .sim()
            .addon_namespace(name)
            .map_err(build_error)?;
        let load = selu_sim::core::dispatch(&lua, &test_ctx, || {
            lua.load(&lowered.lua)
                .set_name(&chunk)
                .call::<()>((name, namespace))
        });
        if let Err(e) = load {
            report.results.push(failure(
                &file,
                vec!["registration".into()],
                e,
                &maps,
                &session,
            ));
            continue;
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
                Ok(()) => report.results.push(TestResult {
                    file: file.clone(),
                    path,
                    outcome: Outcome::Passed,
                    not_simulated: gaps(&session),
                    replay: None,
                }),
                Err(e) => report
                    .results
                    .push(failure(&file, path, e, &maps, &session)),
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
            report.results.last_mut().unwrap().not_simulated = notes;
        }
    }
    Ok(report)
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
fn gaps(s: &Rc<RefCell<Session>>) -> Vec<String> {
    s.borrow()
        .logs()
        .into_iter()
        .filter(|s| s.contains("not simulated"))
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
            observations.borrow_mut().notes.extend(gaps(&s));
            observations
                .borrow_mut()
                .failures
                .extend(s.borrow().sim().ctx().runtime_failures());
            let old = s.borrow().sim().ctx().clone();
            s.try_borrow_mut()
                .map_err(|_| mlua::Error::runtime("cannot reload during a Session callback"))?
                .reload()?;
            stale.set(true);
            old.stale_sources
                .lock()
                .unwrap()
                .extend(observations.borrow().sources.clone());
            Ok(())
        })?,
    )?;
    let s = session.clone();
    sim.set(
        "setUnit",
        lua.create_function(move |_, (token, unit): (String, mlua::Table)| {
            s.borrow().set_unit(
                &token,
                selu_sim::world::Unit {
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
                },
            )
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
    let standard = Lua::new();
    let globals = lua.globals();
    let entries = globals
        .clone()
        .pairs::<String, Value>()
        .collect::<mlua::Result<Vec<_>>>()?;
    for (name, v) in entries {
        if name.starts_with("__")
            || ["test", "describe", "beforeEach", "expect"].contains(&name.as_str())
        {
            continue;
        }
        if matches!(v, Value::Function(_))
            && (name == "print" || standard.globals().get::<Value>(name.clone())?.is_nil())
        {
            wrap(lua, &globals, name, stale.clone())?;
        } else if name.starts_with("C_")
            && let Value::Table(t) = v
        {
            let methods = t
                .clone()
                .pairs::<String, Value>()
                .collect::<mlua::Result<Vec<_>>>()?;
            for (key, v) in methods {
                if matches!(v, Value::Function(_)) {
                    wrap(lua, &t, key, stale.clone())?;
                }
            }
        }
    }
    Ok(())
}

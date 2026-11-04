//! Headless WoW simulator core (DESIGN.md section 7): the compiled Lua
//! runs in a Lua 5.1 VM with a simulated clock, captured `print`, a minimal
//! `CreateFrame`, and event firing from Rust. Anything the sim does not
//! model logs `not simulated: X` once; it never fails silently.

use mlua::{Function, Lua, MultiValue, Table, Value};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

mod regions;
pub mod saved;
mod timers;
mod wow_api;

pub use mlua::Value as SimValue;
pub type SimResult<T> = Result<T, mlua::Error>;

type Shared<T> = Arc<Mutex<T>>;

fn value_string(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
        other => format!("{other:?}"),
    }
}

fn log_once(logs: &Shared<Vec<String>>, warned: &Shared<HashSet<String>>, what: &str) {
    let mut w = warned.lock().unwrap();
    if w.insert(what.to_string()) {
        logs.lock().unwrap().push(format!("not simulated: {what}"));
    }
}

pub struct Sim {
    lua: Lua,
    prints: Shared<Vec<String>>,
    logs: Shared<Vec<String>>,
    clock: Shared<f64>,
}

impl Sim {
    pub fn new() -> SimResult<Sim> {
        let lua = Lua::new();
        regions::install(&lua)?;
        let prints: Shared<Vec<String>> = Arc::new(Mutex::new(Vec::new()));
        let logs: Shared<Vec<String>> = Arc::new(Mutex::new(Vec::new()));
        let warned: Shared<HashSet<String>> = Arc::new(Mutex::new(HashSet::new()));
        let clock: Shared<f64> = Arc::new(Mutex::new(0.0));
        let globals = lua.globals();

        {
            let prints = prints.clone();
            globals.set(
                "print",
                lua.create_function(move |_, args: MultiValue| {
                    let parts: Vec<String> = args.iter().map(value_string).collect();
                    prints.lock().unwrap().push(parts.join("\t"));
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

        // Registry of every created frame, so `fire` can find them.
        globals.set("__selu_sim_frames", lua.create_table()?)?;

        {
            let logs = logs.clone();
            let warned = warned.clone();
            globals.set(
                "CreateFrame",
                lua.create_function(
                    move |lua,
                          (kind, _name, parent): (
                        Option<String>,
                        Option<String>,
                        Option<Table>,
                    )| {
                        let frame = lua.create_table()?;
                        frame.set("__kind", kind.unwrap_or_else(|| "Frame".into()))?;
                        frame.set("__events", lua.create_table()?)?;
                        frame.set("__scripts", lua.create_table()?)?;
                        frame.set("__shown", true)?;
                        frame.set("__parent", parent)?;
                        regions::attach(lua, &frame)?;
                        frame.set(
                            "RegisterEvent",
                            lua.create_function(|_, (this, event): (Table, String)| {
                                this.get::<Table>("__events")?.set(event, true)?;
                                Ok(())
                            })?,
                        )?;
                        frame.set(
                            "UnregisterEvent",
                            lua.create_function(|_, (this, event): (Table, String)| {
                                this.get::<Table>("__events")?.set(event, Value::Nil)?;
                                Ok(())
                            })?,
                        )?;
                        frame.set(
                            "SetScript",
                            lua.create_function(
                                |_, (this, name, func): (Table, String, Option<Function>)| {
                                    this.get::<Table>("__scripts")?.set(name, func)?;
                                    Ok(())
                                },
                            )?,
                        )?;
                        frame.set(
                            "GetScript",
                            lua.create_function(|_, (this, name): (Table, String)| {
                                this.get::<Table>("__scripts")?.get::<Value>(name)
                            })?,
                        )?;
                        frame.set(
                            "Show",
                            lua.create_function(|_, this: Table| {
                                let shown = this.get::<bool>("__shown")?;
                                this.set("__shown", true)?;
                                if !shown
                                    && let Some(f) =
                                        this.get::<Table>("__scripts")?
                                            .get::<Option<Function>>("OnShow")?
                                {
                                    f.call::<()>(this.clone())?;
                                }
                                Ok(())
                            })?,
                        )?;
                        frame.set(
                            "Hide",
                            lua.create_function(|_, this: Table| {
                                this.set("__shown", false)?;
                                Ok(())
                            })?,
                        )?;
                        frame.set(
                            "IsShown",
                            lua.create_function(|_, this: Table| this.get::<bool>("__shown"))?,
                        )?;
                        frame.set(
                            "HookScript",
                            lua.create_function(
                                |lua, (this, name, hook): (Table, String, Function)| {
                                    let scripts = this.get::<Table>("__scripts")?;
                                    let old = scripts.get::<Option<Function>>(name.clone())?;
                                    scripts.set(
                                        name,
                                        lua.create_function(move |_, args: MultiValue| {
                                            if let Some(old) = &old {
                                                old.call::<()>(args.clone())?;
                                            }
                                            hook.call::<()>(args)
                                        })?,
                                    )?;
                                    Ok(())
                                },
                            )?,
                        )?;
                        // Unknown frame methods: log once, return a no-op.
                        let mt = lua.create_table()?;
                        let logs = logs.clone();
                        let warned = warned.clone();
                        mt.set(
                            "__index",
                            lua.create_function(move |lua, (_this, key): (Table, String)| {
                                if key.starts_with("__") {
                                    return Ok(Value::Nil);
                                }
                                log_once(&logs, &warned, &format!("Frame.{key}"));
                                let noop =
                                    lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?;
                                Ok(Value::Function(noop))
                            })?,
                        )?;
                        frame.set_metatable(Some(mt));

                        let registry = lua.globals().get::<Table>("__selu_sim_frames")?;
                        let next = registry.len()? + 1;
                        registry.set(next, frame.clone())?;
                        Ok(frame)
                    },
                )?,
            )?;
        }

        let create: Function = globals.get("CreateFrame")?;
        globals.set("UIParent", create.call::<Table>("Frame")?)?;
        // UIParent belongs to the engine, not the addon's created-frame registry.
        let frames: Table = globals.get("__selu_sim_frames")?;
        frames.raw_set(frames.raw_len(), Value::Nil)?;
        // Unknown globals: log once, hand back a callable no-op so the
        // addon keeps running and the gap is visible.
        {
            let mt = lua.create_table()?;
            let logs = logs.clone();
            let warned = warned.clone();
            mt.set(
                "__index",
                lua.create_function(move |lua, (_t, key): (Value, String)| {
                    log_once(&logs, &warned, &key);
                    let noop = lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?;
                    Ok(Value::Function(noop))
                })?,
            )?;
            globals.set_metatable(Some(mt));
        }
        wow_api::install(&lua)?;
        timers::install(&lua, logs.clone())?;

        Ok(Sim {
            lua,
            prints,
            logs,
            clock,
        })
    }

    /// Load generated Lua files in TOC order: `(file name, source)` pairs.
    pub fn load_files(&self, files: &[(String, String)]) -> SimResult<()> {
        for (name, source) in files {
            self.lua.load(source).set_name(name).exec()?;
        }
        Ok(())
    }

    /// Load an addon's files in TOC order the way the client does: every
    /// file receives `...` = the addon's name and one table shared by all of
    /// its files.
    pub fn load_addon(&self, addon: &str, files: &[(String, String)]) -> SimResult<()> {
        let ns = self.lua.create_table()?;
        for (name, source) in files {
            self.lua
                .load(source)
                .set_name(name)
                .call::<()>((addon, ns.clone()))?;
        }
        self.fire(
            "ADDON_LOADED",
            vec![Value::String(self.lua.create_string(addon)?)],
        )?;
        Ok(())
    }

    pub fn slash(&self, line: &str) -> SimResult<()> {
        self.lua
            .globals()
            .get::<Function>("__selu_sim_slash")?
            .call(line)
    }

    pub fn eval(&self, source: &str) -> SimResult<()> {
        self.lua.load(source).exec()
    }

    /// Fire an event at every frame that registered for it.
    pub fn fire(&self, event: &str, args: Vec<Value>) -> SimResult<usize> {
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
                for a in &args {
                    call.push_back(a.clone());
                }
                func.call::<()>(call)?;
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
        timers::advance(&self.lua, &self.clock, seconds);
    }
    pub fn lua(&self) -> &Lua {
        &self.lua
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_event_roundtrip_and_print_capture() {
        let sim = Sim::new().unwrap();
        sim.eval(
            r#"
            local f = CreateFrame("Frame")
            f:RegisterEvent("PLAYER_LOGIN")
            f:SetScript("OnEvent", function(self, event) print("got " .. event) end)
            "#,
        )
        .unwrap();
        let delivered = sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        assert_eq!(delivered, 1);
        assert_eq!(sim.prints(), vec!["got PLAYER_LOGIN".to_string()]);
        sim.fire("UNIT_HEALTH", vec![]).unwrap();
        assert_eq!(sim.prints().len(), 1, "unregistered event must not fire");
    }

    #[test]
    fn unregistered_methods_and_globals_log_once() {
        let sim = Sim::new().unwrap();
        sim.eval(
            r#"
            local f = CreateFrame("Frame")
            f:Wiggle()
            f:Wiggle()
            TotallyMadeUp(1)
            TotallyMadeUp(2)
            "#,
        )
        .unwrap();
        let logs = sim.logs();
        assert_eq!(
            logs.iter().filter(|l| l.contains("Frame.Wiggle")).count(),
            1,
            "{logs:?}"
        );
        assert_eq!(
            logs.iter()
                .filter(|l| l.contains("not simulated: TotallyMadeUp"))
                .count(),
            1,
            "{logs:?}"
        );
    }

    #[test]
    fn addon_files_share_name_and_table() {
        let sim = Sim::new().unwrap();
        sim.load_addon(
            "Demo",
            &[
                (
                    "a.lua".into(),
                    "local name, ns = ...; ns.x = 1; print(name)".into(),
                ),
                ("b.lua".into(), "local _, ns = ...; print(ns.x + 1)".into()),
            ],
        )
        .unwrap();
        assert_eq!(sim.prints(), ["Demo", "2"]);
    }

    #[test]
    fn simulated_clock_advances() {
        let sim = Sim::new().unwrap();
        sim.advance_time(2.5);
        sim.eval("print(GetTime())").unwrap();
        assert_eq!(sim.prints(), vec!["2.5".to_string()]);
        assert_eq!(sim.now(), 2.5);
    }

    #[test]
    fn unregister_stops_delivery() {
        let sim = Sim::new().unwrap();
        sim.eval(
            r#"
            local f = CreateFrame("Frame")
            f:RegisterEvent("PLAYER_LOGIN")
            f:UnregisterEvent("PLAYER_LOGIN")
            f:SetScript("OnEvent", function() print("should not run") end)
            "#,
        )
        .unwrap();
        assert_eq!(sim.fire("PLAYER_LOGIN", vec![]).unwrap(), 0);
        assert!(sim.prints().is_empty());
    }
}

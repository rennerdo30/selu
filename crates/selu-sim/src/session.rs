//! The Session API, the contract between the core and its front ends
//! (`selu test`, Selu Studio, Rust tests). docs/SIMULATOR.md decisions 5,
//! 13, 18, 20 and 22. Owner: package S2.
//!
//! Groundwork (S0): a `Session` owns a [`Sim`] (the VM and today's models)
//! and adds what front ends need on top: a config, addon folders loaded
//! through their `.toc`, the login sequence, `reload()` with a new
//! generation, snapshots and the trace. S2 inverts the ownership (the VM
//! setup moves here and `Sim` becomes the thin wrapper), and adds saved
//! data across reloads, world setters, `run_until_idle`, flavour `.toc`
//! files, dependencies and replay data. Input belongs to S1, debugging to
//! round 10.

use crate::core::{Generation, Snapshot, TraceRecord, lock};
use crate::frames::LuaTableModel;
use crate::{Sim, SimResult, loader, snapshot};
use mlua::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    /// `flavour@version`, as in `selu.toml`. Informational until S2 picks
    /// flavour `.toc` files and the manifest by it.
    pub target: Option<String>,
    /// UIParent size in pixels at scale 1 (decision 4).
    pub ui_size: (f64, f64),
    /// Seeds `math.random` (decision 13).
    pub seed: u32,
    /// Keep trace records (Studio) or only count them (`selu test`).
    pub trace: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            target: None,
            ui_size: snapshot::DEFAULT_UI_SIZE,
            seed: 1,
            trace: false,
        }
    }
}

pub struct Session {
    config: SessionConfig,
    sim: Sim,
    generation: Generation,
    addons: Vec<PathBuf>,
    logged_in: bool,
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
        })
    }

    fn start_vm(config: &SessionConfig) -> SimResult<Sim> {
        let sim = Sim::new()?;
        crate::frames::configure_ui_size(sim.lua(), config.ui_size)?;
        lock(&sim.ctx().trace).enabled = config.trace;
        sim.lua()
            .load(format!("math.randomseed({})", config.seed))
            .exec()?;
        Ok(sim)
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }
    pub fn generation(&self) -> Generation {
        self.generation
    }
    /// The underlying VM, for tests and for what the Session API doesn't
    /// cover yet.
    pub fn sim(&self) -> &Sim {
        &self.sim
    }

    /// Load an addon folder through its `.toc` and fire `ADDON_LOADED`.
    /// Returns the addon name.
    pub fn load_addon_dir(&mut self, dir: &Path) -> SimResult<String> {
        let name = Self::load_into(&self.sim, dir)?;
        self.addons.push(dir.to_path_buf());
        Ok(name)
    }

    fn load_into(sim: &Sim, dir: &Path) -> SimResult<String> {
        let toc = loader::read_toc(dir).map_err(runtime)?;
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
            vec![Value::Boolean(true), Value::Boolean(false)],
        )?;
        Ok(())
    }

    /// Drop the VM, start a new one, load every addon again and log in again
    /// if the session had. The generation changes. Saved data doesn't
    /// survive yet (S2).
    pub fn reload(&mut self) -> SimResult<()> {
        self.sim = Self::start_vm(&self.config)?;
        self.generation = self.generation.next();
        for dir in &self.addons {
            Self::load_into(&self.sim, dir)?;
        }
        if self.logged_in {
            self.logged_in = false;
            self.login()?;
        }
        Ok(())
    }

    pub fn fire(&self, event: &str, args: Vec<Value>) -> SimResult<usize> {
        self.sim.fire(event, args)
    }
    pub fn slash(&self, line: &str) -> SimResult<()> {
        self.sim.slash(line)
    }
    pub fn eval(&self, source: &str) -> SimResult<()> {
        self.sim.eval(source)
    }
    pub fn advance_time(&self, seconds: f64) {
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
        lock(&self.sim.ctx().trace).since(from)
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

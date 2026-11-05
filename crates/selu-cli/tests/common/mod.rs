//! Shared helpers for the behaviour tests: compile Selu source, run the Lua
//! in the simulator, and return what it printed.

#![allow(dead_code)]

use selu_manifest::Manifest;
use selu_sim::Sim;
use std::path::Path;

pub trait BoundedCommand {
    fn bounded_output(&mut self) -> std::io::Result<std::process::Output>;
}
impl BoundedCommand for std::process::Command {
    fn bounded_output(&mut self) -> std::io::Result<std::process::Output> {
        use std::{
            io::{self, Read},
            process::Stdio,
            time::{Duration, Instant},
        };
        fn drain(mut pipe: impl Read) -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes)?;
            Ok(bytes)
        }
        let mut child = self.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        eprintln!("test CLI PID {}, timeout 60s", child.id());
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        // Both pipes must drain while the child runs, even if either stream
        // exceeds its OS pipe capacity before the other produces any output.
        let stdout = std::thread::spawn(move || drain(stdout));
        let stderr = std::thread::spawn(move || drain(stderr));
        let start = Instant::now();
        let status = (|| {
            loop {
                if let Some(status) = child.try_wait()? {
                    return Ok(status);
                }
                if start.elapsed() > Duration::from_secs(60) {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "test CLI exceeded 60s",
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })();
        if status.is_err() {
            // Retain cleanup on timeout and on wait errors before joining readers.
            let _ = child.kill();
            let _ = child.wait();
        }
        let stdout = stdout
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("stdout reader panicked")));
        let stderr = stderr
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("stderr reader panicked")));
        Ok(std::process::Output {
            status: status?,
            stdout: stdout?,
            stderr: stderr?,
        })
    }
}

/// Synthetic saved input is evaluated only between chunks and ADDON_LOADED.
/// Pre-load snapshots use a separate data-only VM, never the addon VM.
pub struct SavedVm {
    sim: Sim,
    input: std::cell::RefCell<Vec<String>>,
    loaded: std::cell::Cell<bool>,
}
impl SavedVm {
    pub fn new() -> Self {
        Self {
            sim: Sim::new().unwrap(),
            input: Default::default(),
            loaded: Default::default(),
        }
    }
    pub fn eval(&self, input: &str) -> selu_sim::SimResult<()> {
        if self.loaded.get() {
            self.sim.eval(input)
        } else {
            self.input.borrow_mut().push(input.into());
            Ok(())
        }
    }
    pub fn load_addon(&self, addon: &str, files: &[(String, String)]) -> selu_sim::SimResult<()> {
        assert!(!self.loaded.replace(true), "use a fresh VM for each login");
        self.sim.load_addon_with_restore(addon, files, |sim| {
            for input in self.input.borrow().iter() {
                sim.eval(input)?;
            }
            Ok(())
        })
    }
    pub fn logout_saved(
        &self,
        names: &[&str],
    ) -> selu_sim::SimResult<selu_sim::saved::SavedSnapshot> {
        if self.loaded.get() {
            self.sim.logout_saved(names)
        } else {
            let data = Sim::new()?;
            for input in self.input.borrow().iter() {
                data.eval(input)?;
            }
            data.logout_saved(names)
        }
    }
}
impl std::ops::Deref for SavedVm {
    type Target = Sim;
    fn deref(&self) -> &Self::Target {
        &self.sim
    }
}

pub fn manifest() -> Manifest {
    selu_manifest::load_manifest(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json"),
    )
    .unwrap()
}

/// The generated Lua for `src`; panics with the rendered diagnostics if the
/// checker reports anything.
pub fn compile(src: &str) -> String {
    let manifest = manifest();
    let program = selu_syntax::parse("t.selu", src).unwrap_or_else(|d| panic!("{}", d.render(src)));
    let checked = selu_check::check_program(&program, &manifest);
    assert!(
        checked.diagnostics.is_empty(),
        "{}",
        checked
            .diagnostics
            .iter()
            .map(|d| d.render(src))
            .collect::<String>()
    );
    selu_lower::lower(&program, &manifest, &checked.types).lua
}

/// Compiles `src`, loads it, fires PLAYER_LOGIN and returns the printed
/// lines.
pub fn run(src: &str) -> Vec<String> {
    let lua = compile(src);
    let sim = Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua.clone())])
        .unwrap_or_else(|e| panic!("{e}\n--- generated Lua ---\n{lua}"));
    sim.fire("PLAYER_LOGIN", vec![])
        .unwrap_or_else(|e| panic!("{e}\n--- generated Lua ---\n{lua}"));
    sim.prints()
}

/// The diagnostic codes the checker reports for `src`.
pub fn codes(src: &str) -> Vec<String> {
    let program = selu_syntax::parse("t.selu", src).unwrap_or_else(|d| panic!("{}", d.render(src)));
    selu_check::check(&program, &manifest())
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

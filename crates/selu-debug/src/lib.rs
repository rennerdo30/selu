//! Selu Studio's engine side (docs/SIMULATOR.md decisions 2, 6 and 17).
//! Owner: package S5 (round 9); the debugger commands are S6 (round 10).
//!
//! Groundwork (round 8): the command/update protocol and the worker thread
//! that owns the [`selu_sim::Session`]. The GUI never touches Lua: it sends
//! [`Command`]s and draws from [`Update`]s. `selu-studio --headless` drives
//! the same worker from JSON lines, which is also how the protocol is
//! tested.

use selu_sim::{Session, SessionConfig, SimValue, Snapshot, TraceRecord};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

/// An event or command argument, as a front end can write it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Arg {
    Bool(bool),
    Number(f64),
    String(String),
    Nil(()),
}

/// Front end to worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    LoadDefaultUi {
        source: PathBuf,
    },
    SeedSaved {
        dir: PathBuf,
        values: std::collections::BTreeMap<String, selu_sim::saved::SavedValue>,
    },
    LoadAddon {
        dir: PathBuf,
    },
    Login,
    Reload,
    Fire {
        event: String,
        #[serde(default)]
        args: Vec<Arg>,
    },
    Slash {
        line: String,
    },
    AdvanceTime {
        seconds: f64,
    },
    /// Run Lua in the session (Studio's console).
    Eval {
        code: String,
    },
    Scenario {
        state: selu_sim::world::Scenario,
    },
    Snapshot,
    // Round 10 (S6) adds: SetBreakpoints, Pause, Resume, StepIn/Over/Out,
    // Stack, Locals. While paused the worker serves only those and
    // Snapshot (decision 17).
    Shutdown,
}

/// Worker to front end. After every command the worker sends the new
/// prints, logs and trace records it produced, then `Done`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "update", rename_all = "snake_case")]
pub enum Update {
    Scenario {
        state: selu_sim::world::Scenario,
    },
    Prints {
        lines: Vec<String>,
    },
    Logs {
        lines: Vec<String>,
    },
    Trace {
        records: Vec<TraceRecord>,
    },
    Snapshot {
        snapshot: Box<Snapshot>,
    },
    /// A command failed; the session keeps running.
    Error {
        message: String,
    },
    /// The session generation changed (`reload()`): drop stale selections.
    Generation {
        generation: u64,
    },
    Done {
        seq: u64,
    },
}

pub struct Worker {
    commands: Sender<Command>,
    pub updates: Receiver<Update>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Start a session on its own thread. The `Lua` VM is created on that
    /// thread and never leaves it.
    pub fn spawn(config: SessionConfig) -> Worker {
        let (commands, inbox) = channel::<Command>();
        let (outbox, updates) = channel::<Update>();
        let thread = std::thread::spawn(move || run(config, inbox, outbox));
        Worker {
            commands,
            updates,
            thread: Some(thread),
        }
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Send a command and collect its updates up to and including `Done`.
    pub fn call(&self, command: Command) -> Vec<Update> {
        self.send(command);
        let mut out = Vec::new();
        while let Ok(update) = self.updates.recv() {
            let done = matches!(update, Update::Done { .. });
            out.push(update);
            if done {
                break;
            }
        }
        out
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn to_value(session: &Session, arg: &Arg) -> Result<SimValue, String> {
    Ok(match arg {
        Arg::Nil(()) => SimValue::Nil,
        Arg::Bool(b) => SimValue::Boolean(*b),
        Arg::Number(n) => SimValue::Number(*n),
        Arg::String(s) => SimValue::String(
            session
                .sim()
                .lua()
                .create_string(s)
                .map_err(|e| e.to_string())?,
        ),
    })
}

fn run(config: SessionConfig, inbox: Receiver<Command>, outbox: Sender<Update>) {
    let mut session = match Session::new(config) {
        Ok(s) => s,
        Err(e) => {
            let _ = outbox.send(Update::Error {
                message: format!("cannot start the session: {e}"),
            });
            return;
        }
    };
    let (mut prints_seen, mut logs_seen, mut trace_seen) = (0, 0, 0);
    let mut seq = 0;
    let mut manifest_seen = String::new();
    let mut default_ui_start = None;
    while let Ok(command) = inbox.recv() {
        if command == Command::Shutdown {
            break;
        }
        let before = session.generation();
        let result: Result<Option<Update>, String> = match command {
            Command::LoadDefaultUi { source } => {
                default_ui_start = Some(std::time::Instant::now());
                session
                    .load_default_ui(&source, |name| {
                        eprintln!("Default UI: loading {name}");
                        let _ = outbox.send(Update::Logs {
                            lines: vec![format!("Default UI: loading {name}")],
                        });
                    })
                    .map(|report| {
                        let mut lines = vec![format!(
                            "Default UI: {} addons, {} errors, {:.2}s",
                            report.addons_loaded,
                            report.errors.len(),
                            report.elapsed.as_secs_f64()
                        )];
                        let reported: std::collections::HashSet<_> =
                            report.errors.iter().chain(&report.notes).cloned().collect();
                        lines.extend(report.errors.into_iter().map(|s| {
                            if s.starts_with("error:") {
                                s
                            } else {
                                format!("error: {s}")
                            }
                        }));
                        lines.extend(report.notes);
                        let logs = session.logs();
                        lines.extend(logs.iter().filter(|s| !reported.contains(*s)).cloned());
                        logs_seen = logs.len();
                        for line in &lines {
                            eprintln!("{line}");
                        }
                        let _ = outbox.send(Update::Logs { lines });
                        None
                    })
                    .map_err(|e| e.to_string())
            }
            Command::SeedSaved { dir, values } => session
                .seed_saved(&dir, &values)
                .map(|_| None)
                .map_err(|e| e.to_string()),
            Command::LoadAddon { dir } => session
                .load_addon_dir(&dir)
                .map(|_| None)
                .map_err(|e| e.to_string()),
            Command::Login => session
                .login()
                .map(|_| {
                    if default_ui_start.is_some() {
                        finish_default_ui_startup(&session, &outbox, default_ui_start.take());
                    }
                    None
                })
                .map_err(|e| e.to_string()),
            Command::Reload => {
                default_ui_start = Some(std::time::Instant::now());
                session
                    .reload()
                    .map(|_| {
                        finish_default_ui_startup(&session, &outbox, default_ui_start.take());
                        None
                    })
                    .map_err(|e| e.to_string())
            }
            Command::Fire { event, args } => args
                .iter()
                .map(|a| to_value(&session, a))
                .collect::<Result<Vec<_>, _>>()
                .and_then(|args| {
                    session
                        .fire(&event, args)
                        .map(|_| None)
                        .map_err(|e| e.to_string())
                }),
            Command::Slash { line } => session
                .slash(&line)
                .map(|_| None)
                .map_err(|e| e.to_string()),
            Command::AdvanceTime { seconds } => {
                session.advance_time(seconds);
                Ok(None)
            }
            Command::Eval { code } => session.eval(&code).map(|_| None).map_err(|e| e.to_string()),
            Command::Scenario { state } => state
                .apply(session.sim())
                .map(|_| None)
                .map_err(|e| e.to_string()),
            Command::Snapshot => session
                .snapshot()
                .map(|s| {
                    Some(Update::Snapshot {
                        snapshot: Box::new(s),
                    })
                })
                .map_err(|e| e.to_string()),
            Command::Shutdown => unreachable!(),
        };
        if session.generation() != before {
            // A new VM starts its buffers from zero.
            (prints_seen, logs_seen, trace_seen) = (0, 0, 0);
            let _ = outbox.send(Update::Generation {
                generation: session.generation().0,
            });
        }
        let prints = session.prints();
        if prints.len() > prints_seen {
            let _ = outbox.send(Update::Prints {
                lines: prints[prints_seen..].to_vec(),
            });
            prints_seen = prints.len();
        }
        let identities = session.manifest_identities();
        let identity = if identities.is_empty() {
            "manifest: none loaded".into()
        } else {
            format!("manifest loaded: {}", identities.join(", "))
        };
        if identity != manifest_seen {
            let _ = outbox.send(Update::Logs {
                lines: vec![identity.clone()],
            });
            manifest_seen = identity;
        }
        let logs = session.logs();
        if logs.len() > logs_seen {
            let _ = outbox.send(Update::Logs {
                lines: logs[logs_seen..].to_vec(),
            });
            logs_seen = logs.len();
        }
        let records = session.trace_since(trace_seen);
        if let Some(last) = records.last() {
            trace_seen = last.seq + 1;
            let _ = outbox.send(Update::Trace { records });
        }
        match result {
            Ok(Some(update)) => {
                let _ = outbox.send(update);
            }
            Ok(None) => {}
            Err(message) => {
                let _ = outbox.send(Update::Error { message });
            }
        }
        if let Ok(state) = selu_sim::world::Scenario::capture(session.sim()) {
            let _ = outbox.send(Update::Scenario { state });
        }
        let _ = outbox.send(Update::Done { seq });
        seq += 1;
    }
}

fn finish_default_ui_startup(
    session: &Session,
    outbox: &Sender<Update>,
    start: Option<std::time::Instant>,
) {
    let Some(report) = session.default_ui_report() else {
        return;
    };
    // Blizzard layout frames propagate dirty child sizes to their parents on
    // OnUpdate. Let the initial preview settle through four bounded frames.
    for _ in 0..4 {
        session.advance_time(1.0 / 60.0);
    }
    let errors = session.default_ui_startup_errors();
    let summary = format!(
        "Default UI startup completed: {} addons, {} load errors, {} startup errors, {:.2}s total",
        report.addons_loaded,
        report.errors.len(),
        errors.len(),
        start
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(report.elapsed.as_secs_f64())
    );
    eprintln!("{summary}");
    selu_sim::profile::report();
    for error in &errors {
        eprintln!("{error}");
    }
    let mut lines = vec![summary];
    for (name, count) in session.missing_natives().into_iter().take(15) {
        let line = format!("Missing native: {name} ({count} calls)");
        eprintln!("{line}");
        lines.push(line);
    }
    let _ = outbox.send(Update::Logs { lines });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_ui_bootstrap_runs_initial_timers_and_preserves_world_apis() {
        let worker = Worker::spawn(SessionConfig::default());
        worker.call(Command::Eval {
            code: "UnitRace=function() return 'ScenarioRace' end".into(),
        });
        worker.call(Command::LoadDefaultUi {
            source: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../selu-sim/tests/fixtures/default-ui"),
        });
        worker.call(Command::Eval {code:"assert(UnitRace()=='ScenarioRace'); C_Timer.After(0,function() BootstrapTimerRan=true end)".into()});
        worker.call(Command::Login);
        let updates = worker.call(Command::Eval {
            code: "assert(BootstrapTimerRan); assert(GetTime()>0)".into(),
        });
        assert!(
            !updates.iter().any(|u| matches!(u, Update::Error { .. })),
            "{updates:?}"
        );
    }

    fn hello() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../selu-sim/tests/addons/Hello")
    }

    #[test]
    fn worker_runs_a_session_on_its_own_thread() {
        let worker = Worker::spawn(SessionConfig {
            trace: true,
            ..SessionConfig::default()
        });
        let updates = worker.call(Command::LoadAddon { dir: hello() });
        assert!(
            updates.contains(&Update::Prints {
                lines: vec!["Hello loaded".into()]
            }),
            "{updates:?}"
        );
        worker.call(Command::Login);
        let updates = worker.call(Command::Snapshot);
        let Some(Update::Snapshot { snapshot }) = updates
            .iter()
            .find(|u| matches!(u, Update::Snapshot { .. }))
        else {
            panic!("{updates:?}")
        };
        assert!(snapshot.regions.len() >= 2);

        let updates = worker.call(Command::Reload);
        assert!(
            updates.contains(&Update::Generation { generation: 1 }),
            "{updates:?}"
        );
        let updates = worker.call(Command::Eval {
            code: "error('boom')".into(),
        });
        assert!(
            updates
                .iter()
                .any(|u| matches!(u, Update::Error { message } if message.contains("boom"))),
            "{updates:?}"
        );
    }

    #[test]
    fn commands_read_from_json_lines() {
        let c: Command = serde_json::from_str(
            r#"{"command":"fire","event":"UNIT_HEALTH","args":["player", 3, true, null]}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::Fire {
                event: "UNIT_HEALTH".into(),
                args: vec![
                    Arg::String("player".into()),
                    Arg::Number(3.0),
                    Arg::Bool(true),
                    Arg::Nil(())
                ]
            }
        );
    }
}

#[cfg(test)]
mod scenario_tests {
    #[test]
    fn studio_scenario_round_trip() {
        let mut state = selu_sim::world::Scenario::default();
        state.units.insert(
            "target".into(),
            selu_sim::world::Unit {
                classification: "rare".into(),
                ..Default::default()
            },
        );
        state.bags.insert(0, vec![6948]);
        let path =
            std::env::temp_dir().join(format!("selu-studio-scenario-{}.toml", std::process::id()));
        assert!(!path.exists());
        state.save(&path).unwrap();
        let loaded = selu_sim::world::Scenario::load(&path).unwrap();
        assert_eq!(state, loaded);
        let worker = super::Worker::spawn(Default::default());
        let updates = worker.call(super::Command::Scenario { state: loaded });
        assert!(
            !updates
                .iter()
                .any(|u| matches!(u, super::Update::Error { .. })),
            "{updates:?}"
        );
        let updates = worker.call(super::Command::Eval {
            code: "assert(UnitClassification('target')=='rare')".into(),
        });
        assert!(
            !updates
                .iter()
                .any(|u| matches!(u, super::Update::Error { .. })),
            "{updates:?}"
        );
        std::fs::remove_file(path).unwrap();
    }
}

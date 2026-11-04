//! `selu-studio`: Selu Studio, started by `selu debug` (docs/SIMULATOR.md
//! decision 2). Owner: package S5 (round 9).
//!
//! ```text
//! selu-studio [--target flavour@version] [--headless] <addon folder>...
//! ```
//!
//! Groundwork (round 8): `--headless` loads the addons, logs in, then reads
//! one JSON [`Command`] per line from stdin and writes one JSON [`Update`]
//! per line to stdout. Without `--headless` it says that the window comes
//! with S5 and exits with code 2.

use selu_debug::{Command, Update, Worker};
use selu_sim::SessionConfig;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: selu-studio [--target flavour@version] [--headless] <addon folder>...";

fn main() -> ExitCode {
    let mut target = None;
    let mut headless = false;
    let mut addons: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => match args.next() {
                Some(t) => target = Some(t),
                None => {
                    eprintln!("selu-studio: --target needs a value\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--headless" => headless = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other if other.starts_with("--") => {
                eprintln!("selu-studio: unknown option {other}\n{USAGE}");
                return ExitCode::from(2);
            }
            dir => addons.push(PathBuf::from(dir)),
        }
    }
    if addons.is_empty() {
        eprintln!("selu-studio: no addon folder given\n{USAGE}");
        return ExitCode::from(2);
    }
    if !headless {
        eprintln!(
            "selu-studio: the Studio window is not implemented yet (round 9, package S5); \
             `--headless` drives the simulator from JSON lines on stdin"
        );
        return ExitCode::from(2);
    }

    let worker = Worker::spawn(SessionConfig {
        target,
        trace: true,
        ..SessionConfig::default()
    });
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let emit = |updates: Vec<Update>, out: &mut std::io::StdoutLock<'_>| {
        for u in updates {
            let _ = writeln!(
                out,
                "{}",
                serde_json::to_string(&u).expect("updates serialise")
            );
        }
        let _ = out.flush();
    };
    for dir in addons {
        emit(worker.call(Command::LoadAddon { dir }), &mut out);
    }
    emit(worker.call(Command::Login), &mut out);
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Command>(line) {
            Ok(Command::Shutdown) => break,
            Ok(command) => emit(worker.call(command), &mut out),
            Err(e) => emit(
                vec![Update::Error {
                    message: format!("bad command: {e}"),
                }],
                &mut out,
            ),
        }
    }
    ExitCode::SUCCESS
}

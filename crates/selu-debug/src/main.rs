//! `selu-studio`: Selu Studio, started by `selu debug` (docs/SIMULATOR.md
//! decision 2).
//!
//! ```text
//! selu-studio [--target product@build] [--manifest-dir dir | --manifest snapshot] [--project dir --selu exe] [--headless] [--no-client-assets | --client-install root] <addon folder>...
//! ```
//!
//! Opens the Studio window (`app.rs`). `--project` and `--selu` enable
//! rebuild on save. `--headless` instead loads the addons, logs in, then
//! reads one JSON [`Command`] per line from stdin and writes one JSON
//! [`Update`] per line to stdout.

mod app;
mod editor;
mod game;
mod png;
mod settings;
mod text_render;
mod textures;
mod theme;

use selu_debug::{Command, Update, Worker};
use selu_sim::SessionConfig;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: selu-studio [--target product@build] [--manifest-dir dir | --manifest snapshot] [--project dir --selu exe] [--headless] [--no-client-assets | --client-install root] <addon folder>...";

fn main() -> ExitCode {
    let mut target = None;
    let mut manifest_path = None;
    let mut manifest_dir = None;
    let mut headless = false;
    let mut no_client_assets = false;
    let mut client_install = None;
    let mut project = None;
    let mut selu = None;
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
            "--manifest-dir" => match args.next() {
                Some(p) => manifest_dir = Some(PathBuf::from(p)),
                None => {
                    eprintln!("--manifest-dir needs a path");
                    return ExitCode::from(2);
                }
            },
            "--manifest" => match args.next() {
                Some(p) => manifest_path = Some(PathBuf::from(p)),
                None => {
                    eprintln!("--manifest needs a path");
                    return ExitCode::from(2);
                }
            },
            "--project" | "--selu" => match args.next() {
                Some(v) if arg == "--project" => project = Some(PathBuf::from(v)),
                Some(v) => selu = Some(PathBuf::from(v)),
                None => {
                    eprintln!(
                        "selu-studio: {arg} needs a value
{USAGE}"
                    );
                    return ExitCode::from(2);
                }
            },
            "--no-client-assets" => no_client_assets = true,
            "--client-install" => match args.next() {
                Some(p) => client_install = Some(PathBuf::from(p)),
                None => {
                    eprintln!("--client-install needs a path");
                    return ExitCode::from(2);
                }
            },
            "--headless" => headless = true,
            "--list-games" => {
                // Diagnostics: the game clients Run in Game would use.
                let settings = game::Settings::load();
                match game::battlenet_exe() {
                    Some(b) => println!("Battle.net: {}", b.display()),
                    None => println!("Battle.net: not found (games start directly)"),
                }
                for c in game::detect(&settings) {
                    println!(
                        "{:<22} {:<16} {:<34} {}",
                        c.flavour.as_deref().unwrap_or("unknown"),
                        c.exe
                            .as_ref()
                            .and_then(|e| e.file_name())
                            .map_or_else(|| "-".to_string(), |n| n.to_string_lossy().into_owned()),
                        game::launch_code(&c.folder_name()).map_or_else(
                            || "start directly".to_string(),
                            |p| format!("--exec=\"launch {p}\"")
                        ),
                        c.folder.display()
                    );
                }
                return ExitCode::SUCCESS;
            }
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
    // A project that has not built yet opens without an addon folder.
    if addons.is_empty() && (headless || project.is_none()) {
        eprintln!("selu-studio: no addon folder given\n{USAGE}");
        return ExitCode::from(2);
    }
    let mut manifest = match manifest_path {
        Some(path) => {
            // A .selus history store is not a checked snapshot; say so plainly.
            // (Kept here: resolution.rs is a frozen extraction input.)
            let store = path.extension().is_some_and(|e| e == "selus")
                || std::fs::read(&path).is_ok_and(|b| b.starts_with(b"SELUSEG"));
            if store {
                eprintln!(
                    "error[SELU0202]: --manifest expects a checked SELUMAN snapshot, not a .selus history store; use --manifest-dir <manifests directory> with --target <product@build>"
                );
                return ExitCode::FAILURE;
            }
            match selu_manifest::resolution::load_checked_manifest(&path, target.as_deref()) {
                Ok(m) => Some(m),
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        None => None,
    };
    if manifest.is_none() && manifest_dir.is_some() && !addons.is_empty() {
        let result = selu_manifest::resolution::discover(
            manifest_dir.as_deref(),
            &addons[0],
            target.as_ref().map(std::slice::from_ref),
        )
        .and_then(|all| {
            selu_manifest::resolution::select_checked_manifest(&all, target.as_deref()).cloned()
        });
        match result {
            Ok(m) => manifest = Some(m),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    }
    let settings = game::Settings::load();
    let product = manifest
        .as_ref()
        .map(selu_manifest::Manifest::target_string)
        .or_else(|| target.clone());
    let client_install = if no_client_assets || settings.disable_client_assets {
        None
    } else {
        client_install
            .or(settings.client_install.clone())
            .or_else(|| {
                let product = product.as_deref()?.split('@').next()?;
                game::detect(&settings)
                    .into_iter()
                    .find(|c| c.flavour.as_deref() == Some(product))?
                    .folder
                    .parent()
                    .map(PathBuf::from)
            })
    };
    // The owner's whole-drive subst aliases E: and D:; retain the public E: spelling.
    let client_install = client_install.map(|root| {
        let text = root.to_string_lossy();
        if text.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("D:")) {
            PathBuf::from(format!("E:{}", &text[2..]))
        } else {
            root
        }
    });
    match &client_install {
        Some(root) => eprintln!("client assets enabled: {}", root.display()),
        None => eprintln!("client assets unavailable or disabled; bundled font fallback"),
    }
    if !headless {
        let options = app::Options {
            addons,
            client_install,
            manifest,
            manifest_dir,
            target,
            project,
            selu,
        };
        let mut native = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_title("Selu Studio")
                .with_inner_size([1600.0, 980.0])
                .with_min_inner_size([900.0, 560.0])
                // Studio draws its own title bar with the window buttons.
                .with_decorations(false)
                .with_resizable(true)
                .with_icon(
                    eframe::icon_data::from_png_bytes(theme::LOGO_PNG)
                        .expect("the bundled logo is a valid PNG"),
                ),
            ..Default::default()
        };
        if std::env::var_os("SELU_STUDIO_SCREENSHOT").is_some() {
            native.viewport = native
                .viewport
                .with_active(false)
                .with_position([-32000.0, -32000.0]);
        }
        return match eframe::run_native(
            "Selu Studio",
            native,
            Box::new(|cc| {
                theme::install(&cc.egui_ctx);
                Ok(Box::new(app::Studio::new(options, &cc.egui_ctx)))
            }),
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("selu-studio: cannot open the window: {e}");
                ExitCode::FAILURE
            }
        };
    }

    let worker = Worker::spawn(SessionConfig {
        client_install,
        manifest,
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

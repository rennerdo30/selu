//! `selu` command line. `build` works end to end for the current Selu
//! subset; `test` and `debug` are wired to the `selu-test` runner and the
//! `selu-studio` binary (both groundwork until rounds 9 and 10), and `probe`
//! is an honest stub.

use clap::{Parser, Subcommand};

use selu_test::project::{build, find_manifests};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "selu",
    version,
    about = "Selu: a language that compiles to World of Warcraft addon Lua"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read assets from local game storage (never downloads).
    Gamedata {
        #[arg(long)]
        install: PathBuf,
        #[arg(long, default_value = "wowf")]
        product: String,
        #[arg(long)]
        listfile: Option<PathBuf>,
        /// info or cat
        action: String,
        file: Option<String>,
    },
    /// Compile a Selu project to `dist/<Name>/`.
    Build {
        /// Project folder holding `selu.toml` (default: current folder).
        path: Option<PathBuf>,
        /// Override the project targets, e.g. `wow@12.1.5.70783` (comma-separated).
        #[arg(long)]
        target: Option<String>,
        /// Folder holding target manifests (default: search upwards for `manifests/`).
        #[arg(long)]
        manifest_dir: Option<PathBuf>,
    },
    /// Run *.test.selu scenarios in the simulator.
    Test {
        #[arg(long)]
        client_install: Option<PathBuf>,
        #[arg(long)]
        no_client_assets: bool,
        path: Option<PathBuf>,
        #[arg(long)]
        manifest_dir: Option<PathBuf>,
        #[arg(long)]
        target: Option<String>,
        /// Only tests whose name contains this text.
        #[arg(long)]
        filter: Option<String>,
        /// Require nonempty scenarios and refuse simulator gaps (port CI).
        #[arg(long)]
        strict: bool,
        #[arg(long)]
        json: bool,
    },
    /// Build, then start Selu Studio, the virtual WoW simulator (window not implemented yet).
    Debug {
        path: Option<PathBuf>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        manifest_dir: Option<PathBuf>,
    },
    /// Format `.selu` files (default: the project in the current folder).
    Fmt {
        /// Files or folders; a project folder formats its `src/` and `tests/`.
        paths: Vec<PathBuf>,
        /// Change nothing; list the files that aren't formatted and fail if any.
        #[arg(long)]
        check: bool,
    },
    /// List stored products and builds.
    Targets {
        /// Move the project lock to newest stored builds (optionally select products).
        #[arg(long, num_args = 0..)]
        update: Option<Vec<String>>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        manifest_dir: Option<PathBuf>,
    },
    /// Build a probe addon to verify a manifest in game (not implemented yet).
    Probe {
        #[arg(long)]
        target: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Gamedata {
            install,
            product,
            listfile,
            action,
            file,
        } => gamedata(
            install,
            product,
            listfile.as_deref(),
            action,
            file.as_deref(),
        ),
        Command::Build {
            path,
            target,
            manifest_dir,
        } => build(
            path.as_deref().unwrap_or(Path::new(".")),
            target.as_deref(),
            manifest_dir.as_deref(),
        ),
        Command::Targets {
            manifest_dir,
            update,
            yes,
        } => {
            if let Some(products) = update {
                update_targets(manifest_dir.as_deref(), products, *yes)
            } else {
                targets(manifest_dir.as_deref())
            }
        }
        Command::Fmt { paths, check } => fmt(paths, *check),
        Command::Test {
            client_install,
            no_client_assets,
            path,
            manifest_dir,
            target,
            filter,
            strict,
            json,
        } => test(
            client_install.clone(),
            *no_client_assets,
            path.as_deref().unwrap_or(Path::new(".")),
            target.clone(),
            filter.clone(),
            *strict,
            *json,
            manifest_dir.clone(),
        ),
        Command::Debug {
            path,
            target,
            manifest_dir,
        } => debug(
            path.as_deref().unwrap_or(Path::new(".")),
            target.as_deref(),
            manifest_dir.as_deref(),
        ),
        Command::Probe { .. } => Err(not_yet("selu probe", "in-game manifest probing")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn gamedata(
    install: &Path,
    product: &str,
    listfile: Option<&Path>,
    action: &str,
    file: Option<&str>,
) -> Result<(), ExitCode> {
    let run = || -> selu_gamedata::Result<()> {
        let storage = selu_gamedata::Storage::open(install, product, listfile)?;
        match action {
            "info" => println!(
                "{} {}: {} root entries",
                storage.build.product,
                storage.build.version,
                storage.entry_count()
            ),
            "atlas" => {
                println!(
                    "{:#?}",
                    storage.atlas(file.ok_or("atlas requires a name")?)?
                );
            }
            "cat" => {
                use std::io::Write;
                let file = file.ok_or("cat requires a path or FileDataID")?;
                let bytes = match file.parse() {
                    Ok(id) => storage.read(id)?,
                    Err(_) => storage.read_path(file)?,
                };
                std::io::stdout().lock().write_all(&bytes)?;
            }
            _ => return Err("expected info, cat or atlas".into()),
        }
        Ok(())
    };
    run().map_err(|error| {
        eprintln!("gamedata: {error}");
        ExitCode::FAILURE
    })
}

/// The `.selu` files `selu fmt` works on for one argument.
fn fmt_files(path: &Path, out: &mut Vec<PathBuf>) {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if p.is_dir() {
                if !matches!(name, "dist" | "target" | ".git" | "node_modules") {
                    walk(&p, out);
                }
            } else if name.ends_with(".selu") {
                out.push(p);
            }
        }
    }
    if path.is_file() {
        out.push(path.to_path_buf());
    } else if path.join("selu.toml").is_file() {
        walk(&path.join("src"), out);
        walk(&path.join("tests"), out);
    } else {
        walk(path, out);
    }
}

/// Formatter options from the nearest `selu.toml` above `file`.
fn fmt_options(file: &Path) -> selu_fmt::Options {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if let Ok(text) = std::fs::read_to_string(d.join("selu.toml")) {
            let value = toml::from_str::<toml::Value>(&text).ok();
            let get = |key: &str| {
                value
                    .as_ref()
                    .and_then(|v| v.get("format")?.get(key)?.as_integer())
            };
            let defaults = selu_fmt::Options::default();
            return selu_fmt::Options {
                indent: get("indent")
                    .filter(|n| (1..=8).contains(n))
                    .map_or(defaults.indent, |n| n as usize),
                width: get("width")
                    .filter(|n| (40..=400).contains(n))
                    .map_or(defaults.width, |n| n as usize),
            };
        }
        dir = d.parent();
    }
    selu_fmt::Options::default()
}

/// `selu fmt`: format files in place, or check them with `--check`.
fn fmt(paths: &[PathBuf], check: bool) -> Result<(), ExitCode> {
    let mut files = Vec::new();
    if paths.is_empty() {
        fmt_files(Path::new("."), &mut files);
    }
    for p in paths {
        if !p.exists() {
            eprintln!("selu fmt: {} does not exist", p.display());
            return Err(ExitCode::FAILURE);
        }
        fmt_files(p, &mut files);
    }
    let (mut changed, mut failed) = (0, 0);
    for file in &files {
        let text = match std::fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("selu fmt: cannot read {}: {e}", file.display());
                failed += 1;
                continue;
            }
        };
        match selu_fmt::format(&text, &fmt_options(file)) {
            Ok(out) if out == text => {}
            Ok(out) => {
                changed += 1;
                if check {
                    println!("not formatted: {}", file.display());
                } else if let Err(e) = std::fs::write(file, out) {
                    eprintln!("selu fmt: cannot write {}: {e}", file.display());
                    failed += 1;
                } else {
                    println!("formatted {}", file.display());
                }
            }
            Err(e) => {
                eprintln!("selu fmt: {}: {e}; left unchanged", file.display());
                failed += 1;
            }
        }
    }
    if failed > 0 || (check && changed > 0) {
        return Err(ExitCode::FAILURE);
    }
    if !check && changed == 0 {
        println!("{} file(s) already formatted", files.len());
    }
    Ok(())
}

fn not_yet(cmd: &str, what: &str) -> ExitCode {
    eprintln!("{cmd}: not implemented yet ({what} is planned; see DESIGN.md)");
    ExitCode::from(2)
}

/// `selu test` (docs/SIMULATOR.md decision 11). The runner lives in the
/// `selu-test` crate; until package S4 lands it discovers test files and
/// says that it can't run them yet.
#[allow(clippy::too_many_arguments)]
fn test(
    client_install: Option<PathBuf>,
    no_client_assets: bool,
    project_dir: &Path,
    target: Option<String>,
    filter: Option<String>,
    strict: bool,
    json: bool,
    manifest_dir: Option<PathBuf>,
) -> Result<(), ExitCode> {
    let options = selu_test::RunOptions {
        client_install,
        no_client_assets,
        manifest_dir,
        project_dir: project_dir.to_path_buf(),
        target,
        filter,
    };
    match selu_test::run(&options) {
        Ok(report) => {
            println!(
                "{}",
                if json {
                    report.to_json()
                } else {
                    report.to_human()
                }
            );
            match if strict {
                report.strict_exit_code()
            } else {
                report.exit_code()
            } {
                0 => Ok(()),
                code => Err(ExitCode::from(code)),
            }
        }
        Err(e) => {
            eprintln!("selu test: {e}");
            Err(ExitCode::from(2))
        }
    }
}

/// `selu debug` (docs/SIMULATOR.md decision 2): build, then start
/// `selu-studio` with the built addon folder. Studio is a separate binary so
/// the compiler doesn't carry GUI dependencies. A failed build still opens
/// the project: Studio shows the errors and loads the addon once it builds.
fn debug(
    project_dir: &Path,
    target: Option<&str>,
    manifest_dir: Option<&Path>,
) -> Result<(), ExitCode> {
    let addon = match build(project_dir, target, manifest_dir) {
        Ok(()) => Some(built_addon(project_dir, target)?),
        Err(_) => {
            eprintln!("selu debug: the build failed; opening the project in Studio anyway");
            None
        }
    };
    let studio = find_studio().ok_or_else(|| {
        eprintln!(
            "selu debug: selu-studio was not found next to selu or on the PATH; \
             build it with `cargo build --release -p selu-debug` and put it beside selu"
        );
        ExitCode::FAILURE
    })?;
    let mut cmd = std::process::Command::new(&studio);
    if let Some(addon) = &addon {
        cmd.arg("--manifest")
            .arg(selu_manifest::resolution::compiled_manifest_path(addon));
    }
    if let Some(dir) = manifest_dir {
        cmd.arg("--manifest-dir").arg(dir);
    }
    if let Some(t) = target {
        cmd.arg("--target").arg(t);
    }
    // Lets Studio rebuild on save with this same compiler.
    cmd.arg("--project").arg(project_dir);
    if let Ok(selu) = std::env::current_exe() {
        cmd.arg("--selu").arg(selu);
    }
    if let Some(addon) = &addon {
        cmd.arg(addon);
    }
    let status = cmd.status().map_err(|e| {
        eprintln!("selu debug: cannot start {}: {e}", studio.display());
        ExitCode::FAILURE
    })?;
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => Err(ExitCode::from(u8::try_from(code).unwrap_or(1))),
        None => Err(ExitCode::FAILURE),
    }
}

/// The built addon folder under `dist/`: the only one, or the one for the
/// `--target` flavour when the project builds several.
fn built_addon(project_dir: &Path, target: Option<&str>) -> Result<PathBuf, ExitCode> {
    let dist = project_dir.join("dist");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dist)
        .map_err(|e| {
            eprintln!("selu debug: cannot read {}: {e}", dist.display());
            ExitCode::FAILURE
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.') && p.join(format!("{n}.toc")).is_file())
        })
        .collect();
    found.sort();
    if let Some(flavour) = target.and_then(|t| t.split('@').next())
        && found.len() > 1
    {
        found.retain(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(&format!("-{flavour}")))
        });
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => {
            eprintln!("selu debug: no built addon folder under {}", dist.display());
            Err(ExitCode::FAILURE)
        }
        _ => {
            let names: Vec<_> = found
                .iter()
                .filter_map(|p| p.file_name()?.to_str())
                .collect();
            eprintln!(
                "selu debug: several built targets ({}); pick one with --target",
                names.join(", ")
            );
            Err(ExitCode::FAILURE)
        }
    }
}

/// `$SELU_STUDIO` if set (it must exist), else `selu-studio` beside `selu`,
/// else on the path.
fn find_studio() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SELU_STUDIO") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let name = format!("selu-studio{}", std::env::consts::EXE_SUFFIX);
    let beside = std::env::current_exe().ok()?.with_file_name(&name);
    if beside.is_file() {
        return Some(beside);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(&name))
        .find(|p| p.is_file())
}

/// Manifest discovery: `--manifest-dir`, `$SELU_MANIFEST_DIR`, then the
/// nearest `manifests/` folder walking up from the project and the cwd.
///
/// With `flavours`, only the folders those flavours need are read
/// (`<dir>/<flavour>/` plus the small `fixtures/`), instead of every
/// client's manifest (tens of MB of JSON). If a flavour has no folder of its
/// own, everything is read, so "unknown target" errors still list every
/// known flavour.
fn targets(manifest_dir: Option<&Path>) -> Result<(), ExitCode> {
    let manifests = find_manifests(manifest_dir, Path::new("."), None)?;
    for m in &manifests {
        println!(
            "{}  -  {} (interface {}{})",
            m.target_string(),
            selu_manifest::display_name(&m.flavour, &m.build),
            m.interface,
            if m.fixture { ", test fixture" } else { "" }
        );
    }
    Ok(())
}

fn update_targets(
    manifest_dir: Option<&Path>,
    products: &[String],
    yes: bool,
) -> Result<(), ExitCode> {
    let project = Path::new(".");
    let manifests = find_manifests(manifest_dir, project, None)?;
    let result = selu_manifest::lock::Lock::read(project)
        .and_then(|mut lock| lock.update(project, &manifests, products, yes));
    match result {
        Ok(report) => {
            print!("{report}");
            Ok(())
        }
        Err(e) => {
            eprintln!("selu: {e}");
            Err(ExitCode::FAILURE)
        }
    }
}

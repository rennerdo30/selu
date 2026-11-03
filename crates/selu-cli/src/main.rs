//! `selu` command line. `build` works end to end for the current Selu
//! subset; `test`, `debug` and `probe` are honest stubs for now.

use clap::{Parser, Subcommand};
use selu_diagnostics::Diagnostic;
use selu_manifest::{Manifest, TargetSpec};
use serde::Deserialize;
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
    /// Compile a Selu project to `dist/<Name>/`.
    Build {
        /// Project folder holding `selu.json` (default: current folder).
        path: Option<PathBuf>,
        /// Override the project targets, e.g. `retail@12.0` (comma-separated).
        #[arg(long)]
        target: Option<String>,
        /// Folder holding target manifests (default: search upwards for `manifests/`).
        #[arg(long)]
        manifest_dir: Option<PathBuf>,
    },
    /// Run test scenarios in the simulator (not implemented yet).
    Test {
        path: Option<PathBuf>,
        #[arg(long)]
        target: Option<String>,
    },
    /// Build, then start Selu Studio, the virtual WoW simulator (not implemented yet).
    Debug {
        path: Option<PathBuf>,
        #[arg(long)]
        target: Option<String>,
    },
    /// List every known flavour and build.
    Targets {
        #[arg(long)]
        manifest_dir: Option<PathBuf>,
    },
    /// Build a probe addon to verify a manifest in game (not implemented yet).
    Probe {
        #[arg(long)]
        target: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct Project {
    name: String,
    targets: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Build {
            path,
            target,
            manifest_dir,
        } => build(
            path.as_deref().unwrap_or(Path::new(".")),
            target.as_deref(),
            manifest_dir.as_deref(),
        ),
        Command::Targets { manifest_dir } => targets(manifest_dir.as_deref()),
        Command::Test { .. } => Err(not_yet("selu test", "headless test scenarios")),
        Command::Debug { .. } => Err(not_yet(
            "selu debug",
            "Selu Studio, the virtual WoW simulator app",
        )),
        Command::Probe { .. } => Err(not_yet("selu probe", "in-game manifest probing")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn not_yet(cmd: &str, what: &str) -> ExitCode {
    eprintln!("{cmd}: not implemented yet ({what} is planned; see DESIGN.md)");
    ExitCode::from(2)
}

fn fail(d: &Diagnostic, source: &str) -> ExitCode {
    eprint!("{}", d.render(source));
    ExitCode::FAILURE
}

/// Manifest discovery: `--manifest-dir`, `$SELU_MANIFEST_DIR`, then the
/// nearest `manifests/` folder walking up from the project and the cwd.
fn find_manifests(explicit: Option<&Path>, project_dir: &Path) -> Result<Vec<Manifest>, ExitCode> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = explicit {
        candidates.push(p.to_path_buf());
    }
    if let Ok(p) = std::env::var("SELU_MANIFEST_DIR") {
        candidates.push(PathBuf::from(p));
    }
    for base in [project_dir, Path::new(".")] {
        let mut dir: Option<PathBuf> = std::fs::canonicalize(base).ok();
        while let Some(d) = dir {
            candidates.push(d.join("manifests"));
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    for dir in candidates {
        if dir.is_dir() {
            match selu_manifest::load_manifest_dir(&dir) {
                Ok(m) if !m.is_empty() => return Ok(m),
                Ok(_) => continue,
                Err(d) => return Err(fail(&d, "")),
            }
        }
    }
    eprintln!(
        "selu: no target manifests found (looked for a `manifests/` folder; pass --manifest-dir)"
    );
    Err(ExitCode::FAILURE)
}

fn targets(manifest_dir: Option<&Path>) -> Result<(), ExitCode> {
    let manifests = find_manifests(manifest_dir, Path::new("."))?;
    for m in &manifests {
        println!(
            "{}  (interface {}{})",
            m.target_string(),
            m.interface,
            if m.fixture { ", test fixture" } else { "" }
        );
    }
    Ok(())
}

fn build(
    project_dir: &Path,
    target_override: Option<&str>,
    manifest_dir: Option<&Path>,
) -> Result<(), ExitCode> {
    let config_path = project_dir.join("selu.json");
    let config_text = std::fs::read_to_string(&config_path).map_err(|e| {
        eprintln!("selu: cannot read {}: {e}", config_path.display());
        ExitCode::FAILURE
    })?;
    let project: Project = serde_json::from_str(&config_text).map_err(|e| {
        eprintln!("selu: invalid {}: {e}", config_path.display());
        ExitCode::FAILURE
    })?;

    let mut sources: Vec<PathBuf> = Vec::new();
    let src_dir = project_dir.join("src");
    let scan_dir = if src_dir.is_dir() {
        src_dir
    } else {
        project_dir.to_path_buf()
    };
    if let Ok(entries) = std::fs::read_dir(&scan_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|x| x == "selu") {
                sources.push(p);
            }
        }
    }
    sources.sort();
    if sources.is_empty() {
        eprintln!("selu: no .selu files found in {}", scan_dir.display());
        return Err(ExitCode::FAILURE);
    }

    let target_strings: Vec<String> = match target_override {
        Some(t) => t
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        None => project.targets.clone(),
    };
    if target_strings.is_empty() {
        eprintln!("selu: no targets (set `targets` in selu.json or pass --target)");
        return Err(ExitCode::FAILURE);
    }
    let manifests = find_manifests(manifest_dir, project_dir)?;
    let multi = target_strings.len() > 1;

    for target_str in &target_strings {
        let spec = TargetSpec::parse(target_str).map_err(|d| fail(&d, ""))?;
        let manifest = selu_manifest::resolve(&spec, &manifests).map_err(|d| fail(&d, ""))?;
        println!("target {spec} -> {}", manifest.build);

        let out_dir = if multi {
            project_dir
                .join("dist")
                .join(format!("{}-{}", project.name, manifest.flavour))
        } else {
            project_dir.join("dist").join(&project.name)
        };
        std::fs::create_dir_all(&out_dir).map_err(|e| {
            eprintln!("selu: cannot create {}: {e}", out_dir.display());
            ExitCode::FAILURE
        })?;

        let mut lua_files: Vec<String> = Vec::new();
        for source_path in &sources {
            let source = std::fs::read_to_string(source_path).map_err(|e| {
                eprintln!("selu: cannot read {}: {e}", source_path.display());
                ExitCode::FAILURE
            })?;
            let display_name = source_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "main.selu".into());
            let program =
                selu_syntax::parse(&display_name, &source).map_err(|d| fail(&d, &source))?;
            let diags = selu_check::check(&program, manifest);
            if !diags.is_empty() {
                for d in &diags {
                    eprint!("{}", d.render(&source));
                }
                return Err(ExitCode::FAILURE);
            }
            let lowered = selu_lower::lower(&program, manifest);
            let stem = source_path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "main".into());
            let lua_name = format!("{stem}.lua");
            std::fs::write(out_dir.join(&lua_name), &lowered.lua).map_err(|e| {
                eprintln!("selu: cannot write {lua_name}: {e}");
                ExitCode::FAILURE
            })?;
            std::fs::write(
                out_dir.join(format!("{stem}.lua.map.json")),
                lowered.source_map.to_json(),
            )
            .map_err(|e| {
                eprintln!("selu: cannot write source map: {e}");
                ExitCode::FAILURE
            })?;
            lua_files.push(lua_name);
        }
        let toc = selu_lower::generate_toc(&project.name, manifest, &lua_files);
        std::fs::write(out_dir.join(format!("{}.toc", project.name)), toc).map_err(|e| {
            eprintln!("selu: cannot write .toc: {e}");
            ExitCode::FAILURE
        })?;
        println!("built {} -> {}", project.name, out_dir.display());
    }
    Ok(())
}

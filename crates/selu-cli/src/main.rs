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
        /// Project folder holding `selu.toml` (default: current folder).
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
    addon: AddonSection,
}

/// The `[addon]` table of `selu.toml`. Unknown keys are an error so a typo
/// never silently does nothing.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddonSection {
    name: String,
    #[serde(default)]
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
    let config_path = project_dir.join("selu.toml");
    let config_text = std::fs::read_to_string(&config_path).map_err(|e| {
        eprintln!("selu: cannot read {}: {e}", config_path.display());
        ExitCode::FAILURE
    })?;
    let project: Project = toml::from_str(&config_text).map_err(|e| {
        eprintln!(
            "selu: invalid {}:
{e}",
            config_path.display()
        );
        ExitCode::FAILURE
    })?;
    let project = project.addon;

    let src_dir = project_dir.join("src");
    let sources = project_sources(&src_dir)?;

    let target_strings: Vec<String> = match target_override {
        Some(t) => t
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        None => project.targets.clone(),
    };
    if target_strings.is_empty() {
        eprintln!("selu: no targets (set `targets` under [addon] in selu.toml or pass --target)");
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
        let map_dir = project_dir
            .join("dist")
            .join(".selu")
            .join(out_dir.file_name().unwrap_or_default());
        for dir in [&out_dir, &map_dir] {
            std::fs::create_dir_all(dir).map_err(|e| {
                eprintln!("selu: cannot create {}: {e}", dir.display());
                ExitCode::FAILURE
            })?;
        }

        let mut lua_files: Vec<String> = Vec::new();
        let mut exports = std::collections::BTreeMap::new();
        let mut compiled = Vec::new();
        for module in &sources {
            let mut imports = selu_check::ModuleBindings::default();
            let mut lower_imports = std::collections::BTreeMap::new();
            for (from, key) in &module.dependencies {
                let signatures: selu_check::ModuleExports =
                    exports.get(key).cloned().unwrap_or_default();
                imports.imports.insert(from.clone(), signatures.clone());
                lower_imports.insert(from.clone(), (key.clone(), signatures));
            }
            let (checked, signature) = selu_check::check_module(&module.program, manifest, imports);
            for d in &checked.diagnostics {
                eprint!("{}", d.render(&module.source));
            }
            if checked.has_errors() {
                return Err(ExitCode::FAILURE);
            }
            exports.insert(module.key.clone(), signature);
            let bindings =
                selu_lower::ModuleBindings::new(module.key.clone(), lower_imports, &module.program);
            let lowered =
                selu_lower::lower_module(&module.program, manifest, &checked.types, bindings);
            compiled.push((module, lowered));
        }
        for (module, lowered) in compiled {
            let lua_name = format!("{}.lua", module.key);
            let output = out_dir.join(&lua_name);
            let map = map_dir.join(format!("{lua_name}.map.json"));
            for path in [&output, &map] {
                std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| {
                    eprintln!("selu: {e}");
                    ExitCode::FAILURE
                })?;
            }
            std::fs::write(output, lowered.lua).map_err(|e| {
                eprintln!("selu: {e}");
                ExitCode::FAILURE
            })?;
            std::fs::write(map, lowered.source_map.to_json()).map_err(|e| {
                eprintln!("selu: {e}");
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

struct ModuleSource {
    key: String,
    source: String,
    program: selu_syntax::Program,
    dependencies: std::collections::BTreeMap<String, String>,
}
fn import_key(key: &str, from: &str) -> Option<String> {
    if !from.starts_with("./") && !from.starts_with("../") {
        return None;
    }
    if Path::new(from).extension().is_some() || from.contains('\\') {
        return None;
    }
    let joined = Path::new(key).parent()?.join(from);
    let mut parts = Vec::new();
    for part in joined.components() {
        match part {
            std::path::Component::Normal(n) => parts.push(n.to_string_lossy().into_owned()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            _ => return None,
        }
    }
    Some(parts.join("/"))
}
fn project_sources(src: &Path) -> Result<Vec<ModuleSource>, ExitCode> {
    use std::collections::{BTreeMap, BTreeSet};
    fn visit(
        key: &str,
        src: &Path,
        modules: &mut BTreeMap<String, ModuleSource>,
        stack: &mut Vec<String>,
    ) -> Result<(), ExitCode> {
        if modules.contains_key(key) {
            return Ok(());
        }
        let path = src.join(format!("{key}.selu"));
        let source = std::fs::read_to_string(&path).map_err(|e| {
            eprintln!("SELU0400: cannot read {}: {e}", path.display());
            ExitCode::FAILURE
        })?;
        let program =
            selu_syntax::parse(&format!("{key}.selu"), &source).map_err(|d| fail(&d, &source))?;
        stack.push(key.into());
        let mut dependencies = BTreeMap::new();
        for s in &program.stmts {
            if let selu_syntax::StmtKind::Import {
                from,
                from_span,
                default,
                ..
            } = &s.kind
            {
                if default.is_some() {
                    return Err(fail(
                        &Diagnostic::error("SELU0390", "asset imports are not supported yet")
                            .with_span(from_span.clone()),
                        &source,
                    ));
                }
                let dep = import_key(key, from).ok_or_else(|| {
                    fail(
                        &Diagnostic::error(
                            "SELU0400",
                            "module paths must be relative and omit the extension",
                        )
                        .with_span(from_span.clone())
                        .with_help("use a path such as ./other"),
                        &source,
                    )
                })?;
                if !src.join(format!("{dep}.selu")).is_file() {
                    return Err(fail(
                        &Diagnostic::error("SELU0400", format!("module `{from}` does not exist"))
                            .with_span(from_span.clone())
                            .with_help("check the relative module path"),
                        &source,
                    ));
                }
                if let Some(at) = stack.iter().position(|n| n == &dep) {
                    let mut cycle = stack[at..].to_vec();
                    cycle.push(dep.clone());
                    let message = format!("import cycle: {}", cycle.join(" -> "));
                    eprint!(
                        "{}",
                        Diagnostic::error("SELU0403", &message)
                            .with_span(from_span.clone())
                            .render(&source)
                    );
                    for pair in cycle.windows(2).take(cycle.len() - 2) {
                        let text = std::fs::read_to_string(src.join(format!("{}.selu", pair[0])))
                            .map_err(|_| ExitCode::FAILURE)?;
                        let p = selu_syntax::parse(&format!("{}.selu", pair[0]), &text)
                            .map_err(|d| fail(&d, &text))?;
                        for s in p.stmts {
                            if let selu_syntax::StmtKind::Import {
                                from, from_span, ..
                            } = s.kind
                                && import_key(&pair[0], &from).as_ref() == Some(&pair[1])
                            {
                                eprint!(
                                    "{}",
                                    Diagnostic::error("SELU0403", &message)
                                        .with_span(from_span)
                                        .render(&text)
                                );
                            }
                        }
                    }
                    return Err(ExitCode::FAILURE);
                }
                visit(&dep, src, modules, stack)?;
                dependencies.insert(from.clone(), dep);
            }
        }
        stack.pop();
        modules.insert(
            key.into(),
            ModuleSource {
                key: key.into(),
                source,
                program,
                dependencies,
            },
        );
        Ok(())
    }
    let mut modules = BTreeMap::new();
    visit("main", src, &mut modules, &mut Vec::new())?;
    fn scan(
        dir: &Path,
        src: &Path,
        modules: &BTreeMap<String, ModuleSource>,
    ) -> std::io::Result<()> {
        let mut entries = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                scan(&path, src, modules)?;
            } else if path.extension().is_some_and(|e| e == "selu") {
                let key = path
                    .strip_prefix(src)
                    .unwrap()
                    .with_extension("")
                    .to_string_lossy()
                    .replace('\\', "/");
                if !modules.contains_key(&key) {
                    eprintln!(
                        "{}",
                        Diagnostic::warning(
                            "SELU0404",
                            format!("{} is not reachable from src/main.selu", path.display())
                        )
                        .render("")
                    );
                }
            }
        }
        Ok(())
    }
    scan(src, src, &modules).map_err(|e| {
        eprintln!("selu: {e}");
        ExitCode::FAILURE
    })?;
    let mut done = BTreeSet::new();
    let mut ordered = Vec::new();
    while !modules.is_empty() {
        let key = modules
            .iter()
            .find(|(_, m)| m.dependencies.values().all(|d| done.contains(d)))
            .map(|(k, _)| k.clone())
            .expect("acyclic graph");
        done.insert(key.clone());
        ordered.push(modules.remove(&key).unwrap());
    }
    Ok(ordered)
}

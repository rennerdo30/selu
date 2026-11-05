use selu_diagnostics::Diagnostic;
use selu_manifest::{Manifest, TargetSpec};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::ExitCode;
#[derive(Debug, Deserialize)]
struct Project {
    addon: AddonSection,
    #[serde(default)]
    build: BuildSection,
    /// `[format]`: read by `selu fmt` and Studio (validated here so a typo
    /// is an error rather than ignored).
    #[serde(default)]
    #[allow(dead_code)]
    format: FormatSection,
}

/// The `[format]` table of `selu.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormatSection {
    /// Spaces per indentation level (default 2).
    #[serde(default)]
    #[allow(dead_code)]
    indent: Option<usize>,
    /// Line width before lists are wrapped (default 100).
    #[serde(default)]
    #[allow(dead_code)]
    width: Option<usize>,
    /// Studio formats files when it saves them.
    #[serde(default)]
    #[allow(dead_code)]
    on_save: bool,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum BuildLayout {
    #[default]
    PerTarget,
    Shared,
}

/// The `[addon]` table of `selu.toml`. Unknown keys are an error so a typo
/// never silently does nothing.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildSection {
    #[serde(default)]
    strict_api: bool,
    #[serde(default)]
    debug: bool,
    #[serde(default)]
    layout: BuildLayout,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddonSection {
    name: String,
    #[serde(default)]
    targets: Vec<String>,
}

fn fail(d: &Diagnostic, source: &str) -> ExitCode {
    eprint!("{}", d.render(source));
    ExitCode::FAILURE
}

/// Manifest discovery: `--manifest-dir`, `$SELU_MANIFEST_DIR`, then the
/// nearest `manifests/` folder walking up from the project and the cwd.
///
/// With `flavours`, only the folders those flavours need are read
/// (`<dir>/<flavour>/` plus the small `fixtures/`), instead of every
/// client's manifest (tens of MB of JSON). If a flavour has no folder of its
/// own, everything is read, so "unknown target" errors still list every
/// known flavour.
pub fn find_manifests(
    explicit: Option<&Path>,
    project_dir: &Path,
    flavours: Option<&[String]>,
) -> Result<Vec<Manifest>, ExitCode> {
    selu_manifest::resolution::discover(explicit, project_dir, flavours).map_err(|d| fail(&d, ""))
}

pub fn build(
    project_dir: &Path,
    target_override: Option<&str>,
    manifest_dir: Option<&Path>,
) -> Result<(), ExitCode> {
    build_impl(project_dir, target_override, manifest_dir, false, &[])
}
pub fn build_tests(
    project_dir: &Path,
    target_override: Option<&str>,
    roots: &[String],
    manifest_dir: Option<&Path>,
) -> Result<(), ExitCode> {
    build_impl(project_dir, target_override, manifest_dir, true, roots)
}
fn build_impl(
    project_dir: &Path,
    target_override: Option<&str>,
    manifest_dir: Option<&Path>,
    quiet: bool,
    roots: &[String],
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
    let check_options = selu_check::CheckOptions {
        strict_api: project.build.strict_api,
    };
    let shared = project.build.layout == BuildLayout::Shared;
    let debug = project.build.debug;
    let project = project.addon;

    let src_dir = project_dir.join("src");
    let sources = project_sources_roots(&src_dir, roots)?;
    validate_saved_names(&sources, &project.name)?;

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
    let mut lock = selu_manifest::lock::Lock::read(project_dir).map_err(|e| {
        eprintln!("selu: {e}");
        ExitCode::FAILURE
    })?;
    let discovery = lock.discovery_targets(&target_strings).map_err(|e| {
        eprintln!("selu: {e}");
        ExitCode::FAILURE
    })?;
    let manifests = find_manifests(manifest_dir, project_dir, Some(&discovery))?;
    let selected = lock.resolve(&target_strings, &manifests).map_err(|e| {
        eprintln!("selu: {e}");
        ExitCode::FAILURE
    })?;
    lock.write(project_dir).map_err(|e| {
        eprintln!("selu: {e}");
        ExitCode::FAILURE
    })?;
    if shared
        && selected.iter().any(|m| {
            m.lua_dialects
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                != selected[0]
                    .lua_dialects
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
        })
    {
        eprintln!("SELU0503: shared layout requires the same Lua dialect on every target");
        return Err(ExitCode::FAILURE);
    }
    let multi = target_strings.len() > 1;

    let mut saved_shapes = std::collections::BTreeMap::new();
    let mut saved_globals = Vec::new();
    let old_saved: SavedFile = match std::fs::read_to_string(project_dir.join("selu.saved.toml")) {
        Ok(text) => toml::from_str(&text).map_err(|e| {
            eprintln!("SELU0466: invalid selu.saved.toml: {e}");
            ExitCode::FAILURE
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SavedFile::default(),
        Err(e) => {
            eprintln!("SELU0466: cannot read selu.saved.toml: {e}");
            return Err(ExitCode::FAILURE);
        }
    };
    let mut snapshots = std::collections::BTreeMap::<std::path::PathBuf, Vec<&Manifest>>::new();
    for (target_str, manifest) in target_strings.iter().zip(&selected) {
        let spec = TargetSpec::parse(target_str).map_err(|d| fail(&d, ""))?;

        if !quiet {
            println!("target {spec} -> {}", manifest.build);
        }

        let out_dir = if multi && !shared {
            project_dir.join("dist").join(format!(
                "{}-{}",
                project.name,
                selu_manifest::product(&manifest.flavour)
            ))
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
        let addon_errors = sources.iter().any(|m| selu_lower::uses_errors(&m.program));
        for module in &sources {
            let mut imports = selu_check::ModuleBindings::default();
            let mut lower_imports = std::collections::BTreeMap::new();
            for (from, key) in &module.dependencies {
                let signatures: selu_check::ModuleExports =
                    exports.get(key).cloned().unwrap_or_default();
                imports.imports.insert(from.clone(), signatures.clone());
                lower_imports.insert(from.clone(), (key.clone(), signatures));
            }
            let (checked, signature) = selu_check::check_module_targets_with_options(
                &module.program,
                manifest,
                imports,
                &selected,
                check_options,
            );
            for d in &checked.diagnostics {
                let source = d
                    .span
                    .as_ref()
                    .and_then(|span| {
                        sources
                            .iter()
                            .find(|m| format!("{}.selu", m.key) == span.file)
                    })
                    .unwrap_or(module);
                eprint!("{}", d.render(&source.source));
            }
            if checked.has_errors() {
                return Err(ExitCode::FAILURE);
            }
            collect_saved(
                module,
                &checked.types,
                &project.name,
                &mut saved_shapes,
                &mut saved_globals,
            )?;
            exports.insert(module.key.clone(), signature);
            let mut bindings =
                selu_lower::ModuleBindings::new(module.key.clone(), lower_imports, &module.program);
            bindings.addon_name = project.name.clone();
            bindings.addon_errors = addon_errors;
            let lowered = selu_lower::lower_module_layout(
                &module.program,
                &selu_check::targets::with_fallbacks(manifest, &selected),
                &checked.types,
                bindings,
                shared,
                manifest,
            );
            compiled.push((module, lowered));
        }
        validate_saved(&old_saved, &saved_shapes, &sources, &project.name)?;
        let keybindings: Vec<_> = compiled
            .iter()
            .flat_map(|(_, l)| l.bindings.iter().cloned())
            .collect();
        let mut binding_names = std::collections::HashSet::new();
        for b in &keybindings {
            if !b.name.starts_with(&format!("{}_", project.name))
                || b.name.len() == project.name.len() + 1
            {
                let d = Diagnostic::error(
                    "SELU0662",
                    format!(
                        "Binding {} must start with {}_ and an action name.",
                        b.name, project.name
                    ),
                )
                .with_span(b.span.clone())
                .with_help("use the addon name followed by an underscore and the action");
                let source = compiled
                    .iter()
                    .find(|(m, _)| {
                        m.program
                            .stmts
                            .first()
                            .is_some_and(|s| s.span.file == b.span.file)
                    })
                    .map(|(m, _)| m.source.as_str())
                    .unwrap_or("");
                return Err(fail(&d, source));
            }
            if !binding_names.insert(&b.name) {
                let d = Diagnostic::error(
                    "SELU0666",
                    format!("Binding {} is declared more than once.", b.name),
                )
                .with_span(b.span.clone())
                .with_help("give each binding a unique name across the addon");
                let source = compiled
                    .iter()
                    .find(|(m, _)| {
                        m.program
                            .stmts
                            .first()
                            .is_some_and(|s| s.span.file == b.span.file)
                    })
                    .map(|(m, _)| m.source.as_str())
                    .unwrap_or("");
                return Err(fail(&d, source));
            }
        }
        if !keybindings.is_empty() {
            std::fs::write(
                out_dir.join("Bindings.xml"),
                selu_lower::bindings::xml(&keybindings),
            )
            .map_err(|e| {
                eprintln!("selu: cannot write Bindings.xml: {e}");
                ExitCode::FAILURE
            })?;
        } else if out_dir.join("Bindings.xml").exists() {
            std::fs::remove_file(out_dir.join("Bindings.xml")).map_err(|_| ExitCode::FAILURE)?;
        }
        let mut lowered_files: Vec<_> = compiled.iter().map(|(_, l)| l.clone()).collect();
        if let Some(mut runtime) = selu_lower::runtime::share(&mut lowered_files) {
            if debug {
                runtime = runtime.replace(
                    "if false then print(\"Selu: deferred protected call\") end",
                    "if true then print(\"Selu: deferred protected call\") end",
                );
            }
            // __selu module paths are reserved, so the fallback cannot collide.
            let runtime_name = if sources.iter().any(|m| m.key.eq_ignore_ascii_case("Selu")) {
                "__selu_runtime.lua"
            } else {
                "Selu.lua"
            };
            std::fs::write(out_dir.join(runtime_name), runtime).map_err(|e| {
                eprintln!("selu: cannot write runtime: {e}");
                ExitCode::FAILURE
            })?;
            lua_files.push(runtime_name.to_string());
        }
        for ((module, _), lowered) in compiled.into_iter().zip(lowered_files) {
            let lua_name = format!("{}.lua", module.key);
            let output = out_dir.join(&lua_name);
            let map = map_dir.join(format!("{lua_name}.map"));
            for path in [&output, &map] {
                std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| {
                    eprintln!("selu: {e}");
                    ExitCode::FAILURE
                })?;
            }
            let source_map = lowered.source_map(
                &lua_name,
                &format!("src/{}.selu", module.key),
                &module.source,
            );
            // The debug toggle keeps line counts, so the map stays valid.
            std::fs::write(
                output,
                if debug {
                    lowered.lua.replace(
                        "if false then print(\"Selu: deferred protected call\") end",
                        "if true then print(\"Selu: deferred protected call\") end",
                    )
                } else {
                    lowered.lua
                },
            )
            .map_err(|e| {
                eprintln!("selu: {e}");
                ExitCode::FAILURE
            })?;
            std::fs::write(map, source_map.to_json()).map_err(|e| {
                eprintln!("selu: {e}");
                ExitCode::FAILURE
            })?;
            lua_files.push(lua_name);
        }
        let mut toc = if shared {
            selu_lower::targets::generate_shared_toc(&project.name, &selected, &lua_files)
        } else {
            selu_lower::generate_toc(&project.name, manifest, &lua_files)
        };
        for (character, label) in [
            (false, "SavedVariables"),
            (true, "SavedVariablesPerCharacter"),
        ] {
            let names: std::collections::BTreeSet<_> = saved_globals
                .iter()
                .filter(|(_, c)| *c == character)
                .map(|(n, _)| n.as_str())
                .collect();
            if !names.is_empty() {
                toc = format!(
                    "## {label}: {}\n{toc}",
                    names.into_iter().collect::<Vec<_>>().join(", ")
                );
            }
        }
        std::fs::write(out_dir.join(format!("{}.toc", project.name)), toc).map_err(|e| {
            eprintln!("selu: cannot write .toc: {e}");
            ExitCode::FAILURE
        })?;
        snapshots
            .entry(map_dir.join("manifest.selum"))
            .or_default()
            .push(manifest);
        if !quiet {
            println!("built {} -> {}", project.name, out_dir.display());
        }
    }
    for (path, manifests) in snapshots {
        #[derive(Serialize)]
        struct Targets<'a> {
            manifests: &'a [&'a Manifest],
        }
        let bytes = if manifests.len() == 1 {
            selu_manifest::format::encode(manifests[0])
        } else {
            selu_manifest::format::encode(&Targets {
                manifests: &manifests,
            })
        }
        .expect("checked manifests serialize");
        std::fs::write(path, bytes).map_err(|e| {
            eprintln!("selu: cannot write checked snapshots: {e}");
            ExitCode::FAILURE
        })?;
    }
    // Only projects with saved data get the file; one that already has it keeps
    // it up to date, so its recorded history is never dropped silently.
    let saved_path = project_dir.join("selu.saved.toml");
    if saved_shapes.is_empty() && !saved_path.exists() {
        return Ok(());
    }
    let text = toml::to_string_pretty(&SavedFile {
        saved: saved_shapes,
    })
    .expect("saved schema serializes");
    std::fs::write(&saved_path, text).map_err(|e| {
        eprintln!("SELU0466: cannot write saved shapes: {e}");
        ExitCode::FAILURE
    })?;
    Ok(())
}

pub struct ModuleSource {
    pub key: String,
    pub source: String,
    pub program: selu_syntax::Program,
    pub dependencies: std::collections::BTreeMap<String, String>,
}
pub fn import_key(key: &str, from: &str) -> Option<String> {
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
// Resolve each actual directory entry even on case-insensitive hosts.
fn actual_module_key(src: &Path, key: &str) -> Option<String> {
    let wanted = format!("{key}.selu");
    let mut dir = src.to_path_buf();
    let mut actual = Vec::new();
    for component in wanted.split('/') {
        let mut entries = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        entries.sort_by_key(|e| e.file_name());
        let entry = entries
            .iter()
            .find(|e| e.file_name() == component)
            .or_else(|| {
                entries.iter().find(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .to_lowercase()
                        .eq(&component.to_lowercase())
                })
            })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        dir.push(&name);
        actual.push(name);
    }
    if !dir.is_file() {
        return None;
    }
    Some(actual.join("/").strip_suffix(".selu")?.to_string())
}
pub fn project_sources(src: &Path) -> Result<Vec<ModuleSource>, ExitCode> {
    project_sources_roots(src, &[])
}
pub fn project_sources_roots(src: &Path, roots: &[String]) -> Result<Vec<ModuleSource>, ExitCode> {
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
                if let Some(diagnostic) = selu_check::reserved_module_path(from, from_span) {
                    return Err(fail(&diagnostic, &source));
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
                if let Some(actual) = actual_module_key(src, &dep)
                    && actual != dep
                {
                    return Err(fail(
                        &Diagnostic::error(
                            "SELU0405",
                            format!(
                                "module path case does not match the real file `{actual}.selu`"
                            ),
                        )
                        .with_span(from_span.clone())
                        .with_fix(format!("match the path case to `{actual}.selu`")),
                        &source,
                    ));
                }
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
    for root in roots {
        visit(root, src, &mut modules, &mut Vec::new())?;
    }
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

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedFile {
    #[serde(default)]
    saved: std::collections::BTreeMap<String, SavedShape>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedShape {
    version: u32,
    per_character: bool,
    fields: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    legacy: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    readers: std::collections::BTreeMap<String, String>,
}
fn saved_inner(s: &selu_syntax::Stmt) -> &selu_syntax::Stmt {
    if let selu_syntax::StmtKind::Export(inner) = &s.kind {
        inner
    } else {
        s
    }
}
fn collect_saved(
    module: &ModuleSource,
    types: &selu_check::TypeTable,
    addon: &str,
    shapes: &mut std::collections::BTreeMap<String, SavedShape>,
    globals: &mut Vec<(String, bool)>,
) -> Result<(), ExitCode> {
    // Descriptor spelling is a persistence contract. Resolve named shapes while
    // preserving the exact existing spelling for every inline type.
    fn descriptor(ty: &selu_check::Type, active: &mut Vec<selu_check::Type>) -> String {
        use selu_check::Type;
        let ty = selu_check::shape_of(ty);
        if let Some(i) = active.iter().position(|t| t == &ty) {
            return format!("recursive({i})");
        }
        active.push(ty.clone());
        let result = match &ty {
            Type::Optional(t) => format!("{}?", descriptor(t, active)),
            Type::Union(ts) => ts
                .iter()
                .map(|t| descriptor(t, active))
                .collect::<Vec<_>>()
                .join(" | "),
            Type::SavedRepair(t) => format!("SavedRepair<{}>", descriptor(t, active)),
            Type::Array(t) => format!("{}[]", descriptor(t, active)),
            Type::Map(k, v) => format!("Map<{}, {}>", descriptor(k, active), descriptor(v, active)),
            Type::WeakMap(k, v) => format!(
                "WeakMap<{}, {}>",
                descriptor(k, active),
                descriptor(v, active)
            ),
            Type::Set(t) => format!("Set<{}>", descriptor(t, active)),
            Type::Tuple(ts) => format!(
                "[{}]",
                ts.iter()
                    .map(|t| descriptor(t, active))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Type::Object(fs) => format!(
                "{{ {} }}",
                fs.iter()
                    .map(|(n, t)| format!("{n}: {}", descriptor(t, active)))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            t => t.to_string(),
        };
        active.pop();
        result
    }
    fn fields(
        ty: &selu_check::Type,
        path: String,
        out: &mut std::collections::BTreeMap<String, String>,
        active: &mut Vec<selu_check::Type>,
    ) {
        let ty = selu_check::shape_of(ty);
        if let Some(i) = active.iter().position(|t| t == &ty) {
            out.insert(path, format!("recursive({i})"));
            return;
        }
        if let selu_check::Type::Object(fs) = &ty {
            active.push(ty.clone());
            out.insert(path.clone(), "object".into());
            for (key, ty) in fs {
                fields(ty, format!("{path}/{}:{key}", key.len()), out, active);
            }
            active.pop();
        } else {
            out.insert(path, descriptor(&ty, active));
        }
    }
    for s in &module.program.stmts {
        if let selu_syntax::StmtKind::Saved {
            name,
            version,
            per_character,
            adopted,
            legacy,
            version_key,
            defaults,
            ..
        } = &saved_inner(s).kind
        {
            let mut fs = std::collections::BTreeMap::new();
            fields(
                types
                    .saved_schemas
                    .get(&saved_inner(s).span.start)
                    .unwrap_or_else(|| types.get(defaults.id).expect("checked defaults")),
                String::new(),
                &mut fs,
                &mut Vec::new(),
            );
            let global = selu_lower::saved::declaration_global(addon, name, *adopted);
            globals.push((global.clone(), *per_character));
            if let Some(legacy) = legacy {
                globals.push((legacy.clone(), *per_character));
            }
            shapes.insert(
                global,
                SavedShape {
                    version: *version,
                    per_character: *per_character,
                    fields: fs,
                    version_key: version_key.clone(),
                    legacy: legacy.clone(),
                    readers: module
                        .program
                        .stmts
                        .iter()
                        .filter_map(|s| {
                            if let selu_syntax::StmtKind::Migrate { name: n, from, .. } = &s.kind
                                && n == name
                                && version_key.is_some()
                            {
                                return types.saved_readers.get(&s.span.start).map(|t| {
                                    (
                                        from.to_string(),
                                        format!(
                                            "{};context={}",
                                            descriptor(t, &mut vec![]),
                                            types
                                                .saved_contexts
                                                .get(&s.span.start)
                                                .map(|t| descriptor(t, &mut vec![]))
                                                .unwrap_or_default()
                                        ),
                                    )
                                });
                            }
                            None
                        })
                        .collect(),
                },
            );
        }
    }
    Ok(())
}
fn validate_saved(
    old: &SavedFile,
    new: &std::collections::BTreeMap<String, SavedShape>,
    modules: &[ModuleSource],
    addon: &str,
) -> Result<(), ExitCode> {
    let mut migrations: std::collections::BTreeMap<String, Vec<(u32, u32)>> =
        std::collections::BTreeMap::new();
    for module in modules {
        for s in &module.program.stmts {
            if let selu_syntax::StmtKind::Migrate { name, from, to, .. } = &s.kind {
                migrations
                    .entry(
                        module
                            .program
                            .stmts
                            .iter()
                            .find_map(|s| {
                                if let selu_syntax::StmtKind::Saved {
                                    name: n, adopted, ..
                                } = &saved_inner(s).kind
                                {
                                    (n == name).then(|| {
                                        selu_lower::saved::declaration_global(addon, name, *adopted)
                                    })
                                } else {
                                    None
                                }
                            })
                            .expect("checked migration"),
                    )
                    .or_default()
                    .push((*from, *to));
            }
        }
    }
    for (key, shape) in new {
        let mut chain = migrations.remove(key).unwrap_or_default();
        chain.sort();
        if chain.len() as u64 != u64::from(shape.version - 1)
            || chain.iter().enumerate().any(|(i, (from, to))| {
                u64::from(*from) != i as u64 + 1 || u64::from(*to) != i as u64 + 2
            })
        {
            saved_diagnostic(
                key,
                "SELU0471",
                format!(
                    "Saved data {key} {key} needs one migration for every step from version 1 to {}; old fields: {:?}; new fields: {:?}; write migrate <name> <from> -> <to> (old) {{ ... }}",
                    shape.version,
                    old.saved.get(key).map(|s| &s.fields),
                    shape.fields
                ),
                modules,
                addon,
            );
            return Err(ExitCode::FAILURE);
        }
    }
    for (key, before) in &old.saved {
        let Some(after) = new.get(key) else {
            eprintln!(
                "SELU0467: saved declaration {key} was removed; preserve it and migrate its data"
            );
            return Err(ExitCode::FAILURE);
        };
        if before.version_key != after.version_key
            || before.legacy != after.legacy
            || before
                .readers
                .iter()
                .any(|(v, t)| after.readers.get(v) != Some(t))
        {
            saved_diagnostic(
                key,
                "SELU0902",
                "saved adoption contract or historical schema changed".into(),
                modules,
                addon,
            );
            return Err(ExitCode::FAILURE);
        }
        let breaking = before.per_character != after.per_character
            || before
                .fields
                .iter()
                .any(|(k, v)| after.fields.get(k) != Some(v));

        if before.per_character != after.per_character
            || after.version < before.version
            || (breaking && after.version == before.version)
        {
            saved_diagnostic(
                key,
                "SELU0468",
                format!(
                    "Saved shape {key} needs a checked migration; old fields: {:?}; new fields: {:?}; write migrate <name> {} -> {} (old) {{ ... }} and raise the version",
                    before.fields,
                    after.fields,
                    before.version,
                    before.version.saturating_add(1)
                ),
                modules,
                addon,
            );
            return Err(ExitCode::FAILURE);
        }
    }
    Ok(())
}

fn saved_diagnostic(key: &str, code: &str, message: String, modules: &[ModuleSource], addon: &str) {
    for module in modules {
        for stmt in &module.program.stmts {
            let stmt = saved_inner(stmt);
            if let selu_syntax::StmtKind::Saved {
                name,
                name_span,
                adopted,
                ..
            } = &stmt.kind
                && selu_lower::saved::declaration_global(addon, name, *adopted) == key
            {
                eprint!(
                    "{}",
                    Diagnostic::error(code, message.replace("<name>", name))
                        .with_span(name_span.clone())
                        .with_help("preserve historical migrations and commit selu.saved.toml")
                        .render(&module.source)
                );
                return;
            }
        }
    }
}

fn validate_saved_names(modules: &[ModuleSource], addon: &str) -> Result<(), ExitCode> {
    let mut names = std::collections::BTreeMap::new();
    let mut globals = std::collections::BTreeMap::new();
    for module in modules {
        for stmt in &module.program.stmts {
            if let selu_syntax::StmtKind::Saved {
                name,
                name_span,
                adopted,
                ..
            } = &saved_inner(stmt).kind
            {
                let global = selu_lower::saved::declaration_global(addon, name, *adopted);
                let previous: Option<(&ModuleSource, &selu_diagnostics::Span)> =
                    names.get(name).or_else(|| globals.get(&global)).copied();
                if let Some((prior, span)) = previous {
                    for (source, span) in [(prior, span), (module, name_span)] {
                        eprint!(
                            "{}",
                            Diagnostic::error(
                                "SELU0472",
                                format!(
                                    "saved name or global `{name}` is declared twice in this addon"
                                )
                            )
                            .with_span(span.clone())
                            .with_help(
                                "choose distinct saved names and global names across all modules"
                            )
                            .render(&source.source)
                        );
                    }
                    return Err(ExitCode::FAILURE);
                }
                names.insert(name, (module, name_span));
                globals.insert(global, (module, name_span));
            }
        }
    }
    for module in modules {
        for stmt in &module.program.stmts {
            if let selu_syntax::StmtKind::Saved {
                legacy: Some(legacy),
                name_span,
                ..
            } = &saved_inner(stmt).kind
                && globals
                    .insert(legacy.clone(), (module, name_span))
                    .is_some()
            {
                eprintln!(
                    "SELU0900: legacy saved global {legacy} collides with another declaration"
                );
                return Err(ExitCode::FAILURE);
            }
        }
    }
    Ok(())
}

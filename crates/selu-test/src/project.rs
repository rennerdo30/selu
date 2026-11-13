use selu_diagnostics::Diagnostic;
use selu_manifest::{Manifest, TargetSpec};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::ExitCode;
#[derive(Debug, Deserialize)]
struct Project {
    addon: AddonSection,
    /// `[addons]`: other Selu addons this one imports, by name, as paths
    /// to their generated `.selu-api` files (docs/lang/addons.md).
    #[serde(default)]
    #[allow(dead_code)]
    addons: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    lint: LintSection,
    #[serde(default)]
    build: BuildSection,
    /// `[format]`: read by `selu fmt` and Studio (validated here so a typo
    /// is an error rather than ignored).
    #[serde(default)]
    #[allow(dead_code)]
    format: FormatSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LintSection {
    #[serde(default)]
    deny: Vec<DenyEntry>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DenyEntry {
    api: String,
    reason: String,
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
    strict_taint: bool,
    #[serde(default)]
    strict_api: bool,
    #[serde(default)]
    debug: bool,
    #[serde(default)]
    layout: BuildLayout,
    /// Extra files or folders copied into the built addon, relative to the
    /// project folder (for example `"../Media"`). Folders the code references
    /// as `Interface\AddOns\<Name>\<Folder>\...` are copied without this.
    #[serde(default)]
    assets: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddonSection {
    name: String,
    #[serde(default)]
    targets: Vec<String>,
    /// TOC metadata (docs/lang/targets.md): shown in the AddOns list and
    /// read by the addon (`C_AddOns.GetAddOnMetadata`).
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    author: Option<String>,
    /// `IconTexture`: a texture path the client resolves.
    #[serde(default)]
    icon: Option<String>,
}

impl AddonSection {
    /// The `[addon]` metadata as TOC lines; an error names a value with a
    /// line break (it would end the TOC field and start another).
    fn metadata(&self) -> Result<Vec<(&'static str, String)>, String> {
        let mut lines = Vec::new();
        for (field, label, value) in [
            ("title", "Title", &self.title),
            ("notes", "Notes", &self.notes),
            ("version", "Version", &self.version),
            ("author", "Author", &self.author),
            ("icon", "IconTexture", &self.icon),
        ] {
            if let Some(value) = value {
                if value.contains(['\n', '\r']) {
                    return Err(format!("[addon] {field} must be a single line"));
                }
                lines.push((label, value.clone()));
            }
        }
        Ok(lines)
    }
}

/// Applies `[addon]` metadata to a generated TOC: Title and Notes replace
/// the defaults, the other fields follow Notes.
fn apply_metadata(toc: String, metadata: &[(&'static str, String)]) -> String {
    let mut out = Vec::new();
    for line in toc.lines() {
        let replaced = metadata.iter().find(|(label, _)| {
            matches!(*label, "Title" | "Notes") && line.starts_with(&format!("## {label}:"))
        });
        match replaced {
            Some((label, value)) => out.push(format!("## {label}: {value}")),
            None => out.push(line.to_string()),
        }
        if line.starts_with("## Notes:") {
            for (label, value) in metadata {
                if !matches!(*label, "Title" | "Notes") {
                    out.push(format!("## {label}: {value}"));
                }
            }
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
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
    build_impl(
        project_dir,
        target_override,
        manifest_dir,
        false,
        &[],
        false,
    )
}
pub fn build_tests(
    project_dir: &Path,
    target_override: Option<&str>,
    roots: &[String],
    manifest_dir: Option<&Path>,
) -> Result<(), ExitCode> {
    build_impl(
        project_dir,
        target_override,
        manifest_dir,
        true,
        roots,
        false,
    )
}
/// Check source and saved schemas without lowering or writing build artifacts.
pub fn check_project(
    project: &Path,
    target: Option<&str>,
    manifests: Option<&Path>,
) -> Result<(), ExitCode> {
    build_impl(project, target, manifests, true, &[], true)
}
fn build_impl(
    project_dir: &Path,
    target_override: Option<&str>,
    manifest_dir: Option<&Path>,
    quiet: bool,
    roots: &[String],
    check_only: bool,
) -> Result<(), ExitCode> {
    let timing = std::env::var_os("SELU_TIMINGS").is_some();
    let started = std::time::Instant::now();
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
    let strict_taint = project.build.strict_taint;
    let deny: Vec<_> = project
        .lint
        .deny
        .into_iter()
        .map(|e| (e.api, e.reason))
        .collect();
    if deny
        .iter()
        .any(|(api, reason)| api.trim().is_empty() || reason.trim().is_empty())
    {
        eprintln!("selu: lint deny entries require an API and a written reason");
        return Err(ExitCode::FAILURE);
    }
    let metadata = project.addon.metadata().map_err(|e| {
        eprintln!("SELU0506: {e}");
        ExitCode::FAILURE
    })?;
    let shared = project.build.layout == BuildLayout::Shared;
    let debug = project.build.debug;
    let assets = project.build.assets;
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
    let mut selected = lock.resolve(&target_strings, &manifests).map_err(|e| {
        eprintln!("selu: {e}");
        ExitCode::FAILURE
    })?;
    // The installed client names Blizzard's global strings; they become
    // typed string globals for this build only.
    for manifest in &mut selected {
        if let Some(note) = crate::client_assets::add_global_strings(manifest)
            && !quiet
        {
            println!("{note}");
        }
    }
    check_package_products(&sources, &selected)?;
    if !check_only {
        lock.write(project_dir).map_err(|e| {
            eprintln!("selu: {e}");
            ExitCode::FAILURE
        })?;
    }
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

    if timing {
        eprintln!("timing setup {:.3}s", started.elapsed().as_secs_f64());
    }
    let mut check_time = std::time::Duration::ZERO;
    let mut lower_time = std::time::Duration::ZERO;
    let mut imports_time = std::time::Duration::ZERO;
    let mut saved_shapes = std::collections::BTreeMap::new();
    let mut saved_globals = Vec::new();
    // Other addons read through the boundary: optional dependencies, so
    // they load first when installed and are never required.
    let mut optional_deps = std::collections::BTreeSet::new();
    // `export addon { ... }`: what this addon publishes to other Selu addons.
    let mut selu_api: Option<(String, selu_check::foreign::SeluApi)> = None;
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
        for dir in [&out_dir, &map_dir].into_iter().filter(|_| !check_only) {
            std::fs::create_dir_all(dir).map_err(|e| {
                eprintln!("selu: cannot create {}: {e}", dir.display());
                ExitCode::FAILURE
            })?;
        }

        let prepared = selu_check::PreparedTarget::new(manifest, &selected);
        if timing {
            eprintln!("timing prepared target {spec}");
        }
        let mut lua_files: Vec<String> = Vec::new();
        let mut exports = std::collections::BTreeMap::new();
        let mut compiled = Vec::new();
        let addon_errors = sources.iter().any(|m| selu_lower::uses_errors(&m.program));
        for module in &sources {
            let phase = std::time::Instant::now();
            let mut imports = selu_check::ModuleBindings::default();
            let mut lower_imports = std::collections::BTreeMap::new();
            for (from, key) in &module.dependencies {
                let signatures: selu_check::ModuleExports =
                    exports.get(key).cloned().unwrap_or_default();
                imports.imports.insert(from.clone(), signatures.clone());
                lower_imports.insert(from.clone(), (key.clone(), signatures));
            }
            imports_time += phase.elapsed();
            let phase = std::time::Instant::now();
            let (mut checked, signature) =
                prepared.check_module(&module.program, imports, check_options);
            checked.diagnostics.retain(|d| {
                !matches!(
                    d.code.as_str(),
                    "SELU0604" | "SELU0605" | "SELU0606" | "SELU0607"
                )
            });
            checked.diagnostics.extend(selu_check::taint::check(
                &module.program,
                &checked.types,
                strict_taint,
                &deny,
            ));
            if timing {
                eprintln!(
                    "timing module {} check {:.3}s",
                    module.key,
                    phase.elapsed().as_secs_f64()
                );
            }
            check_time += phase.elapsed();
            let phase = std::time::Instant::now();
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
            optional_deps.extend(checked.types.optional_deps());
            if let Some(api) = checked.types.selu_api() {
                if let Some((other, _)) = &selu_api
                    && other != &module.key
                {
                    eprintln!(
                        "error[SELU0714]: an addon publishes one `export addon {{ ... }}` list, but {} and {} both have one",
                        other, module.key
                    );
                    return Err(ExitCode::FAILURE);
                }
                selu_api = Some((module.key.clone(), api.clone()));
            }
            collect_saved(
                module,
                &checked.types,
                &project.name,
                &mut saved_shapes,
                &mut saved_globals,
            )?;
            exports.insert(module.key.clone(), signature);
            if check_only {
                continue;
            }
            let mut bindings =
                selu_lower::ModuleBindings::new(module.key.clone(), lower_imports, &module.program);
            bindings.addon_name = project.name.clone();
            bindings.addon_errors = addon_errors;
            let lowered = selu_lower::lower_module_layout(
                &module.program,
                prepared.manifest(),
                &checked.types,
                bindings,
                shared,
                manifest,
            );
            compiled.push((module, lowered));
            if timing {
                eprintln!(
                    "timing module {} lower {:.3}s",
                    module.key,
                    phase.elapsed().as_secs_f64()
                );
            }
            lower_time += phase.elapsed();
        }
        if timing {
            eprintln!(
                "timing imports {:.3}s check {:.3}s lower {:.3}s elapsed {:.3}s",
                imports_time.as_secs_f64(),
                check_time.as_secs_f64(),
                lower_time.as_secs_f64(),
                started.elapsed().as_secs_f64()
            );
        }
        validate_saved(&old_saved, &saved_shapes, &sources, &project.name)?;
        if check_only {
            continue;
        }
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
        let mut referenced = std::collections::BTreeSet::new();
        for lowered in &lowered_files {
            asset_roots(&lowered.lua, &project.name, &mut referenced);
        }
        for ((module, _), lowered) in compiled.into_iter().zip(lowered_files) {
            let lua_name = format!("{}.lua", crate::packages::output_key(&module.key));
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
                &crate::packages::source_name(&module.key),
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
        let mut toc = apply_metadata(
            if shared {
                selu_lower::targets::generate_shared_toc(&project.name, &selected, &lua_files)
            } else {
                selu_lower::generate_toc(&project.name, manifest, &lua_files)
            },
            &metadata,
        );
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
        if !optional_deps.is_empty() {
            toc = format!(
                "## OptionalDeps: {}
{toc}",
                optional_deps.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        }
        std::fs::write(out_dir.join(format!("{}.toc", project.name)), toc).map_err(|e| {
            eprintln!("selu: cannot write .toc: {e}");
            ExitCode::FAILURE
        })?;
        // The published API, for other Selu addons' `[addons]` (types only;
        // nothing of this addon's code).
        if let Some((_, api)) = &selu_api
            && let Some(dist) = out_dir.parent()
        {
            let text = format!(
                "// Generated by `selu build` from {name}'s `export addon {{ ... }}`.
// Other Selu addons list this file under [addons] in their selu.toml.
addon \"{name}\" fingerprint \"{fp}\" {{
{body}}}
",
                name = project.name,
                fp = api.fingerprint,
                body = api.body,
            );
            std::fs::write(dist.join(format!("{}.selu-api", project.name)), text).map_err(|e| {
                eprintln!("selu: cannot write .selu-api: {e}");
                ExitCode::FAILURE
            })?;
        }
        copy_assets(
            project_dir,
            &out_dir,
            &project.name,
            &assets,
            &referenced,
            &lua_files,
        )?;
        snapshots
            .entry(map_dir.join("manifest.selum"))
            .or_default()
            .push(manifest);
        if !quiet {
            println!("built {} -> {}", project.name, out_dir.display());
        }
    }
    if check_only {
        return Ok(());
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

/// A package used on a product outside its `products` list (SELU0408),
/// reported at the first import of that package.
fn check_package_products(sources: &[ModuleSource], selected: &[Manifest]) -> Result<(), ExitCode> {
    let packages = crate::packages::packages();
    for module in sources {
        let Some(name) = crate::packages::package_of(&module.key) else {
            continue;
        };
        let Some(package) = packages.packages.get(name) else {
            continue;
        };
        for manifest in selected {
            let product = selu_manifest::product(&manifest.flavour);
            if package.products.iter().any(|p| p == product) {
                continue;
            }
            let entry = format!("{}{name}", crate::packages::PREFIX);
            let site = sources.iter().find_map(|m| {
                m.program.stmts.iter().find_map(|s| match &s.kind {
                    selu_syntax::StmtKind::Import {
                        from, from_span, ..
                    } if *from == entry => Some((m, from_span.clone())),
                    _ => None,
                })
            });
            let d = Diagnostic::error(
                "SELU0408",
                format!("package `{entry}` does not support the product `{product}`"),
            )
            .with_help(format!("it supports: {}", package.products.join(", ")));
            return Err(match site {
                Some((m, span)) => fail(&d.with_span(span), &m.source),
                None => fail(&d, ""),
            });
        }
    }
    Ok(())
}

pub struct ModuleSource {
    pub key: String,
    pub source: String,
    pub program: selu_syntax::Program,
    pub dependencies: std::collections::BTreeMap<String, String>,
}
/// The first folder (or file) below `Interface\AddOns\<addon>\` in each
/// path the code names, for example `Media` in
/// `"Interface\\AddOns\\Toolkit\\Media\\Logo"` (Lua source). Matching ignores case,
/// like the game; the spelling in the code is kept.
fn asset_roots(lua: &str, addon: &str, out: &mut std::collections::BTreeSet<String>) {
    let lower = lua.to_ascii_lowercase();
    let addon = addon.to_ascii_lowercase();
    for sep in ["\\\\", "\\", "/"] {
        let needle = format!("interface{sep}addons{sep}{addon}{sep}");
        let mut from = 0;
        while let Some(at) = lower[from..].find(&needle) {
            let start = from + at + needle.len();
            let rest = &lua[start..];
            let end = rest
                .find(['\\', '/', '"', '\'', ']', '\n'])
                .unwrap_or(rest.len());
            if end > 0 {
                out.insert(rest[..end].to_string());
            }
            from = start;
        }
    }
}

/// Copy the addon's media into the built addon: the folders its code
/// references (when the project has them) and `[build] assets`. A file is
/// copied again only when its size or time changed.
fn copy_assets(
    project_dir: &Path,
    out_dir: &Path,
    addon: &str,
    explicit: &[String],
    referenced: &std::collections::BTreeSet<String>,
    lua_files: &[String],
) -> Result<(), ExitCode> {
    let generated = |name: &str| {
        name.eq_ignore_ascii_case(&format!("{addon}.toc"))
            || name.eq_ignore_ascii_case("Bindings.xml")
            || lua_files.iter().any(|f| {
                f.split(['/', '\\'])
                    .next()
                    .is_some_and(|top| top.eq_ignore_ascii_case(name))
            })
    };
    let mut sources: Vec<std::path::PathBuf> = Vec::new();
    for entry in explicit {
        let path = project_dir.join(entry);
        if !path.exists() {
            eprintln!(
                "error[SELU0505]: [build] assets names {entry}, but {} does not exist",
                path.display()
            );
            return Err(ExitCode::FAILURE);
        }
        sources.push(path);
    }
    let names = |dir: &Path| -> Vec<String> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    };
    let in_project = names(project_dir);
    for root in referenced {
        if generated(root) {
            continue;
        }
        let provided = sources.iter().any(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(root))
        });
        if provided {
            continue;
        }
        match in_project.iter().find(|n| n.eq_ignore_ascii_case(root)) {
            Some(name) if !matches!(name.as_str(), "src" | "dist" | "selu.toml" | "selu.lock") => {
                sources.push(project_dir.join(name));
            }
            _ => eprintln!(
                "warning[SELU0504]: the code uses Interface\\AddOns\\{addon}\\{root}\\..., \
                 but the project has no `{root}`; add it to the project folder or to [build] assets"
            ),
        }
    }
    for source in sources {
        let Some(name) = source.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if generated(name) {
            eprintln!(
                "error[SELU0505]: asset `{name}` has the name of a file the build writes; rename it"
            );
            return Err(ExitCode::FAILURE);
        }
        copy_tree(&source, &out_dir.join(name)).map_err(|e| {
            eprintln!("selu: cannot copy {}: {e}", source.display());
            ExitCode::FAILURE
        })?;
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    let meta = std::fs::metadata(from)?;
    if meta.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        return Ok(());
    }
    let same = std::fs::metadata(to)
        .is_ok_and(|old| old.len() == meta.len() && old.modified().ok() == meta.modified().ok());
    if !same {
        std::fs::copy(from, to)?;
        if let Ok(time) = meta.modified() {
            let _ = std::fs::File::options()
                .write(true)
                .open(to)
                .and_then(|f| f.set_modified(time));
        }
    }
    Ok(())
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
/// The source of a module key: a package module, or a file below `src/`.
fn read_module(src: &Path, key: &str) -> Result<String, String> {
    if let Some(name) = key.strip_prefix(selu_syntax::ADDON_IMPORT_PREFIX) {
        // A Selu addon listed under [addons] in selu.toml: its generated
        // .selu-api file. Otherwise a declared other addon beside src/:
        // <project>/addons/<Name>.selu.
        let project = src.parent().unwrap_or(src);
        let listed = std::fs::read_to_string(project.join("selu.toml"))
            .ok()
            .and_then(|t| toml::from_str::<toml::Value>(&t).ok())
            .and_then(|v| {
                v.get("addons")?
                    .get(name)?
                    .as_str()
                    .map(|p| project.join(p))
            });
        let path = listed.unwrap_or_else(|| project.join("addons").join(format!("{name}.selu")));
        return std::fs::read_to_string(&path).map_err(|_| {
            format!(
                "SELU0710 the addon `{name}` is not declared: {} is missing (docs/lang/addons.md)",
                path.display()
            )
        });
    }
    if crate::packages::is_package_key(key) {
        return crate::packages::source(key)
            .map(str::to_string)
            .ok_or_else(|| format!("package module `{key}` does not exist"));
    }
    let path = src.join(format!("{key}.selu"));
    std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// Resolves an import written in module `key` to a module key: `@selu/<name>`
/// to the package entry point, a relative path within the project or within
/// the importing package (docs/lang/packages.md).
pub fn resolve_import(key: &str, from: &str) -> Result<String, Diagnostic> {
    if let Some(name) = from.strip_prefix(selu_syntax::ADDON_IMPORT_PREFIX) {
        let plain = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '));
        if !plain || name.contains("..") {
            return Err(Diagnostic::error(
                "SELU0710",
                format!("`{name}` is not an addon folder name"),
            ));
        }
        return Ok(from.to_string());
    }
    let packages = crate::packages::packages();
    if let Some(rest) = from.strip_prefix(crate::packages::PREFIX) {
        if let Some((name, path)) = rest.split_once('/') {
            return Err(Diagnostic::error(
                "SELU0407",
                format!("`{path}` is internal to package `{name}`"),
            )
            .with_help(format!("import the package by its name: \"@selu/{name}\"")));
        }
        if !packages.packages.contains_key(rest) {
            let known: Vec<_> = packages
                .packages
                .keys()
                .map(|n| format!("@selu/{n}"))
                .collect();
            let mut help = if known.is_empty() {
                "this compiler ships no packages".to_string()
            } else {
                format!("available packages: {}", known.join(", "))
            };
            if let Some(problem) = packages.errors.first() {
                help = format!("{help} ({problem})");
            }
            return Err(
                Diagnostic::error("SELU0407", format!("unknown package `{from}`")).with_help(help),
            );
        }
        return Ok(format!("{}{rest}/index", crate::packages::PREFIX));
    }
    if from.starts_with('@') {
        return Err(Diagnostic::error(
            "SELU0407",
            format!("`{from}` is not a Selu package"),
        )
        .with_help(
            "packages are imported as \"@selu/<name>\"; project modules with a relative path such as ./other",
        ));
    }
    let dep = import_key(key, from).ok_or_else(|| {
        Diagnostic::error(
            "SELU0400",
            "module paths must be relative and omit the extension",
        )
        .with_help("use a path such as ./other")
    })?;
    match crate::packages::package_of(key) {
        Some(name) => {
            if !dep.starts_with(&format!("{}{name}/", crate::packages::PREFIX)) {
                return Err(Diagnostic::error(
                    "SELU0400",
                    format!("module paths in package `{name}` cannot leave the package"),
                )
                .with_help("import another package by its name, such as \"@selu/<name>\""));
            }
        }
        None => {
            if dep.split('/').any(|part| part.starts_with('@')) {
                return Err(Diagnostic::error(
                    "SELU0318",
                    "project module path names starting with @ are reserved for packages",
                )
                .with_help("rename the file or directory so no path segment starts with @"));
            }
        }
    }
    Ok(dep)
}

/// Packages are libraries: the addon owns its saved data and globals.
fn package_rules(program: &selu_syntax::Program, source: &str) -> Result<(), ExitCode> {
    for statement in &program.stmts {
        let inner = match &statement.kind {
            selu_syntax::StmtKind::Export(inner) => inner.as_ref(),
            _ => statement,
        };
        let what = match inner.kind {
            selu_syntax::StmtKind::Saved { .. } => "saved data",
            selu_syntax::StmtKind::Global { .. } => "globals",
            _ => continue,
        };
        return Err(fail(
            &Diagnostic::error("SELU0409", format!("a package cannot declare {what}"))
                .with_span(statement.span.clone())
                .with_help(
                    "the addon that imports the package owns its saved data and globals; take the value from the caller",
                ),
            source,
        ));
    }
    Ok(())
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
        let source = read_module(src, key).map_err(|message| {
            eprintln!("SELU0400: {message}");
            ExitCode::FAILURE
        })?;
        let program =
            selu_syntax::parse(&format!("{key}.selu"), &source).map_err(|d| fail(&d, &source))?;
        if crate::packages::is_package_key(key) {
            package_rules(&program, &source)?;
        }
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
                let dep = match resolve_import(key, from) {
                    Ok(dep) => dep,
                    Err(d) => return Err(fail(&d.with_span(from_span.clone()), &source)),
                };
                if crate::packages::is_addon_key(&dep) {
                    // Read (and reported when missing) when it is visited.
                } else if crate::packages::is_package_key(&dep) {
                    if crate::packages::source(&dep).is_none() {
                        return Err(fail(
                            &Diagnostic::error(
                                "SELU0400",
                                format!("module `{from}` does not exist"),
                            )
                            .with_span(from_span.clone())
                            .with_help("check the relative module path"),
                            &source,
                        ));
                    }
                } else if let Some(actual) = actual_module_key(src, &dep)
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
                if !crate::packages::is_package_key(&dep)
                    && !crate::packages::is_addon_key(&dep)
                    && !src.join(format!("{dep}.selu")).is_file()
                {
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
                        let text = read_module(src, &pair[0]).map_err(|_| ExitCode::FAILURE)?;
                        let p = selu_syntax::parse(&format!("{}.selu", pair[0]), &text)
                            .map_err(|d| fail(&d, &text))?;
                        for s in p.stmts {
                            if let selu_syntax::StmtKind::Import {
                                from, from_span, ..
                            } = s.kind
                                && resolve_import(&pair[0], &from).ok().as_ref() == Some(&pair[1])
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
                            format!("src/{key}.selu is not reachable from src/main.selu")
                        )
                        .with_span(selu_diagnostics::Span {
                            file: format!("{key}.selu"),
                            start: 0,
                            end: 0
                        })
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
            Type::Union(ts) => format!(
                "({})",
                ts.iter()
                    .map(|t| descriptor(t, active))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
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

#[cfg(test)]
mod taint_config_tests {
    use super::*;
    #[test]
    fn parses_project_taint_policy() {
        let p: Project = toml::from_str(
            r#"
[addon]
name = "Test"
targets = ["wow_classic_era"]
[build]
strict_taint = true
[lint]
deny = [{ api = "Example.Unsafe", reason = "documented incident" }]
"#,
        )
        .unwrap();
        assert!(p.build.strict_taint);
        assert_eq!(p.lint.deny[0].api, "Example.Unsafe");
        assert_eq!(p.lint.deny[0].reason, "documented incident");
    }
}

//! Shared compiler and simulator manifest discovery and build metadata paths.
use crate::{Manifest, load_manifest_dir};
use selu_diagnostics::Diagnostic;
use std::path::{Path, PathBuf};
pub fn discover(
    explicit: Option<&Path>,
    project_dir: &Path,
    flavours: Option<&[String]>,
) -> Result<Vec<Manifest>, Diagnostic> {
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
            if dir.join("store").is_dir() {
                let cache = dir
                    .parent()
                    .unwrap_or(&dir)
                    .join("target/selu-manifest-cache");
                let mut manifests =
                    crate::store::load_cached_selected(&dir.join("store"), &cache, flavours)
                        .map_err(|e| Diagnostic::error("SELU0202", e.to_string()))?;
                if dir.join("fixtures").is_dir() {
                    manifests.extend(load_manifest_dir(&dir.join("fixtures"))?);
                }
                if !manifests.is_empty() {
                    return Ok(manifests);
                }
            }
            if let Some(flavours) = flavours
                && !flavours.is_empty()
                && flavours.iter().all(|f| dir.join(f).is_dir())
            {
                let mut out = Vec::new();
                let mut folders: Vec<PathBuf> = flavours.iter().map(|f| dir.join(f)).collect();
                folders.push(dir.join("fixtures"));
                folders.sort();
                folders.dedup();
                for folder in folders.iter().filter(|f| f.is_dir()) {
                    out.extend(load_manifest_dir(folder)?);
                }
                if !out.is_empty() {
                    out.sort_by(|a, b| a.flavour.cmp(&b.flavour).then(a.build.cmp(&b.build)));
                    return Ok(out);
                }
            }
            match load_manifest_dir(&dir) {
                Ok(m) if !m.is_empty() => return Ok(m),
                Ok(_) => continue,
                Err(d) => return Err(d),
            }
        }
    }
    Err(Diagnostic::error(
        "SELU0206",
        "no target manifests found (looked for a manifests folder; pass --manifest-dir)",
    ))
}
/// Compiler metadata stays outside the installed addon and its TOC.
pub fn compiled_manifest_path(addon: &Path) -> PathBuf {
    addon
        .parent()
        .unwrap_or(Path::new("."))
        .join(".selu")
        .join(addon.file_name().unwrap_or_default())
        .join("manifest.selum")
}
/// Select checked data without falling back to an unrelated target.
pub fn select_checked_manifest<'a>(
    manifests: &'a [Manifest],
    target: Option<&str>,
) -> Result<&'a Manifest, Diagnostic> {
    if let Some(target) = target {
        let selected =
            crate::TargetSpec::parse(target).and_then(|spec| crate::resolve(&spec, manifests));
        return selected.map_err(|_|Diagnostic::error("SELU0207",format!("compiled snapshot does not match requested target `{target}`; available targets: {}",manifests.iter().map(Manifest::target_string).collect::<Vec<_>>().join(", "))));
    }
    if manifests.len() == 1 {
        return Ok(&manifests[0]);
    }
    Err(Diagnostic::error(
        "SELU0207",
        "compiled snapshot contains multiple or no targets; select a target explicitly",
    ))
}
/// Read a legacy single-target snapshot or a bundle of per-target snapshots.
pub fn load_checked_manifest(path: &Path, target: Option<&str>) -> Result<Manifest, Diagnostic> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Snapshot {
        Targets { manifests: Vec<Manifest> },
        Single(Box<Manifest>),
    }
    let bytes = std::fs::read(path).map_err(|e| {
        Diagnostic::error(
            "SELU0202",
            format!("cannot read snapshot {}: {e}", path.display()),
        )
    })?;
    if bytes.starts_with(b"SELUSEG") || path.extension().is_some_and(|e| e == "selus") {
        return Err(Diagnostic::error(
            "SELU0202",
            "--manifest expects a checked SELUMAN snapshot, not a .selus history store; use --manifest-dir <manifests directory> with --target <product@build>",
        ));
    }
    let snapshot: Snapshot = crate::format::decode(&bytes).map_err(|e| {
        Diagnostic::error(
            "SELU0202",
            format!("invalid snapshot {}: {e}", path.display()),
        )
    })?;
    let manifests = match snapshot {
        Snapshot::Targets { manifests } => manifests,
        Snapshot::Single(m) => vec![*m],
    };
    select_checked_manifest(&manifests, target).cloned()
}
pub fn compiled_manifest_for_target(
    addon: &Path,
    target: Option<&str>,
) -> Result<Option<Manifest>, Diagnostic> {
    let mut path = compiled_manifest_path(addon);
    if !path.is_file() {
        path.set_extension("json");
    }
    if path.is_file() {
        load_checked_manifest(&path, target).map(Some)
    } else {
        Ok(None)
    }
}
pub fn compiled_manifest(addon: &Path) -> Result<Option<Manifest>, Diagnostic> {
    compiled_manifest_for_target(addon, None)
}

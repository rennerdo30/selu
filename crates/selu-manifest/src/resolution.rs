//! Shared compiler and simulator manifest discovery and build metadata paths.
use crate::{Manifest, load_manifest, load_manifest_dir};
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
        .join("manifest.json")
}
pub fn compiled_manifest(addon: &Path) -> Result<Option<Manifest>, Diagnostic> {
    let path = compiled_manifest_path(addon);
    if path.is_file() {
        load_manifest(&path).map(Some)
    } else {
        Ok(None)
    }
}

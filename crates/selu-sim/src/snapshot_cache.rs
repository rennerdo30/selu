//! Bounded cache for repeated independent sessions; no handler routing state.
use selu_manifest::Manifest;
use std::path::Path;
fn load(path: &Path, target: Option<&str>) -> Result<Manifest, String> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Snapshot {
        Targets { manifests: Vec<Manifest> },
        Single(Box<Manifest>),
    }
    let bytes = std::fs::read(path)
        .map_err(|e| format!("SELU0202: cannot read snapshot {}: {e}", path.display()))?;
    // One content-validated entry bounds retained memory and shares decoding
    // across isolated test sessions. Always read and compare exact bytes: path,
    // length and mtime cannot prove a snapshot has not changed.
    type Cached = Option<(Vec<u8>, std::sync::Arc<Vec<Manifest>>)>;
    static CACHE: std::sync::Mutex<Cached> = std::sync::Mutex::new(None);
    if let Some(manifests) = CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|(cached, _)| cached == &bytes)
        .map(|(_, m)| m.clone())
    {
        return selu_manifest::resolution::select_checked_manifest(&manifests, target)
            .cloned()
            .map_err(|e| e.to_string());
    }
    let snapshot: Snapshot = selu_manifest::format::decode(&bytes)
        .map_err(|e| format!("SELU0202: invalid snapshot {}: {e}", path.display()))?;
    let manifests = match snapshot {
        Snapshot::Targets { manifests } => manifests,
        Snapshot::Single(m) => vec![*m],
    };
    let selected = selu_manifest::resolution::select_checked_manifest(&manifests, target)
        .cloned()
        .map_err(|e| e.to_string());
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((bytes, std::sync::Arc::new(manifests)));
    selected
}
pub(crate) fn compiled_manifest_for_target(
    addon: &Path,
    target: Option<&str>,
) -> Result<Option<Manifest>, String> {
    let mut path = selu_manifest::resolution::compiled_manifest_path(addon);
    if !path.is_file() {
        path.set_extension("json");
    }
    if path.is_file() {
        load(&path, target).map(Some)
    } else {
        Ok(None)
    }
}
#[cfg(test)]
mod cache_tests {
    #[test]
    fn checked_snapshot_cache_checks_content_and_target_each_time() {
        let path =
            std::env::temp_dir().join(format!("selu-se-snapshot-{}.json", std::process::id()));
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json");
        let mut manifest = selu_manifest::load_manifest(&source).unwrap();
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let first = super::load(&path, None).unwrap();
        assert_eq!(first.functions.len(), manifest.functions.len());
        assert!(super::load(&path, Some("wow@99.0.0.1")).is_err());
        manifest.functions.clear();
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(super::load(&path, None).unwrap().functions.is_empty());
        std::fs::write(&path, b"invalid snapshot").unwrap();
        assert!(super::load(&path, None).is_err());
        std::fs::remove_file(path).unwrap();
    }
}

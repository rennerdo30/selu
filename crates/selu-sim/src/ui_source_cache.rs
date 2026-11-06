//! Build-scoped cache of source text, not VM state. Local fallback files are
//! revalidated; CASC text belongs to an immutable content-addressed build.
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
#[derive(Serialize, Deserialize)]
struct Entry {
    text: String,
    local_stamp: Option<(u64, u64)>,
}
#[derive(Default, Serialize, Deserialize)]
struct Cache {
    version: u32,
    identity: String,
    entries: HashMap<PathBuf, Entry>,
    #[serde(skip)]
    path: PathBuf,
    #[serde(skip)]
    dirty: bool,
}
thread_local! { static ACTIVE:RefCell<Option<Cache>>=const {RefCell::new(None)}; }
pub(crate) struct Scope(Option<Cache>);
fn stamp(path: &Path) -> Option<(u64, u64)> {
    let m = path.metadata().ok()?;
    Some((
        m.len(),
        m.modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos() as u64,
    ))
}
impl Scope {
    pub fn new(source: &Path, build: Option<&str>) -> Self {
        use std::hash::{Hash, Hasher};
        let identity = format!("{}:{}", source.display(), build.unwrap_or("checkout"));
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        identity.hash(&mut hash);
        let path = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Selu/ui-source")
            .join(format!("{:016x}.json", hash.finish()));
        Self::at(path, identity)
    }
    fn at(path: PathBuf, identity: String) -> Self {
        let _profile = crate::profile::Span::new("source.cache_open");
        let mut cache = if path.metadata().is_ok_and(|m| m.len() <= 64 * 1024 * 1024) {
            std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Cache>(&bytes).ok())
                .filter(|cache| cache.version == 1 && cache.identity == identity)
                .unwrap_or_default()
        } else {
            Cache::default()
        };
        cache.version = 1;
        cache.identity = identity;
        cache.path = path;
        Self(ACTIVE.with(|active| active.replace(Some(cache))))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        let _profile = crate::profile::Span::new("source.cache_save");
        let cache = ACTIVE.with(|active| active.replace(self.0.take()));
        if let Some(cache) = cache
            && cache.dirty
            && let Some(parent) = cache.path.parent()
            && std::fs::create_dir_all(parent).is_ok()
            && let Ok(bytes) = serde_json::to_vec(&cache)
        {
            // A truncated cache is discarded on the next read; source files remain authoritative.
            let _ = std::fs::write(cache.path, bytes);
        }
    }
}
pub(crate) fn get(path: &Path, local: bool) -> Option<String> {
    ACTIVE.with(|active| {
        let active = active.borrow();
        let cache = active.as_ref()?;
        let entry = cache.entries.get(path)?;
        if local {
            if entry.local_stamp != stamp(path) || entry.local_stamp.is_none() {
                return None;
            }
        } else if entry.local_stamp.is_some() {
            return None;
        }
        crate::profile::count("source.text_cache_hit");
        Some(entry.text.clone())
    })
}
pub(crate) fn put(path: &Path, text: &str, local: bool) {
    ACTIVE.with(|active| {
        if let Some(cache) = active.borrow_mut().as_mut() {
            cache.entries.insert(
                path.to_owned(),
                Entry {
                    text: text.to_owned(),
                    local_stamp: if local { stamp(path) } else { None },
                },
            );
            cache.dirty = true;
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_changes_and_build_changes_invalidate_text() {
        let root =
            std::env::temp_dir().join(format!("selu-ui-source-cache-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("ui.lua");
        let pack = root.join("pack.json");
        std::fs::write(&file, "one").unwrap();
        {
            let _scope = Scope::at(pack.clone(), "build-one".into());
            put(&file, "one", true);
            assert_eq!(get(&file, true).as_deref(), Some("one"));
            assert!(get(&file, false).is_none());
        }
        {
            let _scope = Scope::at(pack.clone(), "build-one".into());
            assert_eq!(get(&file, true).as_deref(), Some("one"));
            std::fs::write(&file, "changed").unwrap();
            assert!(get(&file, true).is_none());
        }
        {
            let _scope = Scope::at(pack.clone(), "build-two".into());
            assert!(get(&file, true).is_none());
        }
        for file in [&file, &pack] {
            std::fs::remove_file(file).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
}

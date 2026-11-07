//! Local decoded-text cache. Only unkeyed donor data enters this cache.
use selu_gamedata::BuildInfo;
use serde::{Deserialize, Serialize};
use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Serialize, Deserialize)]
struct Identity {
    version: u32,
    install: PathBuf,
    product: String,
    build: String,
    content_hash: Option<String>,
    encrypted: bool,
}
#[derive(Serialize, Deserialize)]
struct Decoded {
    identity: Identity,
    strings: Vec<(String, String)>,
}
pub(super) struct Disk {
    dir: PathBuf,
    install: PathBuf,
    product: String,
    build: String,
}
fn hex_hash(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
impl Disk {
    pub fn new(root: &Path, build: &BuildInfo) -> Option<Self> {
        Self::at(
            &PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Selu/interface-text/v1"),
            root,
            build,
        )
    }
    fn at(base: &Path, root: &Path, build: &BuildInfo) -> Option<Self> {
        if !hex_hash(&build.build_key)
            || build.product.is_empty()
            || !build
                .product
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return None;
        }
        let install = root.canonicalize().ok()?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        install.hash(&mut hash);
        Some(Self {
            dir: base
                .join(format!("{:016x}", hash.finish()))
                .join(&build.product)
                .join(&build.build_key),
            install,
            product: build.product.clone(),
            build: build.build_key.clone(),
        })
    }
    fn identity(&self, content_hash: Option<String>) -> Identity {
        Identity {
            version: 1,
            install: self.install.clone(),
            product: self.product.clone(),
            build: self.build.clone(),
            encrypted: content_hash.is_none(),
            content_hash,
        }
    }
    fn matches(&self, i: &Identity) -> bool {
        i.version == 1
            && i.install == self.install
            && i.product == self.product
            && i.build == self.build
    }
    pub fn load(&self) -> Option<Option<Vec<(String, String)>>> {
        let index: Identity = read(&self.dir.join("index.json"))?;
        if !self.matches(&index) {
            return None;
        }
        if index.encrypted {
            return index.content_hash.is_none().then_some(None);
        }
        let hash = index.content_hash?;
        if !hex_hash(&hash) {
            return None;
        }
        let data: Decoded = read(&self.dir.join(format!("{hash}.json")))?;
        if data.identity.encrypted
            || !self.matches(&data.identity)
            || data.identity.content_hash.as_ref() != Some(&hash)
            || data.strings.is_empty()
        {
            return None;
        }
        Some(Some(data.strings))
    }
    pub fn save(&self, data: Option<(&str, &[(String, String)])>) {
        let hash = data.map(|(hash, _)| hash.to_owned());
        if let Some((hash, strings)) = data {
            if !hex_hash(hash) {
                return;
            }
            let decoded = Decoded {
                identity: self.identity(Some(hash.into())),
                strings: strings.to_vec(),
            };
            if write(&self.dir.join(format!("{hash}.json")), &decoded).is_none() {
                return;
            }
        }
        let _ = write(&self.dir.join("index.json"), &self.identity(hash));
    }
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 16 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}
fn write<T: Serialize>(path: &Path, value: &T) -> Option<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::fs::create_dir_all(path.parent()?).ok()?;
    let bytes = serde_json::to_vec(value).ok()?;
    let temp = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&temp, bytes).ok()?;
    let result = std::fs::rename(&temp, path).ok();
    if result.is_none() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoded_cache_is_scoped_to_product_build_and_content_and_refuses_corruption() {
        let root = std::env::temp_dir().join(format!(
            "selu-self-written-text-cache-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let build = |product: &str, key: &str| BuildInfo {
            product: product.into(),
            build_key: key.into(),
            version: "1.0.0.1".into(),
            active: true,
        };
        let b = build("wowf", &"12".repeat(16));
        let disk = Disk::at(&root.join("cache"), &root, &b).unwrap();
        let hash = "34".repeat(16);
        let strings = vec![("SELF_WRITTEN_TAG".into(), "self-written text".into())];
        disk.save(Some((&hash, &strings)));
        assert_eq!(disk.load(), Some(Some(strings.clone())));
        assert!(
            Disk::at(&root.join("cache"), &root, &build("wow", &b.build_key))
                .unwrap()
                .load()
                .is_none()
        );
        assert!(
            Disk::at(&root.join("cache"), &root, &build("wowf", &"56".repeat(16)))
                .unwrap()
                .load()
                .is_none()
        );
        // A new content identity on the same build never returns the old table.
        disk.save(Some((
            &"78".repeat(16),
            &[("NEW_TAG".into(), "new text".into())],
        )));
        assert_eq!(disk.load().unwrap().unwrap()[0].0, "NEW_TAG");
        std::fs::write(
            disk.dir.join(format!("{}.json", "78".repeat(16))),
            b"truncated {",
        )
        .unwrap();
        assert!(disk.load().is_none());
        disk.save(None);
        assert_eq!(disk.load(), Some(None));
        // Invalid identities and path traversal are refused before any lookup.
        assert!(Disk::at(&root, &root, &build("../escape", &b.build_key)).is_none());
        assert!(Disk::at(&root, &root, &build("wowf", "../escape")).is_none());
        let mut bad = disk.identity(Some("../../escape".into()));
        write(&disk.dir.join("index.json"), &bad).unwrap();
        assert!(disk.load().is_none());
        bad.content_hash = None;
        bad.build = "different".into();
        write(&disk.dir.join("index.json"), &bad).unwrap();
        assert!(disk.load().is_none());
        // This test removes only its own individual fixture files, never a tree.
        for entry in std::fs::read_dir(&disk.dir).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
    }
}

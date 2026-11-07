//! Read-only local CASC access. No downloader or extraction pipeline is called.
mod hash;
pub mod keys;
mod local;
use hash::hashpath;
use local::*;
pub use local::{BuildInfo, parse_build_info};
use md5::{Digest, Md5};
use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub mod blp;
/// An opened snapshot of the active product's local index and root.
pub struct Storage {
    install: PathBuf,
    keys: keys::Keyring,
    pub build: BuildInfo,
    root_entry_count: usize,
    /// Every enUS content key a file has, in root order. A file can have
    /// several variants (platform or content flags); only some are installed.
    content_keys: HashMap<u32, Vec<[u8; 16]>>,
    index: CascIndex,
    encoding: EncodingFile,
    data: PathBuf,
    cache: PathBuf,
    names: HashMap<String, u32>,
    name_hashes: HashMap<u64, Option<u32>>,
    atlas_cache: std::sync::OnceLock<std::result::Result<db2::AtlasIndex, String>>,
    global_strings_cache: std::sync::OnceLock<std::result::Result<db2::GlobalStrings, String>>,
}
fn key(value: &str) -> Result<[u8; 16]> {
    Ok(hex::decode(value)?
        .try_into()
        .map_err(|_| "invalid CASC key length")?)
}
impl Storage {
    pub fn open(install: &Path, product: &str, listfile: Option<&Path>) -> Result<Self> {
        Self::open_with_key_file(install, product, listfile, None)
    }
    pub fn open_with_key_file(
        install: &Path,
        product: &str,
        listfile: Option<&Path>,
        key_file: Option<&Path>,
    ) -> Result<Self> {
        let keys = key_file
            .map(keys::Keyring::load)
            .transpose()?
            .unwrap_or_default();
        let builds = parse_build_info(&std::fs::read_to_string(install.join(".build.info"))?)?;
        let mut matches = builds
            .into_iter()
            .filter(|b| b.active && b.product == product);
        let build = matches.next().ok_or("no active build for product")?;
        if matches.next().is_some() {
            return Err("ambiguous active builds".into());
        }
        let build_key = key(&build.build_key)?;
        let config = parse_config(&std::fs::read_to_string(
            install
                .join("Data")
                .join(config_path(&hex::encode(build_key))),
        )?)?;
        let data = install.join("Data/data");
        let index = CascIndex::load(&data)?;

        let read = |ekey: &[u8]| -> Result<Vec<u8>> {
            let entry = index.find(ekey).ok_or("encoding key absent locally")?;
            read_entry_with_keys(
                &data,
                entry.archive_number,
                entry.archive_offset,
                entry.size,
                &keys,
            )
        };
        let encoding = EncodingFile::parse(&read(&key(&config.encoding_ekey)?)?)?;
        let root_key = key(&config.root_ckey)?;
        let enc = encoding.find_ekey(&root_key).ok_or("root absent locally")?;
        let root = RootFile::parse(&read(enc.ekeys.first().ok_or("root encoding missing")?)?)?;
        let name_hashes = index_names(root.iter_all());
        let root_entry_count = root.len();
        let mut content_keys = HashMap::new();
        for (id, entry) in root.iter_all() {
            if entry.locale_flags.matches(LocaleFlags::EN_US) {
                let keys: &mut Vec<[u8; 16]> = content_keys.entry(id).or_default();
                if !keys.contains(&entry.ckey) {
                    keys.push(entry.ckey);
                }
            }
        }
        let mut names = HashMap::new();
        if let Some(file) = listfile {
            for line in std::fs::read_to_string(file)?.lines() {
                let (id, path) = line.split_once(';').ok_or("invalid listfile row")?;
                names.insert(normalize(path), id.parse()?);
            }
        }
        let cache =
            PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA unavailable")?)
                .join("Selu/gamedata")
                .join(hex::encode(product.as_bytes()))
                .join(hex::encode(build_key));
        Ok(Self {
            install: install.to_path_buf(),
            keys,
            cache,
            build,
            root_entry_count,
            content_keys,
            index,
            encoding,
            data,
            names,
            name_hashes,
            atlas_cache: std::sync::OnceLock::new(),
            global_strings_cache: std::sync::OnceLock::new(),
        })
    }
    pub fn entry_count(&self) -> usize {
        self.root_entry_count
    }
    pub fn install_root(&self) -> &Path {
        &self.install
    }
    /// Keyed sessions must not publish decrypted content to shared caches.
    pub fn uses_user_keys(&self) -> bool {
        !self.keys.is_empty()
    }
    pub fn resolve(&self, path: &str) -> Result<u32> {
        let path = normalize(path);
        let hash = hashpath(&path);
        match self.name_hashes.get(&hash) {
            Some(Some(id)) => Ok(*id),
            None => self.names.get(&path).copied().ok_or_else(|| {
                "path has no root name hash; provide an explicit local listfile or FileDataID"
                    .into()
            }),
            Some(None) => Err("ambiguous root name hash".into()),
        }
    }
    pub fn read_path(&self, path: &str) -> Result<Vec<u8>> {
        self.read(self.resolve(path)?)
    }
    /// Availability from this immutable local index; payload integrity is still
    /// checked by read(). Does not download or decode candidate texture sheets.
    pub fn has_local_file(&self, id: u32) -> bool {
        let Some(keys) = self.content_keys.get(&id) else {
            return false;
        };
        keys.iter().any(|ckey| {
            self.cache.join(hex::encode(ckey)).is_file()
                || self.encoding.find_ekey(ckey).is_some_and(|entry| {
                    entry.ekeys.iter().any(|ekey| {
                        self.index
                            .find(ekey)
                            .is_some_and(|entry| local_entry_present(&self.data, entry, ekey))
                    })
                })
        })
    }
    pub fn read(&self, id: u32) -> Result<Vec<u8>> {
        let keys = self
            .content_keys
            .get(&id)
            .ok_or("FileDataID absent for enUS")?;
        // The first variant that is installed locally is the one the client
        // on this machine uses.
        let mut failure = None;
        for ckey in keys {
            match self.read_content(ckey) {
                Ok(Some(bytes)) => return Ok(bytes),
                Ok(None) => {}
                Err(error) => failure = Some(error),
            }
        }
        Err(failure.unwrap_or_else(|| "file not installed locally".into()))
    }
    fn read_content(&self, ckey: &[u8; 16]) -> Result<Option<Vec<u8>>> {
        let cache = self.cache.join(hex::encode(ckey));
        if self.keys.is_empty()
            && let Ok(bytes) = std::fs::read(&cache)
            && Md5::digest(&bytes).as_slice() == ckey
        {
            return Ok(Some(bytes));
        }
        let Some(enc) = self.encoding.find_ekey(ckey) else {
            return Ok(None);
        };
        let mut failure = None;
        for ekey in &enc.ekeys {
            if let Some(e) = self.index.find(ekey) {
                let result = read_entry_with_keys(
                    &self.data,
                    e.archive_number,
                    e.archive_offset,
                    e.size,
                    &self.keys,
                );
                match result {
                    Ok(bytes) => {
                        if Md5::digest(&bytes).as_slice() != ckey {
                            failure = Some("decoded CASC content hash mismatch".into());
                            continue;
                        }
                        if self.keys.is_empty() {
                            std::fs::create_dir_all(&self.cache)?;
                            std::fs::write(&cache, &bytes)?;
                        }
                        return Ok(Some(bytes));
                    }
                    Err(error) => failure = Some(error),
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(None),
        }
    }
}
fn index_names<'a>(
    entries: impl Iterator<Item = (u32, &'a RootEntry)>,
) -> HashMap<u64, Option<u32>> {
    let mut index = HashMap::new();
    for (id, entry) in entries {
        if entry.locale_flags.matches(LocaleFlags::EN_US)
            && let Some(hash) = entry.name_hash
        {
            index
                .entry(hash)
                .and_modify(|value| {
                    if *value != Some(id) {
                        *value = None;
                    }
                })
                .or_insert(Some(id));
        }
    }
    index
}
fn normalize(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}
// CascLib BLTE_ENCODED_HEADER: reversed EKey, 30-byte span header, BLTE.
// https://github.com/ladislav-zezula/CascLib/blob/master/src/CascStructs.h
// Local index keys have only nine bytes; a prefix hit alone is insufficient.
fn local_entry_present(data: &Path, entry: &IndexEntry, ekey: &[u8; 16]) -> bool {
    let check = || -> std::io::Result<bool> {
        let mut file = std::fs::File::open(data.join(format!("data.{:03}", entry.archive_number)))?;
        let Some(end) = entry.archive_offset.checked_add(u64::from(entry.size)) else {
            return Ok(false);
        };
        if entry.size < 34 || file.metadata()?.len() < end {
            return Ok(false);
        }
        file.seek(SeekFrom::Start(entry.archive_offset))?;
        let mut header = [0u8; 34];
        file.read_exact(&mut header)?;
        // Usual layout: the 30-byte entry header (reversed key first), then
        // BLTE. Some entries in current clients are indexed at the BLTE data
        // itself, with the header in front of it; the caller's MD5 check of
        // the decoded content decides.
        Ok(
            (header[..16].iter().rev().eq(ekey.iter()) && &header[30..] == b"BLTE")
                || &header[..4] == b"BLTE",
        )
    };
    check().unwrap_or(false)
}
#[cfg(test)]
fn read_entry(data: &Path, archive: u32, offset: u64, size: u32) -> Result<Vec<u8>> {
    read_entry_with_keys(data, archive, offset, size, &keys::Keyring::default())
}
fn read_entry_with_keys(
    data: &Path,
    archive: u32,
    offset: u64,
    size: u32,
    keys: &keys::Keyring,
) -> Result<Vec<u8>> {
    if !(8..=268_435_456).contains(&size) {
        return Err("invalid or oversized CASC entry".into());
    }
    let mut file = std::fs::File::open(data.join(format!("data.{archive:03}")))?;
    // See local_entry_present: an entry is indexed either at its header or
    // at its BLTE data.
    file.seek(SeekFrom::Start(offset))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    // Indexed at the data, the size covers the BLTE data alone.
    let (start, len) = if &magic == b"BLTE" {
        (offset, size as usize)
    } else {
        let len = (size as usize)
            .checked_sub(30)
            .ok_or("invalid or oversized CASC entry")?;
        (offset.checked_add(30).ok_or("offset overflow")?, len)
    };
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = vec![0; len];
    file.read_exact(&mut bytes)?;
    local::decode_blte_with_keys(&bytes, keys)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entries_indexed_at_their_blte_data_are_read() {
        // Current clients index some entries at the BLTE data itself, with
        // the 30-byte header (key zeroed) in front; the size is the data's.
        let root = std::env::temp_dir().join(format!("selu-entry-data-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut bytes = vec![0u8; 30];
        bytes.extend_from_slice(b"BLTE\0\0\0\0Nhello");
        std::fs::write(root.join("data.000"), &bytes).unwrap();
        let entry = IndexEntry {
            archive_number: 0,
            archive_offset: 30,
            size: 14,
        };
        assert!(
            !local_entry_present(&root, &entry, &[7; 16]),
            "shorter than a header"
        );
        assert_eq!(read_entry(&root, 0, 30, 14).unwrap(), b"hello");
        // The usual layout still reads after the header.
        let mut usual = vec![0u8; 30];
        usual.extend_from_slice(b"BLTE\0\0\0\0Nworld");
        std::fs::write(root.join("data.001"), &usual).unwrap();
        assert_eq!(read_entry(&root, 1, 0, 44).unwrap(), b"world");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn availability_rejects_prefix_collisions_and_truncated_entries() {
        let root = std::env::temp_dir().join(format!("selu-entry-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let ekey = std::array::from_fn(|index| index as u8);
        let entry = IndexEntry {
            archive_number: 0,
            archive_offset: 0,
            size: 34,
        };
        let mut bytes = vec![0; 34];
        for index in 0..16 {
            bytes[index] = ekey[15 - index];
        }
        bytes[30..].copy_from_slice(b"BLTE");
        let file = root.join("data.000");
        std::fs::write(&file, &bytes).unwrap();
        assert!(local_entry_present(&root, &entry, &ekey));
        let mut collision = ekey;
        collision[15] = 99;
        assert!(!local_entry_present(&root, &entry, &collision));
        bytes.pop();
        std::fs::write(&file, &bytes).unwrap();
        assert!(!local_entry_present(&root, &entry, &ekey));
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn root_name_index_preserves_locale_and_ambiguity() {
        let entries = [
            (
                1,
                RootEntry {
                    ckey: [0; 16],
                    name_hash: Some(10),
                    locale_flags: LocaleFlags::EN_US,
                },
            ),
            (
                1,
                RootEntry {
                    ckey: [0; 16],
                    name_hash: Some(10),
                    locale_flags: LocaleFlags(0),
                },
            ),
            (
                2,
                RootEntry {
                    ckey: [0; 16],
                    name_hash: Some(20),
                    locale_flags: LocaleFlags::EN_US,
                },
            ),
            (
                3,
                RootEntry {
                    ckey: [0; 16],
                    name_hash: Some(20),
                    locale_flags: LocaleFlags::EN_US,
                },
            ),
            (
                4,
                RootEntry {
                    ckey: [0; 16],
                    name_hash: Some(30),
                    locale_flags: LocaleFlags(4),
                },
            ),
        ];
        let index = index_names(entries.iter().map(|(id, entry)| (*id, entry)));
        assert_eq!(index.get(&10), Some(&Some(1)));
        assert_eq!(index.get(&20), Some(&None));
        assert!(!index.contains_key(&30));
    }
    #[test]
    fn malformed_build_and_keys_fail() {
        assert!(key("../bad").is_err());
        assert!(parse_build_info("").is_err());
    }
    #[test]
    fn path_hash_normalizes() {
        assert_eq!(
            hashpath(&normalize("Fonts/FRIZQT__.TTF")),
            hashpath("FONTS\\FRIZQT__.TTF")
        );
    }
    #[test]
    #[ignore = "requires owner local install"]
    fn forever_font() {
        let s = Storage::open(
            Path::new("E:/Games/Battle.net/World of Warcraft"),
            "wowf",
            None,
        )
        .unwrap();
        assert!(s.read_path("Fonts/FRIZQT__.TTF").unwrap().len() > 1000);
    }
}

struct Config {
    encoding_ekey: String,
    root_ckey: String,
}
fn parse_config(text: &str) -> Result<Config> {
    let values: HashMap<_, _> = text.lines().filter_map(|l| l.split_once(" = ")).collect();
    Ok(Config {
        encoding_ekey: values
            .get("encoding")
            .and_then(|s| s.split_whitespace().nth(1))
            .ok_or("missing encoding key")?
            .into(),
        root_ckey: values.get("root").ok_or("missing root key")?.to_string(),
    })
}
fn config_path(k: &str) -> PathBuf {
    PathBuf::from("config").join(&k[..2]).join(&k[2..4]).join(k)
}
pub mod db2;

pub mod discovery;

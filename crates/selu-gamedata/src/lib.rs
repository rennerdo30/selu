//! Read-only local CASC access. No downloader or extraction pipeline is called.
mod hash;
mod local;
use hash::hashpath;
use local::*;
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
    pub build: BuildInfo,
    root: RootFile,
    index: CascIndex,
    encoding: EncodingFile,
    data: PathBuf,
    cache: PathBuf,
    names: HashMap<String, u32>,
}
fn key(value: &str) -> Result<[u8; 16]> {
    Ok(hex::decode(value)?
        .try_into()
        .map_err(|_| "invalid CASC key length")?)
}
impl Storage {
    pub fn open(install: &Path, product: &str, listfile: Option<&Path>) -> Result<Self> {
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
            read_entry(
                &data,
                entry.archive_number,
                entry.archive_offset,
                entry.size,
            )
        };
        let encoding = EncodingFile::parse(&read(&key(&config.encoding_ekey)?)?)?;
        let root_key = key(&config.root_ckey)?;
        let enc = encoding.find_ekey(&root_key).ok_or("root absent locally")?;
        let root = RootFile::parse(&read(enc.ekeys.first().ok_or("root encoding missing")?)?)?;
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
            cache,
            build,
            root,
            index,
            encoding,
            data,
            names,
        })
    }
    pub fn entry_count(&self) -> usize {
        self.root.len()
    }
    pub fn resolve(&self, path: &str) -> Result<u32> {
        let path = normalize(path);
        let hash = hashpath(&path);
        let mut ids: Vec<_> = self
            .root
            .iter_all()
            .filter(|(_, e)| {
                e.name_hash == Some(hash) && e.locale_flags.matches(LocaleFlags::EN_US)
            })
            .map(|(id, _)| id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        match ids.as_slice() {
            [id] => Ok(*id),
            [] => self.names.get(&path).copied().ok_or_else(|| {
                "path has no root name hash; provide an explicit local listfile or FileDataID"
                    .into()
            }),
            _ => Err("ambiguous root name hash".into()),
        }
    }
    pub fn read_path(&self, path: &str) -> Result<Vec<u8>> {
        self.read(self.resolve(path)?)
    }
    pub fn read(&self, id: u32) -> Result<Vec<u8>> {
        let root = self
            .root
            .find_by_fdid(id, LocaleFlags::EN_US)
            .ok_or("FileDataID absent for enUS")?;
        let cache = self.cache.join(hex::encode(root.ckey));
        if let Ok(bytes) = std::fs::read(&cache)
            && Md5::digest(&bytes).as_slice() == root.ckey
        {
            return Ok(bytes);
        }
        let enc = self
            .encoding
            .find_ekey(&root.ckey)
            .ok_or("content absent locally")?;
        for ekey in &enc.ekeys {
            if let Some(e) = self.index.find(ekey) {
                let result = read_entry(&self.data, e.archive_number, e.archive_offset, e.size);
                if let Ok(bytes) = result {
                    if Md5::digest(&bytes).as_slice() != root.ckey {
                        continue;
                    }
                    std::fs::create_dir_all(&self.cache)?;
                    std::fs::write(&cache, &bytes)?;
                    return Ok(bytes);
                }
            }
        }
        Err("file not installed locally".into())
    }
}
fn normalize(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}
fn read_entry(data: &Path, archive: u32, offset: u64, size: u32) -> Result<Vec<u8>> {
    if !(30..=268_435_456).contains(&size) {
        return Err("invalid or oversized CASC entry".into());
    }
    let mut file = std::fs::File::open(data.join(format!("data.{archive:03}")))?;
    file.seek(SeekFrom::Start(
        offset.checked_add(30).ok_or("offset overflow")?,
    ))?;
    let mut bytes = vec![0; size as usize - 30];
    file.read_exact(&mut bytes)?;
    decode_blte(&bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
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

//! Immutable dense-ID packs. Hashes deduplicate writes, never individual references.
use crate::Manifest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::BTreeMap, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Index = BTreeMap<String, Vec<u32>>;
type DecodedChunks = BTreeMap<(usize, usize), Vec<Vec<u8>>>;
const MAGIC: &[u8; 8] = b"SELUSEG\0";
const HEADER: usize = 48;
const LIMIT: u64 = 256 * 1024 * 1024;
/// Generator contract version. Bump whenever extraction output rules change.
/// Version 1 is the W2 extraction shipped in the original format-2 pack.
pub const EXTRACTION_VERSION: u32 = 18;
const LEGACY_EXTRACTION_VERSION: u32 = 1;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtractionRevision {
    version: u32,
    content: String,
}
impl ExtractionRevision {
    fn id(&self) -> String {
        format!("v{}-{}", self.version, self.content)
    }
}
fn revision(manifest: &Manifest, version: u32) -> Result<ExtractionRevision> {
    Ok(ExtractionRevision {
        version,
        content: hash(&rmp_serde::to_vec_named(manifest)?),
    })
}
fn game_build(key: &str) -> &str {
    key.split_once('#').map_or(key, |(build, _)| build)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildDelta {
    base: Option<String>,
    sections: BTreeMap<String, Vec<Run>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    revision: Option<ExtractionRevision>,
    #[serde(default, skip_serializing_if = "is_zero")]
    ordinal: u64,
}
fn is_zero(value: &u64) -> bool {
    *value == 0
}
#[derive(Clone, Serialize, Deserialize)]
enum Run {
    Copy { start: usize, length: usize },
    Insert(Vec<u32>),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Chunk {
    first: u32,
    count: u32,
    offset: usize,
    length: usize,
    raw: usize,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    chunks: Vec<Chunk>,
    builds: BTreeMap<String, BuildDelta>,
}
struct Segment {
    bytes: Vec<u8>,
    catalog: Catalog,
    payload: usize,
}
#[derive(Default)]
pub struct Store {
    segments: Vec<Segment>,
    builds: BTreeMap<String, BuildDelta>,
    latest: BTreeMap<String, String>,
    indexes: RefCell<BTreeMap<String, Index>>,
    chunks: RefCell<DecodedChunks>,
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn unpack(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut raw = Vec::new();
    zstd::stream::read::Decoder::new(bytes)?
        .take(LIMIT + 1)
        .read_to_end(&mut raw)?;
    if raw.len() as u64 > LIMIT {
        return Err("store block exceeds 256 MiB".into());
    }
    Ok(raw)
}
fn difference(base: Option<String>, old: &Index, new: &Index) -> BuildDelta {
    let sections = new
        .iter()
        .map(|(name, entries)| {
            let previous = old.get(name).map(Vec::as_slice).unwrap_or_default();
            let positions: BTreeMap<_, _> = previous
                .iter()
                .enumerate()
                .map(|(i, key)| (key, i))
                .collect();
            let mut runs = Vec::new();
            for entry in entries {
                if let Some(&position) = positions.get(entry) {
                    if let Some(Run::Copy { start, length }) = runs.last_mut()
                        && *start + *length == position
                    {
                        *length += 1;
                        continue;
                    }
                    runs.push(Run::Copy {
                        start: position,
                        length: 1,
                    });
                } else if let Some(Run::Insert(values)) = runs.last_mut() {
                    values.push(*entry);
                } else {
                    runs.push(Run::Insert(vec![*entry]));
                }
            }
            (name.clone(), runs)
        })
        .collect();
    BuildDelta {
        base,
        sections,
        revision: None,
        ordinal: 0,
    }
}
fn reconstruct(delta: &BuildDelta, old: &Index) -> Result<Index> {
    delta
        .sections
        .iter()
        .map(|(name, runs)| {
            let previous = old.get(name).map(Vec::as_slice).unwrap_or_default();
            let mut entries = Vec::new();
            for run in runs {
                match run {
                    Run::Copy { start, length } => {
                        let end = start
                            .checked_add(*length)
                            .ok_or("invalid store copy range")?;
                        entries.extend_from_slice(
                            previous
                                .get(*start..end)
                                .ok_or("invalid store copy range")?,
                        );
                    }
                    Run::Insert(values) => entries.extend(values),
                }
            }
            Ok((name.clone(), entries))
        })
        .collect()
}
fn files(directory: &Path) -> Result<Vec<std::path::PathBuf>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut files = std::fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.retain(|p| p.extension().is_some_and(|e| e == "selus"));
    files.sort();
    Ok(files)
}
// Keep the lock file's identity stable; unlinking it would let another process
// lock a different inode. The OS releases the lock when this handle closes.
fn writer_lock(directory: &Path) -> Result<std::fs::File> {
    std::fs::create_dir_all(directory)?;
    let directory = std::fs::canonicalize(directory)?;
    let mut name = directory
        .file_name()
        .ok_or("store has no directory name")?
        .to_os_string();
    name.push(".lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.with_file_name(name))?;
    let start = std::time::Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock)
                if start.elapsed() < std::time::Duration::from_secs(30) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(
                    "timed out waiting for manifest store writer lock; retry the update".into(),
                );
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
        }
    }
}
/// Headers include the checksum of the complete immutable segment payload.
pub fn fingerprint(directory: &Path) -> Result<String> {
    use std::io::Read;
    let mut digest = Sha256::new();
    for path in files(directory)? {
        let mut header = [0; HEADER];
        std::fs::File::open(path)?.read_exact(&mut header)?;
        if &header[..8] != MAGIC || !matches!(&header[8..12], [2 | 3, 0, 1, 0]) {
            return Err("invalid store segment header".into());
        }
        digest.update(header);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
fn requested_builds<'a>(
    targets: impl Iterator<Item = &'a str>,
    requests: Option<&[String]>,
) -> Result<std::collections::BTreeSet<String>> {
    let targets: Vec<_> = targets.collect();
    let Some(requests) = requests else {
        return Ok(targets.into_iter().map(str::to_owned).collect());
    };
    let mut selected = std::collections::BTreeSet::new();
    for request in requests {
        let spec = crate::TargetSpec::parse(request).map_err(|e| e.to_string())?;
        if let Some((_, target)) = targets
            .iter()
            .filter_map(|target| {
                let (product, version) = target.split_once('@')?;
                let build = crate::Build::parse(version).ok()?;
                (product == crate::product(&spec.flavour) && spec.matches(&build))
                    .then_some((build, *target))
            })
            .max_by_key(|(build, _)| (build.build, *build))
        {
            selected.insert(target.to_owned());
        }
    }
    Ok(selected)
}
pub fn load_cached(directory: &Path, cache: &Path) -> Result<Vec<Manifest>> {
    load_cached_selected(directory, cache, None)
}
pub fn load_build(directory: &Path, target: &str) -> Result<Manifest> {
    let cache = directory
        .parent()
        .and_then(Path::parent)
        .unwrap_or(directory)
        .join("target/selu-manifest-cache");
    load_cached_selected(directory, &cache, Some(&[target.to_owned()]))?
        .into_iter()
        .find(|m| m.target_string() == target)
        .ok_or_else(|| missing(target).into())
}
fn missing(target: &str) -> String {
    format!(
        "locked build {target} missing from store; fetch the wow-ui-source clone, then run selu-manifest-gen update --source <clone>"
    )
}
pub fn load_cached_selected(
    directory: &Path,
    cache_root: &Path,
    requests: Option<&[String]>,
) -> Result<Vec<Manifest>> {
    let identity = fingerprint(directory)?;
    let cache = cache_root.join("v4").join(&identity);
    let store = Store::open(directory)?;
    let selected = requested_builds(store.builds(), requests)?;
    let mut manifests = Vec::new();
    for target in selected {
        let path = cache.join(format!("{}.selum", hash(target.as_bytes())));
        let read = || -> Result<Manifest> {
            let bytes = std::fs::read(&path)?;
            let crc = std::fs::read(path.with_extension("crc"))?;
            if crc != crc32fast::hash(&bytes).to_le_bytes() {
                return Err("corrupt manifest cache".into());
            }
            let manifest: Manifest = crate::format::decode(&bytes)?;
            if manifest.target_string() != target {
                return Err("cache build identity mismatch".into());
            }
            Ok(manifest)
        };
        let manifest = if let Ok(manifest) = read() {
            manifest
        } else {
            let key = &store.latest[&target];
            let raw = store.materialize(key)?;
            let manifest = store.decode(key, &raw)?;
            // Only requested builds are materialized and cached.
            if std::fs::create_dir_all(&cache).is_ok() {
                // Serialize the normalized typed manifest directly, without a JSON intermediary.
                let bytes = crate::format::encode(&manifest)?;
                if std::fs::write(&path, &bytes).is_ok() {
                    let _ = std::fs::write(
                        path.with_extension("crc"),
                        crc32fast::hash(&bytes).to_le_bytes(),
                    );
                }
            }
            manifest
        };
        manifests.push(manifest);
    }
    if fingerprint(directory)? != identity {
        return Err("manifest store changed while loading; retry".into());
    }
    Ok(manifests)
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self> {
        Self::open_paths(files(directory)?)
    }
    fn open_paths(paths: Vec<std::path::PathBuf>) -> Result<Self> {
        let mut store = Self::default();
        for path in paths {
            let bytes = std::fs::read(path)?;
            if bytes.len() < HEADER
                || &bytes[..8] != MAGIC
                || !matches!(&bytes[8..12], [2 | 3, 0, 1, 0])
            {
                return Err("unsupported or truncated store header".into());
            }
            if Sha256::digest(&bytes[44..])[..] != bytes[12..44] {
                return Err("store segment checksum mismatch".into());
            }
            let length = u32::from_le_bytes(bytes[44..48].try_into()?) as usize;
            let payload = HEADER.checked_add(length).ok_or("invalid catalog length")?;
            let catalog: Catalog = rmp_serde::from_slice(&unpack(
                bytes.get(HEADER..payload).ok_or("truncated catalog")?,
            )?)?;
            for (key, delta) in &catalog.builds {
                if let Some(revision) = &delta.revision {
                    if bytes[8] != 3
                        || revision.content.len() != 64
                        || !revision
                            .content
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                        || delta.ordinal == 0
                        || key != &format!("{}#{}", game_build(key), revision.id())
                    {
                        return Err("invalid extraction revision index".into());
                    }
                } else if bytes[8] != 2 || key.contains('#') || delta.ordinal != 0 {
                    return Err("invalid legacy extraction index".into());
                }
                if store.builds.insert(key.clone(), delta.clone()).is_some() {
                    return Err("duplicate immutable build index".into());
                }
            }
            for chunk in &catalog.chunks {
                if chunk.count == 0
                    || chunk.first.checked_add(chunk.count).is_none()
                    || chunk.raw as u64 > LIMIT
                    || chunk
                        .offset
                        .checked_add(chunk.length)
                        .and_then(|end| bytes.get(payload..)?.get(..end))
                        .is_none()
                {
                    return Err("invalid store chunk range".into());
                }
            }
            store.segments.push(Segment {
                bytes,
                catalog,
                payload,
            });
        }
        let mut ranges: Vec<_> = store
            .segments
            .iter()
            .flat_map(|s| &s.catalog.chunks)
            .map(|c| (c.first, c.count))
            .collect();
        ranges.sort();
        let mut next = 0u32;
        for (first, count) in ranges {
            if first != next {
                return Err("non-dense or duplicate store entry IDs".into());
            }
            next = next.checked_add(count).ok_or("too many entries")?;
        }
        for (key, delta) in &store.builds {
            let mut seen = std::collections::BTreeSet::new();
            let mut current = Some(key.as_str());
            while let Some(name) = current {
                if !seen.insert(name) {
                    return Err("cyclic store base".into());
                }
                current = store
                    .builds
                    .get(name)
                    .ok_or("missing store base")?
                    .base
                    .as_deref();
            }
            for run in delta.sections.values().flatten() {
                if let Run::Insert(ids) = run
                    && ids.iter().any(|id| *id >= next)
                {
                    return Err("store entry missing".into());
                }
            }
        }
        let mut ordinals = std::collections::BTreeSet::new();
        for (key, delta) in &store.builds {
            let build = game_build(key);
            if !ordinals.insert((build, delta.ordinal)) {
                return Err("duplicate extraction ordinal".into());
            }
            let rank = |delta: &BuildDelta| {
                (
                    delta
                        .revision
                        .as_ref()
                        .map_or(LEGACY_EXTRACTION_VERSION, |r| r.version),
                    delta.ordinal,
                )
            };
            if store
                .latest
                .get(build)
                .is_none_or(|old| rank(delta) > rank(&store.builds[old]))
            {
                store.latest.insert(build.into(), key.clone());
            }
        }
        Ok(store)
    }
    pub fn builds(&self) -> impl Iterator<Item = &str> {
        self.latest.keys().map(String::as_str)
    }
    pub fn extraction_version(&self, target: &str) -> Option<u32> {
        self.latest.get(target).map(|key| {
            self.builds[key]
                .revision
                .as_ref()
                .map_or(LEGACY_EXTRACTION_VERSION, |r| r.version)
        })
    }
    pub fn needs_extraction(&self, target: &str, version: u32) -> bool {
        self.extraction_version(target)
            .is_none_or(|stored| stored < version)
    }
    fn revision_id(&self, key: &str) -> Result<String> {
        match &self.builds[key].revision {
            Some(revision) => Ok(revision.id()),
            None => Ok(revision(
                &self.decode(key, &self.materialize(key)?)?,
                LEGACY_EXTRACTION_VERSION,
            )?
            .id()),
        }
    }
    /// Audit identifiers, ordered by extractor version then append order.
    pub fn revisions(&self, target: &str) -> Result<Vec<String>> {
        let mut keys: Vec<_> = self
            .builds
            .iter()
            .filter(|(key, _)| game_build(key) == target)
            .collect();
        keys.sort_by_key(|(_, delta)| {
            (
                delta
                    .revision
                    .as_ref()
                    .map_or(LEGACY_EXTRACTION_VERSION, |r| r.version),
                delta.ordinal,
            )
        });
        keys.into_iter()
            .map(|(key, _)| self.revision_id(key))
            .collect()
    }
    pub fn latest_revision(&self, target: &str) -> Result<String> {
        self.revision_id(self.latest.get(target).ok_or_else(|| missing(target))?)
    }
    fn revision_key(&self, target: &str, id: &str) -> Result<String> {
        for key in self.builds.keys().filter(|key| game_build(key) == target) {
            if self.revision_id(key)? == id {
                return Ok(key.clone());
            }
        }
        Err(format!("extraction revision {id} missing for {target}").into())
    }
    pub fn load_revision(&self, target: &str, id: &str) -> Result<Manifest> {
        let key = self.revision_key(target, id)?;
        self.decode(&key, &self.materialize(&key)?)
    }
    /// Audit the newest extraction transition for each active locked game build.
    /// This report never changes the project lock or pins an extraction revision.
    pub fn extraction_report(&self, project: &Path) -> Result<String> {
        let lock = crate::lock::Lock::read(project)?;
        let requested = lock.discovery_targets(&crate::lock::project_targets(project)?)?;
        let mut builds = std::collections::BTreeSet::new();
        for target in requested {
            let spec = crate::TargetSpec::parse(&target).map_err(|e| e.to_string())?;
            if spec.parts.len() != 4 {
                return Err("build the project first to create exact game-build locks".into());
            }
            builds.insert(format!(
                "{}@{}",
                crate::product(&spec.flavour),
                spec.parts
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(".")
            ));
        }
        let mut report = String::new();
        for build in builds {
            let revisions = self.revisions(&build)?;
            let latest = revisions.last().ok_or_else(|| missing(&build))?;
            report.push_str(&format!(
                "{build}: extraction {latest} (automatically used; game pin unchanged)\n"
            ));
            if revisions.len() > 1 {
                let previous = &revisions[revisions.len() - 2];
                report.push_str(&format!("  newer extraction: {previous} -> {latest}\n"));
                report.push_str(&crate::lock::api_diff(
                    project,
                    &self.load_revision(&build, previous)?,
                    &self.load_revision(&build, latest)?,
                )?);
            } else {
                report.push_str("  one stored extraction; no earlier revision to compare\n");
            }
        }
        Ok(report)
    }
    fn index(&self, target: &str) -> Result<Index> {
        if let Some(index) = self.indexes.borrow().get(target) {
            return Ok(index.clone());
        }
        let delta = self.builds.get(target).ok_or_else(|| missing(target))?;
        let old = delta
            .base
            .as_ref()
            .map(|b| self.index(b))
            .transpose()?
            .unwrap_or_default();
        let index = reconstruct(delta, &old)?;
        self.indexes
            .borrow_mut()
            .insert(target.into(), index.clone());
        Ok(index)
    }
    fn with_entry<T>(&self, id: u32, f: impl FnOnce(&[u8]) -> T) -> Result<T> {
        for (s, segment) in self.segments.iter().enumerate() {
            if let Some((c, chunk)) = segment
                .catalog
                .chunks
                .iter()
                .enumerate()
                .find(|(_, c)| id >= c.first && id - c.first < c.count)
            {
                if !self.chunks.borrow().contains_key(&(s, c)) {
                    let start = segment.payload + chunk.offset;
                    let raw = unpack(&segment.bytes[start..start + chunk.length])?;
                    if raw.len() != chunk.raw {
                        return Err("store chunk size mismatch".into());
                    }
                    let mut entries = Vec::new();
                    let mut rest = raw.as_slice();
                    while !rest.is_empty() {
                        let len =
                            u32::from_le_bytes(rest.get(..4).ok_or("truncated entry")?.try_into()?)
                                as usize;
                        entries.push(
                            rest.get(4..4usize.checked_add(len).ok_or("invalid entry length")?)
                                .ok_or("truncated entry")?
                                .to_vec(),
                        );
                        rest = &rest[4 + len..];
                    }
                    if entries.len() != chunk.count as usize {
                        return Err("store chunk entry count mismatch".into());
                    }
                    self.chunks.borrow_mut().insert((s, c), entries);
                }
                return Ok(f(
                    &self.chunks.borrow()[&(s, c)][(id - chunk.first) as usize]
                ));
            }
        }
        Err("store entry missing".into())
    }
    fn materialize(&self, target: &str) -> Result<Vec<u8>> {
        let index = self.index(target)?;
        let mut raw = vec![0xdf];
        raw.extend((index.len() as u32).to_be_bytes());
        for (name, ids) in &index {
            let scalar = name.starts_with('=');
            raw.extend(rmp_serde::to_vec(name.trim_start_matches('='))?);
            if scalar {
                if ids.len() != 1 {
                    return Err("invalid scalar index".into());
                }
            } else {
                raw.push(0xdd);
                raw.extend((ids.len() as u32).to_be_bytes());
            }
            for id in ids {
                self.with_entry(*id, |entry| raw.extend_from_slice(entry))?;
            }
        }
        Ok(raw)
    }
    fn decode(&self, target: &str, raw: &[u8]) -> Result<Manifest> {
        let mut manifest: Manifest = rmp_serde::from_slice(raw)?;
        let index = self.index(target)?;
        let files: BTreeMap<_, _> = index
            .get("sourceFiles")
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, id)| (*id as usize, i))
            .collect();
        for entry in manifest
            .data_globals
            .iter_mut()
            .chain(&mut manifest.templates)
        {
            entry.source = *files.get(&entry.source).ok_or("missing provenance file")?;
        }
        if manifest.target_string() != game_build(target) {
            return Err("store build identity mismatch".into());
        }
        if let Some(expected) = &self.builds[target].revision
            && revision(&manifest, expected.version)?.content != expected.content
        {
            return Err("extraction revision content checksum mismatch".into());
        }
        Ok(manifest)
    }
    pub fn load(&self, target: &str) -> Result<Manifest> {
        let key = self.latest.get(target).ok_or_else(|| missing(target))?;
        self.decode(key, &self.materialize(key)?)
    }
    /// Compressed catalog, compressed entry tables, and fixed header sizes.
    pub fn size_breakdown(&self) -> (usize, usize, usize) {
        self.segments.iter().fold((0, 0, 0), |(h, i, e), s| {
            (
                h + HEADER,
                i + s.payload - HEADER,
                e + s.bytes.len() - s.payload,
            )
        })
    }
    fn entry_bytes(&self) -> Result<Vec<Vec<u8>>> {
        let count: u32 = self
            .segments
            .iter()
            .flat_map(|s| &s.catalog.chunks)
            .map(|c| c.count)
            .sum();
        (0..count)
            .map(|id| self.with_entry(id, <[u8]>::to_vec))
            .collect()
    }
    pub fn append(&mut self, directory: &Path, manifests: &[Manifest]) -> Result<u64> {
        self.append_with_chunk_size(directory, manifests, 4096)
    }
    /// Dev measurement control; production uses 4096 entries per independent block.
    pub fn append_with_chunk_size(
        &mut self,
        directory: &Path,
        manifests: &[Manifest],
        chunk_size: usize,
    ) -> Result<u64> {
        self.append_revision(directory, manifests, Some(EXTRACTION_VERSION), chunk_size)
    }
    /// Import snapshots without extractor provenance. They cannot replace an existing
    /// build; new builds use version 0 so the generator will re-extract them.
    pub fn import_unversioned(&mut self, directory: &Path, manifests: &[Manifest]) -> Result<u64> {
        self.append_revision(directory, manifests, None, 4096)
    }
    /// Explicit extractor contract for trusted generation, audits and measurements.
    pub fn append_with_extraction_version(
        &mut self,
        directory: &Path,
        manifests: &[Manifest],
        version: u32,
    ) -> Result<u64> {
        self.append_revision(directory, manifests, Some(version), 4096)
    }
    fn append_revision(
        &mut self,
        directory: &Path,
        inputs: &[Manifest],
        version: Option<u32>,
        chunk_size: usize,
    ) -> Result<u64> {
        if version == Some(0) {
            return Err("extraction version must be positive".into());
        }
        let _writer_lock = writer_lock(directory)?;
        // A caller may have opened this handle before another writer committed.
        *self = Self::open(directory)?;
        // Admission must use the state reopened under the writer lock. Unversioned
        // inputs are never evidence of a newer extraction, even for an older writer.
        let mut manifests = Vec::new();
        for manifest in inputs {
            let target = manifest.target_string();
            if version.is_none() && self.latest.contains_key(&target) {
                if self.load(&target)? != *manifest {
                    return Err(format!(
                        "unversioned import conflicts with stored build {target}; run update to re-extract from game sources"
                    ).into());
                }
            } else {
                manifests.push(manifest);
            }
        }
        if manifests.is_empty() {
            return Ok(0);
        }
        let version = version.unwrap_or(0);
        if chunk_size == 0 {
            return Err("zero chunk size".into());
        }
        let existing = self.entry_bytes()?;
        let mut hashes: BTreeMap<[u8; 32], u32> = existing
            .iter()
            .enumerate()
            .map(|(i, b)| (Sha256::digest(b).into(), i as u32))
            .collect();
        let first = existing.len() as u32;
        let mut entries = Vec::<Vec<u8>>::new();
        let mut indexes = BTreeMap::new();
        for key in self.builds.keys() {
            indexes.insert(key.clone(), self.index(key)?);
        }
        let mut catalog = Catalog::default();
        manifests.sort_by_key(|m| m.target_string());
        let mut pending = BTreeMap::new();
        for &manifest in &manifests {
            let target = manifest.target_string();
            let extraction = revision(manifest, version)?;
            let id = extraction.id();
            let mut exists = false;
            for key in self.builds.keys().filter(|key| game_build(key) == target) {
                if self.revision_id(key)? == id {
                    exists = true;
                    break;
                }
            }
            if exists {
                continue;
            }
            if let Some(previous) = pending.insert(target.clone(), id.clone())
                && previous != id
            {
                return Err(
                    "multiple extraction revisions for one build in an append transaction".into(),
                );
            }
            let key = format!("{target}#{id}");
            let mut intern = |value: &Value| -> Result<u32> {
                let bytes = rmp_serde::to_vec_named(value)?;
                let digest: [u8; 32] = Sha256::digest(&bytes).into();
                if let Some(id) = hashes.get(&digest) {
                    return Ok(*id);
                }
                let id = first
                    .checked_add(u32::try_from(entries.len())?)
                    .ok_or("too many entries")?;
                hashes.insert(digest, id);
                entries.push(bytes);
                Ok(id)
            };
            let mut file_ids = Vec::new();
            for file in &manifest.source_files {
                file_ids.push(intern(&Value::String(file.clone()))?);
            }
            let Value::Object(object) = serde_json::to_value(manifest)? else {
                unreachable!()
            };
            let mut index = Index::new();
            for (section, value) in object {
                let (section, values) = match value {
                    Value::Array(values) => (section, values),
                    scalar => (format!("={section}"), vec![scalar]),
                };
                let mut ids = Vec::new();
                for mut value in values {
                    if matches!(section.as_str(), "dataGlobals" | "templates")
                        && let Some(source) = value.get("source").and_then(Value::as_u64)
                    {
                        value["source"] = serde_json::json!(
                            file_ids
                                .get(source as usize)
                                .ok_or("invalid provenance index")?
                        );
                    }
                    ids.push(intern(&value)?);
                }
                index.insert(section, ids);
            }
            if let Some(previous) = indexes.get(&key) {
                if previous != &index {
                    return Err("conflicting builds in append transaction".into());
                }
                continue;
            }
            // Exact shared-ID overlap selects the closest base; same product has priority.
            let base = indexes.iter().max_by_key(|(key, old)| {
                let overlap: usize = index
                    .iter()
                    .map(|(section, ids)| {
                        let previous: std::collections::BTreeSet<_> =
                            old.get(section).into_iter().flatten().collect();
                        ids.iter().filter(|id| previous.contains(id)).count()
                    })
                    .sum();
                (
                    key.split_once('@')
                        .is_some_and(|(p, _)| p == manifest.flavour),
                    overlap,
                )
            });
            let empty = Index::new();
            let mut delta = difference(
                base.map(|(key, _)| key.clone()),
                base.map(|(_, old)| old).unwrap_or(&empty),
                &index,
            );
            delta.revision = Some(extraction);
            delta.ordinal = self
                .builds
                .iter()
                .filter(|(key, _)| game_build(key) == target)
                .map(|(_, d)| d.ordinal)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("extraction ordinal overflow")?;
            catalog.builds.insert(key.clone(), delta);
            indexes.insert(key, index);
        }
        if catalog.builds.is_empty() {
            return Ok(0);
        }
        let mut data = Vec::new();
        for (i, block) in entries.chunks(chunk_size).enumerate() {
            let mut raw = Vec::new();
            for bytes in block {
                raw.extend((bytes.len() as u32).to_le_bytes());
                raw.extend(bytes);
            }
            let compressed = zstd::stream::encode_all(&raw[..], 9)?;
            catalog.chunks.push(Chunk {
                first: first + (i * chunk_size) as u32,
                count: block.len() as u32,
                offset: data.len(),
                length: compressed.len(),
                raw: raw.len(),
            });
            data.extend(compressed);
        }
        let index = zstd::stream::encode_all(&rmp_serde::to_vec_named(&catalog)?[..], 9)?;
        let mut payload = (index.len() as u32).to_le_bytes().to_vec();
        payload.extend(index);
        payload.extend(data);
        let mut bytes = MAGIC.to_vec();
        bytes.extend([3, 0, 1, 0]);
        bytes.extend(Sha256::digest(&payload));
        bytes.extend(payload);
        std::fs::create_dir_all(directory)?;
        let path = directory.join(format!("{}.selus", hash(&bytes)));
        let temporary = directory.join(format!(".{}-{}.tmp", hash(&bytes), std::process::id()));
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let write = (|| -> Result<Self> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            let mut combined = files(directory)?;
            combined.push(temporary.clone());
            let candidate = Self::open_paths(combined)?;
            // Expand every index to catch invalid combined copy ranges, and
            // reconstruct pending manifests before making the segment visible.
            for key in candidate.builds.keys() {
                candidate.index(key)?;
            }
            for manifest in &manifests {
                if candidate.load_revision(
                    &manifest.target_string(),
                    &revision(manifest, version)?.id(),
                )? != **manifest
                {
                    return Err("staged manifest differs from pending build".into());
                }
            }
            if path.exists() {
                if std::fs::read(&path)? != bytes {
                    return Err("segment filename collision".into());
                }
                std::fs::remove_file(&temporary)?;
            } else {
                std::fs::rename(&temporary, &path)?;
            }
            candidate.indexes.borrow_mut().clear();
            candidate.chunks.borrow_mut().clear();
            Ok(candidate)
        })();
        if write.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        *self = write?;
        Ok(bytes.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unversioned_import_rechecks_stale_handle_and_rejects_whole_batch() {
        let root =
            std::env::temp_dir().join(format!("selu-import-admission-{}", std::process::id()));
        let old: crate::Manifest = serde_json::from_value(serde_json::json!({
            "flavour":"wow", "build":{"major":12,"minor":1,"patch":5,"build":1},
            "interface":120105
        }))
        .unwrap();
        let mut stale = super::Store::open(&root).unwrap();
        let mut writer = super::Store::default();
        writer.append(&root, std::slice::from_ref(&old)).unwrap();
        let mut current = old.clone();
        current.interface += 1;
        writer
            .append(&root, std::slice::from_ref(&current))
            .unwrap();
        let before = std::fs::read_dir(&root).unwrap().count();
        let mut new_build = old.clone();
        new_build.build.build += 1;
        let error = stale
            .import_unversioned(&root, &[new_build.clone(), old])
            .unwrap_err();
        assert!(error.to_string().contains("unversioned import"));
        let reopened = super::Store::open(&root).unwrap();
        assert_eq!(reopened.load(&current.target_string()).unwrap(), current);
        assert!(
            !reopened
                .builds()
                .any(|build| build == new_build.target_string())
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), before);
        assert_eq!(stale.import_unversioned(&root, &[current]).unwrap(), 0);
        assert!(
            stale
                .append_with_extraction_version(&root, &[new_build], 0)
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    use super::*;
    #[test]
    fn extraction_report_diffs_locked_build_and_never_moves_game_lock() {
        let root =
            std::env::temp_dir().join(format!("selu-extraction-report-{}", std::process::id()));
        let project = root.join("project");
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(
            project.join("selu.toml"),
            "[addon]\nname=\"Extraction\"\ntargets=[\"wow\"]\n",
        )
        .unwrap();
        std::fs::write(
            project.join("src/main.selu"),
            "const value = RemovedGlobal;\n",
        )
        .unwrap();
        let old:Manifest=serde_json::from_value(serde_json::json!({"flavour":"wow","build":{"major":12,"minor":1,"patch":5,"build":1},"interface":120105,"globals":[{"name":"RemovedGlobal","type":"number","evidence":"source"}]})).unwrap();
        let mut new = old.clone();
        new.globals.clear();
        let target = old.target_string();
        let mut lock = crate::lock::Lock::read(&project).unwrap();
        lock.resolve(&["wow".into()], std::slice::from_ref(&old))
            .unwrap();
        lock.write(&project).unwrap();
        let before = std::fs::read(project.join("selu.lock")).unwrap();
        let mut store = Store::default();
        store
            .append_with_extraction_version(&root.join("store"), &[old], 1)
            .unwrap();
        store
            .append_with_extraction_version(&root.join("store"), std::slice::from_ref(&new), 2)
            .unwrap();
        let mut other = new.clone();
        other.build.build = 99;
        store
            .append_with_extraction_version(&root.join("store"), &[other], 2)
            .unwrap();
        let report = store.extraction_report(&project).unwrap();
        assert!(
            report.contains("newer extraction: v1-")
                && report.contains(" -> v2-")
                && report.contains("globals: +0, -1")
                && report.contains("main.selu:1:")
                && report.contains("RemovedGlobal"),
            "{report}"
        );
        assert!(!report.contains("12.1.5.99"));
        assert_eq!(
            lock.resolve(&["wow".into()], &[store.load(&target).unwrap()])
                .unwrap(),
            [new]
        );
        assert_eq!(std::fs::read(project.join("selu.lock")).unwrap(), before);
        assert_eq!(lock.targets["wow"], target);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn extraction_versions_audit_and_older_writers_do_not_rollback() {
        let root = std::env::temp_dir().join(format!("selu-revision-order-{}", std::process::id()));
        let old:Manifest=serde_json::from_value(serde_json::json!({"flavour":"wow","build":{"major":12,"minor":1,"patch":5,"build":1},"interface":120105})).unwrap();
        let target = old.target_string();
        let mut store = Store::default();
        store
            .append_with_extraction_version(&root, std::slice::from_ref(&old), 1)
            .unwrap();
        let first = store.latest_revision(&target).unwrap();
        assert!(store.needs_extraction(&target, 2));
        // A contract bump is a new immutable extraction even with identical output.
        assert!(
            store
                .append_with_extraction_version(&root, std::slice::from_ref(&old), 2)
                .unwrap()
                > 0
        );
        let second = store.latest_revision(&target).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            first.split_once('-').unwrap().1,
            second.split_once('-').unwrap().1
        );
        assert!(!store.needs_extraction(&target, 2));
        let mut stale = old.clone();
        stale.interface += 1;
        store
            .append_with_extraction_version(&root, &[stale.clone()], 1)
            .unwrap();
        assert_eq!(store.latest_revision(&target).unwrap(), second);
        assert_eq!(store.load(&target).unwrap(), old);
        assert_eq!(
            store
                .load_revision(&target, &revision(&stale, 1).unwrap().id())
                .unwrap(),
            stale
        );
        assert_eq!(store.load_revision(&target, &first).unwrap(), old);
        assert!(store.load_revision(&target, "v99-missing").is_err());
        assert_eq!(
            store
                .append_with_extraction_version(&root, &[old], 2)
                .unwrap(),
            0
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn reextraction_appends_without_changing_build_pin_or_prior_bytes() {
        let root = std::env::temp_dir().join(format!("selu-reextract-{}", std::process::id()));
        let directory = root.join("store");
        let cache = root.join("cache");
        let old:Manifest=serde_json::from_value(serde_json::json!({"flavour":"wow","build":{"major":12,"minor":1,"patch":5,"build":1},"interface":120105})).unwrap();
        let mut new = old.clone();
        new.interface += 1;
        let target = old.target_string();
        let mut store = Store::default();
        store
            .append(&directory, std::slice::from_ref(&old))
            .unwrap();
        let initial = files(&directory).unwrap()[0].clone();
        let bytes = std::fs::read(&initial).unwrap();
        let original_revision = store.latest_revision(&target).unwrap();
        assert_eq!(
            load_cached_selected(&directory, &cache, Some(std::slice::from_ref(&target)))
                .unwrap()
                .as_slice(),
            std::slice::from_ref(&old)
        );
        store
            .append(&directory, std::slice::from_ref(&new))
            .unwrap();
        assert_eq!(store.builds().collect::<Vec<_>>(), [target.as_str()]);
        assert_eq!(store.load(&target).unwrap(), new);
        assert_eq!(
            load_cached_selected(&directory, &cache, Some(std::slice::from_ref(&target)))
                .unwrap()
                .as_slice(),
            std::slice::from_ref(&new)
        );
        assert_eq!(
            store.load_revision(&target, &original_revision).unwrap(),
            old
        );
        assert_eq!(store.revisions(&target).unwrap().len(), 2);
        assert_eq!(store.append(&directory, &[old]).unwrap(), 0);
        assert_eq!(store.load(&target).unwrap(), new);
        assert_eq!(std::fs::read(initial).unwrap(), bytes);
        assert_eq!(files(&directory).unwrap().len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn independent_writers_preserve_both_pending_builds() {
        let root = std::env::temp_dir().join(format!(
            "selu-writers-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let directory = root.join("store");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut workers = Vec::new();
        for build in [1, 2] {
            let mut writer = Store::open(&directory).unwrap();
            let directory = directory.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                let manifest: Manifest = serde_json::from_value(serde_json::json!({"flavour":"wow","build":{"major":12,"minor":1,"patch":5,"build":build},"interface":120105})).unwrap();
                barrier.wait();
                writer.append(&directory,&[manifest]).map_err(|e| e.to_string())
            }));
        }
        let outcomes: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        assert!(outcomes.iter().all(|r| r.is_ok()), "{outcomes:?}");
        let stored = Store::open(&directory).unwrap();
        assert_eq!(stored.builds().count(), 2);
        for build in [1, 2] {
            assert_eq!(
                stored
                    .load(&format!("wow@12.1.5.{build}"))
                    .unwrap()
                    .build
                    .build,
                build
            );
        }
        let before: Vec<_> = files(&directory)
            .unwrap()
            .into_iter()
            .map(|p| (p.clone(), std::fs::read(p).unwrap()))
            .collect();
        let mut noncanonical = stored.load("wow@12.1.5.1").unwrap();
        noncanonical.build.build = 3;
        for name in ["Z", "A"] {
            noncanonical.globals.push(
                serde_json::from_value(
                    serde_json::json!({"name":name,"evidence":"source","type":"number"}),
                )
                .unwrap(),
            );
        }
        let error = Store::open(&directory)
            .unwrap()
            .append(&directory, &[noncanonical])
            .unwrap_err()
            .to_string();
        assert!(error.contains("revision content checksum"), "{error}");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), before.len());
        for (p, bytes) in before {
            assert_eq!(std::fs::read(p).unwrap(), bytes);
        }
        assert!(Store::open(&directory).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn requested_loads_skip_unrelated_builds_and_repair_only_their_cache() {
        let root = std::env::temp_dir().join(format!("selu-selective-{}", std::process::id()));
        assert!(!root.exists());
        let a: Manifest = serde_json::from_value(serde_json::json!({"flavour":"wow","build":{"major":12,"minor":1,"patch":5,"build":1},"interface":120105,"globals":[{"name":"Shared","type":"number","evidence":"source"}]})).unwrap();
        let mut b = a.clone();
        b.build.build = 2;
        let mut c = a.clone();
        c.flavour = "wowt".into();
        c.build.build = 3;
        let directory = root.join("store");
        let cache = root.join("cache");
        let mut store = Store::default();
        store
            .append_with_chunk_size(&directory, &[a.clone(), b.clone(), c.clone()], 1)
            .unwrap();
        // Same-product base is preferred; another product still reuses the closest build.
        assert_eq!(
            store.builds[&store.latest[&b.target_string()]].base,
            Some(store.latest[&a.target_string()].clone())
        );
        assert!(
            store.builds[&store.latest[&c.target_string()]]
                .base
                .is_some()
        );
        assert_eq!(store.load(&b.target_string()).unwrap(), b);
        assert_eq!(store.indexes.borrow().len(), 2);
        let target = [b.target_string()];
        assert_eq!(
            load_cached_selected(&directory, &cache, Some(&target))
                .unwrap()
                .as_slice(),
            std::slice::from_ref(&b)
        );
        let cached = cache.join("v4").join(fingerprint(&directory).unwrap());
        assert_eq!(std::fs::read_dir(&cached).unwrap().count(), 2);
        let path = cached.join(format!("{}.selum", hash(target[0].as_bytes())));
        std::fs::write(&path, b"corrupt").unwrap();
        assert_eq!(
            load_cached_selected(&directory, &cache, Some(&target)).unwrap(),
            [b]
        );
        let reversed = root.join("reversed");
        Store::default()
            .append_with_chunk_size(&reversed, &[c, a], 1)
            .unwrap();
        // Adding a build never changes bytes of the original segment.
        let initial = files(&reversed).unwrap()[0].clone();
        let bytes = std::fs::read(&initial).unwrap();
        Store::open(&reversed)
            .unwrap()
            .append(&reversed, &[store.load(&target[0]).unwrap()])
            .unwrap();
        assert_eq!(std::fs::read(initial).unwrap(), bytes);
        assert!(
            std::fs::canonicalize(&root)
                .unwrap()
                .starts_with(std::fs::canonicalize(std::env::temp_dir()).unwrap())
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn direct_store_discovery_does_not_load_derived_caches_as_manifests() {
        let root = std::env::temp_dir().join(format!(
            "selu-cache-location-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let directory = root.join("manifests/store");
        let manifest: Manifest = serde_json::from_value(serde_json::json!({"flavour":"wow","build":{"major":12,"minor":1,"patch":5,"build":1},"interface":120105})).unwrap();
        Store::default()
            .append(&directory, std::slice::from_ref(&manifest))
            .unwrap();
        assert_eq!(
            crate::load_manifest_dir(&directory).unwrap().as_slice(),
            std::slice::from_ref(&manifest)
        );
        assert_eq!(
            crate::load_manifest_dir(&root.join("manifests")).unwrap(),
            [manifest]
        );
        assert!(
            std::fs::canonicalize(&root)
                .unwrap()
                .starts_with(std::fs::canonicalize(std::env::temp_dir()).unwrap())
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn requested_builds_select_one_latest_or_the_exact_pin() {
        let builds = ["wow@12.1.5.1", "wow@12.1.5.2", "wowf@1.60.1.1"];
        let ptr = ["wowt@9.2.5.43254", "wowt@9.2.0.43340"];
        assert_eq!(
            requested_builds(ptr.into_iter(), Some(&["wowt".into()])).unwrap(),
            std::collections::BTreeSet::from(["wowt@9.2.0.43340".into()])
        );
        assert_eq!(
            requested_builds(builds.into_iter(), Some(&["wow".into()])).unwrap(),
            std::collections::BTreeSet::from(["wow@12.1.5.2".into()])
        );
        assert_eq!(
            requested_builds(builds.into_iter(), Some(&["wow@12.1.5.1".into()])).unwrap(),
            std::collections::BTreeSet::from(["wow@12.1.5.1".into()])
        );
        assert_eq!(
            requested_builds(builds.into_iter(), Some(&["retail@12.1".into()])).unwrap(),
            std::collections::BTreeSet::from(["wow@12.1.5.2".into()])
        );
    }
    #[test]
    fn scattered_edits_reuse_unchanged_index_entries() {
        let original: Vec<_> = (0..100).collect();
        let mut updated = original.clone();
        updated[1] = 101;
        updated[98] = 102;
        let before = BTreeMap::from([("functions".into(), original)]);
        let after = BTreeMap::from([("functions".into(), updated)]);
        let delta = difference(Some("base".into()), &before, &after);
        assert_eq!(reconstruct(&delta, &before).unwrap(), after);
        let inserted: Vec<_> = delta.sections["functions"]
            .iter()
            .filter_map(|run| {
                if let Run::Insert(values) = run {
                    Some(values.as_slice())
                } else {
                    None
                }
            })
            .flatten()
            .copied()
            .collect();
        assert_eq!(inserted, [101, 102]);
    }
    #[test]
    fn source_file_insertions_do_not_duplicate_unchanged_records() {
        let root = std::env::temp_dir().join(format!(
            "selu-provenance-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first: Manifest = serde_json::from_value(serde_json::json!({"flavour":"wow","build":{"major":12,"minor":1,"patch":5,"build":1},"interface":120105,"sourceFiles":["old.lua"],"dataGlobals":[{"name":"Stable","source":0,"evidence":"source"}]})).unwrap();
        let mut second = first.clone();
        second.build.build += 1;
        second.source_files.insert(0, "new.lua".into());
        second.data_globals[0].source += 1;
        let mut store = Store::default();
        store.append(&root, std::slice::from_ref(&first)).unwrap();
        store.append(&root, std::slice::from_ref(&second)).unwrap();
        assert_eq!(
            store
                .entry_bytes()
                .unwrap()
                .iter()
                .filter(|v| rmp_serde::from_slice::<Value>(v)
                    .unwrap()
                    .get("name")
                    .and_then(Value::as_str)
                    == Some("Stable"))
                .count(),
            1
        );
        let reopened = Store::open(&root).unwrap();
        assert_eq!(reopened.load(&first.target_string()).unwrap(), first);
        assert_eq!(reopened.load(&second.target_string()).unwrap(), second);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn immutable_history_determinism_and_corruption() {
        let root = std::env::temp_dir().join(format!(
            "selu-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first: Manifest = serde_json::from_value(serde_json::json!({"flavour":"wow", "build":{"major":12,"minor":1,"patch":5,"build":1}, "interface":120105})).unwrap();
        let mut second = first.clone();
        second.build.build += 1;
        let mut store = Store::default();
        store.append(&root, std::slice::from_ref(&first)).unwrap();
        let initial = std::fs::read_dir(&root)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let bytes = std::fs::read(&initial).unwrap();
        store.append(&root, std::slice::from_ref(&second)).unwrap();
        assert_eq!(std::fs::read(&initial).unwrap(), bytes);
        assert_eq!(
            store.append(&root, std::slice::from_ref(&second)).unwrap(),
            0
        );
        let loaded = Store::open(&root).unwrap();
        assert_eq!(loaded.load(&first.target_string()).unwrap(), first);
        assert_eq!(loaded.load(&second.target_string()).unwrap(), second);
        let mut conflict = first.clone();
        conflict.interface += 1;
        let original_revision = store.latest_revision(&first.target_string()).unwrap();
        store
            .append(&root, std::slice::from_ref(&conflict))
            .unwrap();
        assert_eq!(store.load(&first.target_string()).unwrap(), conflict);
        assert_eq!(
            store
                .load_revision(&first.target_string(), &original_revision)
                .unwrap(),
            first
        );
        let duplicate = root.join("duplicate");
        let mut other = Store::default();
        other.append(&duplicate, &[first]).unwrap();
        let copy = std::fs::read_dir(&duplicate)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(std::fs::read(copy).unwrap(), bytes);
        let mut corrupt = bytes;
        *corrupt.last_mut().unwrap() ^= 1;
        std::fs::write(initial, corrupt).unwrap();
        assert!(Store::open(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

use crate::Result;
use std::{collections::HashMap, io::Read, path::Path};
pub fn slice(b: &[u8], p: usize, n: usize) -> Result<&[u8]> {
    b.get(p..p.checked_add(n).ok_or("offset overflow")?)
        .ok_or_else(|| "truncated data".into())
}
pub fn le(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(slice(b, p, 4)?.try_into()?))
}
fn be(b: &[u8], p: usize, n: usize) -> Result<u64> {
    Ok(slice(b, p, n)?
        .iter()
        .fold(0, |v, x| (v << 8) | u64::from(*x)))
}
#[derive(Debug)]
pub struct BuildInfo {
    pub product: String,
    pub version: String,
    pub build_key: String,
    pub active: bool,
}
pub fn parse_build_info(text: &str) -> Result<Vec<BuildInfo>> {
    let mut lines = text.lines();
    let header: Vec<_> = lines
        .next()
        .ok_or("missing build header")?
        .split('|')
        .map(|c| c.split('!').next().unwrap_or(c))
        .collect();
    let mut out = Vec::new();
    for line in lines {
        let row: Vec<_> = line.split('|').collect();
        let get = |name: &str| -> Result<&str> {
            let i = header
                .iter()
                .position(|c| *c == name)
                .ok_or("missing build column")?;
            row.get(i).copied().ok_or_else(|| "short build row".into())
        };
        out.push(BuildInfo {
            product: get("Product")?.into(),
            version: get("Version")?.into(),
            build_key: get("Build Key")?.into(),
            active: get("Active")? == "1",
        });
    }
    Ok(out)
}
pub struct IndexEntry {
    pub archive_number: u32,
    pub archive_offset: u64,
    pub size: u32,
}
pub struct CascIndex(HashMap<Vec<u8>, IndexEntry>);
impl CascIndex {
    pub fn load(dir: &Path) -> Result<Self> {
        let mut files = HashMap::new();
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            if name.len() == 14 && name.ends_with(".idx") {
                let bucket = u8::from_str_radix(&name[..2], 16)?;
                let version = u32::from_str_radix(&name[2..10], 16)?;
                let v = files.entry(bucket).or_insert((version, e.path()));
                if version > v.0 {
                    *v = (version, e.path());
                }
            }
        }
        let mut map = HashMap::new();
        for (_, path) in files.into_values() {
            let b = std::fs::read(path)?;
            let h = slice(&b, 0, 40)?;
            if h[8] != 7 || h[9] != 0 || h[12] != 4 || h[13] != 5 || h[14] != 9 || h[15] >= 40 {
                return Err("unsupported local index".into());
            }
            let bits = h[15];
            let n = le(&b, 32)? as usize;
            let entries = slice(&b, 40, n)?;
            if !n.is_multiple_of(18) {
                return Err("partial index entry".into());
            }
            for e in entries.as_chunks::<18>().0 {
                let raw = be(e, 9, 5)?;
                map.insert(
                    e[..9].to_vec(),
                    IndexEntry {
                        archive_number: (raw >> bits) as u32,
                        archive_offset: raw & ((1 << bits) - 1),
                        size: le(e, 14)?,
                    },
                );
            }
        }
        Ok(Self(map))
    }
    pub fn find(&self, key: &[u8]) -> Option<&IndexEntry> {
        self.0.get(key.get(..9)?)
    }
}
pub struct EncodingEntry {
    pub ekeys: Vec<[u8; 16]>,
}
pub struct EncodingFile(HashMap<[u8; 16], EncodingEntry>);
impl EncodingFile {
    pub fn parse(b: &[u8]) -> Result<Self> {
        let h = slice(b, 0, 22)?;
        if &h[..5] != b"EN\x01\x10\x10" {
            return Err("unsupported encoding header".into());
        }
        let count = be(b, 9, 4)? as usize;
        let size = be(b, 5, 2)? as usize * 1024;
        let start = 22 + be(b, 18, 4)? as usize + count * 32;
        let mut out = HashMap::new();
        for page in
            slice(b, start, count.checked_mul(size).ok_or("page overflow")?)?.chunks_exact(size)
        {
            let mut p = 0;
            while p < page.len() && page[p] != 0 {
                let n = page[p] as usize;
                let c = slice(page, p + 6, 16)?.try_into()?;
                let mut keys = Vec::new();
                for k in slice(page, p + 22, n * 16)?.as_chunks::<16>().0 {
                    keys.push(*k);
                }
                out.insert(c, EncodingEntry { ekeys: keys });
                p += 22 + n * 16;
            }
        }
        Ok(Self(out))
    }
    pub fn find_ekey(&self, k: &[u8; 16]) -> Option<&EncodingEntry> {
        self.0.get(k)
    }
}
pub struct RootEntry {
    pub ckey: [u8; 16],
    pub name_hash: Option<u64>,
    pub locale_flags: LocaleFlags,
}
#[derive(Clone, Copy)]
pub struct LocaleFlags(pub u32);
impl LocaleFlags {
    pub const EN_US: Self = Self(2);
    pub fn matches(self, other: Self) -> bool {
        self.0 == 0 || self.0 & other.0 != 0
    }
}
pub struct RootFile(Vec<(u32, RootEntry)>);
impl RootFile {
    pub fn parse(b: &[u8]) -> Result<Self> {
        let mut p = 0;
        let mut version = 0;
        if matches!(slice(b, 0, 4)?, b"TSFM" | b"MFST") {
            let h = le(b, 4)? as usize;
            if (12..=1024).contains(&h) {
                p = h;
                version = le(b, 8)?;
                if version != 1 && version != 2 {
                    return Err("unsupported root version".into());
                }
            } else {
                p = 12;
                version = 1;
            }
        }
        let mut out = Vec::new();
        while p < b.len() {
            let n = le(b, p)? as usize;
            let (flags, locale) = if version == 2 {
                let f = le(b, p + 8)? | le(b, p + 12)? | (u32::from(slice(b, p + 16, 1)?[0]) << 17);
                let l = le(b, p + 4)?;
                p += 17;
                (f, l)
            } else {
                let f = le(b, p + 4)?;
                let l = le(b, p + 8)?;
                p += 12;
                (f, l)
            };
            let named = flags & 0x10000000 == 0;
            let bytes = n
                .checked_mul(if named { 28 } else { 20 })
                .ok_or("root count overflow")?;
            slice(b, p, bytes)?;
            let mut id = 0i64;
            for i in 0..n {
                let delta = le(b, p + i * 4)? as i32 as i64;
                id = if i == 0 { delta } else { id + 1 + delta };
                let c = slice(b, p + n * 4 + i * 16, 16)?.try_into()?;
                let hash = if named {
                    Some(u64::from_le_bytes(
                        slice(b, p + n * 20 + i * 8, 8)?.try_into()?,
                    ))
                } else {
                    None
                };
                out.push((
                    u32::try_from(id)?,
                    RootEntry {
                        ckey: c,
                        name_hash: hash,
                        locale_flags: LocaleFlags(locale),
                    },
                ));
            }
            p += bytes;
        }
        Ok(Self(out))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn iter_all(&self) -> impl Iterator<Item = (u32, &RootEntry)> {
        self.0.iter().map(|(i, e)| (*i, e))
    }
    #[cfg(test)]
    pub fn find_by_fdid(&self, id: u32, locale: LocaleFlags) -> Option<&RootEntry> {
        self.0
            .iter()
            .find(|(i, e)| *i == id && e.locale_flags.matches(locale))
            .map(|(_, e)| e)
    }
}
#[cfg(test)]
pub fn decode_blte(b: &[u8]) -> Result<Vec<u8>> {
    decode_blte_with_keys(b, &crate::keys::Keyring::default())
}
pub fn decode_blte_with_keys(b: &[u8], keys: &crate::keys::Keyring) -> Result<Vec<u8>> {
    if slice(b, 0, 4)? != b"BLTE" {
        return Err("invalid BLTE magic".into());
    }
    let h = be(b, 4, 4)? as usize;
    if h == 0 {
        return block(slice(b, 8, b.len() - 8)?, keys, 0, 0);
    }
    if slice(b, 8, 1)? != [15] {
        return Err("unsupported BLTE table".into());
    }
    let count = be(b, 9, 3)? as usize;
    if h != 12 + count * 24 {
        return Err("invalid BLTE header size".into());
    }
    let mut p = h;
    let mut out = Vec::new();
    for i in 0..count {
        let n = be(b, 12 + i * 24, 4)? as usize;
        let expected = be(b, 16 + i * 24, 4)? as usize;
        let decoded = block(slice(b, p, n)?, keys, u32::try_from(i)?, 0)?;
        if decoded.len() != expected {
            return Err("BLTE size mismatch".into());
        }
        if out.len() + decoded.len() > 268435456 {
            return Err("BLTE output too large".into());
        }
        out.extend(decoded);
        p += n;
    }
    Ok(out)
}
fn block(b: &[u8], keys: &crate::keys::Keyring, index: u32, depth: usize) -> Result<Vec<u8>> {
    if depth > 4 {
        return Err("nested BLTE encryption limit exceeded".into());
    }
    match b.first() {
        Some(b'N') => Ok(b[1..].to_vec()),
        Some(b'Z') => {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(&b[1..])
                .take(268435457)
                .read_to_end(&mut out)?;
            if out.len() > 268435456 {
                return Err("BLTE output too large".into());
            }
            Ok(out)
        }
        Some(b'E') => {
            // This is an identifier, never the encryption key itself. Report
            // it without reading key material or attempting decryption.
            let length = usize::from(*b.get(1).ok_or("truncated encrypted BLTE header")?);
            let name = slice(b, 2, length)?;
            if name.len() != 8 {
                return Err("unsupported BLTE key identifier length".into());
            }
            let id = u64::from_le_bytes(name.try_into()?);
            if keys.is_empty() {
                return Err(format!("encrypted with key {id:016x}").into());
            }
            let at = 2 + length;
            let iv_len = usize::from(*b.get(at).ok_or("truncated BLTE IV")?);
            if !matches!(iv_len, 4 | 8) {
                return Err("unsupported BLTE IV length".into());
            }
            let iv = slice(b, at + 1, iv_len)?;
            let mut nonce = [0u8; 8];
            nonce[..iv_len].copy_from_slice(iv);
            for (byte, mask) in nonce[..4].iter_mut().zip(index.to_le_bytes()) {
                *byte ^= mask;
            }
            if slice(b, at + 1 + iv_len, 1)? != b"S" {
                return Err("unsupported BLTE cipher".into());
            }
            let start = at + 2 + iv_len;
            let mut decoded = slice(
                b,
                start,
                b.len()
                    .checked_sub(start)
                    .ok_or("truncated BLTE cipher data")?,
            )?
            .to_vec();
            keys.apply(id, nonce, &mut decoded)?;
            block(&decoded, keys, index, depth + 1)
        }
        _ => Err("unsupported BLTE block".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blte_raw_and_refusals() {
        assert_eq!(decode_blte(b"BLTE\0\0\0\0Nhello").unwrap(), b"hello");
        assert!(decode_blte(b"BLTE\0\0\0\0Ebad").is_err());
        assert!(decode_blte(b"BAD").is_err());
    }
    #[test]
    fn encrypted_blte_reports_only_its_key_identifier() {
        let mut bytes = b"BLTE\0\0\0\0E\x08".to_vec();
        bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let error = decode_blte(&bytes).unwrap_err().to_string();
        assert!(error.contains("0807060504030201"));
        assert!(!error.contains("not installed"));
        assert!(decode_blte(b"BLTE\0\0\0\0E\x08\x01").is_err());
    }
    #[test]
    fn synthetic_root() {
        let mut b = Vec::new();
        for v in [1u32, 0x10000000, 2, 42] {
            b.extend(v.to_le_bytes());
        }
        b.extend([7u8; 16]);
        let root = RootFile::parse(&b).unwrap();
        assert_eq!(
            root.find_by_fdid(42, LocaleFlags::EN_US).unwrap().ckey,
            [7; 16]
        );
        b.pop();
        assert!(RootFile::parse(&b).is_err());
    }
}

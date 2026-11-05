//! Deterministic MessagePack maps compressed with zstd level 3.
use serde::{Serialize, de::DeserializeOwned};
use std::io::Read;
const MAGIC: &[u8; 8] = b"SELUMAN\0";
const LIMIT: u64 = 256 * 1024 * 1024;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Input maps must have stable ordering. Structs use named maps.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let raw = rmp_serde::to_vec_named(value)?;
    if raw.len() as u64 > LIMIT {
        return Err("manifest exceeds 256 MiB limit".into());
    }
    let mut out = MAGIC.to_vec();
    out.extend(1u16.to_le_bytes()); // format version
    out.extend(1u16.to_le_bytes()); // schema version
    out.extend(zstd::stream::encode_all(&raw[..], 3)?);
    Ok(out)
}

/// Also accept readable JSON fixtures and legacy snapshots.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes
        .iter()
        .copied()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| b == b'{')
    {
        return Ok(serde_json::from_slice(bytes)?);
    }
    if bytes.len() < 12 {
        return Err("truncated compiled manifest header".into());
    }
    if &bytes[..8] != MAGIC {
        return Err("invalid compiled manifest magic".into());
    }
    let format = u16::from_le_bytes([bytes[8], bytes[9]]);
    let schema = u16::from_le_bytes([bytes[10], bytes[11]]);
    if format != 1 {
        return Err(format!("unsupported manifest format version {format}; expected 1").into());
    }
    if schema != 1 {
        return Err(format!("unsupported manifest schema version {schema}; expected 1").into());
    }
    let mut raw = Vec::new();
    zstd::stream::read::Decoder::new(&bytes[12..])?
        .take(LIMIT + 1)
        .read_to_end(&mut raw)?;
    if raw.len() as u64 > LIMIT {
        return Err("manifest exceeds 256 MiB limit".into());
    }
    let mut cursor = std::io::Cursor::new(&raw);
    let value = T::deserialize(&mut rmp_serde::Deserializer::new(&mut cursor))?;
    if cursor.position() != raw.len() as u64 {
        return Err("trailing data in compiled manifest".into());
    }
    Ok(value)
}

/// Lossless grouping used by the generator and materialized history caches.
pub fn encode_manifest(manifest: &crate::Manifest) -> Result<Vec<u8>> {
    let mut json = serde_json::to_value(manifest)?;
    let mut groups = std::collections::BTreeMap::<String, serde_json::Value>::new();
    let mut globals = Vec::new();
    for global in &manifest.globals {
        let mut entry = serde_json::to_value(global)?;
        if global.value.is_some() {
            globals.push(entry);
            continue;
        }
        entry.as_object_mut().unwrap().remove("name");
        let key = serde_json::to_string(&entry)?;
        let group = groups.entry(key).or_insert_with(|| {
            entry["names"] = serde_json::json!([]);
            entry
        });
        group["names"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!(global.name));
    }
    globals.extend(groups.into_values());
    json["globals"] = serde_json::Value::Array(globals);
    encode(&json)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_corruption_and_determinism() {
        let value = serde_json::json!({"false": false, "zero": 0});
        let bytes = encode(&value).unwrap();
        assert_eq!(bytes, encode(&value).unwrap());
        assert_eq!(decode::<serde_json::Value>(&bytes).unwrap(), value);
        for offset in [0, 8, 10] {
            let mut bad = bytes.clone();
            bad[offset] ^= 1;
            assert!(decode::<serde_json::Value>(&bad).is_err());
        }
        for end in [0, 8, 11, bytes.len() - 1] {
            assert!(decode::<serde_json::Value>(&bytes[..end]).is_err());
        }
        for version in [0u16, 2] {
            for offset in [8, 10] {
                let mut bad = bytes.clone();
                bad[offset..offset + 2].copy_from_slice(&version.to_le_bytes());
                assert!(
                    decode::<serde_json::Value>(&bad)
                        .unwrap_err()
                        .to_string()
                        .contains("version")
                );
            }
        }
    }
}

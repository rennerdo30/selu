//! User-supplied keys only. No providers, embedded keys, or discovery.
use crate::Result;
use salsa20::{
    Salsa20, SalsaCore,
    cipher::{KeyIvInit, StreamCipher, consts::U10},
};
use std::{collections::HashMap, path::Path};
use zeroize::Zeroizing;

#[derive(Default)]
pub struct Keyring(HashMap<u64, Zeroizing<Vec<u8>>>);
impl std::fmt::Debug for Keyring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Keyring([redacted])")
    }
}
impl Keyring {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            Zeroizing::new(std::fs::read_to_string(path).map_err(|_| "cannot read user key file")?);
        Self::parse(&text)
    }
    pub fn parse(text: &str) -> Result<Self> {
        let mut ring = Self::default();
        for (i, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let fields: Vec<_> = line.split_whitespace().collect();
            let invalid = || format!("invalid key file entry on line {}", i + 1);
            if fields.len() != 2 || fields[0].len() != 16 {
                return Err(invalid().into());
            }
            let name = u64::from_str_radix(fields[0], 16).map_err(|_| invalid())?;
            let key = Zeroizing::new(hex::decode(fields[1]).map_err(|_| invalid())?);
            if !matches!(key.len(), 16 | 32) || ring.0.contains_key(&name) {
                return Err(invalid().into());
            }
            ring.0.insert(name, key);
        }
        Ok(ring)
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(crate) fn apply(&self, name: u64, nonce: [u8; 8], data: &mut [u8]) -> Result<()> {
        let key = self
            .0
            .get(&name)
            .ok_or_else(|| format!("encrypted with key {name:016x}"))?;
        apply(key, &nonce, data)
    }
}
fn apply(key: &[u8], nonce: &[u8; 8], data: &mut [u8]) -> Result<()> {
    let mut cipher = if key.len() == 32 {
        Salsa20::new_from_slices(key, nonce).map_err(|_| "invalid cipher configuration")?
    } else {
        // Standard Salsa20/128 initialization. The maintained RustCrypto core
        // performs the rounds; the 128-bit variant repeats its key words.
        let mut state = Zeroizing::new([0u32; 16]);
        for (i, word) in b"expand 16-byte k".as_chunks::<4>().0.iter().enumerate() {
            state[i * 5] = u32::from_le_bytes(*word);
        }
        for (i, word) in key.as_chunks::<4>().0.iter().enumerate() {
            let w = u32::from_le_bytes(*word);
            state[1 + i] = w;
            state[11 + i] = w;
        }
        state[6] = u32::from_le_bytes(nonce[..4].try_into()?);
        state[7] = u32::from_le_bytes(nonce[4..].try_into()?);
        Salsa20::from_core(SalsaCore::<U10>::from_raw_state(*state))
    };
    cipher
        .try_apply_keystream(data)
        .map_err(|_| "cipher stream exhausted".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_keys_are_redacted_and_parser_errors_are_content_free() {
        let secret = hex::encode((0..16).map(|n| n * 7 + 3).collect::<Vec<u8>>());
        let ring = Keyring::parse(&format!("0123456789abcdef {secret}\n")).unwrap();
        assert!(!format!("{ring:?}").contains(&secret));
        let error = Keyring::parse(&format!("bad {secret}"))
            .unwrap_err()
            .to_string();
        assert_eq!(error, "invalid key file entry on line 1");
        assert!(
            Keyring::parse(&format!(
                "0123456789abcdef {secret}\n0123456789abcdef {secret}"
            ))
            .is_err()
        );
    }
    #[test]
    fn self_made_encrypted_blte_uses_each_block_nonce_and_explicit_keys_only() {
        use std::io::Write;
        for length in [16, 32] {
            let secret: Vec<u8> = (0..length).map(|n| (n * 5 + 11) as u8).collect();
            let ring =
                Keyring::parse(&format!("0123456789abcdef {}", hex::encode(secret))).unwrap();
            let mut blocks = Vec::new();
            for index in 0..2u32 {
                let mut z =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                z.write_all(b"self-written fixture").unwrap();
                let mut data = vec![b'Z'];
                data.extend(z.finish().unwrap());
                let mut nonce = [2, 4, 6, 8, 0, 0, 0, 0];
                for (byte, mask) in nonce[..4].iter_mut().zip(index.to_le_bytes()) {
                    *byte ^= mask;
                }
                ring.apply(0x0123456789abcdef, nonce, &mut data).unwrap();
                let mut block = vec![b'E', 8];
                block.extend(0x0123456789abcdefu64.to_le_bytes());
                block.extend([4, 2, 4, 6, 8, b'S']);
                block.extend(data);
                blocks.push(block);
            }
            let mut file = b"BLTE".to_vec();
            file.extend(60u32.to_be_bytes());
            file.extend([15, 0, 0, 2]);
            for block in &blocks {
                file.extend((block.len() as u32).to_be_bytes());
                file.extend(20u32.to_be_bytes());
                file.extend([0; 16]);
            }
            for block in blocks {
                file.extend(block);
            }
            assert_eq!(
                crate::local::decode_blte_with_keys(&file, &ring).unwrap(),
                b"self-written fixtureself-written fixture"
            );
            assert!(
                crate::local::decode_blte(&file)
                    .unwrap_err()
                    .to_string()
                    .contains("0123456789abcdef")
            );
            let wrong =
                Keyring::parse(&format!("0123456789abcdef {}", "01".repeat(length))).unwrap();
            assert!(crate::local::decode_blte_with_keys(&file, &wrong).is_err());
        }
    }
}

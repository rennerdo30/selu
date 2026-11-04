//! Short, warm-cache format comparison; one pass per file and format.
use selu_manifest::Manifest;
use std::{
    io::{Read, Write},
    time::Instant,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("file,format,bytes,load_parse_ms,delta_zlib_bytes");
    for entry in std::fs::read_dir("manifests")? {
        let dir = entry?.path();
        if !dir.is_dir() || dir.ends_with("fixtures") {
            continue;
        }
        for file in std::fs::read_dir(dir)? {
            let file = file?.path();
            if file.extension().is_none_or(|x| x != "selum") {
                continue;
            }
            let value: serde_json::Value = selu_manifest::format::decode(&std::fs::read(&file)?)?;
            let mut changed = value.clone();
            changed["interface"] = serde_json::json!(value["interface"].as_u64().unwrap() + 1);
            for format in [
                "original",
                "pretty",
                "minified",
                "gzip",
                "zstd",
                "msgpack",
                "msgpack-zstd",
            ] {
                let encode =
                    |v: &serde_json::Value| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
                        let raw = match format {
                            "original" => {
                                let mut raw = serde_json::to_vec(v)?;
                                raw.push(b'\n');
                                raw
                            }
                            "pretty" => serde_json::to_vec_pretty(v)?,
                            "msgpack" | "msgpack-zstd" => rmp_serde::to_vec_named(v)?,
                            _ => serde_json::to_vec(v)?,
                        };
                        Ok(match format {
                            "zstd" | "msgpack-zstd" => zstd::stream::encode_all(&raw[..], 3)?,
                            "gzip" => {
                                let mut e = flate2::write::GzEncoder::new(
                                    Vec::new(),
                                    flate2::Compression::default(),
                                );
                                e.write_all(&raw)?;
                                e.finish()?
                            }
                            _ => raw,
                        })
                    };
                let bytes = encode(&value)?;
                let next = encode(&changed)?;
                let prefix = bytes.iter().zip(&next).take_while(|(a, b)| a == b).count();
                let suffix = bytes[prefix..]
                    .iter()
                    .rev()
                    .zip(next[prefix..].iter().rev())
                    .take_while(|(a, b)| a == b)
                    .count();
                let mut delta =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                delta.write_all(&next[prefix..next.len() - suffix])?;
                let growth = delta.finish()?.len() + 24;
                let temp = std::env::temp_dir().join(format!("selu-z-{}", std::process::id()));
                std::fs::write(&temp, &bytes)?;
                let start = Instant::now();
                let raw = std::fs::read(&temp)?;
                let raw = match format {
                    "zstd" | "msgpack-zstd" => zstd::stream::decode_all(&raw[..])?,
                    "gzip" => {
                        let mut out = Vec::new();
                        flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut out)?;
                        out
                    }
                    _ => raw,
                };
                let m: Manifest = if format.starts_with("msgpack") {
                    rmp_serde::from_slice(&raw)?
                } else if format == "original" {
                    serde_json::from_str(std::str::from_utf8(&raw)?)?
                } else {
                    serde_json::from_slice(&raw)?
                };
                std::hint::black_box(&m);
                let ms = start.elapsed().as_secs_f64() * 1000.;
                std::fs::remove_file(temp)?;
                println!(
                    "{},{format},{},{ms:.3},{growth}",
                    file.display(),
                    bytes.len()
                );
            }
        }
    }
    Ok(())
}

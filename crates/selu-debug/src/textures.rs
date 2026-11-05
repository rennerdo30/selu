//! Sandboxed addon media. Failed lookups are cached by Studio too.
use std::path::{Component, Path, PathBuf};
pub fn resolve(roots: &[PathBuf], name: &str) -> Result<PathBuf, String> {
    let normalized = name.replace('\\', "/");
    let mut relative = normalized.as_str();
    for prefix in ["Interface/AddOns/", "Interface/Addon/"] {
        if relative
            .to_ascii_lowercase()
            .starts_with(&prefix.to_ascii_lowercase())
        {
            relative = &relative[prefix.len()..];
            break;
        }
    }
    let path = Path::new(relative);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("outside addon sandbox".into());
    }
    for root in roots {
        let Ok(root) = root.canonicalize() else {
            continue;
        };
        let local = path
            .strip_prefix(root.file_name().unwrap_or_default())
            .unwrap_or(path);
        let candidate = root.join(local);
        for candidate in std::iter::once(candidate.clone())
            .chain(["tga", "png", "blp"].map(|e| candidate.with_extension(e)))
        {
            if let Ok(file) = candidate.canonicalize()
                && file.starts_with(&root)
                && file.is_file()
            {
                return Ok(file);
            }
        }
    }
    Err("unavailable asset".into())
}
pub fn load(roots: &[PathBuf], name: &str) -> Result<eframe::egui::ColorImage, String> {
    let file = resolve(roots, name)?;
    if std::fs::metadata(&file).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
        return Err("texture exceeds 32 MiB".into());
    }
    if file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("blp"))
    {
        return blp(&std::fs::read(file).map_err(|e| e.to_string())?);
    }
    let mut reader = image::ImageReader::open(file).map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
    Ok(eframe::egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

/// BLP2 palette and uncompressed BGRA mip zero. Unsupported encodings are explicit.
fn blp(bytes: &[u8]) -> Result<eframe::egui::ColorImage, String> {
    let read = |offset: usize| -> Result<u32, String> {
        let b = bytes.get(offset..offset + 4).ok_or("truncated BLP")?;
        Ok(u32::from_le_bytes(b.try_into().unwrap()))
    };
    if bytes.get(..4) != Some(b"BLP2") {
        return Err("unsupported BLP version".into());
    }
    let encoding = *bytes.get(8).ok_or("truncated BLP")?;
    let alpha = bytes[9];
    let (w, h) = (read(12)? as usize, read(16)? as usize);
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err("invalid BLP dimensions".into());
    }
    let start = read(20)? as usize;
    let len = read(84)? as usize;
    let data = bytes
        .get(start..start.checked_add(len).ok_or("invalid BLP offset")?)
        .ok_or("truncated BLP mip")?;
    let n = w * h;
    let mut rgba = Vec::with_capacity(n * 4);
    match encoding {
        1 => {
            let palette = bytes.get(148..1172).ok_or("truncated BLP palette")?;
            let indexes = data.get(..n).ok_or("truncated BLP indexes")?;
            let alpha_data = &data[n..];
            for (i, index) in indexes.iter().enumerate() {
                let c = &palette[*index as usize * 4..][..4];
                let a = match alpha {
                    0 => 255,
                    1 => {
                        if alpha_data.get(i / 8).ok_or("truncated BLP alpha")? & (1 << (i % 8)) != 0
                        {
                            255
                        } else {
                            0
                        }
                    }
                    4 => {
                        ((alpha_data.get(i / 2).ok_or("truncated BLP alpha")? >> ((i % 2) * 4))
                            & 15)
                            * 17
                    }
                    8 => *alpha_data.get(i).ok_or("truncated BLP alpha")?,
                    _ => return Err("unsupported BLP alpha depth".into()),
                };
                rgba.extend_from_slice(&[c[2], c[1], c[0], a]);
            }
        }
        3 => {
            for c in data
                .get(..n * 4)
                .ok_or("truncated BLP pixels")?
                .chunks_exact(4)
            {
                rgba.extend_from_slice(&[c[2], c[1], c[0], c[3]]);
            }
        }
        2 => {
            // BLP2 stores the same DXT blocks as DDS, without the DDS header.
            let fourcc = match bytes[10] {
                0 => b"DXT1",
                1 => b"DXT3",
                7 => b"DXT5",
                _ => return Err("unsupported BLP alpha encoding".into()),
            };
            let mut dds = vec![0u8; 128];
            dds[..4].copy_from_slice(b"DDS ");
            for (offset, value) in [
                (4, 124u32),
                (8, 0x81007),
                (12, h as u32),
                (16, w as u32),
                (20, len as u32),
                (76, 32),
                (80, 4),
                (108, 0x1000),
            ] {
                dds[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
            dds[84..88].copy_from_slice(fourcc);
            dds.extend_from_slice(data);
            let image = image::load_from_memory_with_format(&dds, image::ImageFormat::Dds)
                .map_err(|e| e.to_string())?
                .to_rgba8();
            return Ok(eframe::egui::ColorImage::from_rgba_unmultiplied(
                [w, h],
                image.as_raw(),
            ));
        }
        _ => return Err("unsupported BLP encoding".into()),
    }
    Ok(eframe::egui::ColorImage::from_rgba_unmultiplied(
        [w, h],
        &rgba,
    ))
}
#[cfg(test)]
mod tests {
    #[test]
    fn fixture_texture_is_real_and_paths_are_sandboxed() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../selu-sim/tests/fixtures/rendering");
        let image = super::load(std::slice::from_ref(&root), "checker.tga").unwrap();
        assert_eq!(image.size, [2, 2]);
        assert_eq!(image.pixels[0], eframe::egui::Color32::RED);
        assert!(super::resolve(std::slice::from_ref(&root), "../Cargo.toml").is_err());
        assert!(super::resolve(&[root], "Interface/Buttons/UI-Panel-Button-Up").is_err());
    }
    #[test]
    fn blp_pixel_and_truncation() {
        let mut b = vec![0u8; 152];
        b[..4].copy_from_slice(b"BLP2");
        b[8] = 3;
        for (p, v) in [(12, 1u32), (16, 1), (20, 148), (84, 4)] {
            b[p..p + 4].copy_from_slice(&v.to_le_bytes());
        }
        b[148..].copy_from_slice(&[3, 2, 1, 255]);
        let image = super::blp(&b).unwrap();
        assert_eq!(image.pixels[0], eframe::egui::Color32::from_rgb(1, 2, 3));
        assert!(super::blp(&b[..150]).is_err());
    }
}

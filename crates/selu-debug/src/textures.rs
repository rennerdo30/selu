//! Sandboxed addon media. Failed lookups are cached by Studio too.
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::path::{Component, Path};
#[cfg(test)]
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
#[cfg(test)]
pub fn load(roots: &[PathBuf], name: &str) -> Result<eframe::egui::ColorImage, String> {
    let mut resolver = selu_sim::assets::Resolver::default();
    for root in roots {
        if let Ok(root) = root.canonicalize() {
            resolver.add_addon(root);
        }
    }
    load_resolved(&resolver, name)
}
pub fn load_resolved(
    resolver: &selu_sim::assets::Resolver,
    name: &str,
) -> Result<eframe::egui::ColorImage, String> {
    let bytes = match resolver.lookup(name) {
        selu_sim::assets::Lookup::Found(bytes) => bytes,
        selu_sim::assets::Lookup::Unavailable => return Err("unavailable client asset".into()),
        selu_sim::assets::Lookup::Missing => return Err("missing addon asset".into()),
    };
    if bytes.starts_with(b"BLP") {
        return blp(&bytes);
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    reader.limits(limits);
    if reader.format().is_none() {
        reader.set_format(image::ImageFormat::Tga);
    }
    let image = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
    Ok(eframe::egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

/// BLP2 palette and uncompressed BGRA mip zero. Unsupported encodings are explicit.
fn blp(bytes: &[u8]) -> Result<eframe::egui::ColorImage, String> {
    let image = selu_gamedata::blp::decode(bytes)?;
    Ok(eframe::egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
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

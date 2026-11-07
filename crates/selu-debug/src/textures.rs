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

/// Rasterize a mask in the target region's coordinate space. Mask regions are
/// inputs to the texture pipeline, not independently painted UI regions.
pub fn mask_alpha(
    output: &mut eframe::egui::ColorImage,
    image: &eframe::egui::ColorImage,
    bounds: [f64; 4],
    uv: [f32; 4],
) {
    let [left, top, width, height] = bounds;
    for y in 0..output.size[1] {
        for x in 0..output.size[0] {
            let u = ((x as f64 + 0.5) / output.size[0] as f64 - left) / width;
            let v = ((y as f64 + 0.5) / output.size[1] as f64 - top) / height;
            let alpha = if width <= 0.0
                || height <= 0.0
                || !(0.0..1.0).contains(&u)
                || !(0.0..1.0).contains(&v)
            {
                0
            } else {
                let sx = ((f64::from(uv[0]) + u * f64::from(uv[1] - uv[0])) * image.size[0] as f64)
                    .floor()
                    .clamp(0.0, image.size[0].saturating_sub(1) as f64)
                    as usize;
                let sy = ((f64::from(uv[2]) + v * f64::from(uv[3] - uv[2])) * image.size[1] as f64)
                    .floor()
                    .clamp(0.0, image.size[1].saturating_sub(1) as f64)
                    as usize;
                image.pixels[sy * image.size[0] + sx].a()
            };
            let at = y * output.size[0] + x;
            let old = output.pixels[at].a();
            output.pixels[at] = eframe::egui::Color32::from_white_alpha(
                (u16::from(old) * u16::from(alpha) / 255) as u8,
            );
        }
    }
}

#[cfg(test)]
mod mask_tests {
    #[test]
    fn masks_multiply_alpha_in_relative_bounds_and_crop_uvs() {
        use eframe::egui::{Color32, ColorImage};
        let mask = ColorImage::new([2, 1], vec![Color32::TRANSPARENT, Color32::WHITE]);
        let mut output = ColorImage::filled([4, 1], Color32::WHITE);
        super::mask_alpha(
            &mut output,
            &mask,
            [0.5, 0.0, 0.5, 1.0],
            [0.0, 1.0, 0.0, 1.0],
        );
        assert_eq!(
            output.pixels.iter().map(Color32::a).collect::<Vec<_>>(),
            vec![0, 0, 0, 255]
        );
        super::mask_alpha(
            &mut output,
            &mask,
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 0.5, 0.0, 1.0],
        );
        assert!(output.pixels.iter().all(|p| p.a() == 0));
    }
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

/// Desaturate source pixels before vertex tint is applied, preserving alpha.
pub fn desaturate(image: &mut eframe::egui::ColorImage) {
    for pixel in &mut image.pixels {
        let [r, g, b, a] = pixel.to_srgba_unmultiplied();
        let gray = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32).round() as u8;
        *pixel = eframe::egui::Color32::from_rgba_unmultiplied(gray, gray, gray, a);
    }
}
#[cfg(test)]
mod w3_tests {
    #[test]
    fn w3_desaturate_pixels_preserves_alpha() {
        use eframe::egui::{Color32, ColorImage};
        let mut image = ColorImage::filled([1, 1], Color32::from_rgb(255, 0, 0));
        super::desaturate(&mut image);
        assert_eq!(image.pixels[0], Color32::from_rgb(76, 76, 76));
        let mut translucent =
            ColorImage::filled([1, 1], Color32::from_rgba_unmultiplied(255, 0, 0, 128));
        super::desaturate(&mut translucent);
        let [r, g, b, a] = translucent.pixels[0].to_srgba_unmultiplied();
        assert_eq!((r, g, b, a), (r, r, r, 128));
        let color = super::desaturated_color([1.0, 0.0, 0.0, 0.5], true);
        assert_eq!(color, [0.299, 0.299, 0.299, 0.5]);
    }
}

pub fn desaturated_color(mut color: [f32; 4], enabled: bool) -> [f32; 4] {
    if enabled {
        let gray = color[0] * 0.299 + color[1] * 0.587 + color[2] * 0.114;
        color[0] = gray;
        color[1] = gray;
        color[2] = gray;
    }
    color
}

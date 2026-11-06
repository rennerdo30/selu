pub fn decode(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    if bytes.len() < 148 {
        return Err("truncated BLP header".into());
    }
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
                .as_chunks::<4>()
                .0
                .iter()
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
            return Ok(image);
        }
        _ => return Err("unsupported BLP encoding".into()),
    }
    image::RgbaImage::from_raw(w as u32, h as u32, rgba)
        .ok_or_else(|| "invalid image length".into())
}
#[cfg(test)]
mod tests {
    #[test]
    fn bgra_and_truncation() {
        let mut b = vec![0; 152];
        b[..4].copy_from_slice(b"BLP2");
        b[8] = 3;
        for (p, v) in [(12, 1u32), (16, 1), (20, 148), (84, 4)] {
            b[p..p + 4].copy_from_slice(&v.to_le_bytes());
        }
        b[148..].copy_from_slice(&[10, 20, 30, 255]);
        assert_eq!(super::decode(&b).unwrap().as_raw(), &[30, 20, 10, 255]);
        for n in 0..b.len() {
            assert!(super::decode(&b[..n]).is_err());
        }
    }
}

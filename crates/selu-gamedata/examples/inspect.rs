use selu_gamedata::{Result, Storage};
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    if a.len() < 3 {
        return Err("usage: inspect INSTALL PRODUCT info|cat [PATH|FDID] [OUTPUT]".into());
    }
    let s = Storage::open(std::path::Path::new(&a[0]), &a[1], None)?;
    if a[2] == "info" {
        println!(
            "{} {} {} entries",
            s.build.product,
            s.build.version,
            s.entry_count()
        );
        return Ok(());
    }
    let name = a.get(3).ok_or("missing file identifier")?;
    if a[2] == "atlas" {
        let atlas = s.atlas(name)?;
        println!("{atlas:#?}");
        if let Some(out) = a.get(4) {
            let image = selu_gamedata::blp::decode(&s.read(atlas.file_data_id)?)?;
            image::imageops::crop_imm(
                &image,
                atlas.left,
                atlas.top,
                atlas.right - atlas.left,
                atlas.bottom - atlas.top,
            )
            .to_image()
            .save(out)?;
        }
        return Ok(());
    }
    let bytes = match name.parse::<u32>() {
        Ok(id) => s.read(id)?,
        Err(_) => s.read_path(name)?,
    };
    if let Some(output) = a.get(4) {
        if output.ends_with(".png") {
            selu_gamedata::blp::decode(&bytes)?.save(output)?;
        } else {
            std::fs::write(output, &bytes)?;
        }
    }
    println!("{name}: {} bytes", bytes.len());
    Ok(())
}

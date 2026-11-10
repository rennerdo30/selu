//! Embeds the packages under `<repo>/packages/` (docs/lang/packages.md), so
//! `selu.exe` alone carries every package's source.
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            // Package tests stay out of the compiler.
            if rel != "tests" {
                collect(&path, root, out);
            }
        } else if rel == "package.toml" || (rel.starts_with("src/") && rel.ends_with(".selu")) {
            out.push((rel, path));
        }
    }
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let packages = manifest_dir.join("../../packages");
    println!("cargo:rerun-if-changed={}", packages.display());
    let mut code = String::from("pub static EMBEDDED: &[(&str, &str, &str)] = &[\n");
    if let Ok(entries) = std::fs::read_dir(&packages) {
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries.into_iter().filter(|e| e.path().is_dir()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let mut files = Vec::new();
            collect(&entry.path(), &entry.path(), &mut files);
            for (rel, path) in files {
                println!("cargo:rerun-if-changed={}", path.display());
                let absolute = path.canonicalize().unwrap();
                writeln!(
                    code,
                    "    ({name:?}, {rel:?}, include_str!({:?})),",
                    absolute.to_string_lossy()
                )
                .unwrap();
            }
        }
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("packages.rs");
    std::fs::write(out, code).unwrap();
}

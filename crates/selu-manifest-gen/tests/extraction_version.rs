//! Frozen extraction contracts. Preserve previous entries; a source/overlay change
//! requires a new EXTRACTION_VERSION and a new digest, never replacing the old one.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const CONTRACTS: &[(u32, &str)] = &[
    (
        1,
        "65607550cd6a16c24eb6c75bedc4211ab3e71749d9135fb6e0d7c5e52df2fa8c",
    ),
    (
        2,
        "f84cf31b73fe2bbc63e9171ea7bca0efe2de4028ae3e58230d0d7095e1efa8b7",
    ),
    (
        3,
        "354b4541b86c03a5ee4d28660d9135ec365537f965cef033672aa0cf1576aea6",
    ),
    (
        4,
        "cfebb4fabdb14b597b7e1ca9ed4a81859e537ce539ff03a0cc16803362ac9006",
    ),
    (
        5,
        "9c47aa2b30292afb33a54168ad54d8bcad8908ef3c6287e68eacb3bf58d661c6",
    ),
    (
        6,
        "7401a317d8a4cfe87273b452029fe74ba4a1bce636c20003477c82f665758cf8",
    ),
    (
        7,
        "8be8a86335ffdbf4c30f6b99a6665a710a132b0cd3da2d00352f719aadd2722f",
    ),
    (
        8,
        "8ed1136d5e4f71f4c0cd857ceda5c69493c4215e6b6e7970169721e843d4223d",
    ),
    (
        9,
        "d73944069bb36d1bb63f74c0152275d872ac5238a66fb030249d0f036b723a2d",
    ),
    (
        10,
        "5fe7249d85501f6b1708ab450485361a6516b981d045124c159a920eb8ea5a7c",
    ),
];

fn collect(directory: &Path, files: &mut Vec<PathBuf>, accept: fn(&Path) -> bool) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files, accept);
        } else if accept(&path) {
            files.push(path);
        }
    }
}

#[test]
fn extraction_sources_and_overlays_require_version_bump() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    // Discover modules and overlays, so adding a file also changes the contract.
    collect(
        &root.join("crates/selu-manifest-gen/src"),
        &mut files,
        |p| p.extension().is_some_and(|e| e == "rs"),
    );
    collect(&root.join("manifests"), &mut files, |p| {
        p.file_name().is_some_and(|n| n == "overlay.toml")
    });
    files.extend([
        root.join("crates/selu-manifest/src/lib.rs"),
        root.join("crates/selu-manifest/src/format.rs"),
        root.join("crates/selu-manifest/src/resolution.rs"),
        root.join("crates/selu-manifest-gen/Cargo.toml"),
    ]);
    files.sort();
    let mut hash = Sha256::new();
    for path in files {
        let name = path
            .strip_prefix(&root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let mut content = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\r\n", "\n");
        if name == "crates/selu-manifest-gen/src/main.rs" {
            // CLI imports/reports are not extraction rules. Cover generation and
            // its git/archive helpers, branch identity, and product/overlay setup.
            let generation = content
                .split_once("// Verified against the exporter's")
                .unwrap()
                .0;
            let clients = content
                .split_once("const CLIENTS:")
                .unwrap()
                .1
                .split_once("#[cfg(test)]")
                .unwrap()
                .0;
            let product = content
                .split_once("fn extract_product(")
                .unwrap()
                .1
                .split_once("fn retained_reextractions(")
                .unwrap()
                .0;
            content =
                format!("{generation}\nconst CLIENTS:{clients}\nfn extract_product({product}");
        }
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((content.len() as u64).to_le_bytes());
        hash.update(content.as_bytes());
    }
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    let version = selu_manifest_gen::EXTRACTION_VERSION;
    let expected = CONTRACTS
        .iter()
        .find(|(v, _)| *v == version)
        .expect("add a frozen digest for the new EXTRACTION_VERSION")
        .1;
    assert_eq!(
        digest, expected,
        "extraction sources/overlays changed without an extraction version bump; bump EXTRACTION_VERSION and append its digest to CONTRACTS, preserving old entries"
    );
}

use selu_manifest::{Manifest, load_manifest};
#[test]
fn compiled_loader_preserves_json_fixture() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../manifests/fixtures/forever-1.0.0.1000.json");
    let expected = load_manifest(&fixture).unwrap();
    let mut bytes = b"SELUMAN\0\x01\0\x01\0".to_vec();
    bytes.extend(
        zstd::stream::encode_all(&rmp_serde::to_vec_named(&expected).unwrap()[..], 3).unwrap(),
    );
    let path = std::env::temp_dir().join(format!("selu-z-test-{}.selum", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let result = load_manifest(&path);
    std::fs::remove_file(path).unwrap();
    let actual: Manifest = result.expect("compiled manifest must load");
    assert_eq!(actual, expected);
}

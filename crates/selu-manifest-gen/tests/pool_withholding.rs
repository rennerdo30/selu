use selu_manifest::{Manifest, store::load_build};

#[test]
fn modified_pool_source_withholds_contracts_without_blocking_extraction() {
    let mut m = load_build(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store"),
        "wow@12.1.5.70391",
    )
    .unwrap();
    let original_tooltip = m
        .widgets
        .iter()
        .find(|w| w.name == "GameTooltip")
        .unwrap()
        .clone();
    let changed = "function CreateFramePool(frameType) return {} end -- new release\n";
    selu_manifest_gen::pools::extend(
        &mut m,
        &[(
            "Interface/AddOns/Blizzard_SharedXMLBase/Pools.lua".into(),
            changed.into(),
        )],
    )
    .unwrap();
    assert!(m.function("CreateFramePool").unwrap().signature.is_none());
    assert!(!m.widgets.iter().any(|w| w.name.starts_with("W3")));
    assert_eq!(
        m.widgets.iter().find(|w| w.name == "GameTooltip").unwrap(),
        &original_tooltip
    );
    assert!(
        selu_manifest_gen::pools::warning_summary(&m)
            .starts_with("WARNING: pool contracts withheld")
    );
    let bytes = selu_manifest_gen::encode(&mut m).unwrap();
    let restored: Manifest = selu_manifest::format::decode(&bytes).unwrap();
    let value = serde_json::to_value(&restored).unwrap();
    let warnings = value["source"]["warnings"].as_array().unwrap();
    assert!(warnings.iter().any(|w| {
        w.as_str()
            .unwrap()
            .contains("pool contracts withheld for wow@12.1.5.70391: Pools.lua changed")
    }));
    let directory = std::env::temp_dir().join(format!("selu-pool-warning-{}", std::process::id()));
    assert!(!directory.exists());
    let mut store = selu_manifest::store::Store::default();
    store
        .append(&directory, std::slice::from_ref(&restored))
        .unwrap();
    let stored = selu_manifest::store::Store::open(&directory)
        .unwrap()
        .load(&restored.target_string())
        .unwrap();
    assert_eq!(
        selu_manifest_gen::pools::warning_summary(&stored),
        selu_manifest_gen::pools::warning_summary(&restored)
    );
    // Only files in the new, uniquely owned test directory are removed.
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        assert!(path.is_file());
        std::fs::remove_file(path).unwrap();
    }
    std::fs::remove_dir(&directory).unwrap();
    std::fs::remove_file(directory.with_extension("lock")).unwrap();
}

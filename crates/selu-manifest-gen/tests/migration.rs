use selu_manifest::{Manifest, store::Store};
use std::{path::Path, process::Command, time::Duration};

fn migrate(root: &Path) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_selu-manifest-gen"))
        .arg("migrate")
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    eprintln!("migration PID {}; timeout 30 seconds", child.id());
    let start = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(30) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("migration timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn obsolete_unversioned_import_cannot_replace_current_extraction() {
    let root = std::env::temp_dir().join(format!("selu-migrate-conflict-{}", std::process::id()));
    let destination = root.join("manifests/store");
    let current: Manifest = serde_json::from_value(serde_json::json!({
        "flavour":"wow", "build":{"major":12,"minor":1,"patch":5,"build":1},
        "interface":120105
    }))
    .unwrap();
    let mut store = Store::default();
    store
        .append(&destination, std::slice::from_ref(&current))
        .unwrap();
    let original = store.latest_revision(&current.target_string()).unwrap();
    let prior: Vec<_> = std::fs::read_dir(&destination)
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    let mut obsolete = current.clone();
    obsolete.interface -= 1;
    // As in the review, source commit and game build are unchanged.
    obsolete.flavour = "retail".into();
    std::fs::write(
        root.join("manifests/old.selum"),
        selu_manifest::format::encode(&obsolete).unwrap(),
    )
    .unwrap();
    let result = migrate(&root);
    assert!(
        !result.status.success(),
        "obsolete import unexpectedly succeeded"
    );
    let message = String::from_utf8(result.stderr).unwrap();
    assert!(
        message.contains("unversioned import") && message.contains(&current.target_string()),
        "{message}"
    );
    let reopened = Store::open(&destination).unwrap();
    assert_eq!(reopened.load(&current.target_string()).unwrap(), current);
    assert_eq!(
        reopened.latest_revision(&current.target_string()).unwrap(),
        original
    );
    assert_eq!(
        std::fs::read_dir(&destination).unwrap().count(),
        prior.len()
    );
    for (path, bytes) in prior {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    // An identical import is harmless and cannot create a new revision.
    std::fs::write(
        root.join("manifests/old.selum"),
        selu_manifest::format::encode(&current).unwrap(),
    )
    .unwrap();
    assert!(migrate(&root).status.success());
    assert_eq!(
        Store::open(&destination)
            .unwrap()
            .revisions(&current.target_string())
            .unwrap()
            .len(),
        1
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn new_unversioned_build_stays_pending_real_extraction() {
    let root = std::env::temp_dir().join(format!("selu-migrate-bootstrap-{}", std::process::id()));
    std::fs::create_dir_all(root.join("manifests")).unwrap();
    let imported: Manifest = serde_json::from_value(serde_json::json!({
        "flavour":"wow", "build":{"major":12,"minor":1,"patch":5,"build":2},
        "interface":120105
    }))
    .unwrap();
    std::fs::write(
        root.join("manifests/old.selum"),
        selu_manifest::format::encode(&imported).unwrap(),
    )
    .unwrap();
    assert!(migrate(&root).status.success());
    let destination = root.join("manifests/store");
    let mut store = Store::open(&destination).unwrap();
    let target = imported.target_string();
    assert!(store.needs_extraction(&target, selu_manifest_gen::EXTRACTION_VERSION));
    let old_revision = store.latest_revision(&target).unwrap();
    assert!(old_revision.starts_with("v0-"));
    // Only the actual generator path is allowed to upgrade this import.
    let mut generated = imported.clone();
    generated.interface += 1;
    store
        .append(&destination, std::slice::from_ref(&generated))
        .unwrap();
    assert!(!store.needs_extraction(&target, selu_manifest_gen::EXTRACTION_VERSION));
    assert_eq!(
        store.load_revision(&target, &old_revision).unwrap(),
        imported
    );
    assert!(!migrate(&root).status.success());
    assert_eq!(
        Store::open(&destination).unwrap().load(&target).unwrap(),
        generated
    );
    std::fs::remove_dir_all(root).unwrap();
}

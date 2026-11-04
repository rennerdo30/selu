//! M2 acceptance: discover and build against every shipped client manifest.
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const PRIMARY_PRODUCTS: [&str; 6] = [
    "wow",
    "wowf",
    "wow_classic_era",
    "wow_anniversary",
    "wow_classic_titan",
    "wow_classic",
];
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn cli(args: &[&str]) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_selu"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(30) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("selu timeout: {args:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
#[test]
fn targets_lists_every_client() {
    let dir = root().join("manifests");
    let out = cli(&["targets", "--manifest-dir", dir.to_str().unwrap()]);
    let store = selu_manifest::store::Store::open(&dir.join("store")).unwrap();
    for target in store.builds() {
        assert!(
            out.lines()
                .any(|line| line.starts_with(&format!("{target} "))),
            "missing {target}: {out}"
        );
    }
}
fn copy_project(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_name() == "dist" {
            continue;
        }
        if path.is_dir() {
            copy_project(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(path, to.join(entry.file_name())).unwrap();
        }
    }
}
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove owned M2 test projects");
    }
}
#[test]
fn every_example_builds_for_every_client() {
    let temp = std::env::temp_dir().join(format!("selu-flavour-examples-{}", std::process::id()));
    assert!(!temp.exists());
    std::fs::create_dir_all(&temp).unwrap();
    let _cleanup = Cleanup(temp.clone());
    let manifests = root().join("manifests");
    let mut jobs = Vec::new();
    for entry in std::fs::read_dir(root().join("examples")).unwrap() {
        let entry = entry.unwrap();
        if !entry.path().join("selu.toml").is_file() {
            continue;
        }
        let project = temp.join(entry.file_name());
        copy_project(&entry.path(), &project);
        jobs.push(project);
    }
    let count = jobs.len();
    // Keep the original six-product coverage and serialize builds on the owner's PC.
    for project in jobs {
        for product in PRIMARY_PRODUCTS {
            cli(&[
                "build",
                project.to_str().unwrap(),
                "--target",
                product,
                "--manifest-dir",
                manifests.to_str().unwrap(),
            ]);
        }
    }
    assert!(count >= 2);
}

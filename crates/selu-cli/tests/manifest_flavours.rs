//! M2 acceptance: discover and build against every shipped client manifest.
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const FLAVOURS: [&str; 6] = [
    "retail",
    "forever",
    "classic-era",
    "classic-anniversary",
    "classic-titan",
    "classic",
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
    for flavour in FLAVOURS {
        assert!(
            out.lines().any(|l| l.starts_with(&format!("{flavour}@"))),
            "missing {flavour}: {out}"
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
    let temp = PathBuf::from("E:/AgentData/claude_temp/m2")
        .join(format!("examples-{}", std::process::id()));
    assert!(!temp.exists());
    std::fs::create_dir_all(&temp).unwrap();
    let _cleanup = Cleanup(temp.clone());
    let manifests = root().join("manifests");
    let mut count = 0;
    for entry in std::fs::read_dir(root().join("examples")).unwrap() {
        let entry = entry.unwrap();
        if !entry.path().join("selu.toml").is_file() {
            continue;
        }
        let project = temp.join(entry.file_name());
        copy_project(&entry.path(), &project);
        for flavour in FLAVOURS {
            cli(&[
                "build",
                project.to_str().unwrap(),
                "--target",
                flavour,
                "--manifest-dir",
                manifests.to_str().unwrap(),
            ]);
        }
        count += 1;
    }
    assert!(count >= 2);
}

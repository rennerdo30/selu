//! End to end: build `examples/hello` with the real `selu` binary, load
//! the generated Lua into the simulator in TOC order, fire PLAYER_LOGIN,
//! and check what the addon printed.

use selu_sim::Sim;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn hello_builds_runs_and_prints_on_login() {
    let root = workspace();
    let example = root.join("examples/hello");
    let manifests = root.join("manifests");
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--manifest-dir",
            manifests.to_str().unwrap(),
        ])
        .output()
        .expect("run selu build");
    assert!(
        output.status.success(),
        "selu build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("target wowf@1.0.0.1000 -> 1.0.0.1000"),
        "{stdout}"
    );

    let dist = example.join("dist/Hello");
    let toc = std::fs::read_to_string(dist.join("Hello.toc")).unwrap();
    assert!(toc.contains("## Interface: 100000"), "{toc}");
    assert!(
        !dist.join("main.lua.map").exists(),
        "source maps must stay out of the addon folder"
    );
    assert!(example.join("dist/.selu/Hello/main.lua.map").exists());

    // Load in TOC order: the non-`##` lines of the .toc.
    let mut files = Vec::new();
    for line in toc.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("##") {
            continue;
        }
        let source = std::fs::read_to_string(dist.join(line)).unwrap();
        files.push((line.to_string(), source));
    }
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].0, "Selu.lua");
    assert!(
        files[1].1.contains("__selu_on(\"PLAYER_LOGIN\""),
        "{}",
        files[1].1
    );

    let sim = Sim::new().unwrap();
    sim.load_addon("Hello", &files).unwrap();
    assert!(sim.prints().is_empty(), "nothing prints before login");
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(
        sim.prints(),
        vec![
            "Welcome, adventurer!".to_string(),
            "Before your next quest: check your bags and repair your gear.".to_string(),
        ]
    );
}

#[test]
fn stubs_exit_nonzero_with_a_clear_message() {
    // `test` and `debug` are wired now (tests/commands.rs).
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("probe")
        .output()
        .unwrap();
    assert!(!output.status.success(), "probe must exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not implemented yet"), "probe: {stderr}");
}

#[test]
fn version_and_targets_work() {
    let version = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).contains("selu"));

    let manifests = workspace().join("manifests");
    let targets = Command::new(env!("CARGO_BIN_EXE_selu"))
        .args(["targets", "--manifest-dir", manifests.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(targets.status.success());
    let stdout = String::from_utf8_lossy(&targets.stdout);
    assert!(stdout.contains("wowf@1.0.0.1000"), "{stdout}");
    assert!(stdout.contains("test fixture"), "{stdout}");
}

#[test]
fn missing_api_on_target_fails_the_build() {
    let root = workspace();
    let dir = std::env::temp_dir().join(format!("selu-e2e-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]
name = \"Bad\"
targets = [\"wowf@1.0.0.1000\"]
",
    )
    .unwrap();
    std::fs::write(dir.join("src/main.selu"), "NoSuchGlobal(1);\n").unwrap();
    let manifests = root.join("manifests");
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .args([
            "build",
            dir.to_str().unwrap(),
            "--manifest-dir",
            manifests.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("SELU0301"), "{stderr}");
    assert!(stderr.contains("wowf@1.0.0.1000"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

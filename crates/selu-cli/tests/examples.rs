//! Every example under `examples/` that has an `expected.txt` is built with
//! the real `selu` binary, loaded into the simulator in `.toc` order the way
//! the client loads it, and must print exactly the lines of `expected.txt`
//! after `PLAYER_LOGIN`. Examples can't silently break this way.

use selu_sim::Sim;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// `name = "..."` from the `[addon]` table of `selu.toml`.
fn addon_name(example: &Path) -> String {
    let text = std::fs::read_to_string(example.join("selu.toml")).unwrap();
    let value: toml::Value = toml::from_str(&text).unwrap();
    value["addon"]["name"].as_str().unwrap().to_string()
}

fn run_example(example: &Path) -> Vec<String> {
    let manifests = workspace().join("manifests");
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .args([
            "build",
            example.to_str().unwrap(),
            "--manifest-dir",
            manifests.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} failed to build:\n{}",
        example.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let name = addon_name(example);
    let dist = example.join("dist").join(&name);
    let toc = std::fs::read_to_string(dist.join(format!("{name}.toc"))).unwrap();
    let mut files = Vec::new();
    for line in toc.lines().map(str::trim) {
        if line.is_empty() || line.starts_with("##") {
            continue;
        }
        let source = std::fs::read_to_string(dist.join(line.replace('\\', "/"))).unwrap();
        let reference = if line == "main.lua" {
            example.join("expected.lua")
        } else {
            example.join(format!("expected.{}", line.replace('\\', "/")))
        };
        if std::env::var_os("SELU_UPDATE_LUA").is_some() {
            if let Some(parent) = reference.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&reference, &source).unwrap();
        }
        assert_eq!(
            source,
            std::fs::read_to_string(&reference).unwrap(),
            "{}",
            reference.display()
        );
        files.push((line.to_string(), source));
    }
    let sim = Sim::new().unwrap();
    sim.load_addon(&name, &files)
        .unwrap_or_else(|e| panic!("{}: {e}", example.display()));
    sim.fire("PLAYER_LOGIN", vec![])
        .unwrap_or_else(|e| panic!("{}: {e}", example.display()));
    sim.prints()
}

#[test]
fn every_example_prints_its_expected_output() {
    let mut checked = 0;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(workspace().join("examples"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("expected.txt").exists())
        .collect();
    entries.sort();
    for example in entries {
        let expected: Vec<String> = std::fs::read_to_string(example.join("expected.txt"))
            .unwrap()
            .lines()
            .map(|l| l.replace("\\t", "\t"))
            .collect();
        assert_eq!(
            run_example(&example),
            expected,
            "{} printed something else",
            example.display()
        );
        checked += 1;
    }
    assert!(checked >= 2, "expected at least two runnable examples");
}

//! `@selu/std`, the standard library package: it type-checks on every product
//! and its helpers behave as documented under the simulator.
mod common;
use common::BoundedCommand;
use std::{path::PathBuf, process::Command};

const PRODUCTS: [&str; 6] = [
    "wow",
    "wowf",
    "wow_classic",
    "wow_classic_era",
    "wow_anniversary",
    "wow_classic_titan",
];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/std-package")
        .join(name)
}

fn project(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("selu-std-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    let targets = PRODUCTS
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        root.join("selu.toml"),
        format!("[addon]\nname = \"StdCheck\"\ntargets = [{targets}]\n"),
    )
    .unwrap();
    std::fs::copy(fixture("main.selu"), root.join("src/main.selu")).unwrap();
    std::fs::copy(fixture("std.test.selu"), root.join("tests/std.test.selu")).unwrap();
    root
}

fn selu(args: &[&str], root: &PathBuf) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_selu"))
        .args(args)
        .arg(root)
        .arg("--manifest-dir")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .env_remove("SELU_PACKAGES_DIR")
        .bounded_output()
        .unwrap()
}

#[test]
fn std_is_listed_for_every_product() {
    let toml = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/std/package.toml"),
    )
    .unwrap();
    for product in PRODUCTS {
        assert!(toml.contains(&format!("\"{product}\"")), "{product}");
    }
}

#[test]
fn std_type_checks_on_every_product() {
    let root = project("check");
    for product in PRODUCTS {
        let out = selu(&["check", "--target", product], &root);
        assert!(
            out.status.success(),
            "{product}:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn std_helpers_behave_under_the_simulator() {
    let root = project("test");
    let out = selu(&["test", "--ui", "none", "--json"], &root);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: selu_test::Report = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report.results.len(), 11, "{stdout}");
    std::fs::remove_dir_all(root).unwrap();
}

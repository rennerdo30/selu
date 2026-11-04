//! `selu test` and `selu debug` wiring (round 8 groundwork; the runner and
//! the Studio window come in rounds 9 and 10).

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fresh copy of `examples/hello` in a temp folder.
fn hello_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("selu-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let example = repo().join("examples/hello");
    std::fs::copy(example.join("selu.toml"), dir.join("selu.toml")).unwrap();
    for entry in std::fs::read_dir(example.join("src")).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(&path, dir.join("src").join(path.file_name().unwrap())).unwrap();
    }
    dir
}

fn selu(args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_selu"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn selu_test_discovers_files_and_says_the_runner_is_not_there_yet() {
    let dir = hello_project("test");
    let (code, err) = selu(&["test", dir.to_str().unwrap()]);
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("no *.test.selu files"), "{err}");

    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(dir.join("tests/boot.test.selu"), "").unwrap();
    let (code, err) = selu(&["test", dir.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, Some(2), "{err}");
    assert!(err.contains("found 1 test file(s)"), "{err}");
    assert!(err.contains("tests/boot.test.selu"), "{err}");
}

#[test]
fn selu_debug_builds_then_hands_over_to_studio() {
    let dir = hello_project("debug");
    let manifests = repo().join("manifests");
    let (code, err) = selu(&[
        "debug",
        dir.to_str().unwrap(),
        "--manifest-dir",
        manifests.to_str().unwrap(),
    ]);
    let built = dir.join("dist/Hello/Hello.toc").is_file();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(built, "selu debug builds first: {err}");
    // `selu-studio` sits beside `selu` when the whole workspace is built;
    // either way the message says what happens next.
    if err.contains("selu-studio was not found") {
        assert_eq!(code, Some(1), "{err}");
    } else {
        assert_eq!(code, Some(2), "{err}");
        assert!(
            err.contains("Studio window is not implemented yet"),
            "{err}"
        );
    }
}

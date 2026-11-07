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
fn selu_test_runs_discovered_files() {
    let dir = hello_project("test");
    let (code, err) = selu(&["test", dir.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}");
    assert!(err.contains("no *.test.selu files"), "{err}");

    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(dir.join("tests/boot.test.selu"), "").unwrap();
    let (code, err) = selu(&["test", dir.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, Some(0), "{err}");
}

#[test]
fn selu_debug_builds_then_hands_over_to_studio() {
    let dir = hello_project("debug");
    let manifests = repo().join("manifests");
    // Point at a missing Studio so the test never opens a window.
    let out = Command::new(env!("CARGO_BIN_EXE_selu"))
        .args([
            "debug",
            dir.to_str().unwrap(),
            "--manifest-dir",
            manifests.to_str().unwrap(),
        ])
        .env("SELU_STUDIO", dir.join("no-such-studio.exe"))
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    let built = dir.join("dist/Hello/Hello.toc").is_file();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(built, "selu debug builds first: {err}");
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("selu-studio was not found"), "{err}");
}

#[test]
fn selu_debug_opens_a_project_that_does_not_build() {
    let dir = hello_project("debug-broken");
    let main = dir.join("src/main.selu");
    let mut text = std::fs::read_to_string(&main).unwrap();
    text.push_str(
        "
const broken: number = \"text\";
",
    );
    std::fs::write(&main, text).unwrap();
    let manifests = repo().join("manifests");
    let out = Command::new(env!("CARGO_BIN_EXE_selu"))
        .args([
            "debug",
            dir.to_str().unwrap(),
            "--manifest-dir",
            manifests.to_str().unwrap(),
        ])
        .env("SELU_STUDIO", dir.join("no-such-studio.exe"))
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
    // The build fails, yet selu debug still goes on to start Studio.
    assert!(
        err.contains("opening the project in Studio anyway"),
        "{err}"
    );
    assert!(err.contains("selu-studio was not found"), "{err}");
}

#[test]
fn build_copies_the_media_the_addon_uses() {
    let dir = hello_project("assets");
    let main = dir.join("src/main.selu");
    let mut text = std::fs::read_to_string(&main).unwrap();
    text.push_str(
        r#"
export const logo = "Interface\\AddOns\\Hello\\Media\\Logo";
export const missing = "Interface\\AddOns\\Hello\\Sounds\\Ping.ogg";
"#,
    );
    std::fs::write(&main, text).unwrap();
    std::fs::create_dir_all(dir.join("Media/Sub")).unwrap();
    std::fs::write(dir.join("Media/Logo.tga"), b"tga").unwrap();
    std::fs::write(dir.join("Media/Sub/Glow.blp"), b"blp").unwrap();
    std::fs::create_dir_all(dir.join("Unused")).unwrap();
    // An explicit asset outside the project keeps its own name.
    let shared = dir.with_extension("shared-fonts");
    let _ = std::fs::remove_dir_all(&shared);
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(shared.join("Font.ttf"), b"ttf").unwrap();
    let toml = std::fs::read_to_string(dir.join("selu.toml")).unwrap();
    let rel = format!("../{}", shared.file_name().unwrap().to_str().unwrap());
    std::fs::write(
        dir.join("selu.toml"),
        format!("{toml}\n[build]\nassets = [\"{rel}\"]\n"),
    )
    .unwrap();
    let manifests = repo().join("manifests");
    let (code, err) = selu(&[
        "build",
        dir.to_str().unwrap(),
        "--manifest-dir",
        manifests.to_str().unwrap(),
    ]);
    let out = dir.join("dist/Hello");
    let copied = (
        out.join("Media/Logo.tga").is_file(),
        out.join("Media/Sub/Glow.blp").is_file(),
        out.join(shared.file_name().unwrap())
            .join("Font.ttf")
            .is_file(),
        out.join("Unused").exists(),
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&shared);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(copied, (true, true, true, false), "{err}");
    assert!(err.contains("SELU0504") && err.contains("Sounds"), "{err}");
}

#[test]
fn taint_project_policy_enforces_deny_and_strict_mode() {
    let dir = hello_project("taint-policy");
    let manifests = repo().join("manifests");
    let config = "[addon]\nname = \"TaintTest\"\ntargets = [\"wow@12.1.5.70391\"]\n";
    std::fs::write(dir.join("src/main.selu"), "hook(\"GetTime\", () => {});").unwrap();
    std::fs::write(dir.join("selu.toml"), config).unwrap();
    assert!(selu_test::project::check_project(&dir, None, Some(&manifests)).is_ok());
    std::fs::write(
        dir.join("selu.toml"),
        format!("{config}\n[build]\nstrict_taint = true\n"),
    )
    .unwrap();
    assert!(selu_test::project::check_project(&dir, None, Some(&manifests)).is_err());
    std::fs::write(dir.join("src/main.selu"), "GetTime();").unwrap();
    std::fs::write(dir.join("selu.toml"), format!("{config}\n[lint]\ndeny = [{{ api = \"GetTime\", reason = \"project regression fixture\" }}]\n")).unwrap();
    assert!(selu_test::project::check_project(&dir, None, Some(&manifests)).is_err());
}

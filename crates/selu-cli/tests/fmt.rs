//! `selu fmt`: every example and the proof port format without changing
//! their tokens, still parse, and format to a fixed point.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn selu_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n != "dist" && n != "target") {
                selu_files(&p, out);
            }
        } else if p.extension().is_some_and(|e| e == "selu") {
            out.push(p);
        }
    }
}

#[test]
fn every_example_formats_safely() {
    let mut files = Vec::new();
    selu_files(&repo().join("examples"), &mut files);
    selu_files(&repo().join("proof"), &mut files);
    assert!(files.len() >= 15, "found only {} files", files.len());
    let options = selu_fmt::Options::default();
    for file in files {
        let src = std::fs::read_to_string(&file).unwrap();
        let out =
            selu_fmt::format(&src, &options).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        assert_eq!(
            selu_fmt::code_tokens(&src).unwrap(),
            selu_fmt::code_tokens(&out).unwrap(),
            "{}: formatting changed the tokens",
            file.display()
        );
        assert_eq!(
            selu_fmt::format(&out, &options).unwrap(),
            out,
            "{}: not idempotent",
            file.display()
        );
        let name = file.file_name().unwrap().to_string_lossy();
        if let Err(d) = selu_syntax::parse(&name, &out) {
            panic!(
                "{}: formatted code no longer parses:\n{}\n{out}",
                file.display(),
                d.render(&out)
            );
        }
    }
}

#[test]
fn fmt_command_writes_and_checks() {
    let dir = std::env::temp_dir().join(format!("selu-fmt-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"F\"\ntargets = [\"wowf@1.0.0.1000\"]\n\n[format]\nindent = 4\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.selu"),
        "on PLAYER_LOGIN(){\nprint(1+2);\n}\n",
    )
    .unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_selu"))
            .args(args)
            .output()
            .unwrap()
    };
    let d = dir.to_str().unwrap();
    let check = run(&["fmt", "--check", d]);
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stdout).contains("not formatted"));
    let write = run(&["fmt", d]);
    assert!(
        write.status.success(),
        "{}",
        String::from_utf8_lossy(&write.stderr)
    );
    let text = std::fs::read_to_string(dir.join("src/main.selu")).unwrap();
    assert_eq!(
        text, "on PLAYER_LOGIN() {\n    print(1 + 2);\n}\n",
        "[format] indent = 4"
    );
    assert!(run(&["fmt", "--check", d]).status.success());
    // Broken code is reported and left alone.
    std::fs::write(dir.join("src/bad.selu"), "const s = \"open;\n").unwrap();
    let bad = run(&["fmt", d]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("unclosed string"));
    assert_eq!(
        std::fs::read_to_string(dir.join("src/bad.selu")).unwrap(),
        "const s = \"open;\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

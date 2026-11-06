mod common;
use common::BoundedCommand;
use std::{path::Path, process::Command};

#[test]
fn check_ignores_build_copies_and_reports_each_real_orphan_once() {
    let dir = std::env::temp_dir().join(format!("selu-check-paths-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src/ui")).unwrap();
    std::fs::create_dir_all(dir.join("dist/.selu/check/src/ui")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"Check\"\ntargets = [\"wowf@1.0.0.1000\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.selu"),
        "import { value } from \"./value\"; print(value);",
    )
    .unwrap();
    std::fs::write(dir.join("src/value.selu"), "export const value = 1;").unwrap();
    std::fs::write(dir.join("src/ui/orphan.selu"), "invalid unused source").unwrap();
    std::fs::write(
        dir.join("dist/.selu/check/src/ui/ghost.selu"),
        "invalid copy",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("check")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .env("SELU_TIMINGS", "1")
        .bounded_output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.matches("warning[SELU0404]").count(), 1, "{stderr}");
    assert!(
        stderr.contains("src/ui/orphan.selu is not reachable"),
        "{stderr}"
    );
    assert!(stderr.contains("ui/orphan.selu:1:1"), "{stderr}");
    assert!(
        !stderr.contains("ghost") && !stderr.contains("dist"),
        "{stderr}"
    );
    assert_eq!(
        stderr.matches("timing prepared target").count(),
        1,
        "{stderr}"
    );
    assert_eq!(
        stderr
            .lines()
            .filter(|l| l.starts_with("timing module ") && l.contains(" check "))
            .count(),
        2,
        "{stderr}"
    );
    assert!(!dir.join("dist/Check").exists());
    assert!(!dir.join("selu.lock").exists());
}

#[test]
fn module_exports_share_final_metadata_and_class_shapes() {
    let m = common::manifest();
    let p = selu_syntax::parse("base.selu", "export class Base { x = 1; } export const first = new Base(); export const second = new Base();").unwrap();
    let prepared = selu_check::PreparedTarget::new(&m, std::slice::from_ref(&m));
    let (checked, exports) = prepared.check_module(&p, Default::default(), Default::default());
    assert!(!checked.has_errors(), "{:?}", checked.diagnostics);
    let a = &exports["first"];
    let b = &exports["second"];
    assert!(std::sync::Arc::ptr_eq(&a.classes, &b.classes));
    assert!(std::sync::Arc::ptr_eq(&a.type_contexts, &b.type_contexts));
    let (selu_check::Type::Class(_, _, a), selu_check::Type::Class(_, _, b)) = (&a.ty, &b.ty)
    else {
        panic!("expected instances")
    };
    assert!(std::sync::Arc::ptr_eq(a, b));
}

#[test]
fn contextual_scalar_union_arms_preserve_types_and_errors() {
    assert_eq!(
        common::run(
            r#"
function choose(v: "left" | "right" | 7 | true | null): void { print(v); }
choose("right"); choose(7); choose(true); choose(null);
"#
        ),
        ["right", "7", "true", "nil"]
    );
    assert!(
        common::codes(r#"function f(v: "left" | 7): void {} f("wrong");"#)
            .contains(&"SELU0313".to_string())
    );
}

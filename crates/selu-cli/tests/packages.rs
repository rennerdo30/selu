//! Packages that ship inside the compiler (docs/lang/packages.md).
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ALL_PRODUCTS: &str = "[\"wow\", \"wowf\", \"wow_classic\", \"wow_classic_era\", \"wow_anniversary\", \"wow_classic_titan\"]";

fn temp(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("selu-packages-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

/// A package folder for `SELU_PACKAGES_DIR`: `package.toml` plus sources.
fn package(root: &Path, name: &str, products: &str, files: &[(&str, &str)]) {
    write(
        &root.join(name),
        &[(
            "package.toml",
            &format!("name = \"{name}\"\ndescription = \"test\"\nproducts = {products}\n"),
        )],
    );
    let files: Vec<_> = files
        .iter()
        .map(|(n, t)| (format!("src/{n}"), *t))
        .collect();
    for (n, t) in &files {
        write(&root.join(name), &[(n.as_str(), *t)]);
    }
}

/// Builds a project; `packages` replaces the embedded set when given.
fn build(label: &str, files: &[(&str, &str)], packages: Option<&Path>) -> (PathBuf, Output) {
    let dir = temp(label);
    write(
        &dir,
        &[(
            "selu.toml",
            "[addon]\nname = \"Modules\"\ntargets = [\"wowf@1.0.0.1000\"]\n",
        )],
    );
    let files: Vec<_> = files
        .iter()
        .map(|(n, t)| (format!("src/{n}"), *t))
        .collect();
    for (n, t) in &files {
        write(&dir, &[(n.as_str(), *t)]);
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_selu"));
    command
        .arg("build")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"));
    match packages {
        Some(path) => command.env("SELU_PACKAGES_DIR", path),
        None => command.env_remove("SELU_PACKAGES_DIR"),
    };
    let output = command.output().unwrap();
    (dir, output)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn embedded_package_builds_into_the_addon() {
    let (dir, output) = build(
        "embedded",
        &[(
            "main.selu",
            "import { limit, Counter } from \"@selu/test-package\";\nconst counter = new Counter(3);\nprint(counter.Add(5), limit(9, 0, 4));\n",
        )],
        None,
    );
    let err = stderr(&output);
    assert!(output.status.success(), "{err}");
    assert!(!err.contains("SELU0404"), "{err}");
    let dist = dir.join("dist/Modules");
    let toc = std::fs::read_to_string(dist.join("Modules.toc")).unwrap();
    let lua: Vec<_> = toc.lines().filter(|l| l.ends_with(".lua")).collect();
    let position = |name: &str| lua.iter().position(|l| *l == name);
    let math = position("selu_packages/test-package/math.lua").expect("math listed");
    let index = position("selu_packages/test-package/index.lua").expect("index listed");
    let main = position("main.lua").expect("main listed");
    assert!(math < index && index < main, "{lua:?}");
    let index_lua =
        std::fs::read_to_string(dist.join("selu_packages/test-package/index.lua")).unwrap();
    assert!(
        index_lua.contains("@selu/test-package/index"),
        "{index_lua}"
    );
    let map = std::fs::read_to_string(
        dir.join("dist/.selu/Modules/selu_packages/test-package/index.lua.map"),
    )
    .unwrap();
    assert!(map.contains("@selu/test-package/index.selu"), "{map}");
}

#[test]
fn unknown_package_lists_the_available_ones() {
    let (_, output) = build(
        "unknown",
        &[(
            "main.selu",
            "import { x } from \"@selu/nope\";\nprint(x);\n",
        )],
        None,
    );
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(
        err.contains("SELU0407") && err.contains("unknown package"),
        "{err}"
    );
    assert!(err.contains("@selu/test-package"), "{err}");
}

#[test]
fn package_internals_are_not_importable() {
    let (_, output) = build(
        "internal",
        &[(
            "main.selu",
            "import { clamp } from \"@selu/test-package/math\";\nprint(clamp(1, 0, 2));\n",
        )],
        None,
    );
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(
        err.contains("SELU0407") && err.contains("internal"),
        "{err}"
    );
}

#[test]
fn other_scopes_and_at_paths_are_refused() {
    let (_, output) = build(
        "scope",
        &[(
            "main.selu",
            "import { x } from \"@other/lib\";\nprint(x);\n",
        )],
        None,
    );
    let err = stderr(&output);
    assert!(
        err.contains("SELU0407") && err.contains("not a Selu package"),
        "{err}"
    );
    let (_, output) = build(
        "at-path",
        &[
            ("main.selu", "import { x } from \"./@lib\";\nprint(x);\n"),
            ("@lib.selu", "export const x = 1;\n"),
        ],
        None,
    );
    let err = stderr(&output);
    assert!(
        err.contains("SELU0318") && err.contains("reserved for packages"),
        "{err}"
    );
}

#[test]
fn packages_import_packages_and_cycles_are_reported() {
    let packages = temp("pkg-pair");
    package(
        &packages,
        "top",
        ALL_PRODUCTS,
        &[(
            "index.selu",
            "import { base } from \"@selu/base\";\nexport function top(): number { return base() + 1; }\n",
        )],
    );
    package(
        &packages,
        "base",
        ALL_PRODUCTS,
        &[(
            "index.selu",
            "export function base(): number { return 1; }\n",
        )],
    );
    let (dir, output) = build(
        "pair",
        &[(
            "main.selu",
            "import { top } from \"@selu/top\";\nprint(top());\n",
        )],
        Some(&packages),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let toc = std::fs::read_to_string(dir.join("dist/Modules/Modules.toc")).unwrap();
    let base = toc.find("selu_packages/base/index.lua").unwrap();
    let top = toc.find("selu_packages/top/index.lua").unwrap();
    assert!(base < top, "{toc}");

    let cyclic = temp("pkg-cycle");
    package(
        &cyclic,
        "left",
        ALL_PRODUCTS,
        &[(
            "index.selu",
            "import { right } from \"@selu/right\";\nexport function left(): number { return right(); }\n",
        )],
    );
    package(
        &cyclic,
        "right",
        ALL_PRODUCTS,
        &[(
            "index.selu",
            "import { left } from \"@selu/left\";\nexport function right(): number { return left(); }\n",
        )],
    );
    let (_, output) = build(
        "cycle",
        &[(
            "main.selu",
            "import { left } from \"@selu/left\";\nprint(left());\n",
        )],
        Some(&cyclic),
    );
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(
        err.contains("SELU0403") && err.contains("@selu/left/index"),
        "{err}"
    );
}

#[test]
fn packages_cannot_declare_saved_data_or_globals() {
    for (label, source, what) in [
        (
            "saved",
            "saved global PackageDB: { n: number } = { n: 0 };\nexport function f(): number { return 1; }\n",
            "saved data",
        ),
        (
            "global",
            "export global PackageValue = 1;\nexport function f(): number { return 1; }\n",
            "globals",
        ),
    ] {
        let packages = temp(&format!("pkg-{label}"));
        package(&packages, "lib", ALL_PRODUCTS, &[("index.selu", source)]);
        let (_, output) = build(
            label,
            &[(
                "main.selu",
                "import { f } from \"@selu/lib\";\nprint(f());\n",
            )],
            Some(&packages),
        );
        let err = stderr(&output);
        assert!(!output.status.success(), "{label}");
        assert!(
            err.contains("SELU0409") && err.contains(what),
            "{label}: {err}"
        );
        assert!(err.contains("@selu/lib/index.selu"), "{label}: {err}");
    }
}

#[test]
fn package_products_are_enforced() {
    let packages = temp("pkg-products");
    package(
        &packages,
        "retailonly",
        "[\"wow\"]",
        &[("index.selu", "export function f(): number { return 1; }\n")],
    );
    let (_, output) = build(
        "products",
        &[(
            "main.selu",
            "import { f } from \"@selu/retailonly\";\nprint(f());\n",
        )],
        Some(&packages),
    );
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(err.contains("SELU0408") && err.contains("wowf"), "{err}");
}

#[test]
fn package_paths_stay_inside_and_errors_name_the_package_file() {
    let packages = temp("pkg-escape");
    package(
        &packages,
        "lib",
        ALL_PRODUCTS,
        &[(
            "index.selu",
            "import { x } from \"../../outside\";\nexport function f(): number { return x; }\n",
        )],
    );
    let (_, output) = build(
        "escape",
        &[(
            "main.selu",
            "import { f } from \"@selu/lib\";\nprint(f());\n",
        )],
        Some(&packages),
    );
    let err = stderr(&output);
    assert!(
        err.contains("SELU0400") && err.contains("cannot leave the package"),
        "{err}"
    );

    let broken = temp("pkg-broken");
    package(
        &broken,
        "lib",
        ALL_PRODUCTS,
        &[(
            "index.selu",
            "export function f(): number { return \"text\"; }\n",
        )],
    );
    let (_, output) = build(
        "broken",
        &[(
            "main.selu",
            "import { f } from \"@selu/lib\";\nprint(f());\n",
        )],
        Some(&broken),
    );
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(err.contains("@selu/lib/index.selu"), "{err}");
}

use std::path::Path;
use std::process::Command;

#[test]
fn entry_ignores_unreachable_invalid_source() {
    let dir = std::env::temp_dir().join(format!("selu-modules-entry-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"Modules\"\ntargets = [\"wowf@1.0.0.1000\"]\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/main.selu"), "print(7);").unwrap();
    std::fs::write(dir.join("src/unused.selu"), "this is invalid").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("unused.selu"));
    let toc = std::fs::read_to_string(dir.join("dist/Modules/Modules.toc")).unwrap();
    assert_eq!(
        toc.lines()
            .filter(|s| s.ends_with(".lua"))
            .collect::<Vec<_>>(),
        ["main.lua"]
    );
}

fn project(label: &str, files: &[(&str, &str)]) -> (std::path::PathBuf, std::process::Output) {
    let dir = std::env::temp_dir().join(format!("selu-d1-{label}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"Modules\"\ntargets = [\"wowf@1.0.0.1000\"]\n",
    )
    .unwrap();
    for (name, source) in files {
        let path = dir.join("src").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    let result = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .output()
        .unwrap();
    (dir, result)
}
fn run(label: &str, files: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
    let (dir, result) = project(label, files);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let dist = dir.join("dist/Modules");
    let toc = std::fs::read_to_string(dist.join("Modules.toc")).unwrap();
    let names: Vec<String> = toc
        .lines()
        .filter(|s| s.ends_with(".lua"))
        .map(str::to_owned)
        .collect();
    let files: Vec<_> = names
        .iter()
        .map(|n| (n.clone(), std::fs::read_to_string(dist.join(n)).unwrap()))
        .collect();
    for n in names.iter().filter(|n| n.as_str() != "Selu.lua") {
        let map: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("dist/.selu/Modules").join(format!("{n}.map")))
                .unwrap(),
        )
        .unwrap();
        let entries = map["ranges"].as_array().unwrap();
        assert!(!entries.is_empty());
        for entry in entries {
            assert_eq!(
                map["sources"][entry["source"].as_u64().unwrap() as usize]["path"]
                    .as_str()
                    .unwrap(),
                format!("src/{}", n.replace(".lua", ".selu"))
            );
        }
    }
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon("Modules", &files).unwrap();
    if label == "live" {
        sim.eval("External()").unwrap();
    }
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    (sim.prints(), names)
}
#[test]
fn live_alias_const_signature_shadow_and_global() {
    let (prints, order) = run(
        "live",
        &[
            (
                "main.selu",
                "import { count as n, bump, label } from \"./state\"; print(n); bump(); print(n); print(label); { const n = 99; print(n); } function shadow(n: number): number { return n; } print(shadow(88)); export global External = bump; on PLAYER_LOGIN() { print(n); }",
            ),
            (
                "state.selu",
                "export let count = 1; export const label = \"ready\"; export function bump(): void { count += 1; }",
            ),
        ],
    );
    assert_eq!(prints, ["1", "2", "ready", "99", "88", "3"]);
    assert_eq!(order, ["Selu.lua", "state.lua", "main.lua"]);
}
#[test]
fn nested_paths_ties_and_private_names() {
    let (prints, order) = run(
        "nested",
        &[
            (
                "main.selu",
                "import { b } from \"./z\"; import { a } from \"./dir/a\"; print(a); print(b);",
            ),
            (
                "z.selu",
                "const privateName = 2; export const b = privateName;",
            ),
            (
                "dir/a.selu",
                "const privateName = 1; export const a = privateName;",
            ),
        ],
    );
    assert_eq!(prints, ["1", "2"]);
    assert_eq!(order, ["dir/a.lua", "z.lua", "main.lua"]);
}
#[test]
fn forward_export_function_uses_later_local() {
    let (prints, _) = run(
        "forward",
        &[
            (
                "main.selu",
                "import { get } from \"./state\"; print(get());",
            ),
            (
                "state.selu",
                "export function get(): number { return value; } export let value = 42;",
            ),
        ],
    );
    assert_eq!(prints, ["42"]);
}
#[test]
fn project_errors_render_the_owning_source() {
    for (label, files, code, source) in [
        (
            "missing",
            vec![("main.selu", "import { x } from \"./absent\";")],
            "SELU0400",
            "main.selu",
        ),
        (
            "name",
            vec![
                ("main.selu", "import { nope } from \"./state\";"),
                ("state.selu", "export const value = 1;"),
            ],
            "SELU0401",
            "main.selu",
        ),
        (
            "readonly",
            vec![
                ("main.selu", "import { value } from \"./state\"; value = 2;"),
                ("state.selu", "export let value = 1;"),
            ],
            "SELU0402",
            "main.selu",
        ),
        (
            "signature",
            vec![
                ("main.selu", "import { f } from \"./state\"; f(\"bad\");"),
                (
                    "state.selu",
                    "export function f(n: number): number { return n; }",
                ),
            ],
            "SELU0335",
            "main.selu",
        ),
        (
            "child",
            vec![
                ("main.selu", "import { value } from \"./state\";"),
                ("state.selu", "export const value: number = \"bad\";"),
            ],
            "SELU0336",
            "state.selu",
        ),
        (
            "cycle",
            vec![
                ("main.selu", "import { value } from \"./state\";"),
                (
                    "state.selu",
                    "import { other } from \"./main\"; export const value = 1;",
                ),
            ],
            "SELU0403",
            "state.selu",
        ),
        (
            "asset",
            vec![("main.selu", "import logo from \"./logo.png\";")],
            "SELU0390",
            "main.selu",
        ),
    ] {
        let (_, result) = project(label, &files);
        assert!(!result.status.success(), "{label}");
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            stderr.contains(&format!("error[{code}]")) && stderr.contains(&format!("{source}:1:")),
            "{label}: {stderr}"
        );
        let selected = match label {
            "missing" => "\"./absent\"",
            "name" => "nope",
            "readonly" => "value =",
            "signature" | "child" => "\"bad\"",
            "cycle" => "\"./main\"",
            "asset" => "\"./logo.png\"",
            _ => unreachable!(),
        };
        let text = files.iter().find(|(name, _)| *name == source).unwrap().1;
        let start = text.find(selected).unwrap();
        assert!(
            stderr.contains(&format!("{source}:1:{}", start + 1)),
            "{label}: {stderr}"
        );
        let expected_length = if label == "readonly" {
            "value".len()
        } else {
            selected.len()
        };
        let underline = stderr.lines().find(|line| line.contains('^')).unwrap();
        assert_eq!(
            underline.chars().filter(|c| *c == '^').count(),
            expected_length,
            "{label}: {stderr}"
        );
        if label == "cycle" {
            assert!(
                stderr.contains("main -> state -> main") && stderr.contains("main.selu"),
                "{stderr}"
            );
        }
    }
}

#[test]
fn example_builds_and_runs_in_toc_order() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let example = root.join("examples/modules");
    let result = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&example)
        .arg("--manifest-dir")
        .arg(root.join("manifests"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let dist = example.join("dist/Modules");
    let toc = std::fs::read_to_string(dist.join("Modules.toc")).unwrap();
    let files: Vec<_> = toc
        .lines()
        .filter(|s| s.ends_with(".lua"))
        .map(|n| (n.into(), std::fs::read_to_string(dist.join(n)).unwrap()))
        .collect();
    assert_eq!(
        files
            .iter()
            .map(|(n, _): &(String, String)| n.as_str())
            .collect::<Vec<_>>(),
        ["Selu.lua", "counter.lua", "main.lua"]
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon("Modules", &files).unwrap();
    assert_eq!(sim.prints(), Vec::<String>::new());
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(
        sim.prints(),
        [
            "Quest journal: sample route complete",
            "Sample quests completed: 2",
        ]
    );
}

#[test]
fn single_file_exports_still_lower() {
    let manifest = selu_manifest::load_manifest(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json"),
    )
    .unwrap();
    let program = selu_syntax::parse(
        "single.selu",
        "export function get(): number { return value; } export const value = 3; print(get());",
    )
    .unwrap();
    let checked = selu_check::check_program(&program, &manifest);
    assert!(!checked.has_errors());
    let lua = selu_lower::lower(&program, &manifest, &checked.types).lua;
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("single.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["3"]);
}

#[test]
fn imports_are_initialized_before_statements_and_reads_keep_order() {
    let (prints, _) = run(
        "order",
        &[
            (
                "main.selu",
                "print(count, change(), count); import { count, change } from \"./state\";",
            ),
            (
                "state.selu",
                "export let count = 1; export function change(): number { count = 5; return 9; }",
            ),
        ],
    );
    assert_eq!(prints, ["1\t9\t5"]);
}

fn checked_module(source: &str) -> selu_check::Checked {
    let manifest = selu_manifest::load_manifest(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json"),
    )
    .unwrap();
    let program = selu_syntax::parse("main.selu", source).unwrap();
    let mut bindings = selu_check::ModuleBindings::default();
    bindings.imports.insert(
        "./state".into(),
        [(
            "n".into(),
            selu_check::ModuleExport {
                ty: selu_check::Type::Number,
                mutable: false,
                saved: false,
                ..Default::default()
            },
        )]
        .into(),
    );
    selu_check::check_module(&program, &manifest, bindings).0
}
fn exact_error(source: &str, code: &str, selected: &str) {
    let checked = checked_module(source);
    assert_eq!(checked.diagnostics.len(), 1, "{:?}", checked.diagnostics);
    let d = &checked.diagnostics[0];
    assert_eq!(d.code, code);
    let span = d.span.as_ref().unwrap();
    assert_eq!(span.file, "main.selu");
    assert_eq!(
        (span.start, span.end),
        (
            source.find(selected).unwrap(),
            source.find(selected).unwrap() + selected.len()
        )
    );
}
#[test]
fn review_alias_keyword() {
    exact_error("import { n as end } from \"./state\";", "SELU0304", "end");
}
#[test]
fn review_alias_reserved() {
    exact_error(
        "import { n as __selu_ns } from \"./state\";",
        "SELU0318",
        "__selu_ns",
    );
}
#[test]
fn review_case_variant() {
    let (_, output) = project(
        "case",
        &[
            (
                "main.selu",
                "import { n } from \"./state\"; import { n as other } from \"./State\";",
            ),
            ("state.selu", "export const n = 1;"),
        ],
    );
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(
        text.contains("error[SELU0405]")
            && text.contains("main.selu:1:57")
            && text.contains("state.selu")
            && text.contains("fix:"),
        "{text}"
    );
}
#[test]
fn review_addon_global() {
    let (dir, output) = project("addon-global", &[("main.selu", "export global ADDON = 8;")]);
    assert!(output.status.success());
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon(
        "Modules",
        &[(
            "main.lua".into(),
            std::fs::read_to_string(dir.join("dist/Modules/main.lua")).unwrap(),
        )],
    )
    .unwrap();
    sim.eval("assert(ADDON == 8)").unwrap();
}
#[test]
fn review_nested_import() {
    exact_error(
        "{ import { n } from \"./state\"; }",
        "SELU0406",
        "import { n } from \"./state\";",
    );
}
#[test]
fn review_nested_export() {
    exact_error("{ export const n = 1; }", "SELU0406", "export const n = 1;");
}
#[test]
fn review_global_live() {
    let (prints, _) = run(
        "global-live",
        &[
            (
                "main.selu",
                "import { n, change } from \"./state\"; print(n); change(); print(n);",
            ),
            (
                "state.selu",
                "export global n = 1; export function change(): void { n = 2; }",
            ),
        ],
    );
    assert_eq!(prints, ["1", "2"]);
}
#[test]
fn review_shared_event_frame() {
    let (dir, output) = project(
        "events",
        &[
            (
                "main.selu",
                "import { n } from \"./state\"; on PLAYER_LOGIN() { print(n); }",
            ),
            (
                "state.selu",
                "export const n = 1; on PLAYER_LOGIN() { print(2); }",
            ),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let toc = std::fs::read_to_string(dir.join("dist/Modules/Modules.toc")).unwrap();
    assert_eq!(
        toc.lines().find(|line| line.ends_with(".lua")),
        Some("Selu.lua")
    );
    let runtime = std::fs::read_to_string(dir.join("dist/Modules/Selu.lua")).unwrap();
    assert_eq!(runtime.matches("-- Selu: events").count(), 1);
    assert!(
        !runtime.contains("__selu_saved")
            && !runtime.contains("__selu_errors")
            && !runtime.contains("__selu_wow")
            && !runtime.contains("__selu_has_event")
    );
    let files: Vec<_> = ["Selu.lua", "state.lua", "main.lua"]
        .iter()
        .map(|n| {
            (
                n.to_string(),
                std::fs::read_to_string(dir.join("dist/Modules").join(n)).unwrap(),
            )
        })
        .collect();
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon("Modules", &files).unwrap();
    sim.eval("assert(#__selu_sim_frames == 1)").unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(sim.prints(), ["2", "1"]);
}

#[test]
fn review_dependency_chain_and_diamond() {
    let (prints, order) = run(
        "diamond",
        &[
            (
                "main.selu",
                "import { a } from \"./a\"; import { b } from \"./b\"; print(a); print(b);",
            ),
            (
                "a.selu",
                "import { z } from \"./z\"; export const a = z + 1;",
            ),
            (
                "b.selu",
                "import { z } from \"./z\"; export const b = z + 2;",
            ),
            ("z.selu", "export const z = 10; print(z);"),
        ],
    );
    assert_eq!(prints, ["10", "11", "12"]);
    assert_eq!(order, ["z.lua", "a.lua", "b.lua", "main.lua"]);
}
#[test]
fn review_missing_export_help_is_generic() {
    let source = "import { greetng } from \"./state\";";
    let manifest = selu_manifest::load_manifest(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json"),
    )
    .unwrap();
    let program = selu_syntax::parse("main.selu", source).unwrap();
    let mut bindings = selu_check::ModuleBindings::default();
    bindings.imports.insert(
        "./state".into(),
        ["aaa", "greeting"]
            .into_iter()
            .map(|n| {
                (
                    n.into(),
                    selu_check::ModuleExport {
                        ty: selu_check::Type::Number,
                        mutable: false,
                        saved: false,
                        ..Default::default()
                    },
                )
            })
            .collect(),
    );
    let checked = selu_check::check_module(&program, &manifest, bindings).0;
    assert_eq!(checked.diagnostics.len(), 1);
    assert_eq!(checked.diagnostics[0].code, "SELU0401");
    assert!(
        !checked.diagnostics[0]
            .help
            .as_ref()
            .unwrap()
            .contains("aaa")
    );
    assert_eq!(
        checked.diagnostics[0].span.as_ref().unwrap(),
        &selu_diagnostics::Span::new("main.selu", 9, 16)
    );
}

#[test]
fn reexports_publish_values_classes_types_and_live_bindings() {
    let (prints, order) = run(
        "reexport",
        &[
            (
                "main.selu",
                "import { label, inc as increase, Box, count, Point } from \"./kit/index\"; const p: Point = { x: 2, y: 3 }; const b = new Box(p.x + p.y); print(label); print(b.Get()); increase(); increase(); print(count);",
            ),
            (
                "kit/index.selu",
                "export { label, bump as inc, count, Point } from \"./state\"; export { Box } from \"./box\";",
            ),
            (
                "kit/state.selu",
                "export let count = 0; export const label = \"kit\"; export function bump(): void { count += 1; } export type Point = { x: number; y: number };",
            ),
            (
                "kit/box.selu",
                "export class Box { value: number; constructor(value: number) { this.value = value; } Get(): number { return this.value * 2; } }",
            ),
        ],
    );
    assert_eq!(prints, ["kit", "10", "2"]);
    assert_eq!(
        order,
        ["kit/box.lua", "kit/state.lua", "kit/index.lua", "main.lua"]
    );
}

#[test]
fn reexport_of_a_missing_name_is_reported() {
    let (_, result) = project(
        "reexport-missing",
        &[
            (
                "main.selu",
                "import { nope } from \"./index\"; print(nope);",
            ),
            ("index.selu", "export { nope } from \"./state\";"),
            ("state.selu", "export const there = 1;"),
        ],
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(stderr.contains("SELU0401"), "{stderr}");
}

#[test]
fn addon_name_is_the_built_folder_name() {
    let (prints, _) = run(
        "addon-name",
        &[
            (
                "main.selu",
                "import { label } from \"./label\"; print(addonName()); print(label());",
            ),
            (
                "label.selu",
                "export function label(): string { return addonName() + \"!\"; }",
            ),
        ],
    );
    assert_eq!(prints, ["Modules", "Modules!"]);
    let (_, result) = project("addon-name-args", &[("main.selu", "print(addonName(1));")]);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(stderr.contains("SELU0649"), "{stderr}");
}

#[test]
fn addon_metadata_reaches_the_toc() {
    let dir = std::env::temp_dir().join(format!("selu-toc-meta-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"Meta\"\ntargets = [\"wowf@1.0.0.1000\"]\ntitle = \"Meta Addon\"\nnotes = \"Does things.\"\nversion = \"1.2.3-beta.4\"\nauthor = \"Someone\"\nicon = \"Interface\\\\AddOns\\\\Meta\\\\Logo\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/main.selu"), "print(1);").unwrap();
    let build = |dir: &Path| {
        Command::new(env!("CARGO_BIN_EXE_selu"))
            .arg("build")
            .arg(dir)
            .arg("--manifest-dir")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
            .output()
            .unwrap()
    };
    let output = build(&dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let toc = std::fs::read_to_string(dir.join("dist/Meta/Meta.toc")).unwrap();
    for line in [
        "## Title: Meta Addon",
        "## Notes: Does things.",
        "## Version: 1.2.3-beta.4",
        "## Author: Someone",
        "## IconTexture: Interface\\AddOns\\Meta\\Logo",
    ] {
        assert!(toc.lines().any(|l| l == line), "{line} missing from\n{toc}");
    }
    assert!(!toc.contains("Built with Selu"), "{toc}");
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"Meta\"\ntargets = [\"wowf@1.0.0.1000\"]\nversion = \"1\\n## Title: Injected\"\n",
    )
    .unwrap();
    let output = build(&dir);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SELU0506"));
}

#[test]
fn literal_contexts_tuples_and_queue_methods_run() {
    let (prints, _) = run(
        "literal-contexts",
        &[(
            "main.selu",
            "type R = \"in\" | \"out\"; function f(x: boolean): R { return x ? \"in\" : \"out\"; } const r: R = f(false); print(`${r}`); function pair(a: string?): [string, string]? { if (a == null) { return null; } return [a, \"b\"]; } const [x, y] = pair(\"a\"); print((x ?? \"-\") + (y ?? \"-\")); const [u, v] = pair(null); print((u ?? \"-\") + (v ?? \"-\")); const t: [number, string] = [1, \"x\"]; print(t[1]); const xs = [1, 2, 3]; const first = xs.shift(); xs.unshift(9); print(first ?? -1); print(xs.join(\",\"));",
        )],
    );
    assert_eq!(prints, ["out", "ab", "--", "x", "1", "9,2,3"]);
}

#[test]
fn an_optional_tuple_call_must_be_destructured() {
    let (_, result) = project(
        "optional-tuple",
        &[(
            "main.selu",
            "function pair(): [string, string]? { return null; } const p = pair(); print(p);",
        )],
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(stderr.contains("SELU0377"), "{stderr}");
}

#[test]
fn generic_functions_infer_across_modules_and_callbacks() {
    let (prints, _) = run(
        "generics",
        &[
            (
                "main.selu",
                "import { first, mapAll, adopt } from \"./util\"; const n = first([3, 4]) ?? 0; print(n + 1); const lens = mapAll([\"a\", \"bcd\"], (s) => s.length); print(lens[0] + lens[1]); const m = new Map<string, number>(); m.set(\"old\", 2); m.set(\"new\", 5); print(adopt(m, (a, b) => a + b)); print(m.get(\"new\") ?? -1);",
            ),
            (
                "util.selu",
                "export function first<T>(xs: T[]): T? { return xs.length > 0 ? xs[0] : null; } export function mapAll<T, U>(xs: T[], f: (x: T) => U): U[] { const out: U[] = []; for (const x of xs) { out.push(f(x)); } return out; } export function adopt<T>(t: Map<string, T>, merge: (a: T, b: T) => T): boolean { const old = t.get(\"old\"); if (old == null) { return false; } const cur = t.get(\"new\"); t.set(\"new\", cur != null ? merge(cur, old) : old); t.delete(\"old\"); return true; }",
            ),
        ],
    );
    assert_eq!(prints, ["4", "4", "true", "7"]);
}

#[test]
fn generic_bodies_are_opaque_and_calls_are_checked() {
    for (label, source, code) in [
        (
            "generic-opaque",
            "function bad<T>(v: T): number { return v + 1; } print(bad(1));",
            "SELU0323",
        ),
        (
            "generic-mismatch",
            "function id<T>(v: T): T { return v; } const s: string = id(1); print(s);",
            "SELU0336",
        ),
        (
            "generic-uninferred",
            "function make<T>(): T[] { return []; } print(make());",
            "SELU0651",
        ),
        (
            "generic-twice",
            "function two<T, T>(v: T): T { return v; } print(two(1));",
            "SELU0650",
        ),
    ] {
        let (_, result) = project(label, &[("main.selu", source)]);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success(), "{label}");
        assert!(stderr.contains(code), "{label}: {stderr}");
    }
}

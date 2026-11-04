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
    // A project without `saved` data gets no `selu.saved.toml`.
    let uses_saved = std::fs::read_dir(example.join("src"))
        .unwrap()
        .flatten()
        .any(|f| std::fs::read_to_string(f.path()).is_ok_and(|s| s.contains("saved ")));
    assert_eq!(
        example.join("selu.saved.toml").exists(),
        uses_saved,
        "{}: selu.saved.toml must exist exactly when the project declares saved data",
        example.display()
    );
    let dist = example.join("dist").join(&name);
    let toc = std::fs::read_to_string(dist.join(format!("{name}.toc"))).unwrap();
    let expected_bindings = example.join("expected.Bindings.xml");
    let bindings = dist.join("Bindings.xml");
    assert_eq!(
        bindings.exists(),
        expected_bindings.exists(),
        "{}: Bindings.xml fixture presence",
        example.display()
    );
    if expected_bindings.exists() {
        assert_eq!(
            std::fs::read_to_string(&bindings).unwrap(),
            std::fs::read_to_string(&expected_bindings).unwrap(),
            "{}",
            expected_bindings.display()
        );
    }
    let mut files = Vec::new();
    let mut mapped_sources = std::collections::BTreeSet::new();
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
        let map_path = example
            .join("dist/.selu")
            .join(&name)
            .join(format!("{line}.map"));
        if map_path.exists() {
            let map =
                selu_sourcemap::SourceMap::from_json(&std::fs::read_to_string(map_path).unwrap())
                    .unwrap();
            assert!(!toc.contains(".map"));
            if map.sources.is_empty() {
                assert!(map.ranges.is_empty(), "runtime has no source positions");
            }
            for input_file in &map.sources {
                mapped_sources.insert(input_file.path.clone());
                let input = std::fs::read_to_string(example.join(&input_file.path)).unwrap();
                assert_eq!(input_file.hash, selu_sourcemap::content_hash(&input));
                assert_statement_coverage(&input, &input_file.path, &source, &map);
                let mut missing = map.clone();
                missing.ranges.clear();
                assert!(
                    std::panic::catch_unwind(|| assert_statement_coverage(
                        &input,
                        &input_file.path,
                        &source,
                        &missing
                    ))
                    .is_err(),
                    "clearing this module's ranges must fail coverage"
                );
            }
        }
        files.push((line.to_string(), source));
    }
    assert_source_inventory(example, &example.join("src"), &mapped_sources);
    let sim = Sim::new().unwrap();
    for manifest in selu_manifest::load_manifest_dir(&manifests).unwrap() {
        for family in manifest.frame_families {
            sim.register_frame_family(&family.prefix, &family.suffix)
                .unwrap();
        }
    }
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

/// Independent source expectations: never derive this inventory from map ranges.
fn assert_statement_coverage(input: &str, path: &str, lua: &str, map: &selu_sourcemap::SourceMap) {
    use selu_syntax::{
        Expr, ExprKind, Stmt, StmtKind,
        visit::{self, Child},
    };
    fn anchor(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Ident(n) => n.clone(),
            ExprKind::This => "self".into(),
            ExprKind::Super => "__init".into(),
            ExprKind::Member { object, name } if matches!(object.kind, ExprKind::Super) => {
                name.clone()
            }
            ExprKind::Member { object, .. } | ExprKind::Index { object, .. } => anchor(object),
            ExprKind::Call { callee, .. } => {
                if let ExprKind::Ident(n) = &callee.kind {
                    if n == "hookScript" {
                        return "HookScript".into();
                    }
                    if n == "keybinding" {
                        return "BINDING_NAME_".into();
                    }
                    if n == "command" {
                        return "__selu".into();
                    }
                }
                anchor(callee)
            }
            ExprKind::Assign { target, .. }
            | ExprKind::CompoundAssign { target, .. }
            | ExprKind::Update { target, .. } => anchor(target),
            ExprKind::TaggedTemplate { .. } => "__selu".into(),
            _ => panic!(
                "add a semantic code expectation for expression {:?}",
                e.kind
            ),
        }
    }
    fn argument(e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Ident(n) | ExprKind::Number(n) => Some(n.clone()),
            ExprKind::Str(s) if !s.contains(['\n', '\r', '\t', '"', '\\']) => Some(s.clone()),
            ExprKind::Call { .. } | ExprKind::Member { .. } => Some(anchor(e)),
            _ => None,
        }
    }
    fn stmt(s: &Stmt, out: &mut Vec<(usize, Vec<String>, Vec<String>)>) {
        // has(GetTime) is decided for this example's fixture target. Its dead
        // branch and the erased test itself have no executable statement.
        if let StmtKind::If {
            cond, then_branch, ..
        } = &s.kind
            && let ExprKind::Call { callee, args } = &cond.kind
            && matches!(&callee.kind, ExprKind::Ident(n) if n == "has")
        {
            assert!(matches!(&args[0].kind, ExprKind::Ident(n) if n == "GetTime"));
            for s in then_branch {
                stmt(s, out);
            }
            return;
        }
        let code = match &s.kind {
            StmtKind::TypeDecl(_)
            | StmtKind::Import { .. }
            | StmtKind::Export(_)
            | StmtKind::Block(_) => None,
            StmtKind::VarDecl { name, .. }
            | StmtKind::Global { name, .. }
            | StmtKind::FuncDecl { name, .. }
            | StmtKind::Saved { name, .. }
            | StmtKind::Migrate { name, .. } => Some(vec![name.clone()]),
            StmtKind::Class(c) => Some(vec![c.name.clone()]),
            StmtKind::OnHandler { .. } => Some(vec!["__selu_on".into()]),
            StmtKind::Expr(e) => Some(vec![anchor(e)]),
            StmtKind::Return(_) => Some(vec!["return".into()]),
            StmtKind::Throw { .. } => Some(vec!["__selu_raise".into()]),
            StmtKind::Try { .. } => Some(vec!["do".into(), "__selu_protect".into()]),
            StmtKind::If { .. } => Some(vec!["if".into()]),
            StmtKind::While { .. } | StmtKind::For { .. } | StmtKind::ForOf { .. } => {
                Some(vec!["for".into(), "while".into(), "do".into()])
            }
            StmtKind::DoWhile { .. } => Some(vec!["repeat".into()]),
            StmtKind::Break | StmtKind::Continue => Some(vec!["break".into()]),
            StmtKind::Destructure { pattern, .. } => {
                let name = match pattern {
                    selu_syntax::Pattern::Array(ns) => &ns.iter().flatten().next().unwrap().0,
                    selu_syntax::Pattern::Object(ns) => &ns[0].1,
                };
                Some(vec![name.clone()])
            }
        };
        if let Some(code) = code {
            let mut required = Vec::new();
            match &s.kind {
                StmtKind::Expr(e) => {
                    if let ExprKind::Call { callee, args } = &e.kind {
                        if !matches!(&callee.kind, ExprKind::Ident(n) if n == "command") {
                            required.extend(args.iter().filter_map(argument));
                        }
                        if let ExprKind::Member { name, .. } = &callee.kind
                            && ![
                                "set", "push", "add", "delete", "clear", "pop", "shift", "unshift",
                            ]
                            .contains(&name.as_str())
                        {
                            required.push(name.clone());
                        }
                    }
                }
                StmtKind::Return(Some(e)) => required.extend(argument(e)),
                StmtKind::Throw { value } => required.extend(argument(value)),
                _ => {}
            }
            out.push((s.span.start, code, required));
        }
        visit::child_stmts_and_exprs(s, &mut |c| child(c, out));
    }
    fn child(c: Child<'_>, out: &mut Vec<(usize, Vec<String>, Vec<String>)>) {
        match c {
            Child::Stmt(s) => stmt(s, out),
            Child::Expr(e) => visit::child_exprs_and_stmts(e, &mut |c| child(c, out)),
        }
    }
    let program = selu_syntax::parse(path, input).unwrap();
    let mut statements = Vec::new();
    for s in &program.stmts {
        stmt(s, &mut statements);
    }
    assert!(
        !statements.is_empty(),
        "example must exercise executable statements"
    );
    for (offset, code, required) in statements {
        let line = input[..offset].bytes().filter(|b| *b == b'\n').count() as u32 + 1;
        let generated = map
            .generated_lines(path, line)
            .into_iter()
            .filter_map(|g| {
                let winning = map.lookup(g).unwrap();
                (winning.path == path && winning.line == line)
                    .then(|| lua.lines().nth(g as usize - 1).unwrap())
            })
            .collect::<Vec<_>>();
        let emitted = generated.join("\n").to_lowercase();
        assert!(
            code.iter().any(|c| emitted.contains(&c.to_lowercase()))
                && required.iter().all(|c| emitted.contains(&c.to_lowercase())),
            "{path}:{line} expects {code:?} and operands {required:?} in generated lines whose winning position is this statement; source: {}",
            input.lines().nth(line as usize - 1).unwrap()
        );
    }
}

#[test]
#[should_panic(expected = "expects")]
fn coverage_rejects_empty_ranges() {
    let input = "const value = 1;";
    let mut map = selu_sourcemap::SourceMap::new("main.lua");
    map.add_source("src/main.selu", input);
    assert_statement_coverage(input, "src/main.selu", "local value = 1", &map);
}

fn assert_source_inventory(
    project: &Path,
    directory: &Path,
    mapped: &std::collections::BTreeSet<String>,
) {
    fn executable(s: &selu_syntax::Stmt) -> bool {
        match &s.kind {
            selu_syntax::StmtKind::Import { .. } | selu_syntax::StmtKind::TypeDecl(_) => false,
            selu_syntax::StmtKind::Export(inner) => executable(inner),
            _ => true,
        }
    }
    for file in std::fs::read_dir(directory).unwrap() {
        let file = file.unwrap().path();
        if file.is_dir() {
            assert_source_inventory(project, &file, mapped);
        } else if file.extension().is_some_and(|s| s == "selu") {
            let path = file
                .strip_prefix(project)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let input = std::fs::read_to_string(&file).unwrap();
            let program = selu_syntax::parse(&path, &input).unwrap();
            if program.stmts.iter().any(executable) {
                assert!(mapped.contains(&path), "missing source map for {path}");
            }
        }
    }
}

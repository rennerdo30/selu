mod common;
use common::*;
use std::{path::PathBuf, process::Command};
fn project(label: &str, files: &[(&str, &str)]) -> (PathBuf, std::process::Output) {
    let root = std::env::temp_dir().join(format!("selu-L-review-{label}-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("selu.toml"),
        "[addon]\nname=\"Review\"\ntargets=[\"forever@1.0\"]\n",
    )
    .unwrap();
    for (name, text) in files {
        std::fs::write(root.join("src").join(name), text).unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&root)
        .arg("--manifest-dir")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .output()
        .unwrap();
    (root, out)
}
fn run_project(label: &str, files: &[(&str, &str)]) -> (PathBuf, Vec<String>) {
    let (root, out) = project(label, files);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let dist = root.join("dist/Review");
    let toc = std::fs::read_to_string(dist.join("Review.toc")).unwrap();
    let files: Vec<_> = toc
        .lines()
        .filter(|n| n.ends_with(".lua"))
        .map(|n| (n.into(), std::fs::read_to_string(dist.join(n)).unwrap()))
        .collect();
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon("Review", &files).unwrap();
    (root, sim.prints())
}
#[test]
fn r01_field_convention() {
    let s = "type Fn = (n: number) => number; interface X { fn: Fn; } const x: X = { fn: (n: number) => n + 1 }; print(x.fn(3));";
    assert_eq!(run(s), vec!["4"]);
    assert!(compile(s).contains("x.fn(3)"));
}
#[test]
fn r02_generic_identity() {
    let (_, out) = project(
        "identity",
        &[
            ("a.selu", "export class C { n=3; }"),
            ("b.selu", "export class C { text=\"ok\"; }"),
            (
                "main.selu",
                r#"import { C as A } from "./a";
import { C as B } from "./b";
interface Box<T> { value:T; }
interface Holder<T> { box:Box<T>; }
const a:Holder<A> = {box:{value:new A()}};
const b:Holder<B> = a;
print(b.box.value.text.upper());"#,
            ),
        ],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0336"));
}
#[test]
fn r03_alias_validation() {
    assert_eq!(codes("type Values = number?[];"), vec!["SELU0350"]);
}
#[test]
fn r04_unknown_if() {
    assert_eq!(
        codes("function f(x: unknown) { if (x) { print(1); } }"),
        vec!["SELU0328"]
    );
}
#[test]
fn r04_unknown_not() {
    assert_eq!(
        codes("function f(x: unknown) { const b = !x; }"),
        vec!["SELU0328"]
    );
}
#[test]
fn r04_unknown_and() {
    assert_eq!(
        codes("function f(x: unknown) { const b = x && true; }"),
        vec!["SELU0328"]
    );
    assert_eq!(
        codes("function f(x: unknown) { if (x && true) { print(1); } }"),
        vec!["SELU0328"]
    );
}
#[test]
fn r04_unknown_or() {
    assert_eq!(
        codes("function f(x: unknown) { if (x || false) { print(1); } }"),
        vec!["SELU0328"]
    );
}
#[test]
fn r05_scalar_sort() {
    assert_eq!(
        run("type N = number; const xs: N[] = [3,1]; xs.sort(); print(xs.join(\",\"));"),
        vec!["1,3"]
    );
}
#[test]
fn r05_boolean_join() {
    assert_eq!(
        run("type B = boolean; const xs: B[] = [true,false]; print(xs.join(\",\"));"),
        vec!["true,false"]
    );
}
#[test]
fn r05_saved() {
    assert!(codes("type N = number; saved settings = { values: new Map<string,N>() };").is_empty());
}
#[test]
fn r06_optional_field() {
    assert_eq!(
        run(
            "type Maybe = number?; interface X { n?: Maybe; } const x: X = {n:3}; const n=x.n; if(n != null) { print(n + 1); }"
        ),
        vec!["4"]
    );
}
#[test]
fn r06_optional_value() {
    assert_eq!(
        run("type Maybe = number?; let n: Maybe? = 3; if(n != null) { print(n + 1); }"),
        vec!["4"]
    );
}
#[test]
fn r07_recursive_ab() {
    assert!(
        codes("type A = B; type B = { a: A?; n: number }; const b: A = { a:null, n:3 };")
            .is_empty()
    );
}
#[test]
fn r07_recursive_ba() {
    assert!(
        codes("type B = { a: A?; n: number }; type A = B; const b: A = { a:null, n:3 };")
            .is_empty()
    );
}
#[test]
fn r08_table_preserves() {
    assert_eq!(
        run(
            "interface X { n:number; } function f(x:X?):number { if(typeof x == \"table\") { return x.n; } return 0; } print(f({n:3}));"
        ),
        vec!["3"]
    );
}
#[test]
fn r08_function_preserves() {
    assert_eq!(
        run(
            "type Fn = (n:number)=>number; function f(x: Fn?):number { if(typeof x == \"function\") { return x(3); } return 0; } print(f((n:number)=>n+1));"
        ),
        vec!["4"]
    );
}
#[test]
fn r08_nil() {
    assert_eq!(
        run(
            "function f(x:number?):number { if(typeof x == \"nil\") { const n:null=x; return 0; } return x+1; } print(f(null),f(2));"
        ),
        vec!["0\t3"]
    );
}
#[test]
fn r09_rest() {
    assert_eq!(
        run(
            "interface X { sum(...xs: number[]): number; } const x:X={sum:(...xs:number[])=>xs.length}; print(x.sum(1,2));"
        ),
        vec!["2"]
    );
}
#[test]
fn r09_bad_rest() {
    assert!(
        selu_syntax::parse(
            "t.selu",
            "interface X { sum(...xs: number[], n:number): number; }"
        )
        .is_err()
    );
}
#[test]
fn r10_dual_export() {
    let (_, p) = run_project(
        "dual",
        &[
            (
                "types.selu",
                "export type X=number;\nexport const X=\"ok\";",
            ),
            (
                "main.selu",
                "import { X } from \"./types\";\nconst x:X=3;\nprint(X,x);",
            ),
        ],
    );
    assert_eq!(p, vec!["ok\t3"]);
}
#[test]
fn r10_duplicate() {
    let (_, out) = project(
        "duplicate",
        &[
            ("types.selu", "export type X=number;"),
            ("main.selu", "import { X } from \"./types\"; type X=string;"),
        ],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0520"));
}
#[test]
fn r11_context() {
    let (_, p) = run_project(
        "context",
        &[
            (
                "types.selu",
                "class C { n=3; }\nexport interface Box { item:C; }\nexport function create():Box { return {item:new C()}; }",
            ),
            (
                "main.selu",
                "import { Box, create } from \"./types\";\nconst b:Box=create();\nprint(b.item.n);",
            ),
        ],
    );
    assert_eq!(p, vec!["3"]);
}
#[test]
fn r12_names() {
    let s = "type Count=number; function get():Count { return 3; } const s:string=get();";
    let p = selu_syntax::parse("t.selu", s).unwrap();
    let d = selu_check::check(&p, &manifest());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Count"));
}
#[test]
fn r13_cached() {
    let s = "type Bad={x:Missing}; const a:Bad={x:1}; const b:Bad={x:2};";
    assert_eq!(codes(s), vec!["SELU0320"]);
}
#[test]
fn r13_spans() {
    for (s, needle) in [
        ("type LongName<T,T> = T;", "T>"),
        ("interface X { n:number; n:string; }", "n:string"),
    ] {
        let p = selu_syntax::parse("t.selu", s).unwrap();
        let d = selu_check::check(&p, &manifest());
        assert_eq!(d.len(), 1);
        let span = d[0].span.as_ref().unwrap();
        assert_eq!(&s[span.start..span.end], &needle[..1]);
        assert_eq!(span.start, s.find(needle).unwrap());
    }
}
#[test]
fn r14_erasure() {
    let (root, _) = run_project(
        "erase",
        &[
            ("types.selu", "export type X=number;"),
            (
                "main.selu",
                "import { X } from \"./types\";\nexport interface Y { x:X; }",
            ),
        ],
    );
    assert_eq!(
        std::fs::read_to_string(root.join("dist/Review/main.lua")).unwrap(),
        "local __selu_addon, __selu_ns = ...\n"
    );
}
#[test]
fn r15_adjacent() {
    assert!(selu_syntax::parse("t.selu", "type Box<T>={v:T};").is_ok());
}
#[test]
fn r10_reverse_export() {
    let (_, p) = run_project(
        "reverse",
        &[
            (
                "types.selu",
                "export const X=\"ok\";\nexport type X=number;",
            ),
            (
                "main.selu",
                "import { X } from \"./types\";\nconst x:X=3;\nprint(X,x);",
            ),
        ],
    );
    assert_eq!(p, vec!["ok\t3"]);
}
#[test]
fn r11_diagnostic_source() {
    let (_, out) = project(
        "source",
        &[
            ("types.selu", "export type List<T> = T[];"),
            (
                "main.selu",
                "import { List } from \"./types\";\nconst x:List<number?> = [];",
            ),
        ],
    );
    let errors = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(errors.contains("types.selu:1:"), "{errors}");
    assert!(errors.contains("export type List<T> = T[];"), "{errors}");
    assert_eq!(errors.matches("SELU0350").count(), 1, "{errors}");
}
#[test]
fn r05_shape_sites() {
    let source = "class C { n=3; } type View=C; const c:View={n:3};";
    let p = selu_syntax::parse("t.selu", source).unwrap();
    let ds = selu_check::check(&p, &manifest());
    assert_eq!(ds.len(), 1);
    assert!(
        ds[0].help.as_ref().is_some_and(|h| h.contains("new C")),
        "{ds:?}"
    );

    assert_eq!(
        run(r#"
        type N=number;
        type S=string;
        type F=(n:number)=>number;
        type Numbers=N[];
        type Item={ n:N; };
        const xs:Numbers=[2,1];
        const f:F=(n:N)=>n+1;
        const item:Item={n:3};
        const {n}=item;
        let i:N=0;
        i++;
        const text:S="hello";
        print(n, i, text.length, xs.map(f).join(","));
        for (const value of xs) { print(value+1); }
        log.info`${text} ${i}`;
    "#),
        vec!["3\t1\t5\t3,2", "3", "2"]
    );
}
#[test]
fn r02_identity_is_not_a_display_label() {
    let source = r#"
        class C { n=3; }
        class D { text="ok"; }
        type A=C;
        type B=D;
        interface Holder<T> { value:T; }
        const a:Holder<A>={value:new C()};
        const b:Holder<B>=a;
    "#;
    assert_eq!(codes(source), vec!["SELU0336"]);
}
#[test]
fn r05_api_scalar_alias() {
    let manifests = selu_manifest::load_manifest_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests"),
    )
    .unwrap();
    let manifest = manifests
        .iter()
        .find(|m| m.flavour == "retail" && !m.fixture)
        .unwrap();
    let source = "type N=number; const n:N=1; UnitName(n);";
    let p = selu_syntax::parse("t.selu", source).unwrap();
    let ds = selu_check::check(&p, manifest);
    assert!(ds.is_empty(), "{ds:?}");
    let p = selu_syntax::parse(
        "t.selu",
        "type F=(unit:string)=>string?; const f:F=UnitName;",
    )
    .unwrap();
    let ds = selu_check::check(&p, manifest);
    assert_eq!(
        ds.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        vec!["SELU0485"]
    );
}
fn rebuild(root: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(root)
        .arg("--manifest-dir")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .output()
        .unwrap()
}
#[test]
fn r16_alias_body_migration() {
    let (root, out) = project(
        "schema-body",
        &[(
            "main.selu",
            "type N=number; saved settings={values:new Map<string,N>()};",
        )],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let before = std::fs::read(root.join("selu.saved.toml")).unwrap();
    std::fs::write(
        root.join("src/main.selu"),
        "type N=string; saved settings={values:new Map<string,N>()};",
    )
    .unwrap();
    let out = rebuild(&root);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0468"));
    assert_eq!(std::fs::read(root.join("selu.saved.toml")).unwrap(), before);
}
#[test]
fn r16_inline_alias_compatibility() {
    let (root, out) = project(
        "schema-inline",
        &[(
            "main.selu",
            "saved settings={values:new Map<string,number>()};",
        )],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let before = std::fs::read(root.join("selu.saved.toml")).unwrap();
    std::fs::write(
        root.join("src/main.selu"),
        "type N=number; saved settings={values:new Map<string,N>()};",
    )
    .unwrap();
    let out = rebuild(&root);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(root.join("selu.saved.toml")).unwrap(), before);
}

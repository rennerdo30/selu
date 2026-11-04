mod common;
use common::*;
#[test]
fn named_shapes_and_generics() {
    assert_eq!(
        run(r#"
        type Pair<T extends number> = [T, T];
        interface Base { label: string; }
        interface Item extends Base { amount: number; note?: string; }
        const item: Item = { label: "ok", amount: 3 };
        function pair(): Pair<number> { return [item.amount, 4]; }
        const [a, b] = pair();
        print(item.label, a, b);
    "#),
        vec!["ok\t3\t4"]
    );
}
#[test]
fn recursive_members() {
    assert_eq!(
        run(r#"
        interface Node { value: number; next: Node?; }
        const node: Node = { value: 2, next: null };
        function read(n: Node): number {
            const next = n.next;
            if (next != null) { return next.value; }
            return n.value;
        }
        print(read(node));
    "#),
        vec!["2"]
    );
}
#[test]
fn unknown_requires_narrowing() {
    assert_eq!(
        run(r#"
        const type = (x: unknown) => "wrong";
        function read(x: unknown): number {
            if (typeof x == "number") { return x + 1; }
            return 0;
        }
        print(read(2), read("x"));
    "#),
        vec!["3\t0"]
    );
    assert_eq!(
        codes("function f(x: unknown): number { return x; }"),
        vec!["SELU0337"]
    );
}
#[test]
fn constraints_and_named_diagnostics() {
    assert_eq!(
        codes("type P<T extends number> = [T,T]; const p: P<string> = [\"a\",\"b\"];"),
        vec!["SELU0524"]
    );
    let src = "type Count = number; const n: Count = false;";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let ds = selu_check::check(&p, &manifest());
    assert_eq!(ds.len(), 1);
    assert!(ds[0].render(src).contains("Count"));
}
#[test]
fn type_only_erasure() {
    let src = "export type Count = number; export interface Record { value: Count; }";
    assert_eq!(compile(src), "");
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let (checked, _) = selu_check::check_module(&p, &manifest(), Default::default());
    assert!(!checked.has_errors());
    let bindings = selu_lower::ModuleBindings::new("types".into(), Default::default(), &p);
    let lua = selu_lower::lower_module(&p, &manifest(), &checked.types, bindings).lua;
    assert_eq!(lua, "local __selu_addon, __selu_ns = ...\n");
}
#[test]
fn contextual_aliases() {
    assert_eq!(
        codes("type Maybe = number?; const xs: Maybe[] = [];"),
        vec!["SELU0350"]
    );
    assert_eq!(
        run(r#"
        type Numbers = number[];
        type Transform = (x: number) => number;
        const xs: Numbers = [];
        const f: Transform = (x) => x + 1;
        type Maybe = number?;
        type Args = number[];
        let maybe: Maybe = null;
        function total(...values: Args): number { return values.length; }
        xs.push(f(3));
        print(xs[0], total(1, 2), maybe);
    "#),
        vec!["4\t2\tnil"]
    );
}
#[test]
fn nested_recursive_literals_and_generics() {
    assert_eq!(
        run(r#"
        interface Node<T> { value: T; next: Node<T>?; }
        const node: Node<number> = { value: 1, next: { value: 2, next: null } };
        const next = node.next;
        if (next != null) { print(next.value); }
    "#),
        vec!["2"]
    );
    assert_eq!(codes("type A = A;"), vec!["SELU0523"]);
    assert_eq!(codes("type A = A?;"), vec!["SELU0523"]);
    assert_eq!(
        selu_check::types::manifest_type("unknown"),
        selu_check::Type::Untyped
    );
}
#[test]
fn module_types_are_private_and_importable() {
    let source = selu_syntax::parse("dep.selu", "type Private = number; export type Pair<T> = [T,Private]; export interface Item { label: string; }").unwrap();
    let (checked, exports) = selu_check::check_module(&source, &manifest(), Default::default());
    assert!(!checked.has_errors(), "{:?}", checked.diagnostics);
    let src = "import { Pair as P, Item as end } from \"./dep\"; function f(): P<string> { return [\"ok\", 3]; } const [a,b] = f(); const item: end = { label: a }; print(item.label,b);";
    let p = selu_syntax::parse("main.selu", src).unwrap();
    let mut bindings = selu_check::ModuleBindings::default();
    bindings.imports.insert("./dep".into(), exports.clone());
    let (checked, _) = selu_check::check_module(&p, &manifest(), bindings);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut imports = std::collections::BTreeMap::new();
    imports.insert("./dep".into(), ("dep".into(), exports));
    let lower_bindings = selu_lower::ModuleBindings::new("main".into(), imports, &p);
    let lua = selu_lower::lower_module(&p, &manifest(), &checked.types, lower_bindings).lua;
    assert!(!lua.contains("local P"));
    assert!(!lua.contains("local Item"));
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("main.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), vec!["ok\t3"]);
}
#[test]
fn type_names_and_interface_methods() {
    assert_eq!(
        run(
            "type end = number; interface F { apply(x: number): number; } const f: F = { apply: (x: number) => x + 2 }; const n: end = f.apply(3); print(n);"
        ),
        vec!["5"]
    );
    assert_eq!(codes("type __seluHidden = number;"), vec!["SELU0318"]);
    assert_eq!(
        codes("function f(x: unknown) { return x + 1; }"),
        vec!["SELU0323"]
    );
}

mod common;
use common::{codes, run};

#[test]
fn fields_methods_and_constructor() {
    assert_eq!(
        run(r#"
class Point {
 x: number; y = 2;
 constructor(x: number) { this.x = x; }
 sum(): number { return this.x + this.y; }
}
on PLAYER_LOGIN() { const p: Point = new Point(3); print(p.sum()); }
"#),
        ["5"]
    );
}
#[test]
fn inheritance_static_and_arrows() {
    assert_eq!(
        run(r#"
class Base {
 x = 1;
 constructor(n: number) { this.x = n; }
 value(): number { return this.x; }
}
class Child extends Base {
 static count = 0;
 y = this.x + 1;
 constructor(n: number) { super(n); Child.count += 1; }
 value(): number { const f = () => this.y; return super.value() + f(); }
 static make(n: number): Child { return new Child(n); }
}
on PLAYER_LOGIN() { const c = Child.make(4); const b: Base = c; print(b.value(), Child.count); }
"#),
        ["9\t1"]
    );
}
#[test]
fn class_errors() {
    for (src, expected) in [
        ("class A { x; }", "SELU0421"),
        (
            "class A { x = 1; } const a = new A(); a.x = false;",
            "SELU0355",
        ),
        (
            "class A { f(n: number) {} } const a = new A(); a.f();",
            "SELU0334",
        ),
        (
            "class A {} const a = new A(); print(a.missing);",
            "SELU0427",
        ),
        (
            "class A {} class B extends A { constructor() { print(1); super(); } }",
            "SELU0424",
        ),
        ("class A { static f() { return this; } }", "SELU0425"),
        (
            "class A { f() { const g = function() { return this; }; } }",
            "SELU0425",
        ),
        ("print(this);", "SELU0425"),
        (
            "class A { f(n: number): number { return n; } } class B extends A { f(n: string): number { return 1; } }",
            "SELU0423",
        ),
        ("class A { constructor(n: number) {} } new A();", "SELU0429"),
        (
            "class A { constructor(n: number) {} } new A(false);",
            "SELU0430",
        ),
        ("class A { x: number = false; }", "SELU0422"),
        ("const a = new A(); class A {}", "SELU0313"),
    ] {
        assert_eq!(codes(src), [expected], "{src}");
    }
    let src = "class A {} const a = new A(); print(a.missing);";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let d = selu_check::check(&p, &common::manifest());
    assert_eq!(&src[d[0].span.as_ref().unwrap().range()], "a.missing");
}

#[test]
fn initializer_order_defaults_and_implicit_constructor() {
    assert_eq!(
        run(r#"
let order = "";
function mark(s: string): number { order += s; return 1; }
class A {
 a = mark("a");
 constructor(n: number = 3) { order += "A"; this.a = n; }
}
class B extends A {
 b = mark("b");
 constructor(n: number = 7) { super(n); order += "B"; }
}
class C extends B { c = mark("c"); }
on PLAYER_LOGIN() { const c = new C(); print(order, c.a); }
"#),
        ["aAbBc\t7"]
    );
}
#[test]
fn forward_class_and_shadowed_class_types() {
    assert_eq!(
        run(r#"
function make(): A { return new A(); }
class A { x = 1; }
on PLAYER_LOGIN() {
 const a: A = make();
 { class A { x = "inner"; } const a: A = new A(); print(a.x); }
 const b: A = new A(); print(a.x, b.x);
}
"#),
        ["inner", "1\t1"]
    );
}
#[test]
fn constructor_returns_and_static_initializers() {
    assert_eq!(
        run(r#"
class A {
 static answer = A.make();
 static make(): number { return 42; }
 constructor() { return; }
}
on PLAYER_LOGIN() { const a = new A(); print(A.answer); }
"#),
        ["42"]
    );
}

#[test]
fn receiver_hygiene_and_readable_methods() {
    let src = r#"
class Point {
 x = 5;
 constructor(self: number) { this.x = self; }
 length(): number { return this.x; }
 callback(): number { const f = (self: number) => this.x + self; return f(7); }
 localValue(): number { const self = 9; return this.x + self; }
}
on PLAYER_LOGIN() { const p = new Point(3); print(p.length(), p.callback(), p.localValue()); }
"#;
    assert_eq!(run(src), ["3\t10\t12"]);
    let lua = common::compile(src);
    assert!(lua.contains("function Point:length()\n"));
    assert!(lua.contains("function Point.callback(__selu_self)\n"));
}
#[test]
fn class_before_function() {
    assert_eq!(
        run(
            "class A { x = 8; } function make(): A { return new A(); } on PLAYER_LOGIN() { print(make().x); }"
        ),
        ["8"]
    );
}
#[test]
fn exported_class_type_predeclaration() {
    assert_eq!(
        codes("function make(): A { return new A(); } export class A {}"),
        ["SELU0390"]
    );
}

#[test]
fn reject_class_runtime_collisions_and_repeated_super() {
    assert_eq!(codes("class A { static new() {} }"), ["SELU0431"]);
    assert_eq!(
        codes("class A {} class B extends A { constructor() { super(); super(); } }"),
        ["SELU0432"]
    );
    assert_eq!(
        codes("class A { constructor(end: number) {} }"),
        ["SELU0431"]
    );
}
#[test]
fn defaults_precede_instance_initializers() {
    assert_eq!(
        run(r#"
let order = "";
function mark(s: string): number { order += s; return 1; }
class A { x = mark("x"); constructor(n: number = mark("d")) { order += "A"; } }
on PLAYER_LOGIN() { const a = new A(); print(order); }
"#),
        ["dxA"]
    );
}
#[test]
fn super_captures_the_base_class_value() {
    assert_eq!(
        run(r#"
class Base { value(): number { return 2; } }
const Alias = Base;
class Child extends Alias {
 value(): number { const Base = 100; return super.value() + Base; }
}
on PLAYER_LOGIN() { print(new Child().value()); }
"#),
        ["102"]
    );
}

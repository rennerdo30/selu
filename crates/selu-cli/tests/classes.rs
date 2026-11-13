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
    assert!(lua.contains("function Point.methods:length()\n"));
    assert!(lua.contains("function Point.methods.callback(__selu_self)\n"));
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
        Vec::<String>::new()
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
fn base_initializers_precede_defaults() {
    assert_eq!(
        run(r#"
let order = "";
function mark(s: string): number { order += s; return 1; }
class A { x = mark("x"); constructor(n: number = mark("d")) { order += "A"; } }
on PLAYER_LOGIN() { const a = new A(); print(order); }
"#),
        ["xdA"]
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

#[test]
fn review_static_method_collision() {
    assert_eq!(
        run(
            "class A { f() { return 1; } static f() { return 2; } } on PLAYER_LOGIN() { print(new A().f(), A.f()); }"
        ),
        ["1\t2"]
    );
}
#[test]
fn review_static_field_collision() {
    assert_eq!(
        run(
            "class A { f() { return 1; } static f = 2; } on PLAYER_LOGIN() { print(new A().f(), A.f); }"
        ),
        ["1\t2"]
    );
}
#[test]
fn review_static_inheritance_collision() {
    assert_eq!(
        run(
            "class A { static f() { return 2; } } class B extends A { f() { return 1; } } on PLAYER_LOGIN() { print(new B().f(), B.f()); }"
        ),
        ["1\t2"]
    );
}
#[test]
fn review_callable_field() {
    assert_eq!(
        run(
            "class A { x = 5; f = (n: number) => this.x + n; } on PLAYER_LOGIN() { print(new A().f(2)); }"
        ),
        ["7"]
    );
}
#[test]
fn review_initializer_parameter_scope() {
    assert_eq!(
        run(
            "const x = 10; class A { y = x; constructor(x: number) {} } on PLAYER_LOGIN() { print(new A(99).y); }"
        ),
        ["10"]
    );
}
#[test]
fn review_initializer_class_scope() {
    assert_eq!(
        run(
            "class A { static n = 10; x = A.n; constructor(A: number) {} } on PLAYER_LOGIN() { print(new A(99).x); }"
        ),
        ["10"]
    );
}
#[test]
fn review_outer_self_field() {
    assert_eq!(
        run("const self = 10; class A { y = self; } on PLAYER_LOGIN() { print(new A().y); }"),
        ["10"]
    );
}
#[test]
fn review_outer_self_method() {
    assert_eq!(
        run(
            "const self = 10; class A { f() { return self; } } on PLAYER_LOGIN() { print(new A().f()); }"
        ),
        ["10"]
    );
}
#[test]
fn review_nested_self_class() {
    assert_eq!(
        run(
            "class A { x = 3; f() { class self {} return this.x; } } on PLAYER_LOGIN() { print(new A().f()); }"
        ),
        ["3"]
    );
}
#[test]
fn review_outer_self_default() {
    assert_eq!(
        run(
            "const self = 10; class A { f(n = self) { return n; } } on PLAYER_LOGIN() { print(new A().f()); }"
        ),
        ["10"]
    );
}
#[test]
fn review_field_override() {
    assert_eq!(
        codes("class A { x = 5; } class B extends A { x = \"bad\"; }"),
        ["SELU0423"]
    );
}
#[test]
fn review_method_field_override() {
    assert_eq!(
        codes("class A { f() { return 1; } } class B extends A { f = 5; }"),
        ["SELU0423"]
    );
}
#[test]
fn review_annotated_override() {
    assert_eq!(
        codes("class A { x: number = 5; } class B extends A { x: string = \"bad\"; }"),
        ["SELU0423"]
    );
}
#[test]
fn review_static_override() {
    assert_eq!(
        codes("class A { static f(n: number) {} } class B extends A { static f(n: string) {} }"),
        ["SELU0423"]
    );
}
#[test]
fn review_this_super_argument() {
    assert!(codes("class A { x = 1; constructor(n: number) {} } class B extends A { constructor() { super(this.x); } }").contains(&"SELU0433".into()));
}
#[test]
fn review_this_derived_default() {
    assert!(
        codes("class A {} class B extends A { x = 1; constructor(n = this.x) { super(); } }")
            .contains(&"SELU0433".into())
    );
}
#[test]
fn review_delayed_class() {
    assert_eq!(
        run(
            "class A { b = new B(); } class B { x = 7; } on PLAYER_LOGIN() { print(new A().b.x); }"
        ),
        ["7"]
    );
}
#[test]
fn review_delayed_local() {
    assert_eq!(
        run("class A { x = y; } const y = 7; on PLAYER_LOGIN() { print(new A().x); }"),
        ["7"]
    );
}
#[test]
fn review_split_constructor_forward() {
    assert_eq!(
        run(
            "class A { f: () => number; constructor(f: () => number) { this.f = f; } } class B extends A { constructor() { super(() => x); const x = 5; } } on PLAYER_LOGIN() { print(new B().f()); }"
        ),
        ["5"]
    );
}
#[test]
fn review_checked_method_defaults() {
    assert_eq!(
        codes("class A { f(n = 1) {} } new A().f(\"bad\");"),
        ["SELU0335"]
    );
}
#[test]
fn review_checked_constructor_defaults() {
    assert_eq!(
        codes("class A { constructor(n = 1) {} } new A(\"bad\");"),
        ["SELU0430"]
    );
}
#[test]
fn review_function_decl_this() {
    assert_eq!(
        codes("class A { f() { function g() { return this; } } }"),
        ["SELU0425"]
    );
}
#[test]
fn review_nested_super_context() {
    assert_eq!(
        run(
            "class A {} class B extends A { f() { class C extends A { constructor() { super(); } } return new C(); } } on PLAYER_LOGIN() { const c = new B().f(); print(1); }"
        ),
        ["1"]
    );
}
#[test]
fn review_class_to_shape() {
    assert_eq!(
        run(
            "class A { x = 5; } function f(a: { x: number }) { return a.x; } on PLAYER_LOGIN() { print(f(new A())); }"
        ),
        ["5"]
    );
}
#[test]
fn review_object_to_class_help() {
    let src = "class A { x = 5; } const a: A = { x: 5 };";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let d = selu_check::check(&p, &common::manifest());
    assert!(d[0].help.as_ref().unwrap().contains("new A("));
}
#[test]
fn review_runtime_shadowing() {
    assert_eq!(
        run("class setmetatable {} on PLAYER_LOGIN() { const a = new setmetatable(); print(1); }"),
        ["1"]
    );
}
#[test]
fn review_diagnostic_span_help() {
    let src = "class A { constructor(end: number) {} }";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let d = selu_check::check(&p, &common::manifest());
    assert_eq!(&src[d[0].span.as_ref().unwrap().range()], "end");
}
#[test]
fn review_named_constructor() {
    let lua = common::compile("class Point { constructor(x: number, y: number) {} }");
    assert!(lua.contains("function Point.new(x, y)"));
}
#[test]
fn review_standard_derived_order() {
    assert_eq!(
        run(r#"
let log = "";
function mark(s: string): number { log += s; return 0; }
class A { a = mark("a"); constructor() { mark("A"); } }
class B extends A { b = mark("b"); constructor(x: number = mark("d")) { super(); mark("B"); } }
on PLAYER_LOGIN() { const b = new B(); print(log); }
"#),
        ["daAbB"]
    );
}
#[test]
fn review_method_call_markers() {
    let src = "class A { m() { return 1; } f = () => 2; } class B extends A {} on PLAYER_LOGIN() { const p: A = new B(); print(p.m(), p.f()); }";
    assert_eq!(run(src), ["1\t2"]);
    let lua = common::compile(src);
    assert!(lua.contains("p:m()"));
    assert!(lua.contains("p.f()"));
}
#[test]
fn review_object_class_help_all_sites() {
    for src in [
        "class A { x = 5; } function f(a: A) {} f({ x: 5 });",
        "class A { x = 5; } let a: A = new A(); a = { x: 5 };",
        "class A { x = 5; } function f(): A { return { x: 5 }; }",
    ] {
        let p = selu_syntax::parse("t.selu", src).unwrap();
        let d = selu_check::check(&p, &common::manifest());
        assert_eq!(d.len(), 1);
        let h = d[0].help.as_ref().unwrap();
        assert!(
            h.contains("metatable") && h.contains("new A("),
            "{src}: {h}"
        );
    }
}
#[test]
fn review_early_annotation_shape() {
    assert_eq!(
        run(
            "function f(a: A): number { const p: { x: number } = a; return p.x; } class A { x = 5; } on PLAYER_LOGIN() { print(f(new A())); }"
        ),
        ["5"]
    );
}
#[test]
fn review_function_shapes_rejected() {
    assert_eq!(
        codes(
            "class A { x = 5; f() {} } function take(a: { x: number; f: () => void }) {} take(new A());"
        ),
        ["SELU0335"]
    );
}
#[test]
fn review_eager_static_initializer() {
    assert_eq!(
        codes("class A { static x = y; } const y = 5;"),
        ["SELU0313"]
    );
}
#[test]
fn review_forward_declarations_preserve_super_order() {
    assert_eq!(
        run(
            "class A { x = 1; f: () => number; constructor(f: () => number) { this.f = f; } } class B extends A { y = this.x + 1; constructor() { super(() => x); const x = 5; } } on PLAYER_LOGIN() { const b = new B(); print(b.y, b.f()); }"
        ),
        ["2\t5"]
    );
}

#[test]
fn review_runtime_member_names() {
    assert_eq!(codes("class A { static __fields = 1; }"), ["SELU0431"]);
    assert_eq!(codes("class A { static __proto = 1; }"), ["SELU0431"]);
}

#[test]
fn review_constructor_and_override_help() {
    for (src, code) in [
        ("class A { constructor(n: number) {} } new A();", "SELU0429"),
        (
            "class A { constructor(n: number) {} } new A(false);",
            "SELU0430",
        ),
        (
            "class A { x = 5; } class B extends A { x = false; }",
            "SELU0423",
        ),
    ] {
        let p = selu_syntax::parse("t.selu", src).unwrap();
        let ds = selu_check::check(&p, &common::manifest());
        let d = ds.iter().find(|d| d.code == code).unwrap();
        let help = d.help.as_ref().unwrap();
        assert!(
            help.contains("expected") && help.contains("number"),
            "{help}"
        );
    }
}
#[test]
fn review_immediate_this_before_super() {
    for src in [
        "class A { x = 1; constructor(n: number) {} } class B extends A { constructor() { super((() => this.x)()); } }",
        "class A { value() { return 1; } constructor(n: number) {} } class B extends A { constructor() { super(super.value()); } }",
    ] {
        assert!(codes(src).contains(&"SELU0433".into()), "{src}");
    }
}

fn class_module_sources(base: &str, main: &str) -> (Vec<String>, String) {
    use std::collections::BTreeMap;
    let manifest = common::manifest();
    let base = selu_syntax::parse("base.selu", base).unwrap();
    let main = selu_syntax::parse("main.selu", main).unwrap();
    let (checked, exports) = selu_check::check_module(&base, &manifest, Default::default());
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let base_lua = selu_lower::lower_module(
        &base,
        &manifest,
        &checked.types,
        selu_lower::ModuleBindings::new("base".into(), BTreeMap::new(), &base),
    )
    .lua;
    let mut bindings = selu_check::ModuleBindings::default();
    bindings.imports.insert("./base".into(), exports.clone());
    let (checked, _) = selu_check::check_module(&main, &manifest, bindings);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let main_lua = selu_lower::lower_module(
        &main,
        &manifest,
        &checked.types,
        selu_lower::ModuleBindings::new(
            "main".into(),
            BTreeMap::from([("./base".into(), ("base".into(), exports))]),
            &main,
        ),
    )
    .lua;
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon(
        "Classes",
        &[
            ("base.lua".into(), base_lua),
            ("main.lua".into(), main_lua.clone()),
        ],
    )
    .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    (sim.prints(), main_lua)
}
#[test]
fn integrated_exported_classes_and_same_names() {
    let (out, _) = class_module_sources(
        "export class A { x: number; constructor(x: number) { this.x = x; } value(n: number = 1): number { return this.x + n; } static make(n: number): A { return new A(n); } }",
        "export class A extends Base { constructor(x: number) { super(x); } value(n: number = 1): number { return super.value(n) + 10; } } import { A as Base } from \"./base\"; on PLAYER_LOGIN() { const b: Base = Base.make(4); const a: A = new A(5); const up: Base = a; print(b.value(), up.value(2)); }",
    );
    assert_eq!(out, ["5\t17"]);
}
#[test]
fn integrated_mutable_import_class_scopes() {
    let (out, _) = class_module_sources(
        "export let count = 3;",
        r#"import { count } from "./base";
class A { x: number; constructor(count: number) { this.x = count; } value(count: number): number { const n = count; return n + this.x; } localValue(): number { const count = 7; return count; } }
on PLAYER_LOGIN() { const a = new A(5); print(a.value(6),a.localValue(),count); }"#,
    );
    assert_eq!(out, ["11\t7\t3"]);
}
#[test]
fn integrated_module_runtime_capture() {
    let (out, lua) = class_module_sources(
        "export const n = 7;",
        r#"import { n } from "./base"; class setmetatable {} on PLAYER_LOGIN() { const a = new setmetatable(); print(n); }"#,
    );
    assert_eq!(out, ["7"]);
    assert!(lua.starts_with(
        "local __selu_addon, __selu_ns = ...\nlocal __selu_setmetatable = setmetatable\n"
    ));
}
#[test]
fn integrated_wow_class_method() {
    let lua = common::compile(
        "class A { x = 4; message(): string { log.info`count ${this.x}`; return L`Count ${this.x}`; } } on PLAYER_LOGIN() { print(new A().message()); }",
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon("ClassWow",&[("main.lua".into(),lua),("verify.lua".into(),"local _, ns = ...; function VerifyClassLog() assert(#ns.__selu_wow.buffer == 1); assert(ns.__selu_wow.buffer[1].message == 'count 4') end".into())]).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.eval("VerifyClassLog()").unwrap();
    assert_eq!(sim.prints(), ["Count 4"]);
}

#[test]
fn readable_standalone_class() {
    assert_eq!(
        common::compile(
            "class Tracker { total = 0; add(amount: number) { this.total += amount; } }"
        ),
        r#"local __selu_setmetatable = setmetatable
local Tracker = {}
Tracker.methods = {}
Tracker.methods.__index = Tracker.methods
function Tracker.new()
  local self = __selu_setmetatable({}, Tracker.methods)
  self.total = 0
  return self
end
function Tracker.methods:add(amount)
  self.total = self.total + amount
end
"#
    );
}
#[test]
fn readable_stable_compound() {
    assert_eq!(
        common::compile("const p = { x: 1 }; const amount = 2; p.x += amount;"),
        "local p = { x = 1 }\nlocal amount = 2\np.x = p.x + amount\n"
    );
}
#[test]
fn integrated_wow_module_fallback() {
    let src = r#"class A { x = 4; register(): string { command("/class", (args) => { print(args); }); log.info`count ${this.x}`; return L`Count ${this.x}`; } } on PLAYER_LOGIN() { print(new A().register()); }"#;
    let p = selu_syntax::parse("main.selu", src).unwrap();
    let m = common::manifest();
    let (checked, _) = selu_check::check_module(&p, &m, Default::default());
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let lua = selu_lower::lower_module(
        &p,
        &m,
        &checked.types,
        selu_lower::ModuleBindings::new("main".into(), Default::default(), &p),
    )
    .lua;
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("main.lua".into(), lua)]).unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.slash("/class hello").unwrap();
    assert_eq!(sim.prints(), ["Count 4", "hello"]);
}
#[test]
fn readable_example_hierarchy_exact() {
    let source = include_str!("../../../examples/classes/src/main.selu");
    let mut program = selu_syntax::parse("example.selu", source).unwrap();
    program
        .stmts
        .retain(|s| matches!(s.kind, selu_syntax::StmtKind::Class(_)));
    let manifest = common::manifest();
    let checked = selu_check::check_program(&program, &manifest);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let lua = selu_lower::lower(&program, &manifest, &checked.types).lua;
    assert_eq!(
        lua,
        r#"local __selu_setmetatable = setmetatable
local Tracker = {}
Tracker.methods = {}
Tracker.methods.__index = Tracker.methods
function Tracker.__init(self, name)
  self.total = 0
  self.name = name
  Tracker.created = Tracker.created + 1
end
function Tracker.new(name)
  local self = __selu_setmetatable({}, Tracker.methods)
  Tracker.__init(self, name)
  return self
end
function Tracker.methods:record(amount)
  self.total = self.total + amount
end
function Tracker.methods:summary()
  return string.format("%s: %s", self.name, self.total)
end
Tracker.created = 0
local LootTracker = {}
LootTracker.methods = {}
LootTracker.methods.__index = LootTracker.methods
__selu_setmetatable(LootTracker, { __index = Tracker })
__selu_setmetatable(LootTracker.methods, { __index = Tracker.methods })
function LootTracker.new(name)
  local self = __selu_setmetatable({}, LootTracker.methods)
  Tracker.__init(self, name)
  self.multiplier = 2
  return self
end
function LootTracker.make()
  return LootTracker.new("Loot")
end
function LootTracker.methods:record(amount)
  Tracker.methods.record(self, amount * self.multiplier)
end
function LootTracker.methods:collect()
  local add = function(amount)
    self:record(amount)
  end
  add(3)
  add(2)
end
"#
    );
}
#[test]
fn readable_alias_only_for_shadowed_class() {
    let src = "class A { constructor(A: number) {} }";
    assert_eq!(
        common::compile(src),
        r#"local __selu_setmetatable = setmetatable
local A = {}
local __selu_class_6 = A
__selu_class_6.methods = {}
__selu_class_6.methods.__index = __selu_class_6.methods
function A.new(A)
  local self = __selu_setmetatable({}, __selu_class_6.methods)
  return self
end
"#
    );
    assert_eq!(
        run(
            "class A { constructor(A: number) {} } on PLAYER_LOGIN() { const a = new A(99); print(7); }"
        ),
        ["7"]
    );
}
#[test]
fn readable_shadowed_base_alias() {
    let src = "class Base { f(): number { return 1; } } class Child extends Base { f(Base: number = 2): number { return super.f() + Base; } } on PLAYER_LOGIN() { print(new Child().f()); }";
    assert_eq!(run(src), ["3"]);
    let lua = common::compile(src);
    assert!(lua.contains("local __selu_base_"));
}
#[test]
fn integrated_class_identities_are_nominal_across_modules() {
    use std::collections::BTreeMap;
    let manifest = common::manifest();
    let base = selu_syntax::parse("base.selu", "export class A { x = 1; }").unwrap();
    let (_, exports) = selu_check::check_module(&base, &manifest, Default::default());
    let main=selu_syntax::parse("main.selu","export class A { x = 1; } import { A as Other } from \"./base\"; const a: Other = new A();").unwrap();
    let checked = selu_check::check_module(
        &main,
        &manifest,
        selu_check::ModuleBindings {
            imports: BTreeMap::from([("./base".into(), exports)]),
            ..Default::default()
        },
    )
    .0;
    assert_eq!(
        checked
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["SELU0336"]
    );
}
#[test]
fn integrated_imported_type_before_import() {
    let (out, _) = class_module_sources(
        "export class A { x = 7; }",
        r#"function read(p: Base): number { return p.x; } import { A as Base } from "./base"; on PLAYER_LOGIN() { print(read(new Base())); }"#,
    );
    assert_eq!(out, ["7"]);
}

#[test]
fn integrated_function_signature_before_import() {
    use std::collections::BTreeMap;
    let m = common::manifest();
    let p = selu_syntax::parse(
        "base.selu",
        "export function f(n: number): number { return n; }",
    )
    .unwrap();
    let (_, exports) = selu_check::check_module(&p, &m, Default::default());
    let p = selu_syntax::parse(
        "main.selu",
        r#"function wrapper(): number { return f("bad"); } import { f } from "./base";"#,
    )
    .unwrap();
    let checked = selu_check::check_module(
        &p,
        &m,
        selu_check::ModuleBindings {
            imports: BTreeMap::from([("./base".into(), exports)]),
            ..Default::default()
        },
    )
    .0;
    assert_eq!(
        checked
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["SELU0335"]
    );
}
#[test]
fn readable_method_table_keeps_instance_names_separate() {
    assert_eq!(
        run(
            "class A { methods = 7; } class B { methods(): number { return 8; } } on PLAYER_LOGIN() { print(new A().methods,new B().methods()); }"
        ),
        ["7\t8"]
    );
    assert_eq!(codes("class A { static methods = 7; }"), ["SELU0431"]);
}
#[test]
fn readable_unrelated_scopes_do_not_force_aliases() {
    let src =
        "class Base { x = 1; } function unrelated() { let Base = 3; } class Child extends Base {}";
    assert!(!common::compile(src).contains("__selu_base_"));
}
#[test]
fn readable_alias_in_another_scope_does_not_choose_the_base() {
    assert_eq!(
        run(
            "class Base { x = 1; } class Other { x = 2; } const Alias = Base; function unrelated() { const Alias = Other; } class Child extends Alias {} on PLAYER_LOGIN() { print(new Child().x); }"
        ),
        ["1"]
    );
}
#[test]
fn a_class_instance_fits_an_optional_option_of_its_class() {
    // An object literal widens a class value to the field's own class type
    // (CreateListWell(parent, { collapse: CollapseStore() })).
    assert!(
        codes(
            "class State { open = false; } type Options = { width?: number; state?: State }; function take(o: Options?): number { return 1; } const s = new State(); take({ width: 2, state: s });"
        )
        .is_empty()
    );
    // A class value still keeps its own type where no class is expected.
    assert!(!codes("class State { open = false; } const s = new State(); const o: { state: number } = { state: s };").is_empty());
}

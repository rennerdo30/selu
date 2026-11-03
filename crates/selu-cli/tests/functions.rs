mod common;
use common::{codes, compile, run};

#[test]
fn closures_and_context() {
    assert_eq!(
        run(
            "let x = 1; const f: (n: number) => number = n => n + x; x = 4; const g = function (n: number) { return f(n); }; print(g(3));"
        ),
        ["7"]
    );
}
#[test]
fn contextual_argument_and_forward_capture() {
    assert_eq!(
        run(
            "function apply(f: (n: number) => number) { return f(2); } const get = () => later; let later = 5; print(apply(n => n + get()));"
        ),
        ["7"]
    );
}
#[test]
fn defaults_and_rest() {
    assert_eq!(
        run(
            "function f(x: number = 7, ...rest: number[]) { const [a, b] = rest; print(x); print(a); print(b); } f(); f(2, 3, 4);"
        ),
        ["7", "nil", "nil", "2", "3", "4"]
    );
}
#[test]
fn defaults_are_lazy_and_see_prior_parameters() {
    assert_eq!(
        run(
            "let calls = 0; function d() { calls = calls + 1; return 9; } const f = (x: boolean = false, y: number = d()) => { print(x); print(y); }; f(true, 4); f(); function g(x: number, y: number = x + 1) { print(y); } g(3); print(calls);"
        ),
        ["true", "4", "false", "9", "4", "1"]
    );
}
#[test]
fn tuple_returns_and_forwarding() {
    let src = "function pair(): [number, string] { return [1, \"a\"]; } function forward(): [number, string] { return pair(); } const [a, b] = forward(); print(a); print(b); pair();";
    assert_eq!(run(src), ["1", "a"]);
    assert!(compile(src).contains("return 1, \"a\""));
}
#[test]
fn destructure_game() {
    assert_eq!(
        run_host(
            "const [a, , c] = GetTime(); print(a); print(c);",
            "GetTime = function() return 1, 2, 3 end"
        ),
        ["1", "3"]
    );
}
// Fixture tables supplied by the host stand in for package B's object/array literals.
fn run_host(src: &str, prelude: &str) -> Vec<String> {
    let lua = compile(src);
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("host.lua".into(), prelude.into()), ("t.lua".into(), lua)])
        .unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    sim.prints()
}
#[test]
fn destructure_values_once() {
    assert_eq!(
        run_host(
            "const { x, y: renamed } = UIParent.next(); print(x); print(renamed); const xs: [number, string] = GameTooltip; const [a, b] = xs; print(a); print(b);",
            "local count = 0; UIParent = {next = function(self) count = count + 1; print(count); return {x=4,y=5} end}; GameTooltip = {7, 'b'};"
        ),
        ["1", "4", "5", "7", "b"]
    );
}
#[test]
fn own_object_dot_and_game_colon() {
    let src = "const own: { f: (x: number) => number } = UIParent; print(own.f(3)); const frame = CreateFrame(\"Frame\"); frame.Hide(); print(frame.IsShown());";
    assert_eq!(
        run_host(src, "UIParent = {f = function(x) return x + 1 end};"),
        ["4", "false"]
    );
    let lua = compile(src);
    assert!(lua.contains("own.f(3)"));
    assert!(lua.contains("frame:Hide()"));
}
#[test]
fn rejects_bad_calls() {
    assert!(codes("const f = (n: number) => n; f(\"bad\");").contains(&"SELU0335".into()));
    assert!(
        codes("function f(x: number = 1, ...xs: number[]) {} f(1, \"bad\");")
            .contains(&"SELU0335".into())
    );
    assert!(codes("function f(x: number) {} f();").contains(&"SELU0334".into()));
    assert!(codes("function f(x: number) {} f(1, 2);").contains(&"SELU0334".into()));
    assert!(codes("const f: (n: number) => string = n => n + 1;").contains(&"SELU0337".into()));
}
#[test]
fn rejects_bad_parameters_and_patterns() {
    assert!(codes("const p: {y: number} = UIParent; const {x} = p;").contains(&"SELU0375".into()));
    assert!(
        codes("function f(): [number, string] { return [1, \"a\"]; } const [a, b, c] = f();")
            .contains(&"SELU0373".into())
    );
    assert!(codes("function f(x: number = y, y: number = 1) {}").contains(&"SELU0313".into()));
    assert!(codes("const f: () => number = (x: number) => x;").contains(&"SELU0336".into()));
    assert!(codes("function f(...xs: number) {}").contains(&"SELU0370".into()));
    assert!(codes("function f(...xs: number[], x: number) {}").contains(&"SELU0371".into()));
    assert!(codes("function f(x: number = \"bad\") {}").contains(&"SELU0372".into()));
    assert!(codes("const [x] = 1;").contains(&"SELU0374".into()));
    assert!(codes("const {x} = 1;").contains(&"SELU0376".into()));
    assert!(
        codes("function f(): [number, string] { return [1, 2]; }").contains(&"SELU0337".into())
    );
}
#[test]
fn tuple_call_single_value_is_error() {
    let src = "function f(): [number, string] { return [1, \"a\"]; } const x = f();";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let ds = selu_check::check(&p, &common::manifest());
    let d = ds.iter().find(|d| d.code == "SELU0377").unwrap();
    assert_eq!(
        d.span.as_ref().unwrap().start as usize,
        src.rfind("f()").unwrap()
    );
    assert!(codes("function f(): [number, string] { return [1, \"a\"]; } function g(): [number, number] { return f(); }").contains(&"SELU0337".into()));
}

#[test]
fn default_closure_captures_rest() {
    assert_eq!(
        run(
            "function f(get: () => number[] = () => rest, ...rest: number[]) { const [a] = get(); print(a); } f();"
        ),
        ["nil"]
    );
}

#[test]
fn default_parameter_type_is_inferred() {
    assert_eq!(
        run(
            "function f(x = 4) { return x + 1; } print(f()); const g = (s = \"a\") => s + \"b\"; print(g());"
        ),
        ["5", "ab"]
    );
    assert!(codes("function f(x = 4) {} f(\"bad\");").contains(&"SELU0335".into()));
}

#[test]
fn tuple_value_return_and_exact_result_count() {
    assert_eq!(
        run_host(
            "const xs: [number, string] = UIParent; function f(): [number, string] { return xs; } const [a, b] = f(); print(a); print(b); function g(): [number, number] { return [1, GetTime()]; } const [c, d] = g(); print(c); print(d);",
            "UIParent = {7, 'b'}; GetTime = function() return 2, 3 end;"
        ),
        ["7", "b", "1", "2"]
    );
}

#[test]
fn review_rest_assignment_positions() {
    let src = "let g = (x: number, ...xs: string[]) => {}; function f(x: number, y: number = 1, ...xs: string[]) { print(y + 1); } g = f; g(1, \"bad\");";
    assert!(codes(src).contains(&"SELU0336".into()));
}
#[test]
fn review_recursive_arrow() {
    assert_eq!(
        run(
            "const f: (n: number) => number = n => { if (n == 0) { return 42; } return f(n - 1); }; print(f(1));"
        ),
        ["42"]
    );
}
#[test]
fn review_forward_destructure() {
    assert_eq!(
        run("const get = () => a; const [a] = GetTime(); print(get());"),
        ["0"]
    );
}
#[test]
fn review_tuple_conditions() {
    let actual: Vec<bool> = [
        "if (pair()) { print(1); }",
        "const b = !pair();",
        "const b = pair() && true;",
        "const b = true || pair();",
    ]
    .iter()
    .map(|use_| {
        let src = format!("function pair(): [number, number] {{ return [1, 2]; }} {use_}");
        codes(&src).contains(&"SELU0377".into())
    })
    .collect();
    assert_eq!(actual, [true, true, true, true]);
}
#[test]
fn review_empty_destructure() {
    assert_eq!(
        run_host(
            "const [] = GetTime();",
            "GetTime = function() print('once'); return 1 end"
        ),
        ["once"]
    );
}
#[test]
fn review_written_rest_type() {
    assert!(codes("const f: (...xs: number[]) => void = (...xs: number[]) => {};").is_empty());
}
#[test]
fn review_nullable_rest() {
    assert!(codes("function f(...xs: number?[]) {} f(1, null, 2);").contains(&"SELU0370".into()));
}
#[test]
fn review_tuple_result_count() {
    let src = "function f(): [number, number] { return [1, GetTime()]; } global ReviewPair = f;";
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[
        (
            "host.lua".into(),
            "GetTime = function() return 2, 3 end".into(),
        ),
        ("t.lua".into(), compile(src)),
        (
            "count.lua".into(),
            "print(select('#', ReviewPair()))".into(),
        ),
    ])
    .unwrap();
    assert_eq!(sim.prints(), ["2"]);
}
#[test]
fn review_missing_field_span() {
    let src = "const p: { x: number } = UIParent; const { missing: renamed } = p;";
    let p = selu_syntax::parse("t.selu", src).unwrap();
    let ds = selu_check::check(&p, &common::manifest());
    let d = ds.iter().find(|d| d.code == "SELU0375").unwrap();
    let span = d.span.as_ref().unwrap();
    assert_eq!(&src[span.start as usize..span.end as usize], "missing");
}
#[test]
fn review_default_null_uses_default() {
    assert_eq!(
        run("function f(x: number? = 7) { print(x); } f(null);"),
        ["7"]
    );
}

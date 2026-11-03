mod common;
use common::*;
use selu_sim::Sim;
#[test]
fn slash_and_context() {
    let lua = compile("command(\"/test\", \"/t\", (args) => { print(args.upper()); });");
    let sim = Sim::new().unwrap();
    sim.load_addon("Test", &[("t.lua".into(), lua)]).unwrap();
    sim.slash("/t hello").unwrap();
    assert_eq!(sim.prints(), ["HELLO"]);
}
#[test]
fn hooks() {
    let lua = compile(
        "hook(\"GetTime\", () => { print(\"hook\"); }); print(GetTime()); const f = CreateFrame(\"Frame\"); hookScript(f, \"OnShow\", (self) => { print(\"shown\"); }); f.Hide(); f.Show();",
    );
    let sim = Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["hook", "0", "shown"]);
}
#[test]
fn logging_and_locale() {
    assert_eq!(
        run(
            "let n = 0; function next(): number { n += 1; return n; } log.debug`ignored ${next()}`; log.info`100% ${true}`; print(L`100% ${next()} missing`); print(n);"
        ),
        ["100% 1 missing", "1"]
    );
}
#[test]
fn shared_runtime_translations_and_buffer() {
    let sim = Sim::new().unwrap();
    let a = compile("log.warnOnce`once`; command(\"/one\", () => {});");
    let b = compile(
        "log.warnOnce`once`; command(\"/two\", () => {}); print(L`${1} then ${2}%`); for (let i = 0; i < 205; i++) { log.info`entry ${i}`; }",
    );
    sim.load_addon("Shared", &[("setup.lua".into(), "local _, ns = ...; ns.wow = nil".into()),("a.lua".into(), a), ("translation.lua".into(), "local _, ns = ...; assert(#ns.wow.buffer == 1); ns.wow.locales.enUS = {['{0} then {1}%'] = '{1}% before {0}'}".into()), ("b.lua".into(),b), ("verify.lua".into(), "local _, ns = ...; assert(#ns.wow.buffer == 200); assert(ns.wow.buffer[1].message == 'entry 5'); assert(ns.wow.buffer[200].message == 'entry 204'); assert(ns.wow.commands == 2); assert(rawget(_G, 'SLASH_SELU_Shared_1') == nil); assert(SLASH_SELU_Shared_11 == '/one'); assert(SLASH_SELU_Shared_21 == '/two')".into())]).unwrap();
    assert_eq!(sim.prints(), ["2% before 1"]);
}
#[test]
fn method_hooks_preserve_arguments_results_and_order() {
    let sim = Sim::new().unwrap();
    let lua = compile(
        "global object = CreateFrame(\"Frame\"); hook(object, \"Test\", (self, x) => { print(x); }); print(object.Test(7));",
    );
    sim.eval("local create = CreateFrame; function CreateFrame(...) local f = create(...); function f:Test(x) print('original'); return x + 1 end; return f end").unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["original", "7", "8"]);
}
#[test]
fn shadows_and_errors() {
    assert_eq!(
        run(
            "function command(x: string) { print(x); } command(\"local\"); const log = { info: command }; log.info(\"field\");"
        ),
        ["local", "field"]
    );
    assert_eq!(codes("command();"), ["SELU0440"]);
    assert_eq!(codes("command(\"bad\", () => {});"), ["SELU0441"]);
    assert_eq!(codes("command(\"/a\", 1);"), ["SELU0442"]);
    assert_eq!(
        codes("let x: number? = null; print(L`${x}`);"),
        ["SELU0443"]
    );
    assert_eq!(codes("print(L`a` + L`b`);"), ["SELU0444"]);
}
#[test]
fn keys() {
    let p =
        selu_syntax::parse("t.selu", "print(L`${1} missing`); print(L`${2} missing`);").unwrap();
    let m = manifest();
    let c = selu_check::check_program(&p, &m);
    assert!(!c.has_errors());
    assert_eq!(
        selu_lower::lower(&p, &m, &c.types).locale_keys,
        ["{0} missing"]
    );
}

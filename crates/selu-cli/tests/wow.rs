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
    sim.load_addon("Shared", &[("setup.lua".into(), "local _, ns = ...; ns.wow = nil".into()),("a.lua".into(), a), ("translation.lua".into(), "local _, ns = ...; assert(#ns.wow.buffer == 1); ns.wow.locales.enUS = {['{0} then {1}%'] = '{1}% before {0}'}".into()), ("b.lua".into(),b), ("verify.lua".into(), "local _, ns = ...; assert(#ns.wow.buffer == 200); assert(ns.wow.buffer[1].message == 'entry 5'); assert(ns.wow.buffer[200].message == 'entry 204'); assert(ns.wow.commands == 2); assert(rawget(_G, 'SLASH_SELU_Shared_1') == nil); assert(SLASH_SELU_Shared_1_1 == '/one'); assert(SLASH_SELU_Shared_2_1 == '/two')".into())]).unwrap();
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
#[test]
fn review_slash_collision_across_files() {
    let aliases = (1..=11)
        .map(|i| format!("\"/alias{i}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let a = compile(&format!(
        "command({aliases}, (args) => {{ print(\"first\"); }});"
    ));
    let mut src = String::new();
    for i in 2..=11 {
        src.push_str(&format!(
            "command(\"/command{i}\", (args) => {{ print(\"command{i}\"); }});"
        ));
    }
    let sim = Sim::new().unwrap();
    sim.load_addon(
        "Collision",
        &[("a.lua".into(), a), ("b.lua".into(), compile(&src))],
    )
    .unwrap();
    sim.slash("/alias11").unwrap();
    sim.slash("/command11").unwrap();
    assert_eq!(sim.prints(), ["first", "command11"]);
}
#[test]
fn review_client_adapter() {
    let sim = Sim::new().unwrap();
    sim.load_addon(
        "Adapter",
        &[(
            "a.lua".into(),
            compile(
                "command(\"/a\", (args: string, n: number = 1) => { print(args); print(n); });",
            ),
        )],
    )
    .unwrap();
    sim.eval("for _, handler in pairs(SlashCmdList) do handler('hello', {}) end")
        .unwrap();
    assert_eq!(sim.prints(), ["hello", "1"]);
}
#[test]
fn review_sim_client_convention() {
    let sim = Sim::new().unwrap();
    sim.eval("SLASH_PROBE1 = '/probe'; SlashCmdList.PROBE = function(msg, editBox) print(msg); print(type(editBox)) end").unwrap();
    sim.slash("  /PrObE   hello world  \t").unwrap();
    assert_eq!(sim.prints(), ["hello world", "table"]);
}
#[test]
fn review_sim_empty_and_unmatched() {
    let sim = Sim::new().unwrap();
    sim.eval(
        "SLASH_PROBE1 = '/probe'; SlashCmdList.PROBE = function() error('should not dispatch') end",
    )
    .unwrap();
    for line in ["", " \t ", "/missing"] {
        let err = sim.slash(line).unwrap_err().to_string();
        assert!(err.contains("unknown slash command:"), "{line:?}: {err}");
        assert!(!err.contains("nil value"), "{err}");
    }
}
#[test]
fn review_rest_parameter() {
    assert_eq!(
        codes("command(\"/a\", (...args: number[]) => { print(args.length); });"),
        ["SELU0442"]
    );
}
#[test]
fn review_hook_targets() {
    let sources = [
        r#"hook(1, "GetTime", () => {});"#,
        r#"hookScript(false, "OnShow", () => {});"#,
    ];
    let actual: Vec<_> = sources.iter().map(|src| codes(src)).collect();
    assert_eq!(actual, [vec!["SELU0446"], vec!["SELU0446"]]);
}

#[test]
fn review_fallback_allocation() {
    let sim = Sim::new().unwrap();
    let src =
        "global label = (n: number) => L`${n} missing`; global literal = () => L`100% ready`;";
    sim.load_files(&[("a.lua".into(), compile(src))]).unwrap();
    sim.eval("assert(label(2) == '2 missing'); assert(literal() == '100% ready'); collectgarbage('collect'); collectgarbage('stop'); local before = collectgarbage('count'); for i = 1, 1000 do assert(label(2) == '2 missing'); assert(literal() == '100% ready') end; local growth = collectgarbage('count') - before; collectgarbage('restart'); assert(growth < 1, 'fallback allocated ' .. growth .. ' KiB')").unwrap();
}
#[test]
fn review_literal_braces_and_cache() {
    let sim = Sim::new().unwrap();
    let src = "global label = () => L`literal {0} ${2}`;";
    sim.load_addon("Braces", &[("a.lua".into(),compile(src)),("b.lua".into(), "local _, ns = ...; assert(label() == 'literal {0} 2'); ns.wow.locales.enUS = {['literal {{0}} {0}'] = 'translated {{0}} {0}'}; assert(label() == 'translated {0} 2'); local cached = ns.wow.formats['translated {{0}} {0}']; assert(type(cached) == 'table'); assert(label() == 'translated {0} 2'); assert(ns.wow.formats['translated {{0}} {0}'] == cached)".into())]).unwrap();
}
#[test]
fn review_log_exact_message() {
    let sim = Sim::new().unwrap();
    sim.load_addon("Log", &[("a.lua".into(),compile("log.info`100% ${true}`;")),("verify.lua".into(),"local _, ns = ...; assert(#ns.wow.buffer == 1); assert(ns.wow.buffer[1].level == 'info'); assert(ns.wow.buffer[1].message == '100% true')".into())]).unwrap();
}
#[test]
fn review_template_limit() {
    let actual: Vec<_> = ["L", "log.info"]
        .iter()
        .map(|tag| codes(&format!("{tag}`{}`;", "${1}".repeat(240))))
        .collect();
    assert_eq!(actual, [vec!["SELU0445"], vec!["SELU0445"]]);
}

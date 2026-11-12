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
    // Hooking a game function runs our code inside the game's call: the
    // taint warning (SELU0604) is expected, and the hooks still run.
    let lua = compile_expecting(
        "hook(\"GetTime\", () => { print(\"hook\"); }); print(GetTime()); const f = CreateFrame(\"Frame\"); hookScript(f, \"OnShow\", (self) => { print(\"shown\"); }); f.Hide(); f.Show();",
        &["SELU0604"],
    );
    let sim = Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["hook", "0", "shown"]);
}
#[test]
fn a_reviewed_hook_has_no_warning_and_no_value() {
    // allowTaint may wrap a call that returns nothing; it is then a statement.
    let lua = compile(
        "allowTaint(\"reviewed: only repaints\", hookScript(UIParent, \"OnShow\", (self) => { print(\"shown\"); })); UIParent.Hide(); UIParent.Show();",
    );
    let sim = Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["shown"]);
    // Where a value is needed, a wrapped call without one is still an error.
    assert!(
        !codes(
            "const x = allowTaint(\"reviewed\", hookScript(UIParent, \"OnShow\", (self) => {}));"
        )
        .is_empty()
    );
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
    sim.load_addon("Shared", &[("setup.lua".into(), "local _, ns = ...; ns.__selu_wow = nil".into()),("a.lua".into(), a), ("translation.lua".into(), "local _, ns = ...; assert(#ns.__selu_wow.buffer == 1); ns.__selu_wow.locales.enUS = {['{0} then {1}%'] = '{1}% before {0}'}".into()), ("b.lua".into(),b), ("verify.lua".into(), "local _, ns = ...; assert(#ns.__selu_wow.buffer == 200); assert(ns.__selu_wow.buffer[1].message == 'entry 5'); assert(ns.__selu_wow.buffer[200].message == 'entry 204'); assert(ns.__selu_wow.commands == 2); assert(rawget(_G, 'SLASH_SELU_Shared_1') == nil); assert(SLASH_SELU_Shared_1_1 == '/one'); assert(SLASH_SELU_Shared_2_1 == '/two')".into())]).unwrap();
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
fn hooked_functions_and_methods_must_be_in_the_game_data() {
    // Widget methods are covered in frames.rs (this fixture has no widgets).
    let src = r#"allowTaint("probe", hook("GetTime", () => {}));"#;
    assert!(codes(src).is_empty(), "{src}: {:?}", codes(src));
    let sources = [
        r#"allowTaint("probe", hook("NoSuchGameFunction", () => {}));"#,
        r#"const name = "GetTime"; allowTaint("probe", hook(name, () => {}));"#,
    ];
    let actual: Vec<_> = sources.iter().map(|src| codes(src)).collect();
    assert_eq!(actual, [vec!["SELU0447"], vec!["SELU0441"]]);
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
    sim.load_addon("Braces", &[("a.lua".into(),compile(src)),("b.lua".into(), "local _, ns = ...; assert(label() == 'literal {0} 2'); ns.__selu_wow.locales.enUS = {['literal {{0}} {0}'] = 'translated {{0}} {0}'}; assert(label() == 'translated {0} 2'); local cached = ns.__selu_wow.formats['translated {{0}} {0}']; assert(type(cached) == 'table'); assert(label() == 'translated {0} 2'); assert(ns.__selu_wow.formats['translated {{0}} {0}'] == cached)".into())]).unwrap();
}
#[test]
fn review_log_exact_message() {
    let sim = Sim::new().unwrap();
    sim.load_addon("Log", &[("a.lua".into(),compile("log.info`100% ${true}`;")),("verify.lua".into(),"local _, ns = ...; assert(#ns.__selu_wow.buffer == 1); assert(ns.__selu_wow.buffer[1].level == 'info'); assert(ns.__selu_wow.buffer[1].message == '100% true')".into())]).unwrap();
}
#[test]
fn review_template_limit() {
    let actual: Vec<_> = ["L", "log.info"]
        .iter()
        .map(|tag| codes(&format!("{tag}`{}`;", "${1}".repeat(240))))
        .collect();
    assert_eq!(actual, [vec!["SELU0445"], vec!["SELU0445"]]);
}
#[test]
fn modules_share_wow_runtime_in_toc_order() {
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!("selu-d4-module-runtime-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"SharedWow\"\ntargets = [\"wowf@1.0.0.1000\"]\n",
    )
    .unwrap();
    for (file, src) in [
        (
            "first.selu",
            r#"const ADDON = "user"; const ns = "private"; export const first = L`First`; command("/first", (args) => { print(L`First`); print(args); }); log.info`first`; log.warnOnce`shared`; on PLAYER_LOGIN() { print(ADDON); print(ns); }"#,
        ),
        (
            "main.selu",
            r#"import { first } from "./first"; import {} from "./leaf"; const second = L`Second`; command("/second", (args) => { print(L`Second`); print(args); }); log.info`second`; log.warnOnce`shared`; on PLAYER_LOGIN() { print(first); }"#,
        ),
        ("leaf.selu", r#"log.debug`disabled`;"#),
    ] {
        std::fs::write(dir.join("src").join(file), src).unwrap();
    }
    let result = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let dist = dir.join("dist/SharedWow");
    let toc = std::fs::read_to_string(dist.join("SharedWow.toc")).unwrap();
    let mut files: Vec<_> = toc
        .lines()
        .filter(|l| l.ends_with(".lua"))
        .map(|n| (n.to_owned(), std::fs::read_to_string(dist.join(n)).unwrap()))
        .collect();
    let sim = Sim::new().unwrap();
    // This assertion runs after all compiled modules with exactly the same namespace.
    files.push(("verify.lua".into(), r#"local _, ns = ...; local w = ns.__selu_wow; assert(w.commands == 2); assert(#w.buffer == 3); assert(w.buffer[1].level == 'info' and w.buffer[1].message == 'first'); assert(w.buffer[2].level == 'warnOnce' and w.buffer[2].message == 'shared'); assert(w.buffer[3].level == 'info' and w.buffer[3].message == 'second'); assert(type(ns.__selu_events) == 'function'); w.locales.enUS = {First = 'Premier', Second = 'Deuxieme'}"#.into()));
    sim.load_addon("SharedWow", &files).unwrap();
    sim.slash("/first one").unwrap();
    sim.slash("/second two").unwrap();
    sim.fire("PLAYER_LOGIN", vec![]).unwrap();
    assert_eq!(
        sim.prints(),
        [
            "Premier", "one", "Deuxieme", "two", "user", "private", "First"
        ]
    );
    for (file, lua) in &files[..files.len() - 1] {
        assert_eq!(lua.matches(" = ...").count(), 1, "loader in {file}: {lua}");
        assert!(
            lua.starts_with("local __selu_addon, __selu_ns = ...\n"),
            "{lua}"
        );
    }
    // Exercise the public module entry directly: locale keys must survive it.
    let p = selu_syntax::parse("key.selu", "export const text = L`Module ${3}`;").unwrap();
    let m = manifest();
    let c = selu_check::check_program(&p, &m);
    assert!(!c.has_errors());
    let lowered = selu_lower::lower_module(
        &p,
        &m,
        &c.types,
        selu_lower::ModuleBindings::new("key".into(), Default::default(), &p),
    );
    assert_eq!(lowered.locale_keys, ["Module {0}"]);
}

mod common;
use common::compile;
#[test]
fn compound_fields() {
    assert_eq!(
        compile("let x = 0; const a = { b: 2 }; const c = 3; x += a.b * c;"),
        "local x = 0\nlocal a = { b = 2 }\nlocal c = 3\nx = x + a.b * c\n"
    );
}
#[test]
fn tuple_brackets() {
    assert_eq!(
        compile("function f() { return 1; } function g(): [number, number] { return [f(), 2]; }"),
        "local function f()\n  return 1\nend\nlocal function g()\n  return f(), 2\nend\n"
    );
}

fn reference(case: &str, src: &str) {
    reference_lua(case, compile(src));
}
fn reference_lua(case: &str, lua: String) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/readability")
        .join(format!("{case}.lua"));
    if std::env::var_os("SELU_UPDATE_LUA").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &lua).unwrap();
    }
    assert_eq!(lua, std::fs::read_to_string(path).unwrap(), "{case}");
}
#[test]
fn tables() {
    reference(
        "tables",
        r#"const settings = { firstSettingWithLongName: 123, secondSettingWithLongName: 456, thirdSettingWithAnEvenLongerNameToCrossTheLimit: 789 };"#,
    );
}
#[test]
fn callbacks() {
    reference(
        "callbacks",
        r#"
const xs = [1, 2, 3];
const mapped = xs.map((x) => x * 2);
const filtered = xs.filter((x) => x > 1);
const found = xs.find((x) => x > 1);
const some = xs.some((x) => x > 1);
const every = xs.every((x) => x > 0);
const total = xs.reduce((sum, x) => sum + x, 0);
xs.forEach((x) => { print(x); });
"#,
    );
}
#[test]
fn map_update() {
    reference(
        "map-update",
        r#"const m = new Map<string, number>(); const k = "key"; const n = 3; m.set(k, (m.get(k) ?? 0) + n);"#,
    );
}
#[test]
fn stable_field_update() {
    reference(
        "field-update",
        "const t = { f: 1 }; const a = { b: 2 }; t.f += a.b;",
    );
}
#[test]
fn log_text() {
    reference(
        "text",
        r#"const value = "ready"; log.info`100% ready`; log.info`${value}`; print(L`ready`);"#,
    );
}
#[test]
fn events() {
    reference(
        "events",
        "on PLAYER_LOGIN() { print(1); } on PLAYER_LOGIN() { print(2); }",
    );
}
#[test]
fn captures() {
    reference(
        "captures",
        "saved settings = { enabled: false }; class C { value = 1; } on PLAYER_LOGIN() { settings.whenReady(data => { print(data.enabled); }); }",
    );
}
#[test]
fn callback_semantics() {
    assert_eq!(
        common::run(
            r#"
let xs = [1, 2]; const original = xs;
const ys = xs.map((x) => { xs = [9]; return x * 2; });
print(ys.join(",")); print(original.join(","));
const zs = original.map((x) => { if (x == 1) return 7; return x; });
print(zs.join(","));
"#
        ),
        ["2,4", "1,2", "7,2"]
    );
}

fn review_run(src: &str) -> Vec<String> {
    let case = if src.contains("const ys") {
        "review-tuple-map"
    } else if src.contains("function hit") {
        "review-tuple-foreach"
    } else {
        "review-discard"
    };
    reference(case, src);
    common::run(src)
}
#[test]
fn review_discarded_foreach_expressions() {
    assert_eq!(
        review_run(
            "const xs = [1]; xs.forEach((x) => x * 2); xs.forEach((x) => x); print(\"done\");"
        ),
        ["done"]
    );
}
#[test]
fn review_multiple_return_foreach() {
    assert_eq!(
        review_run(
            r#"function hit(x: number): number { print(x); return x; }
const xs = [1];
xs.forEach((x): [number, number] => { return [hit(x), hit(x + 10)]; });
print("done");"#
        ),
        ["1", "11", "done"]
    );
}
#[test]
fn review_multiple_return_map() {
    assert_eq!(
        review_run(
            r#"function hit(x: number): number { print(x); return x; }
const xs = [1];
const ys = xs.map((x): [number, number] => { return [hit(x), hit(x + 10)]; });
print("done");"#
        ),
        ["1", "11", "done"]
    );
}
fn getter_run(case: &str, src: &str) -> Vec<String> {
    reference(case, src);
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("UIParent = setmetatable({}, { __index = function() return mutate() end })")
        .unwrap();
    sim.load_files(&[("getter.lua".into(), compile(src))])
        .unwrap();
    sim.prints()
}
#[test]
fn review_compound_getter() {
    assert_eq!(
        getter_run(
            "review-compound-getter",
            r#"let x = 1;
global mutate = () => { x = 100; return 2; };
const obj = UIParent;
x += obj.n;
print(x);"#
        ),
        ["3"]
    );
}
#[test]
fn review_map_getter() {
    assert_eq!(
        getter_run(
            "review-map-getter",
            r#"const m = new Map<string, number>();
let k = "a";
global mutate = () => { k = "b"; return 2; };
const obj: { n: number } = UIParent;
m.set(k, (m.get(k) ?? 0) + obj.n);
print(m.get("a"));
print(m.get("b"));"#
        ),
        ["2", "nil"]
    );
}
#[test]
fn review_translation_single_value() {
    let src = r#"function f(): string { return UnitName("player"); }
global emit = () => { print(L`${f()}`); };
print(L`ready`);"#;
    let mut manifest = common::manifest();
    let mut function = manifest.functions[0].clone();
    function.name = "UnitName".into();
    manifest.functions.push(function);
    let program = selu_syntax::parse("locale.selu", src).unwrap();
    let checked = selu_check::check_program(&program, &manifest);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let lua = selu_lower::lower(&program, &manifest, &checked.types).lua;
    reference_lua("review-locale", lua.clone());
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("UnitName = function() return 'First', 'Realm' end")
        .unwrap();
    let err = sim
        .load_addon(
            "Locale",
            &[
                ("main.lua".into(), lua),
                (
                    "translate.lua".into(),
                    "local _, ns = ...; ns.__selu_wow.locales.enUS = {['{0}'] = '{1}'}; emit()"
                        .into(),
                ),
            ],
        )
        .unwrap_err();
    assert!(err.to_string().contains("invalid locale argument"), "{err}");
}
#[test]
fn review_standalone_prelude_order() {
    let lua = compile(
        "class C { total = 0; add(amount: number) { this.total += amount; } } log.info`ready`;",
    );
    assert!(
        lua.find("local __selu_setmetatable").unwrap()
            < lua.find("if not __selu_ns.__selu_wow").unwrap()
    );
    assert!(lua.contains("self.total = self.total + amount"));
    reference_lua("review-prelude", lua);
}
#[test]
fn review_statement_prefix_width() {
    let lua = compile(
        "const aVeryLongLocalVariableNameThatConsumesTheRemainingColumns = { first: 123456789, second: 123456789, third: 123456789 };",
    );
    assert!(lua.lines().all(|l| l.len() <= 100), "{lua}");
    reference_lua("review-prefix", lua);
}

#[test]
fn review_origin_reassignment_and_shadowing() {
    assert_eq!(
        getter_run(
            "review-reassigned",
            r#"let x = 1;
global mutate = () => { x = 100; return 2; };
let obj: { n: number } = { n: 9 };
obj = UIParent;
x += obj.n;
print(x);"#
        ),
        ["3"]
    );
    assert_eq!(
        getter_run(
            "review-shadowed",
            r#"const obj = { n: 9 };
let x = 1;
global mutate = () => { x = 100; return 2; };
function use(obj: { n: number }) {
  x += obj.n;
  print(x);
}
use(UIParent);"#
        ),
        ["3"]
    );
}
#[test]
fn review_class_and_literal_origins() {
    let src = "class C { total = 0; add(amount: number) { this.total += amount; } } const c = new C(); c.add(3); print(c.total); let obj = { n: 2 }; let x = 1; x += obj.n; print(x);";
    assert_eq!(common::run(src), ["3", "3"]);
    let lua = compile(src);
    assert!(lua.contains("self.total = self.total + amount"));
    assert!(lua.contains("x = x + obj.n"));
    reference_lua("review-origins", lua);
}

#[test]
fn review_field_store_getter_receiver() {
    let src = r#"let t = { x: 0 };
const old = t;
global mutate = () => { t = { x: 9 }; return 2; };
const obj = UIParent;
t.x = obj.n;
print(old.x);"#;
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("UIParent = setmetatable({}, { __index = function() return mutate() end })")
        .unwrap();
    sim.load_files(&[("store.lua".into(), compile(src))])
        .unwrap();
    assert_eq!(sim.prints(), ["2"]);
    reference("review-field-store", src);
}
#[test]
fn review_index_store_getter_receiver() {
    let src = r#"let t = { x: 0 };
const old = t;
global mutate = () => { t = { x: 9 }; return 2; };
const obj: number[] = UIParent;
t.x = obj[0];
print(old.x);"#;
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval("UIParent = setmetatable({}, { __index = function() return mutate() end })")
        .unwrap();
    sim.load_files(&[("store.lua".into(), compile(src))])
        .unwrap();
    assert_eq!(sim.prints(), ["2"]);
    reference("review-index-store", src);
}

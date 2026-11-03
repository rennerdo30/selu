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
    let lua = compile(src);
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
        "saved settings = { enabled: false }; class C { value = 1; } on PLAYER_LOGIN() { print(settings.enabled); }",
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

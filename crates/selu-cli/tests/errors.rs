mod common;
use common::{codes, compile, run};
#[test]
fn catches_and_finally() {
    assert_eq!(
        run(
            r#"try { throw new Error("bad"); } catch (e) { print(e.message); } finally { print("clean"); }"#
        ),
        ["t.selu:1: bad", "clean"]
    );
}
#[test]
fn returns_and_overrides() {
    assert_eq!(
        run(
            r#"function f(): number { try { return 1; } finally { print("clean"); return 2; } } print(f()); function g() { try { throw new Error("bad"); } catch (e) { return e.message; } finally { print("done"); } } print(g());"#
        ),
        ["clean", "2", "done", "t.selu:1: bad"]
    );
}
#[test]
fn loop_transfers() {
    assert_eq!(
        run(
            r#"for(let i=0;i<3;i++) { try { break; } finally { if(i<2) { continue; } } print("bad"); } print("end");"#
        ),
        ["end"]
    );
    assert_eq!(
        run(r#"for(let i=0;i<3;i++) { try { continue; } finally { break; } } print("end");"#),
        ["end"]
    );
    assert_eq!(
        run(
            r#"for (let i = 0; i < 4; i++) { try { if (i == 1) { continue; } if (i == 3) { break; } print(i); } finally { print("clean"); } }"#
        ),
        ["0", "clean", "clean", "2", "clean", "clean"]
    );
    assert_eq!(
        run(
            r#"for (let i = 0; i < 4; i++) { try { throw new Error("x"); } catch (e) { if (i == 1) { continue; } if (i == 2) { break; } print(i); } finally { print("f"); } }"#
        ),
        ["0", "f", "f", "f"]
    );
    assert_eq!(
        run(
            r#"for (let i = 0; i < 4; i++) { try { print(i); } finally { if (i == 0) { continue; } break; } print("unreachable"); }"#
        ),
        ["0", "1"]
    );
}
#[test]
fn nested_and_tuple() {
    assert_eq!(
        run(
            r#"function f(): [number, boolean, string?] { try { try { return [3, false, null]; } finally { print("inner"); } } finally { print("outer"); } } const [a,b,c] = f(); print(a,b,c);"#
        ),
        ["inner", "outer", "3\tfalse\tnil"]
    );
    assert_eq!(
        run(
            r#"try { try { throw new Error("one"); } catch (e) { throw new Error("two"); } finally { print("inner"); } } catch (e) { print(e.message); }"#
        ),
        ["inner", "t.selu:1: two"]
    );
    assert_eq!(
        run(
            r#"try { for (let i=0; i<2; i++) { if(i==0) { continue; } break; } print("done"); } finally { print("f"); }"#
        ),
        ["done", "f"]
    );
}
#[test]
fn rejects_non_errors() {
    assert!(codes("throw 1;").contains(&"SELU0541".into()));
    assert!(codes(r#"throw {message: "x"};"#).contains(&"SELU0541".into()));
    assert!(codes("new Error(1);").contains(&"SELU0540".into()));
    assert!(codes("try {} catch(e) { const n: number = e.message; }").contains(&"SELU0336".into()));
}
#[test]
fn lua_errors_and_escaping_events() {
    let lua = compile(r#"global fail = () => {}; try { fail(); } catch(e) { print(e.message); }"#);
    // Inject a Lua-origin failure at the call boundary.
    let lua = lua.replace("fail()", "error(\"lua failure\", 0)");
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    assert_eq!(sim.prints(), ["lua failure"]);
    let lua = compile("on PLAYER_LOGIN() {\n throw new Error(\"escape\");\n}");
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    assert!(
        sim.fire("PLAYER_LOGIN", vec![])
            .unwrap_err()
            .to_string()
            .contains("t.selu:2: escape")
    );
}

#[test]
fn catch_binding_is_not_a_literal_origin() {
    let src = r#"const e = {message: "outer"}; let x = "start"; try { throw new Error("caught"); } catch(e) { x += e.message; } print(x);"#;
    let lua = compile(src);
    assert!(
        lua.contains(" = x\n"),
        "missing conservative old-value snapshot:\n{lua}"
    );
    assert_eq!(run(src), ["startt.selu:1: caught"]);
}
#[test]
fn catch_shadows_live_import() {
    use std::{fs, path::Path, process::Command};
    let dir = std::env::temp_dir().join(format!("selu-errors-import-{}", std::process::id()));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"ErrorsImport\"\ntargets = [\"forever@1.0\"]\n",
    )
    .unwrap();
    fs::write(dir.join("src/value.selu"), "export let e = 7;").unwrap();
    fs::write(dir.join("src/main.selu"), r#"import {e} from "./value"; try { throw new Error("caught"); } catch(e) { print(e.message); } print(e);"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let dist = dir.join("dist/ErrorsImport");
    let toc = fs::read_to_string(dist.join("ErrorsImport.toc")).unwrap();
    let files: Vec<_> = toc
        .lines()
        .filter(|l| l.ends_with(".lua"))
        .map(|l| (l.to_string(), fs::read_to_string(dist.join(l)).unwrap()))
        .collect();
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_addon("ErrorsImport", &files).unwrap();
    assert_eq!(sim.prints(), ["main.selu:1: caught", "7"]);
}

#[test]
fn forwarded_tuple_and_bare_return() {
    assert_eq!(
        run(
            r#"function pair(): [number,boolean] { return [4,false]; } function f(): [number,boolean] { try { return pair(); } finally { print("f"); } } const [a,b] = f(); print(a,b); function g() { try { while(true) { return; } } finally { print("g"); } print("bad"); } g();"#
        ),
        ["f", "4\tfalse", "g"]
    );
}
#[test]
fn literal_protected_body_is_hoisted() {
    let src = "for(let i=0;i<2;i++) { try {} finally { print(i); } }";
    let lua = compile(src);
    assert!(
        lua.find("local function __selu_").unwrap() < lua.find("for i").unwrap(),
        "{lua}"
    );
    assert_eq!(run(src), ["0", "1"]);
}

#[test]
fn catch_parameter_obeys_local_name_rules() {
    assert!(codes("try {} catch(__selu_t1) {}").contains(&"SELU0318".into()));
    assert!(codes("try {} catch(end) {}").contains(&"SELU0304".into()));
}

#[test]
fn error_aliases_keep_nominal_identity() {
    assert_eq!(
        run(
            r#"type Message = string; type Failure = Error; const message: Message = "alias"; const failure: Failure = new Error(message); try { throw failure; } catch(e) { print(e.message); }"#
        ),
        ["t.selu:1: alias"]
    );
    assert!(codes(r#"const e: Error = {message: "fake"}; throw e;"#).contains(&"SELU0336".into()));
}

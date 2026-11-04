mod common;
use common::{codes, compile, run};
#[test]
fn catches_and_finally() {
    assert_eq!(
        run(
            r#"try { throw new Error("bad"); } catch (e) { print(e.message); } finally { print("clean"); }"#
        ),
        ["bad", "clean"]
    );
}
#[test]
fn returns_and_overrides() {
    assert_eq!(
        run(
            r#"function f(): number { try { return 1; } finally { print("clean"); return 2; } } print(f()); function g() { try { throw new Error("bad"); } catch (e) { return e.message; } finally { print("done"); } } print(g());"#
        ),
        ["clean", "2", "done", "bad"]
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
        ["inner", "two"]
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
    assert_eq!(run(src), ["startcaught"]);
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
    assert_eq!(sim.prints(), ["caught", "7"]);
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
        ["alias"]
    );
    assert!(codes(r#"const e: Error = {message: "fake"}; throw e;"#).contains(&"SELU0336".into()));
}

#[test]
fn hoisting_preserves_source_callback_identity() {
    assert_eq!(
        run(
            r#"global pcall = (f) => f; let previous = () => 0; for(let i=0;i<2;i++) { const current = pcall(() => 1); print(current == previous); previous = current; }"#
        ),
        ["false", "false"]
    );
}

#[test]
fn review_error_identity_and_message() {
    assert_eq!(
        run(
            r#"const original = new Error("x"); try { throw original; } catch(e) { print(e == original, e.message); e.message = "changed"; } print(original.message);"#
        ),
        ["true\tx", "changed"]
    );
}
#[test]
fn review_rethrow_formats_once() {
    let lua = compile(
        "const original = new Error(\"original\");\non PLAYER_LOGIN() {\n try { throw original; } catch(e) {\n print(e == original, e.message);\n throw e;\n }\n}",
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    let error = sim.fire("PLAYER_LOGIN", vec![]).unwrap_err().to_string();
    assert_eq!(sim.prints(), ["true\toriginal"]);
    // The position is where the Error was created, not where it was thrown.
    assert!(error.contains("t.selu:1: original"), "{error}");
    assert!(!error.contains("t.selu:3:"), "{error}");
    assert!(!error.contains("t.selu:5:"), "{error}");
    assert_eq!(error.matches("t.selu:").count(), 1, "{error}");
}
#[test]
fn review_unused_self_catch_binding() {
    assert_eq!(
        run(
            r#"class C { value = 7; test() { try { throw new Error("x"); } catch(self) { print(this.value); } } } new C().test();"#
        ),
        ["7"]
    );
}
#[test]
fn review_runtime_primitive_shadowing() {
    assert_eq!(
        run(
            r#"function test() { const error = () => {}; const pcall = () => {}; const tostring = () => "wrong"; const setmetatable = () => {}; const getmetatable = () => {}; try { throw new Error("right"); } catch(e) { print(e.message); } finally { print("finally"); } } test();"#
        ),
        ["right", "finally"]
    );
}
#[test]
fn review_capture_free_loop_allocation() {
    let lua = compile(
        "global count = 0; function work() { count++; } global run = () => { for(let i=0;i<1024;i++) { try { work(); } finally {} } };",
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    sim.eval("collectgarbage('collect'); collectgarbage('stop'); before = collectgarbage('count'); run(); print(count); print(collectgarbage('count') - before)")
        .unwrap();
    let prints = sim.prints();
    assert_eq!(prints[0], "1024");
    let growth: f64 = prints[1].parse().unwrap();
    eprintln!("Protected loop allocation: {growth} KiB for 1024 iterations");
    assert!(growth < 16.0, "loop allocated {growth} KiB");
}
#[test]
fn review_optional_catch_binding() {
    assert_eq!(
        run(
            r#"try { throw new Error("x"); } catch { print("caught"); } finally { print("finally"); }"#
        ),
        ["caught", "finally"]
    );
}
#[test]
fn review_empty_error_message() {
    assert_eq!(
        run(
            "const e = new Error(); print(e.message); try { throw e; } catch(caught) { print(caught == e); }"
        ),
        ["", "true"]
    );
}
#[test]
fn review_error_name_is_reserved() {
    assert!(codes("class Error {} new Error();").contains(&"SELU0542".into()));
    assert!(codes("function f(Error) {}").contains(&"SELU0542".into()));
    assert!(codes("function f() { let Error = 1; }").contains(&"SELU0542".into()));
}
#[test]
fn review_protected_conversion_keeps_raw_value() {
    let lua=compile(r#"global fail = () => {}; try { fail(); } catch(e) { print(e.message); print(e.value == RawFailure); }"#.replace("RawFailure", "fail").as_str());
    let lua = lua.replace("fail()", "error(fail, 0)");
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[(
        "t.lua".into(),
        lua.replace(
            "fail = function()\nend",
            "fail = setmetatable({}, {__tostring=function() error('broken conversion') end})",
        ),
    )])
    .unwrap();
    assert_eq!(sim.prints(), ["<error object>", "true"]);
}
#[test]
fn review_no_cascading_error_diagnostics() {
    assert_eq!(codes("throw missing;"), ["SELU0310"]);
    assert_eq!(codes("new Error(missing);"), ["SELU0310"]);
    assert_eq!(codes("throw new Error(missing, 1);"), ["SELU0310"]);
}

/// Removes a test project directory, also when the test fails.
struct TempProject(std::path::PathBuf);
impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn review_cross_module_error_identity() {
    use std::{fs, path::Path, process::Command};
    let dir = std::env::temp_dir().join(format!("selu-e-module-{}", std::process::id()));
    let _cleanup = TempProject(dir.clone());
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"CrossErrors\"\ntargets = [\"forever@1.0\"]\n",
    )
    .unwrap();
    fs::write(dir.join("src/fail.selu"),r#"export const original = new Error("module"); export function fail() { throw original; }"#).unwrap();
    // A module without try/throw: its hook is still a callback entry.
    fs::write(dir.join("src/ping.selu"),r#"import {fail} from "./fail"; hook("GetTime", () => { fail(); }); export const loaded = true;"#).unwrap();
    fs::write(dir.join("src/main.selu"),r#"import {original,fail} from "./fail"; import {loaded} from "./ping"; try { fail(); } catch(e) { print(e == original, e.message); e.message = "mutated"; } print(original.message); global escape = () => { throw original; }; global probe = () => { try { try { GetTime(); } catch(e) { print(e == original, e.message); } throw original; } catch(e) { print(e == original, e.message); } }; on PLAYER_LOGIN() { try { fail(); } catch(e) { throw e; } }"#).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("build")
        .arg(&dir)
        .arg("--manifest-dir")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let dist = dir.join("dist/CrossErrors");
    let toc = fs::read_to_string(dist.join("CrossErrors.toc")).unwrap();
    let files: Vec<_> = toc
        .lines()
        .filter(|l| l.ends_with(".lua"))
        .map(|l| (l.to_string(), fs::read_to_string(dist.join(l)).unwrap()))
        .collect();
    let sim = selu_sim::Sim::new().unwrap();
    let ping = &files.iter().find(|(n, _)| n == "ping.lua").unwrap().1;
    assert!(
        ping.contains("local __selu_errors = __selu_ns.__selu_errors"),
        "{ping}"
    );
    assert!(
        ping.contains("hooksecurefunc(\"GetTime\", __selu_entry(function()"),
        "{ping}"
    );
    assert!(!ping.contains("__selu_pcall"), "{ping}");
    sim.load_addon("CrossErrors", &files).unwrap();
    assert_eq!(sim.prints(), ["true\tmodule", "mutated"]);
    // The hook throws at depth 0 (text), and the try's depth is restored:
    // the following throw still keeps its identity.
    sim.eval("probe()").unwrap();
    assert_eq!(
        sim.prints()[2..],
        ["false\tfail.selu:1: mutated", "true\tmutated"]
    );
    // Back outside every try, a throw reaches the host as text again.
    sim.eval("local ok, err = pcall(escape); print(type(err), err)")
        .unwrap();
    assert_eq!(sim.prints()[4], "string\tfail.selu:1: mutated");
    let error = sim.fire("PLAYER_LOGIN", vec![]).unwrap_err().to_string();
    assert_eq!(error.matches("fail.selu:1: mutated").count(), 1, "{error}");
    assert!(!error.contains("main.selu:"), "{error}");
}

// WoW runs script handlers, hooks, slash commands and events behind its own
// boundary: an error there goes to geterrorhandler(), which formats it with
// `%s`, and the code that triggered the callback (here frame.Show() inside a
// Selu try) keeps running. The simulator lets callback errors propagate to the
// caller, so this test models the engine boundary explicitly: `trigger` runs
// inside the try and dispatches every generated callback through the
// simulator's frame scripts, hooked function, SlashCmdList and event frame,
// each under a pcall standing in for the engine. What escapes each boundary is
// what WoW's error handler would receive: it must be text with one position.
#[test]
fn recheck_callback_boundary_receives_text() {
    let lua = compile(
        "global Target = () => {};\nconst frame = CreateFrame(\"Frame\");\nframe.Hide();\nhookScript(frame, \"OnShow\", (self) => {\n throw new Error(\"show\");\n});\nhook(\"Target\", () => {\n throw new Error(\"hook\");\n});\ncommand(\"/selutest\", (msg) => {\n throw new Error(\"command\");\n});\non PLAYER_LOGIN() {\n throw new Error(\"event\");\n}\nglobal trigger = () => {};\nglobal run = () => {\n try { trigger(); } catch (e) { print(\"caught\", e.message); }\n};",
    );
    let sim = selu_sim::Sim::new().unwrap();
    // The engine boundary: each hooked script runs under its own pcall. The
    // simulator's own HookScript chain is Rust, which would wrap the value.
    sim.eval(
        r#"
escaped = {}
function engine(fn, ...)
  local ok, err = pcall(fn, ...)
  if not ok then escaped[#escaped + 1] = err end
end
local create = CreateFrame
CreateFrame = function(...)
  local frame = create(...)
  frame.HookScript = function(self, name, hook)
    local scripts = rawget(self, "__scripts")
    local old = scripts[name]
    scripts[name] = function(...)
      if old then engine(old, ...) end
      engine(hook, ...)
    end
  end
  return frame
end
"#,
    )
    .unwrap();
    sim.load_files(&[("t.lua".into(), lua.clone())]).unwrap();
    sim.eval(
        r#"
trigger = function()
  for _, f in ipairs(__selu_sim_frames) do
    local scripts = rawget(f, "__scripts")
    if rawget(scripts, "OnShow") then f:Show() end
    if rawget(f, "__events").PLAYER_LOGIN then
      engine(scripts.OnEvent, f, "PLAYER_LOGIN")
    end
  end
  engine(Target)
  for _, handler in pairs(SlashCmdList) do engine(handler, "") end
end
run()
for _, err in ipairs(escaped) do print(type(err), tostring(err)) end
"#,
    )
    .unwrap_or_else(|e| panic!("{e}\n{lua}"));
    let mut prints = sim.prints();
    prints.sort();
    assert_eq!(
        prints,
        [
            "string\tt.selu:11: command",
            "string\tt.selu:14: event",
            "string\tt.selu:5: show",
            "string\tt.selu:8: hook",
        ],
        "{lua}"
    );
    // The try's protection depth survives the callbacks: a later throw in
    // the same protected block is still caught with its identity.
    sim.eval("trigger = function() end").unwrap();
    let lua2 = compile(
        "const frame = CreateFrame(\"Frame\");\nframe.Hide();\nhookScript(frame, \"OnShow\", (self) => { print(\"shown\"); });\nconst original = new Error(\"after\");\ntry {\n frame.Show();\n throw original;\n} catch (e) { print(e == original, e.message); }",
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua2)]).unwrap();
    assert_eq!(sim.prints(), ["shown", "true\tafter"]);
}

#[test]
fn recheck_rethrown_native_error_keeps_lua_location_only() {
    let lua = compile(
        "global fail = () => {};\non PLAYER_LOGIN() {\n try { fail(); } catch (e) {\n throw e;\n }\n}",
    );
    let lua = lua.replace(
        "fail = function()\nend",
        "fail = function() local t; return t.x end",
    );
    assert!(lua.contains("local t; return t.x"), "{lua}");
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    let error = sim.fire("PLAYER_LOGIN", vec![]).unwrap_err().to_string();
    assert!(error.contains("attempt to index"), "{error}");
    assert!(!error.contains("t.selu:"), "{error}");
}

#[test]
fn recheck_reused_error_reports_creation_position() {
    let lua = compile(
        "const shared = new Error(\"again\");\non PLAYER_LOGIN() {\n try { throw shared; } catch (e) { print(\"first\"); }\n throw shared;\n}",
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("t.lua".into(), lua)]).unwrap();
    let error = sim.fire("PLAYER_LOGIN", vec![]).unwrap_err().to_string();
    assert_eq!(sim.prints(), ["first"]);
    assert_eq!(error.matches("t.selu:1: again").count(), 1, "{error}");
    assert!(!error.contains("t.selu:3:"), "{error}");
    assert!(!error.contains("t.selu:4:"), "{error}");
}

#[test]
fn recheck_reserved_error_parameter_reported_once() {
    let found = codes("function f(Error) {}");
    assert_eq!(
        found.iter().filter(|c| *c == "SELU0542").count(),
        1,
        "{found:?}"
    );
}

//! End-to-end runner contracts, using the actual CLI build and its Lua output.
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn project(tag: &str, tests: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("selu-s4-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(
        dir.join("selu.toml"),
        "[addon]\nname = \"Runner\"\ntargets = [\"forever@1.0\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.selu"),
        "on PLAYER_LOGIN() { print(\"boot\"); }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/math.selu"),
        "export function double(x: number): number { return x * 2; }\n",
    )
    .unwrap();
    std::fs::write(dir.join("tests/api.test.selu"), tests).unwrap();
    dir
}
fn run(dir: &Path, filter: Option<&str>) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_selu"));
    cmd.arg("test").arg(dir).arg("--json");
    if let Some(filter) = filter {
        cmd.args(["--filter", filter]);
    }
    cmd.output().unwrap()
}
#[test]
fn runner_api_reload_imports_spies_and_isolation() {
    let dir = project(
        "api",
        r#"
import { double } from "../src/math";
let count = 0;
beforeEach(() => { count += 1; });
describe("api", () => {
  test("assertions", () => {
    expect(count).toBe(1);
    expect(double(3)).toBe(6);
    expect({ x: [1, 2] }).toEqual({ x: [1, 2] });
    expect(1).not.toBe(2);
    expect(null).toBeNull();
    expect(() => { throw new Error("boom"); }).toThrow();
    expect("abcdef").toContain("bcd");
    expect(2).toBeLessThan(3);
    expect(3).toBeGreaterThan(2);
    const spy = sim.spy("print");
    sim.fire("PLAYER_LOGIN");
    expect(spy).toHaveBeenCalled();
    expect(spy).toHaveBeenCalledWith("boot");
  });
  test("reload", () => {
    expect(count).toBe(2);
    sim.reload();
    sim.fire("PLAYER_LOGIN");
    expect(sim.prints()).toContain("boot");
    expect(sim.frame("UIParent")).not.toBeNull();
    sim.setMoney(450);
    sim.setCombat(true);
    sim.setCombat(false);
    sim.setUnit("target", { health: 30, maxHealth: 50 });
    sim.advanceTime(0.1);
  });
});
"#,
    );
    let out = run(&dir, None);
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let report: selu_test::Report = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report.results.len(), 2);
    let out = run(&dir, Some("assertions"));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<selu_test::Report>(&out.stdout)
            .unwrap()
            .results
            .len(),
        1
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn runner_failure_map_replay_budget_and_file_isolation() {
    let dir = project(
        "fail",
        r#"
let n = 0;
test("fail", () => { sim.setMoney(55); expect(3).toBe(4); });
test("runaway", () => { while (true) { n += 1; } });
test("after runaway", () => { expect(sim.prints()).toContain("boot"); });
"#,
    );
    std::fs::write(
        dir.join("src/other.test.selu"),
        "test(\"fresh file\", () => { expect(sim.prints()).toEqual([\"boot\"]); });",
    )
    .unwrap();
    let out = run(&dir, None);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: selu_test::Report = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report.results.len(), 4);
    assert_eq!(report.failed(), 2, "{}", report.to_human());
    let failure = report.results.iter().find(|r| r.path == ["fail"]).unwrap();
    assert!(
        matches!(&failure.outcome,selu_test::Outcome::Failed {location:Some(location),..} if location.contains("tests/api.test.selu:3"))
    );
    assert!(
        failure
            .replay
            .as_ref()
            .unwrap()
            .commands
            .contains(&selu_test::HostCommand::SetMoney { copper: 55 })
    );
    assert_eq!(
        run(&dir, None).stdout,
        out.stdout,
        "same seed gives same report"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn runner_rejects_production_intrinsics_and_reports_gaps() {
    let dir = project(
        "gap",
        r#"
test("gap", () => { expect(() => { perf.allocations(() => {}); }).toThrow(); });
test("gap again", () => { expect(() => { perf.allocations(() => {}); }).toThrow(); });
"#,
    );
    let out = run(&dir, None);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: selu_test::Report = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report.results.len(), 2);
    for result in report.results {
        assert_eq!(result.not_simulated, ["not simulated: perf.allocations"]);
    }
    std::fs::write(dir.join("src/main.selu"), "expect(1).toBe(1);").unwrap();
    let out = run(&dir, None);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("SELU0620"));
    std::fs::remove_dir_all(dir).unwrap();
}

fn typed_timers(dir: &Path) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("manifests/fixtures/forever-1.0.0.1000.json")).unwrap(),
    )
    .unwrap();
    manifest["functions"].as_array_mut().unwrap().push(serde_json::json!({"name":"C_Timer.After","evidence":"verified","signature":{"name":"After","arguments":[{"name":"seconds","type":"number"},{"name":"callback","type":"LuaFunctionReference"}]}}));
    std::fs::create_dir_all(dir.join("manifests")).unwrap();
    std::fs::write(dir.join("manifests/forever.json"), manifest.to_string()).unwrap();
}
fn report(out: &std::process::Output) -> selu_test::Report {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}
#[test]
fn asynchronous_callbacks_fail_the_owning_test() {
    let dir = project(
        "callbacks",
        r#"
test("timer", () => { C_Timer.After(0.1, () => { expect(1).toBe(2); }); sim.advanceTime(0.2); });
test("update", () => { const f=CreateFrame("Frame"); f.SetScript("OnUpdate", () => { expect(1).toBe(2); }); sim.advanceTime(0.02); f.Hide(); });
test("queued", () => { const f=CreateFrame("Frame"); f.SetScript("OnSizeChanged", () => { expect(1).toBe(2); }); f.SetSize(10,10); });
test("reload timer", () => { sim.reload(); sim.advanceTime(0.2); });
test("reload callback", () => { const f=sim.frame("UIParent"); if(f != null) { f.SetScript("OnSizeChanged", () => { expect(1).toBe(2); }); f.SetSize(11,12); } });
"#,
    );
    typed_timers(&dir);
    std::fs::write(
        dir.join("src/main.selu"),
        "on PLAYER_LOGIN() { C_Timer.After(0.1, () => { throw new Error(\"addon timer\"); }); }",
    )
    .unwrap();
    let out = run(&dir, None);
    assert_eq!(out.status.code(), Some(1));
    let report = report(&out);
    assert_eq!(report.results.len(), 5);
    assert_eq!(report.failed(), 5, "{}", report.to_human());
    assert!(
        report.results.iter().all(|r| matches!(
            &r.outcome,
            selu_test::Outcome::Failed {
                location: Some(_),
                ..
            }
        )),
        "{}",
        report.to_human()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn stale_world_and_imports_fail_explicitly_after_reload() {
    let dir = project(
        "stale",
        r#"
import { double } from "../src/math";
const read = GetTime;
const oldFrame = UIParent;
test("clock", () => { sim.advanceTime(2); sim.reload(); sim.advanceTime(3); expect(GetTime()).toBe(5); });
test("import", () => { expect(double(2)).toBe(4); });
test("aliased API", () => { expect(read()).toBe(5); });
test("stale frame", () => { expect(oldFrame.GetWidth()).toBe(1920); });
test("helpers remain current", () => { expect(sim.now()).toBe(5); expect(sim.frame("UIParent")).not.toBeNull(); });
"#,
    );
    let out = run(&dir, None);
    assert_eq!(out.status.code(), Some(1));
    let report = report(&out);
    assert_eq!(report.failed(), 4, "{}", report.to_human());
    for r in &report.results[..4] {
        assert!(
            matches!(&r.outcome,selu_test::Outcome::Failed {message,..} if message.contains("SELU0623") && message.contains("stale")),
            "{r:?}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn setup_errors_are_mapped_reports_for_all_files() {
    for (tag, source) in [
        ("top", "throw new Error(\"top\");"),
        ("login", "on PLAYER_LOGIN() { throw new Error(\"login\"); }"),
    ] {
        let dir = project(tag, "test(\"never\", () => {});");
        std::fs::write(
            dir.join("tests/other.test.selu"),
            "test(\"other\", () => {});",
        )
        .unwrap();
        std::fs::write(dir.join("src/main.selu"), source).unwrap();
        let out = run(&dir, None);
        assert_eq!(out.status.code(), Some(1));
        let report = report(&out);
        assert_eq!(report.results.len(), 2);
        assert_eq!(report.failed(), 2);
        for r in report.results {
            assert!(
                matches!(r.outcome,selu_test::Outcome::Failed {location:Some(ref at),..} if at.contains("src/main.selu:1"))
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn spies_preserve_vm_values_and_frames_preserve_identity() {
    let dir = project(
        "handles",
        r#"
test("spy values", () => {
  const spy=sim.spy("print"); const callback=() => {}; const cycle: unknown[]=[]; cycle.push(cycle);
  print(callback); print(UIParent); print(cycle);
  expect(spy).toHaveBeenCalledWith(callback); expect(spy).toHaveBeenCalledWith(UIParent); expect(spy).toHaveBeenCalledWith(cycle);
});
test("same VM frames", () => {
  const f=CreateFrame("Frame", "Child", UIParent); f.SetSize(23,42);
  expect(sim.frame("Child")).toBe(sim.frame("Child"));
  const proxy=sim.frame("Child"); if(proxy != null) { expect(proxy.GetParent()).toBe(UIParent); const [w,h]=proxy.GetSize(); expect(w).toBe(23); expect(h).toBe(42); }
});
test("cross VM frames", () => {
  sim.reload(); const child=sim.frame("NamedChild"); const root=sim.frame("UIParent");
  if(child != null && root != null) {
    expect(child).toBe(sim.frame("NamedChild")); expect(child.GetParent()).toBe(root);
    const [w,h]=child.GetSize(); expect(w).toBe(23); expect(h).toBe(42);
    let count=0; const callback=(frame) => {expect(frame).toBe(child); count += 1;}; child.SetScript("OnSizeChanged", callback); expect(child.GetScript("OnSizeChanged")).toBe(callback); child.SetSize(40,50); expect(count).toBe(1);
    const spy=sim.spy("print"); sim.fire("PLAYER_LOGIN"); expect(spy).toHaveBeenCalledWith("boot"); expect(spy).toHaveBeenCalledWith(child); expect(spy).not.toHaveBeenCalledWith(root);
  } else { expect(0).toBe(1); }
});
"#,
    );
    std::fs::write(
        dir.join("src/main.selu"),
        "const child=CreateFrame(\"Frame\",\"NamedChild\",UIParent); child.SetSize(23,42); on PLAYER_LOGIN() { print(\"boot\"); print(child); }",
    )
    .unwrap();
    let out = run(&dir, None);
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(report(&out).results.len(), 3);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn invalid_intrinsic_arguments_are_compiler_errors() {
    for (i, source) in [
        "test(\"bad\",123);",
        "describe(\"bad\",123);",
        "beforeEach(123);",
        "test(\"bad\", () => {expect(1).toBeLessThan(null);});",
        "test(\"bad\", () => {expect(1).toBeLessThan();});",
        "test(\"bad\", () => {expect(1).toBe();});",
        "test(\"bad\", () => {expect(1).toBeGreaterThan(\"a\");});",
    ]
    .iter()
    .enumerate()
    {
        let dir = project(&format!("bad-arg-{i}"), source);
        let out = run(&dir, None);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{source}: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn reload_attempts_invalidate_only_replaced_generations_even_on_error() {
    for (tag, source) in [
        (
            "reload-load-error",
            "if(GetTime()>0){ throw new Error(\"reload load\"); }",
        ),
        (
            "reload-login-error",
            "on PLAYER_LOGIN(){ if(GetTime()>0){ throw new Error(\"reload login\"); } }",
        ),
    ] {
        let dir = project(
            tag,
            r#"
test("failed reload stale read",()=>{ sim.advanceTime(1); expect(()=>{sim.reload();}).toThrow(); sim.advanceTime(3); expect(sim.now()).toBe(4); expect(GetTime()).toBe(1); });
test("current clock",()=>{expect(sim.now()).toBe(4);});
"#,
        );
        std::fs::write(dir.join("src/main.selu"), source).unwrap();
        let out = run(&dir, None);
        assert_eq!(out.status.code(), Some(1));
        let result = report(&out);
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.failed(), 1, "{}", result.to_human());
        assert!(
            matches!(&result.results[0].outcome,selu_test::Outcome::Failed{message,location:Some(at)} if message.contains("SELU0623") && at.contains("tests/api.test.selu:")),
            "{}",
            result.to_human()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    // A logout exception precedes replacement: that generation is still current.
    let dir = project(
        "reload-no-replacement",
        r#"test("still current",()=>{sim.advanceTime(1);expect(()=>{sim.reload();}).toThrow();sim.advanceTime(3);expect(GetTime()).toBe(4);expect(sim.now()).toBe(4);});"#,
    );
    typed_logout(&dir);
    std::fs::write(
        dir.join("src/main.selu"),
        "on PLAYER_LOGOUT(){throw new Error(\"before replacement\");}",
    )
    .unwrap();
    let out = run(&dir, None);
    assert!(out.status.success(), "{}", report(&out).to_human());
    std::fs::remove_dir_all(dir).unwrap();
}

fn typed_logout(dir: &Path) {
    typed_timers(dir);
    let path = dir.join("manifests/forever.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["events"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"name":"PLAYER_LOGOUT","params":[],"evidence":"verified"}));
    std::fs::write(path, manifest.to_string()).unwrap();
}

#[test]
fn cross_vm_spies_compare_values_and_cycles_in_both_directions() {
    let dir = project(
        "cross-vm-values",
        r#"
test("same VM control",()=>{const spy=sim.spy("print");print([1,2]);expect(spy).not.toHaveBeenCalledWith([9,9]);});
test("wrong arguments",()=>{sim.reload();const spy=sim.spy("print");sim.fire("PLAYER_LOGIN");expect(spy).toHaveBeenCalledWith([9,9]);});
test("values and cycles",()=>{
  const spy=sim.spy("print");sim.fire("PLAYER_LOGIN");
  expect(spy).not.toHaveBeenCalledWith([9,9]);expect(spy).toHaveBeenCalledWith([1,2]);
  const empty:number[]=[];expect(spy).not.toHaveBeenCalledWith(empty);expect(spy).not.toHaveBeenCalledWith([1]);expect(spy).not.toHaveBeenCalledWith([1,2,3]);
  expect(spy).toHaveBeenCalledWith({nested:[1,2]});expect(spy).not.toHaveBeenCalledWith({nested:[1,9]});
  const cycle:unknown[]=[];cycle.push(cycle);cycle.push(5);expect(spy).toHaveBeenCalledWith(cycle);
  const different:unknown[]=[];different.push(different);different.push(9);expect(spy).not.toHaveBeenCalledWith(different);
});
"#,
    );
    std::fs::write(dir.join("src/main.selu"),"const cycle:unknown[]=[];cycle.push(cycle);cycle.push(5);on PLAYER_LOGIN(){print([1,2]);print({nested:[1,2]});print(cycle);}").unwrap();
    let out = run(&dir, None);
    assert_eq!(out.status.code(), Some(1));
    let result = report(&out);
    assert_eq!(result.results.len(), 3);
    assert_eq!(result.failed(), 1, "{}", result.to_human());
    assert!(
        matches!(&result.results[1].outcome,selu_test::Outcome::Failed {message,..} if message.contains("9") && message.contains("1=1") && message.contains("2=2")),
        "{}",
        result.to_human()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failures_during_retirement_survive_successful_and_failed_reload_attempts() {
    let source = "const f=CreateFrame(\"Frame\");\nf.SetScript(\"OnSizeChanged\",()=>{throw new Error(\"logout queued failure\");});\non PLAYER_LOGOUT(){if(GetTime()>0){f.SetSize(10,10);}}\n";
    for (tag, tests, login_error, failed) in [
        (
            "initial-retirement",
            "test(\"logout failure in original VM\",()=>{sim.advanceTime(1);sim.reload();});",
            false,
            1,
        ),
        (
            "intermediate-retirement",
            "test(\"first reload\",()=>{sim.reload();});test(\"logout failure in second VM\",()=>{sim.advanceTime(1);sim.reload();});",
            false,
            1,
        ),
        (
            "error-retirement",
            "test(\"first reload\",()=>{sim.reload();});test(\"logout failure plus login error\",()=>{sim.advanceTime(1);expect(()=>{sim.reload();}).toThrow();});",
            true,
            1,
        ),
    ] {
        let dir = project(tag, tests);
        typed_logout(&dir);
        let suffix = if login_error {
            "on PLAYER_LOGIN(){if(GetTime()>0){throw new Error(\"login after replacement\");}}"
        } else {
            ""
        };
        std::fs::write(dir.join("src/main.selu"), format!("{source}{suffix}")).unwrap();
        let out = run(&dir, None);
        assert_eq!(out.status.code(), Some(1));
        let result = report(&out);
        assert_eq!(result.failed(), failed, "{}", result.to_human());
        let last = result.results.last().unwrap();
        assert!(
            matches!(&last.outcome,selu_test::Outcome::Failed {message,location:Some(at)} if message.contains("logout queued failure") && at=="src/main.selu:2"),
            "{}",
            result.to_human()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn registration_flush_errors_are_mapped_and_other_files_continue() {
    let dir = project(
        "registration-flush",
        r#"
const f=CreateFrame("Frame");
f.SetScript("OnSizeChanged",()=>{expect(1).toBe(2);});
f.SetSize(10,10);
test("following",()=>{expect(1).toBe(1);});
"#,
    );
    std::fs::write(
        dir.join("tests/other.test.selu"),
        "test(\"other file\",()=>{expect(1).toBe(1);});",
    )
    .unwrap();
    let out = run(&dir, None);
    assert_eq!(out.status.code(), Some(1));
    let result = report(&out);
    assert_eq!(result.results.len(), 2);
    assert_eq!(result.failed(), 1);
    assert_eq!(result.results[0].path, ["registration"]);
    assert!(
        matches!(&result.results[0].outcome,selu_test::Outcome::Failed {message,location:Some(at)} if message.contains("expected 1 to be 2") && at=="tests/api.test.selu:3")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

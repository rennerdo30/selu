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

//! Deterministic test runner and reports shared by the CLI and future Studio integration.
pub mod project;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Every `*.test.selu` under the project's `src/` and `tests/` folders,
/// sorted, with forward-slash project-relative paths.
pub fn discover(project_dir: &Path) -> std::io::Result<Vec<String>> {
    let mut found = Vec::new();
    for top in ["src", "tests"] {
        let dir = project_dir.join(top);
        if dir.is_dir() {
            walk(project_dir, &dir, &mut found)?;
        }
    }
    found.sort();
    Ok(found)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".test.selu"))
        {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Passed,
    Failed {
        message: String,
        /// `src/x.test.selu:12`, already source-mapped.
        location: Option<String>,
    },
    Skipped,
}

/// One host command a test sent to the session, in order. A failing test's
/// list plus its seed lets `selu debug --test` replay it (decision 22).
pub use selu_sim::session::HostCommand;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Replay {
    pub seed: u32,
    pub commands: Vec<HostCommand>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestResult {
    pub file: String,
    /// `describe` names and the test name, outermost first.
    pub path: Vec<String>,
    #[serde(flatten)]
    pub outcome: Outcome,
    /// Gaps the test ran into (`not simulated: X`), so a pass that relied on
    /// unmodelled behaviour is visible (decision 21).
    pub not_simulated: Vec<String>,
    pub replay: Option<Replay>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub target: Option<String>,
    pub results: Vec<TestResult>,
}

impl Report {
    pub fn count(&self, pred: impl Fn(&Outcome) -> bool) -> usize {
        self.results.iter().filter(|r| pred(&r.outcome)).count()
    }
    pub fn failed(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Failed { .. }))
    }
    /// 0 when nothing failed, 1 otherwise (decision 11).
    pub fn exit_code(&self) -> u8 {
        u8::from(self.failed() > 0)
    }

    /// Port CI requires an exercised scenario and refuses unmodelled behaviour.
    pub fn strict_exit_code(&self) -> u8 {
        u8::from(
            self.results.is_empty()
                || self.failed() > 0
                || self.results.iter().any(|r| !r.not_simulated.is_empty()),
        )
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("reports always serialise")
    }

    pub fn to_human(&self) -> String {
        let mut out = String::new();
        for r in &self.results {
            let name = r.path.join(" > ");
            match &r.outcome {
                Outcome::Passed => out.push_str(&format!("  ok    {} {name}\n", r.file)),
                Outcome::Skipped => out.push_str(&format!("  skip  {} {name}\n", r.file)),
                Outcome::Failed { message, location } => {
                    out.push_str(&format!("  FAIL  {} {name}\n", r.file));
                    if let Some(at) = location {
                        out.push_str(&format!("        at {at}\n"));
                    }
                    for line in message.lines() {
                        out.push_str(&format!("        {line}\n"));
                    }
                }
            }
            for gap in &r.not_simulated {
                out.push_str(&format!("        note: {gap}\n"));
            }
        }
        let passed = self.count(|o| matches!(o, Outcome::Passed));
        let skipped = self.count(|o| matches!(o, Outcome::Skipped));
        out.push_str(&format!(
            "{passed} passed, {} failed, {skipped} skipped\n",
            self.failed()
        ));
        out
    }
}

/// What a run needs from the CLI.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub client_install: Option<PathBuf>,
    pub no_client_assets: bool,
    pub manifest_dir: Option<PathBuf>,
    pub project_dir: PathBuf,
    pub target: Option<String>,
    /// Only tests whose joined path contains this text.
    pub filter: Option<String>,
    /// Blizzard's UI loaded from the user's client before the addon.
    pub ui: TestUi,
    /// Test files run at once, each in its own session; 0 picks a small
    /// default (half the cores, at most 4).
    pub jobs: usize,
}

/// Which of Blizzard's own UI a test session runs, read from the user's
/// installed client (never shipped). `Core` is the lean real subset Studio
/// uses by default; without an installed client the run says so and goes on
/// without Blizzard's UI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TestUi {
    None,
    #[default]
    Core,
    Full,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunError {
    NoTests,
    Build(String),
    Io(String),
    Fixture {
        diagnostic: selu_diagnostics::Diagnostic,
        source: String,
    },
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::NoTests => write!(f, "no *.test.selu files under src/ or tests/"),
            RunError::Build(e) => write!(f, "{e}"),
            RunError::Io(e) => write!(f, "{e}"),
            RunError::Fixture { diagnostic, source } => write!(f, "{}", diagnostic.render(source)),
        }
    }
}

mod bridge;
mod runner;
mod scenario;
pub use runner::run;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_test_files_in_src_and_tests() {
        let root = std::env::temp_dir().join(format!("selu-test-discover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/ui")).unwrap();
        std::fs::create_dir_all(root.join("tests")).unwrap();
        std::fs::write(root.join("src/main.selu"), "").unwrap();
        std::fs::write(root.join("src/ui/list.test.selu"), "").unwrap();
        std::fs::write(root.join("tests/boot.test.selu"), "").unwrap();
        let found = discover(&root).unwrap();
        let err = run(&RunOptions {
            client_install: None,
            no_client_assets: true,
            manifest_dir: None,
            project_dir: root.clone(),
            target: None,
            filter: None,
            ui: TestUi::None,
            jobs: 1,
        })
        .unwrap_err();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, ["src/ui/list.test.selu", "tests/boot.test.selu"]);
        assert!(matches!(err, RunError::Io(_) | RunError::Build(_)));
    }

    #[test]
    fn report_renders_and_sets_the_exit_code() {
        let report = Report {
            target: Some("retail@12.0".into()),
            results: vec![
                TestResult {
                    file: "tests/a.test.selu".into(),
                    path: vec!["junk".into(), "sells grey items".into()],
                    outcome: Outcome::Passed,
                    not_simulated: vec![],
                    replay: None,
                },
                TestResult {
                    file: "tests/a.test.selu".into(),
                    path: vec!["junk".into(), "prints the total".into()],
                    outcome: Outcome::Failed {
                        message: "expected 3 prints, got 2".into(),
                        location: Some("tests/a.test.selu:14".into()),
                    },
                    not_simulated: vec!["not simulated: C_Container.UseContainerItem".into()],
                    replay: Some(Replay {
                        seed: 1,
                        commands: vec![HostCommand::Fire {
                            event: "MERCHANT_SHOW".into(),
                            args: vec![],
                        }],
                    }),
                },
            ],
        };
        assert_eq!(report.exit_code(), 1);
        let human = report.to_human();
        assert!(
            human.contains("FAIL  tests/a.test.selu junk > prints the total"),
            "{human}"
        );
        assert!(
            human.ends_with("1 passed, 1 failed, 0 skipped\n"),
            "{human}"
        );
        let back: Report = serde_json::from_str(&report.to_json()).unwrap();
        assert_eq!(back, report);
        assert_eq!(Report::default().exit_code(), 0);
    }
}

#[cfg(test)]
mod sb_tests {
    use super::*;
    #[test]
    fn strict_gate_rejects_empty_and_gaps() {
        assert_eq!(Report::default().strict_exit_code(), 1);
        let mut report = Report {
            target: None,
            results: vec![TestResult {
                file: "tests/a.test.selu".into(),
                path: vec!["scenario".into()],
                outcome: Outcome::Passed,
                not_simulated: vec!["not simulated: API".into()],
                replay: None,
            }],
        };
        assert_eq!(report.strict_exit_code(), 1);
        report.results[0].not_simulated.clear();
        assert_eq!(report.strict_exit_code(), 0);
    }
}

mod client_assets;

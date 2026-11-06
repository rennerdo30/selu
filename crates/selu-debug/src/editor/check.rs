//! Check unsaved buffers using the check-only command and a private temporary copy.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    /// Project-relative path with forward slashes (`src/main.selu`).
    pub file: Option<String>,
    /// 1-based.
    pub line: u32,
    pub column: u32,
    /// `help:` and `fix:` notes.
    pub notes: Vec<String>,
}

pub struct Job {
    pub seq: u64,
    pub project: PathBuf,
    pub selu: PathBuf,
    pub target: Option<String>,
    /// Unsaved buffers: project-relative path and text.
    pub buffers: Vec<(String, String)>,
}

pub struct Outcome {
    pub seq: u64,
    pub problems: Vec<Problem>,
    /// The build succeeded (warnings allowed).
    pub ok: bool,
}

pub struct Checker {
    jobs: Sender<Job>,
    pub results: Receiver<Outcome>,
}

impl Checker {
    pub fn spawn(repaint: eframe::egui::Context) -> Checker {
        let (jobs, inbox) = channel::<Job>();
        let (outbox, results) = channel::<Outcome>();
        std::thread::spawn(move || {
            while let Ok(mut job) = inbox.recv() {
                // Only the newest edit matters.
                while let Ok(newer) = inbox.try_recv() {
                    job = newer;
                }
                let outcome = run(&job);
                if outbox.send(outcome).is_err() {
                    break;
                }
                repaint.request_repaint();
            }
        });
        Checker { jobs, results }
    }

    pub fn submit(&self, job: Job) {
        let _ = self.jobs.send(job);
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let path = entry.path();
        if path.is_dir() {
            let skip = matches!(
                name.to_str(),
                Some("dist" | "target" | ".git" | "node_modules")
            );
            if !skip {
                copy_dir(&path, &to.join(&name))?;
            }
        } else {
            std::fs::copy(&path, to.join(&name))?;
        }
    }
    Ok(())
}

fn run(job: &Job) -> Outcome {
    let mirror = std::env::temp_dir().join(format!(
        "selu-check-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let failed = |message: String| Outcome {
        seq: job.seq,
        problems: vec![Problem {
            severity: Severity::Error,
            code: "studio".into(),
            message,
            file: None,
            line: 0,
            column: 0,
            notes: vec![],
        }],
        ok: false,
    };
    if let Err(e) = copy_dir(&job.project, &mirror) {
        return failed(format!("cannot prepare the check folder: {e}"));
    }
    for (rel, text) in &job.buffers {
        let path = mirror.join(rel);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, text) {
            return failed(format!("cannot write {}: {e}", path.display()));
        }
    }
    let mut cmd = std::process::Command::new(&job.selu);
    cmd.arg("check").arg(&mirror).current_dir(&job.project);
    if let Some(t) = &job.target {
        cmd.arg("--target").arg(t);
    }
    let result = match bounded_output(&mut cmd) {
        Ok(out) => Outcome {
            seq: job.seq,
            problems: parse(&String::from_utf8_lossy(&out.stderr)),
            ok: out.status.success(),
        },
        Err(e) => failed(format!("cannot run {}: {e}", job.selu.display())),
    };
    let _ = std::fs::remove_dir_all(&mirror);
    result
}

fn bounded_output(cmd: &mut std::process::Command) -> std::io::Result<std::process::Output> {
    use std::{
        io::{self, Read},
        process::Stdio,
        time::{Duration, Instant},
    };
    fn drain(mut pipe: impl Read) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    eprintln!("Studio check PID {}, timeout 60s", child.id());
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    // Both pipes must drain while the child runs, even if either stream
    // exceeds its OS pipe capacity before the other produces any output.
    let stdout = std::thread::spawn(move || drain(stdout));
    let stderr = std::thread::spawn(move || drain(stderr));
    let start = Instant::now();
    let status = (|| {
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if start.elapsed() > Duration::from_secs(60) {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "compiler check exceeded 60s",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    if status.is_err() {
        // Retain cleanup on timeout and on wait errors before joining readers.
        let _ = child.kill();
        let _ = child.wait();
    }
    let stdout = stdout
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("stdout reader panicked")));
    let stderr = stderr
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("stderr reader panicked")));
    Ok(std::process::Output {
        status: status?,
        stdout: stdout?,
        stderr: stderr?,
    })
}

/// Parse `selu build`'s rendered diagnostics:
///
/// ```text
/// error[SELU0101]: unknown name `pirnt`
///   ┌─ main.selu:3:3
///   ...
///   = help: did you mean `print`?
/// ```
///
/// Positions name the module file relative to `src/`. Lines starting with
/// `selu:` are errors without a position.
pub fn parse(stderr: &str) -> Vec<Problem> {
    let mut out: Vec<Problem> = Vec::new();
    for line in stderr.lines() {
        let trimmed = line.trim_start();
        let head = trimmed
            .strip_prefix("error[")
            .map(|r| (Severity::Error, r))
            .or_else(|| {
                trimmed
                    .strip_prefix("warning[")
                    .map(|r| (Severity::Warning, r))
            });
        if let Some((severity, rest)) = head
            && let Some((code, message)) = rest.split_once("]: ")
        {
            out.push(Problem {
                severity,
                code: code.to_string(),
                message: message.to_string(),
                file: None,
                line: 0,
                column: 0,
                notes: vec![],
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("┌─ ")
            && let Some(p) = out.last_mut()
            && p.file.is_none()
        {
            let mut parts = rest.trim().rsplitn(3, ':');
            let column = parts.next().and_then(|c| c.parse().ok());
            let line_no = parts.next().and_then(|l| l.parse().ok());
            if let (Some(column), Some(line_no), Some(file)) = (column, line_no, parts.next()) {
                let file = file.replace('\\', "/");
                p.file = Some(if file.starts_with("src/") {
                    file
                } else {
                    format!("src/{file}")
                });
                p.line = line_no;
                p.column = column;
            }
            continue;
        }
        if let Some(note) = trimmed.strip_prefix("= ")
            && let Some(p) = out.last_mut()
        {
            p.notes.push(note.to_string());
            continue;
        }
        if let Some(message) = trimmed.strip_prefix("selu: ") {
            out.push(Problem {
                severity: Severity::Error,
                code: "selu".into(),
                message: message.to_string(),
                file: None,
                line: 0,
                column: 0,
                notes: vec![],
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_copy_excludes_dist_and_warning_opens_real_source() {
        let root = std::env::temp_dir().join(format!("selu-studio-copy-{}", std::process::id()));
        let source = root.join("project");
        let mirror = root.join("mirror");
        std::fs::create_dir_all(source.join("src/ui")).unwrap();
        std::fs::create_dir_all(source.join("dist/.selu/check/src")).unwrap();
        std::fs::write(source.join("src/ui/orphan.selu"), "").unwrap();
        std::fs::write(source.join("dist/.selu/check/src/ghost.selu"), "").unwrap();
        copy_dir(&source, &mirror).unwrap();
        assert!(mirror.join("src/ui/orphan.selu").exists());
        assert!(!mirror.join("dist").exists());
        let problems = parse(
            "warning[SELU0404]: src/ui/orphan.selu is not reachable from src/main.selu\n  ┌─ ui/orphan.selu:1:1\n",
        );
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].file.as_deref(), Some("src/ui/orphan.selu"));
        assert!(source.join(problems[0].file.as_ref().unwrap()).is_file());
    }

    #[test]
    fn parses_rendered_diagnostics() {
        let text = "error[SELU0101]: unknown name `pirnt`\n  ┌─ main.selu:3:5\n  │\n3 │   pirnt(1);\n  │   ^^^^^\n  │\n  = help: did you mean `print`?\n\nwarning[SELU0483]: unknown frame type\n  ┌─ ui/list.selu:10:1\n\nselu: cannot read x\n";
        let p = parse(text);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].code, "SELU0101");
        assert_eq!(p[0].file.as_deref(), Some("src/main.selu"));
        assert_eq!((p[0].line, p[0].column), (3, 5));
        assert_eq!(p[0].notes, ["help: did you mean `print`?"]);
        assert_eq!(p[1].severity, Severity::Warning);
        assert_eq!(p[1].file.as_deref(), Some("src/ui/list.selu"));
        assert_eq!(p[2].file, None);
    }
}

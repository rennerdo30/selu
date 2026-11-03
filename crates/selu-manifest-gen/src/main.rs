use selu_manifest::{Build, Manifest, Source};
use selu_manifest_gen::{apply_overlay, encode, merge_system, read_documents};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn git(repo: &str, args: &[&str]) -> Result<String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Drain concurrently so large documentation files cannot fill the pipe.
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        use std::io::Read;
        let mut s = String::new();
        let mut r = stdout;
        r.read_to_string(&mut s).map(|_| s)
    });
    let err = std::thread::spawn(move || {
        use std::io::Read;
        let mut s = String::new();
        let mut r = stderr;
        r.read_to_string(&mut s).map(|_| s)
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > Duration::from_secs(30) {
            child.kill()?;
            child.wait()?;
            return Err("git command timeout".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let text = out.join().map_err(|_| "stdout reader failed")??;
    let error = err.join().map_err(|_| "stderr reader failed")??;
    if !status.success() {
        return Err(error.into());
    }
    Ok(text)
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: selu-manifest-gen <flavour> <ref-or-commit> <output.json> <source-repository> (reads overlay.toml beside output)".into());
    }
    let (flavour, reference, output, repo) = (&args[0], &args[1], Path::new(&args[2]), &args[3]);
    let log = git(repo, &["log", "-1", "--format=%H%n%s", reference, "--"])?;
    let mut lines = log.lines();
    let commit = lines.next().ok_or("missing commit")?.to_owned();
    let title = lines.next().ok_or("missing build title")?;
    let (version, build) = title
        .split_once(" (")
        .ok_or("commit title must be version (build)")?;
    let build = Build::parse(&format!("{}.{}", version, build.trim_end_matches(')')))
        .map_err(|e| format!("{e:?}"))?;
    eprintln!("source {reference} -> {commit}: {title}");
    let mut m: Manifest = serde_json::from_value(
        serde_json::json!({"flavour":flavour,"build":build,"interface":0,"luaDialects":["lua51"],"source":{"commit":commit,"version":version,"interfaceSource":""}}),
    )?;
    let root = "Interface/AddOns/Blizzard_APIDocumentationGenerated";
    let paths = git(repo, &["ls-tree", "-r", "--name-only", &commit, "--", root])?;
    let mut sources = vec![];
    for path in paths.lines().filter(|p| p.ends_with(".lua")) {
        sources.push(git(repo, &["show", &format!("{commit}:{path}")])?);
    }
    let systems = read_documents(&sources)?;
    if systems.is_empty() {
        return Err("no documentation tables found".into());
    }
    for system in systems {
        merge_system(&mut m, system);
    }
    let overlay = output
        .parent()
        .ok_or("output needs parent directory")?
        .join("overlay.toml");
    if overlay.exists() {
        apply_overlay(&mut m, &std::fs::read_to_string(overlay)?)?;
    }
    if m.interface == 0 {
        return Err("interface requires a sourced overlay".into());
    }
    let bytes = encode(&mut m)?;
    std::fs::write(output, &bytes)?;
    let Source { commit, .. } = m.source.unwrap();
    println!(
        "{}: {} functions, {} events, {} tables, {} enums, {} widgets; {} bytes; {}",
        output.display(),
        m.functions.len(),
        m.events.len(),
        m.tables.len(),
        m.enums.len(),
        m.widgets.len(),
        bytes.len(),
        commit
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("SELU0205: {e}");
        std::process::exit(1);
    }
}

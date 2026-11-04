use selu_manifest::{Build, Manifest, Source};
use selu_manifest_gen::{apply_overlay, encode, merge_system, read_documents};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn git(repo: &str, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git_bytes(repo, args)?)?)
}
fn git_bytes(repo: &str, args: &[&str]) -> Result<Vec<u8>> {
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
        let mut s = Vec::new();
        let mut r = stdout;
        r.read_to_end(&mut s).map(|_| s)
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
fn generate(args: &[String]) -> Result<()> {
    if !(4..=5).contains(&args.len()) {
        return Err("usage: selu-manifest-gen <flavour> <ref-or-commit> <output.selum> <source-repository> (reads overlay.toml beside output; optional fifth argument: engine-gap-report.json)".into());
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
    let temp = std::env::temp_dir().join(format!(
        "selu-manifest-gen-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir_all(temp.parent().unwrap())?;
    std::fs::create_dir(&temp)?;
    struct ArchiveCleanup(std::path::PathBuf);
    impl Drop for ArchiveCleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0.join("source.tar"));
            let _ = std::fs::remove_dir(&self.0);
        }
    }
    let _cleanup = ArchiveCleanup(temp.clone());
    let archive = temp.join("source.tar");
    git(
        repo,
        &[
            "archive",
            "--format=tar",
            "-o",
            archive.to_str().ok_or("invalid archive path")?,
            &commit,
            "Interface/AddOns",
        ],
    )?;
    let mut source_globals = selu_manifest_gen::source::Sources::default();
    let mut xml = selu_manifest_gen::xml::Xml::default();
    xml.schema(&git(
        repo,
        &[
            "show",
            &format!("{commit}:Interface/AddOns/Blizzard_SharedXML/UI.xsd"),
        ],
    )?)?;
    let mut sources = vec![];
    let mut factory_sources = vec![];
    let mut files = 0;
    for entry in tar::Archive::new(std::fs::File::open(archive)?).entries()? {
        use std::io::Read;
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        if !path.ends_with(".lua") && !path.ends_with(".xml") {
            continue;
        }
        let mut bytes = vec![];
        entry.read_to_end(&mut bytes)?;
        let text = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("non-UTF8 Lua file parsed with byte-preserving Latin-1 mapping: {path}");
                e.into_bytes().into_iter().map(char::from).collect()
            }
        };
        if path.ends_with(".xml") {
            xml.parse(&path, &text)?;
            continue;
        }
        source_globals.parse(&path, &text)?;
        // Helper bodies are needed for bounded factory effect proofs too.
        factory_sources.push((path.clone(), text.clone()));
        files += 1;
        if path.starts_with("Interface/AddOns/Blizzard_APIDocumentationGenerated/") {
            sources.push(text);
        }
    }
    let systems = read_documents(&sources)?;
    if systems.is_empty() {
        return Err("no documentation tables found".into());
    }
    for system in systems {
        merge_system(&mut m, system);
    }
    let widget_calls = source_globals.widget_calls.clone();
    let source_methods = source_globals.source_methods.clone();
    let (defined, called) = source_globals.extend_manifest(&mut m);
    eprintln!("source globals: {defined} defined, {called} called; parsed {files} Lua files");
    xml.extend_manifest(&mut m);
    selu_manifest_gen::factory::extend(&mut m, &factory_sources);
    let overlay = output
        .parent()
        .ok_or("output needs parent directory")?
        .join("overlay.toml");
    if overlay.exists() {
        let overlay_text = std::fs::read_to_string(overlay)?;
        apply_overlay(&mut m, &overlay_text)?;
        let parsed: selu_manifest_gen::Overlay = toml::from_str(&overlay_text)?;
        if parsed.interface.is_some_and(|i| i.basis == "convention") {
            m.interface = build.major * 10000 + build.minor * 100 + build.patch;
        }
    }
    if m.interface == 0 {
        return Err("interface requires a sourced overlay".into());
    }
    if let Some(report) = args.get(4) {
        std::fs::write(
            report,
            serde_json::to_vec_pretty(&selu_manifest_gen::coverage::inventory(
                &m,
                &widget_calls,
                &source_methods,
            ))?,
        )?;
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
const CLIENTS: &[(&str, &str)] = &[
    ("classic", "classic"),
    ("classic-anniversary", "classic_anniversary"),
    ("classic-era", "classic_era"),
    ("classic-titan", "classic_titan"),
    ("forever", "forever"),
    ("retail", "live"),
];
fn names_added_removed(old: &[String], new: &[String]) -> (usize, usize) {
    use std::collections::BTreeSet;
    let old: BTreeSet<_> = old.iter().collect();
    let new: BTreeSet<_> = new.iter().collect();
    (new.difference(&old).count(), old.difference(&new).count())
}
fn update(args: &[String]) -> Result<()> {
    let repo = "E:/Development/wow-ui-source";
    let keep = args.iter().any(|a| a == "--keep-old");
    let selected: Vec<_> = args.iter().filter(|a| a.as_str() != "--keep-old").collect();
    for name in &selected {
        if !CLIENTS
            .iter()
            .any(|(f, b)| name.as_str() == *f || name.as_str() == *b)
        {
            return Err(format!("unknown flavour: {name}").into());
        }
    }
    let doc_path = Path::new("docs/manifests.md");
    let mut doc = std::fs::read_to_string(doc_path)?;
    for &(flavour, branch) in CLIENTS {
        if !selected.is_empty()
            && !selected
                .iter()
                .any(|a| a.as_str() == flavour || a.as_str() == branch)
        {
            continue;
        }
        let reference = format!("refs/remotes/origin/{branch}");
        let log = git(
            repo,
            &["log", "-1", "--format=%H%n%s%n%ct", &reference, "--"],
        )?;
        let mut lines = log.lines();
        let commit = lines.next().ok_or("missing commit")?;
        let title = lines.next().ok_or("missing title")?;
        let timestamp: u64 = lines.next().ok_or("missing timestamp")?.parse()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        if now.saturating_sub(timestamp) > 14 * 86400 {
            eprintln!(
                "{flavour}: remote-tracking tip is older than 14 days; fetch the source clone before updating if a newer release is expected"
            );
        }
        let (version, build) = title
            .split_once(" (")
            .ok_or("commit title must be version (build)")?;
        let build = Build::parse(&format!("{version}.{}", build.trim_end_matches(')')))
            .map_err(|e| format!("{e:?}"))?;
        let directory = Path::new("manifests").join(flavour);
        let mut old = Vec::new();
        for entry in std::fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.extension().and_then(|x| x.to_str()) == Some("selum") {
                let m: Manifest = selu_manifest::format::decode(&std::fs::read(&path)?)?;
                if m.flavour != flavour {
                    return Err("manifest flavour differs from directory".into());
                }
                old.push((path, m));
            }
        }
        old.sort_by_key(|a| a.1.build);
        let output = directory.join(format!("{version}.{}.selum", build.build));
        generate(&[
            flavour.into(),
            commit.into(),
            output.to_str().ok_or("invalid output path")?.into(),
            repo.into(),
        ])?;
        let new: Manifest = selu_manifest::format::decode(&std::fs::read(&output)?)?;
        let previous = old.last().map(|(_, m)| m);
        let old_functions: Vec<String> = previous
            .map(|m| m.functions.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default();
        let old_events: Vec<String> = previous
            .map(|m| m.events.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default();
        let (fa, fr) = names_added_removed(
            &old_functions,
            &new.functions
                .iter()
                .map(|f| f.name.clone())
                .collect::<Vec<_>>(),
        );
        let (ea, er) = names_added_removed(
            &old_events,
            &new.events
                .iter()
                .map(|f| f.name.clone())
                .collect::<Vec<_>>(),
        );
        println!(
            "{flavour}: {:?} -> {:?}; APIs +{fa}/-{fr}, events +{ea}/-{er}",
            previous.map(|m| &m.build),
            new.build
        );
        let prefix = format!("| `{flavour}` |");
        let row = format!(
            "| `{flavour}` | `{branch}` | {version} ({}) | {} | `{commit}` |",
            build.build, new.interface
        );
        if !doc.lines().any(|line| line.starts_with(&prefix)) {
            return Err(format!("missing pin row for {flavour}").into());
        }
        doc = doc
            .lines()
            .map(|line| {
                if line.starts_with(&prefix) {
                    row.as_str()
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(doc_path, &doc)?;
        if !keep {
            for (path, _) in old {
                if path != output {
                    std::fs::remove_file(path)?;
                }
            }
        }
    }
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 2 && args[0] == "dump" {
        let value: serde_json::Value = selu_manifest::format::decode(&std::fs::read(&args[1])?)?;
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "update") {
        update(&args[1..])
    } else {
        generate(&args)
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("SELU0205: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn counts_compare_names_not_only_lengths() {
        assert_eq!(
            super::names_added_removed(&["A".into(), "B".into()], &["B".into(), "C".into()]),
            (1, 1)
        );
    }
}

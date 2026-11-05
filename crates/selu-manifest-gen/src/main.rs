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
// Verified against the exporter's TACT product set and each branch's workflow.
const CLIENTS: &[(&str, &str)] = &[
    ("live", "wow"),
    ("ptr", "wowt"),
    ("ptr2", "wowxptr"),
    ("beta", "wow_beta"),
    ("forever", "wowf"),
    ("classic", "wow_classic"),
    ("classic_ptr", "wow_classic_ptr"),
    ("classic_beta", "wow_classic_beta"),
    ("classic_era", "wow_classic_era"),
    ("classic_era_ptr", "wow_classic_era_ptr"),
    ("classic_anniversary", "wow_anniversary"),
    ("classic_titan", "wow_classic_titan"),
];
fn build_title(title: &str) -> Option<Build> {
    let (version, build) = title.split_once(" (")?;
    Build::parse(&format!("{version}.{}", build.strip_suffix(')')?)).ok()
}
#[cfg(test)]
fn names_added_removed(old: &[String], new: &[String]) -> (usize, usize) {
    use std::collections::BTreeSet;
    let old: BTreeSet<_> = old.iter().collect();
    let new: BTreeSet<_> = new.iter().collect();
    (new.difference(&old).count(), old.difference(&new).count())
}
fn refresh_build_table(doc: &str, rows: &[String]) -> Result<String> {
    let start_marker = "<!-- selu-store-builds:start -->";
    let end_marker = "<!-- selu-store-builds:end -->";
    let heading = "## Stored product builds";
    let section = format!(
        "{start_marker}\n{heading}\n\n| Product | Branch | Full build | Interface | Commit |\n|---|---|---|---|---|\n{}\n{end_marker}",
        rows.join("\n")
    );
    let mut output = doc.to_owned();
    if let Some(start) = doc.find(start_marker) {
        let end = start
            + doc[start..]
                .find(end_marker)
                .ok_or("missing generated build table end marker")?
            + end_marker.len();
        output.replace_range(start..end, &section);
    } else if let Some(start) = doc.find(heading) {
        let tail = start + heading.len();
        let end = doc[tail..]
            .find("\n## ")
            .map_or(doc.len(), |offset| tail + offset);
        output.replace_range(start..end, &format!("{section}\n"));
    } else {
        output.push_str(&format!("\n{section}\n"));
    }
    Ok(output)
}

fn update(args: &[String]) -> Result<()> {
    let repo = "E:/Development/wow-ui-source";
    let directory = Path::new("manifests/store");
    let mut store = selu_manifest::store::Store::open(directory)?;
    let refs = git(
        repo,
        &["for-each-ref", "--format=%(refname)", "refs/remotes/origin"],
    )?;
    let selected: Vec<_> = args
        .iter()
        .filter(|a| a.as_str() != "--keep-old")
        .map(String::as_str)
        .collect();
    for name in &selected {
        if !CLIENTS
            .iter()
            .any(|(branch, product)| name == branch || selu_manifest::product(name) == *product)
        {
            return Err(format!(
                "unknown product or unmapped branch {name}; add a verified mapping first"
            )
            .into());
        }
        if selu_manifest::product(name) != *name {
            eprintln!(
                "warning: selector `{name}` is deprecated; use `{}`",
                selu_manifest::product(name)
            );
        }
    }
    let mut rows = Vec::new();
    let mut pending = Vec::new();
    for reference in refs.lines().filter(|r| !r.ends_with("/HEAD")) {
        let branch = reference
            .strip_prefix("refs/remotes/origin/")
            .ok_or("invalid branch ref")?;
        if branch == "automation" {
            continue;
        }
        let Some(&(_, product)) = CLIENTS.iter().find(|(b, _)| *b == branch) else {
            eprintln!("unmapped source branch {branch}; add a verified product mapping");
            continue;
        };
        if !selected.is_empty()
            && !selected
                .iter()
                .any(|a| *a == branch || selu_manifest::product(a) == product)
        {
            continue;
        }
        let log = git(repo, &["log", "--format=%H %s", reference, "--"])?;
        let (commit, build) = log
            .lines()
            .find_map(|line| {
                let (commit, title) = line.split_once(' ')?;
                Some((commit, build_title(title)?))
            })
            .ok_or_else(|| format!("no build commit on {branch}"))?;
        let target = format!("{product}@{build}");
        if !store.builds().any(|b| b == target) {
            // Reuse the reviewed widget supplements; interface remains a build convention.
            let family = match product {
                "wowf" => "forever",
                "wow_classic" | "wow_classic_ptr" | "wow_classic_beta" => "classic",
                "wow_classic_era" | "wow_classic_era_ptr" => "classic-era",
                "wow_anniversary" => "classic-anniversary",
                "wow_classic_titan" => "classic-titan",
                _ => "retail",
            };
            let temp =
                std::env::temp_dir().join(format!("selu-update-{}-{product}", std::process::id()));
            std::fs::create_dir(&temp)?;
            let output = temp.join("manifest.selum");
            let result = (|| -> Result<Manifest> {
                std::fs::copy(
                    Path::new("manifests").join(family).join("overlay.toml"),
                    temp.join("overlay.toml"),
                )?;
                generate(&[
                    product.into(),
                    commit.into(),
                    output.to_str().ok_or("invalid temp path")?.into(),
                    repo.into(),
                ])?;
                selu_manifest::format::decode(&std::fs::read(&output)?)
            })();
            for name in ["manifest.selum", "overlay.toml"] {
                let _ = std::fs::remove_file(temp.join(name));
            }
            let _ = std::fs::remove_dir(&temp);
            pending.push(result?);
            println!("{target}: prepared");
        }
    }
    let growth = store.append(directory, &pending)?;
    println!("{} builds: appended {growth} bytes", pending.len());
    for &(branch, product) in CLIENTS {
        if !refs
            .lines()
            .any(|reference| reference == format!("refs/remotes/origin/{branch}"))
        {
            continue;
        }
        let newest = store
            .builds()
            .filter(|target| {
                target
                    .split_once('@')
                    .is_some_and(|(name, _)| name == product)
            })
            .max_by_key(|target| {
                target
                    .split_once('@')
                    .and_then(|(_, version)| Build::parse(version).ok())
                    .map(|build| (build.build, build))
            });
        if let Some(target) = newest {
            let manifest = store.load(target)?;
            let source = manifest
                .source
                .as_ref()
                .ok_or("stored build lacks source provenance")?;
            rows.push(format!(
                "| `{product}` | `{branch}` | {} | {} | `{}` |",
                manifest.build, manifest.interface, source.commit
            ));
        }
    }
    rows.sort();
    let doc_path = Path::new("docs/manifests.md");
    let doc = std::fs::read_to_string(doc_path)?;
    let updated = refresh_build_table(&doc, &rows)?;
    if updated != doc {
        std::fs::write(doc_path, updated)?;
    }
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "migrate") {
        let mut manifests = selu_manifest::load_manifest_dir(Path::new("manifests"))?;
        manifests.retain(|m| !m.fixture);
        for manifest in &mut manifests {
            manifest.flavour = selu_manifest::product(&manifest.flavour).into();
        }
        let destination = Path::new(args.get(1).map(String::as_str).unwrap_or("manifests/store"));
        let mut store = selu_manifest::store::Store::open(destination)?;
        let growth = store.append(destination, &manifests)?;
        let verified = selu_manifest::store::Store::open(destination)?;
        for manifest in &manifests {
            if verified.load(&manifest.target_string())? != *manifest {
                return Err("migration round trip changed manifest data".into());
            }
        }

        println!(
            "migrated and verified {} builds; {growth} bytes",
            manifests.len()
        );
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "list") {
        let store = selu_manifest::store::Store::open(Path::new(
            args.get(1).map(String::as_str).unwrap_or("manifests/store"),
        ))?;
        for build in store.builds() {
            println!("{build}");
        }
        return Ok(());
    }
    if args.len() == 3 && args[0] == "dump" {
        let store = selu_manifest::store::Store::open(Path::new(&args[1]))?;
        println!("{}", serde_json::to_string_pretty(&store.load(&args[2])?)?);
        return Ok(());
    }
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
    fn generated_build_table_preserves_other_sections() {
        let source = "Intro\n## Stored product builds\nold rows\n\n## Keep\nOwner notes\n";
        let updated = super::refresh_build_table(source, &["new row".into()]).unwrap();
        assert!(updated.contains("## Keep\nOwner notes\n"));
        assert_eq!(
            super::refresh_build_table(&updated, &["new row".into()]).unwrap(),
            updated
        );
        assert!(super::refresh_build_table("<!-- selu-store-builds:start -->", &[]).is_err());
    }
    #[test]
    fn counts_compare_names_not_only_lengths() {
        assert_eq!(
            super::names_added_removed(&["A".into(), "B".into()], &["B".into(), "C".into()]),
            (1, 1)
        );
    }
}

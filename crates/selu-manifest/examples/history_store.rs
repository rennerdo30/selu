//! One-pass history-store sizing and cold/warm cache measurement.
use selu_manifest::{Manifest, store};
use std::{path::Path, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("manifests/store");
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--reextract") {
        let mut old = selu_manifest::load_manifest(Path::new(&args[1]))?;
        let mut new = selu_manifest::load_manifest(Path::new(&args[2]))?;
        old.flavour = selu_manifest::product(&old.flavour).into();
        new.flavour = selu_manifest::product(&new.flavour).into();
        assert_eq!(old.target_string(), new.target_string());
        let output = Path::new(&args[3]);
        assert!(!output.exists(), "measurement stores must be fresh");
        let changed = output.join("changed");
        let mut pack = store::Store::default();
        let initial =
            pack.append_with_extraction_version(&changed, std::slice::from_ref(&old), 1)?;
        let first = pack.latest_revision(&old.target_string())?;
        let prior: Vec<_> = std::fs::read_dir(&changed)?
            .map(|entry| {
                let path = entry?.path();
                let bytes = std::fs::read(&path)?;
                Ok((path, bytes))
            })
            .collect::<std::io::Result<_>>()?;
        let delta = pack.append_with_extraction_version(&changed, std::slice::from_ref(&new), 2)?;
        assert_eq!(pack.load_revision(&old.target_string(), &first)?, old);
        assert_eq!(pack.load(&new.target_string())?, new);
        for (path, bytes) in prior {
            assert_eq!(std::fs::read(path)?, bytes);
        }
        println!(
            "same-build actual pre-W2 -> W2 re-extraction: initial {initial} bytes, appended {delta} bytes"
        );
        println!(
            "re-extraction no-op: {} bytes",
            pack.append_with_extraction_version(&changed, std::slice::from_ref(&new), 2)?
        );
        let only_version = output.join("version-only");
        let mut pack = store::Store::default();
        pack.append_with_extraction_version(&only_version, std::slice::from_ref(&new), 1)?;
        println!(
            "identical output, new extractor version: {} bytes",
            pack.append_with_extraction_version(&only_version, &[new], 2)?
        );
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--verify") {
        let pack = store::Store::open(Path::new(&args[1]))?;
        let snapshots = selu_manifest::load_manifest_dir(Path::new(&args[2]))?;
        assert_eq!(
            pack.builds().count(),
            snapshots.len(),
            "stored build set differs from fresh generation"
        );
        for mut manifest in snapshots {
            manifest.flavour = selu_manifest::product(&manifest.flavour).into();
            let target = manifest.target_string();
            if pack.load(&target)? != manifest {
                return Err(
                    format!("reconstruction differs from fresh generation for {target}").into(),
                );
            }
        }
        println!(
            "verified all {} reconstructions against fresh generation",
            pack.builds().count()
        );
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--load") {
        let start = Instant::now();
        let manifests = store::load_cached_selected(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Some(&args[3..]),
        )?;
        println!(
            "fresh process load {} builds: {} ms",
            manifests.len(),
            start.elapsed().as_millis()
        );
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--legacy-one") {
        let start = Instant::now();
        std::hint::black_box(selu_manifest::load_manifest(Path::new(&args[1]))?);
        println!("legacy one build: {} ms", start.elapsed().as_millis());
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--repack") {
        let mut snapshots = selu_manifest::load_manifest_dir(Path::new(&args[1]))?;
        for manifest in &mut snapshots {
            manifest.flavour = selu_manifest::product(&manifest.flavour).into();
        }
        snapshots.sort_by_key(|m| m.target_string());
        let mut pack = store::Store::default();
        let start = Instant::now();
        let size =
            pack.append_with_chunk_size(Path::new(&args[2]), &snapshots, args[3].parse()?)?;
        println!(
            "pack {size} bytes; breakdown {:?}; write {} ms",
            pack.size_breakdown(),
            start.elapsed().as_millis()
        );
        for manifest in &snapshots {
            assert_eq!(pack.load(&manifest.target_string())?, *manifest);
        }
        println!("verified {} exact snapshots", snapshots.len());
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--probe") {
        let path = Path::new(&args[1]);
        let cache = Path::new(&args[2]);
        let requests: Vec<_> = args[3..].to_vec();
        let start = Instant::now();
        let pack = store::Store::open(path)?;
        for target in &requests {
            std::hint::black_box(pack.load(target)?);
        }
        println!(
            "uncached direct {} builds: {} ms",
            requests.len(),
            start.elapsed().as_millis()
        );
        drop(pack);
        for mode in ["first load including cache creation", "warm cache"] {
            let start = Instant::now();
            let manifests = store::load_cached_selected(path, cache, Some(&requests))?;
            println!(
                "{mode} {} builds: {} ms",
                manifests.len(),
                start.elapsed().as_millis()
            );
        }
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--cached") {
        let exact = [
            "wow@12.1.5.70391",
            "wowf@1.60.1.70532",
            "wow_classic@5.5.4.70631",
            "wow_classic_era@1.15.9.70576",
            "wow_anniversary@2.5.6.70577",
            "wow_classic_titan@3.80.2.70522",
        ]
        .map(String::from);
        let start = Instant::now();
        let manifests = store::load_cached_selected(
            &directory,
            Path::new(args.get(1).ok_or("missing cache directory")?),
            Some(&exact),
        )?;
        println!(
            "fresh process cached same six: {} builds, {} ms",
            manifests.len(),
            start.elapsed().as_millis()
        );
        return Ok(());
    }
    let cache = root.join("target/history-measurement").join(format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let baseline_root = args
        .iter()
        .position(|a| a == "--baseline")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("manifests"));
    let start = Instant::now();
    let mut baseline = Vec::<Manifest>::new();
    for family in [
        "retail",
        "forever",
        "classic",
        "classic-era",
        "classic-anniversary",
        "classic-titan",
    ] {
        baseline.extend(selu_manifest::load_manifest_dir(
            &baseline_root.join(family),
        )?);
    }
    if baseline.len() != 6 {
        return Err("pass --baseline <directory of six pre-migration manifests>".into());
    }
    println!("legacy six builds: {} ms", start.elapsed().as_millis());
    if args.first().is_some_and(|a| a == "--legacy-only") {
        return Ok(());
    }
    let exact: Vec<String> = baseline
        .iter()
        .map(|m| format!("{}@{}", selu_manifest::product(&m.flavour), m.build))
        .collect();
    let migrated = store::Store::open(&directory)?;
    for mut original in baseline {
        original.flavour = selu_manifest::product(&original.flavour).into();
        assert_eq!(migrated.load(&original.target_string())?, original);
    }
    println!("migration: six exact build round trips identical except product identity");
    drop(migrated);
    let start = Instant::now();
    let manifests = store::load_cached(&directory, &cache)?;
    println!(
        "first load {} builds and cache creation: {} ms",
        manifests.len(),
        start.elapsed().as_millis()
    );
    drop(manifests);
    let start = Instant::now();
    let warm = store::load_cached(&directory, &cache)?;
    println!(
        "cached all {} builds: {} ms",
        warm.len(),
        start.elapsed().as_millis()
    );
    drop(warm);
    let start = Instant::now();
    let matching = store::load_cached_selected(&directory, &cache, Some(&exact))?;
    println!(
        "cached same six {} builds: {} ms",
        matching.len(),
        start.elapsed().as_millis()
    );
    drop(matching);
    let start = Instant::now();
    let product = vec!["wow".to_string()];
    let selected = store::load_cached_selected(&directory, &cache, Some(&product))?;
    println!(
        "cached wow {} builds: {} ms",
        selected.len(),
        start.elapsed().as_millis()
    );
    let previous_target = exact
        .iter()
        .find(|target| target.starts_with("wow@"))
        .ok_or("no baseline wow build")?;
    let previous = store::load_build(&directory, previous_target)?;
    let mut manifest = selected
        .into_iter()
        .max_by_key(|m| m.build)
        .ok_or("no wow build")?;
    let actual = cache.join("actual-release");
    let mut actual_store = store::Store::default();
    actual_store.append(&actual, &[previous])?;
    let actual_growth = actual_store.append(&actual, std::slice::from_ref(&manifest))?;
    println!("actual newer retail commit: appended {actual_growth} bytes");
    let simulation = cache.join("simulation");
    let mut simulated = store::Store::default();
    let initial = simulated.append(&simulation, std::slice::from_ref(&manifest))?;
    manifest.build.build += 1;
    manifest.interface += 1;
    manifest.functions[1].available = !manifest.functions[1].available;
    let last = manifest.functions.len() - 2;
    manifest.functions[last].available = !manifest.functions[last].available;
    manifest
        .source_files
        .insert(0, "synthetic-new-source.lua".into());
    for entry in manifest
        .data_globals
        .iter_mut()
        .chain(&mut manifest.templates)
    {
        entry.source += 1;
    }
    let growth = simulated.append(&simulation, &[manifest])?;
    println!(
        "synthetic next-build two distant changes plus source-file insertion: initial {initial} bytes, appended {growth} bytes"
    );
    let size: u64 = std::fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|e| e == "selus"))
        .map(|e| e.metadata().map(|m| m.len()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .sum();
    println!("store: {size} bytes");
    Ok(())
}

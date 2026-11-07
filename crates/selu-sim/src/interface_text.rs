//! Readable sibling text is a compatibility fallback, never a UI substitute.
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
#[path = "interface_text_cache.rs"]
mod decoded_cache;
pub(crate) fn timing(phase: &str, product: &str, elapsed: std::time::Duration) {
    if std::env::var_os("SELU_SIM_PROFILE").is_some() {
        eprintln!(
            "TEXT_PROFILE {phase} {product}: {:.6}s",
            elapsed.as_secs_f64()
        );
    }
}

// Text only, never keyed storage or decrypted content. Build keys invalidate
// sibling snapshots when the installed client changes.
type Text = Arc<Vec<(String, String)>>;
type Cache = std::collections::HashMap<(std::path::PathBuf, String, String), Option<Text>>;
fn readable(root: &Path, build: &selu_gamedata::BuildInfo) -> Option<Text> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    let key = (
        root.canonicalize().ok()?,
        build.product.clone(),
        build.build_key.clone(),
    );
    if let Some(text) = cache.get(&key) {
        if std::env::var_os("SELU_SIM_PROFILE").is_some() {
            eprintln!("TEXT_CACHE memory hit {}", build.product);
        }
        return text.clone();
    }
    let disk = decoded_cache::Disk::new(root, build);
    let started = std::time::Instant::now();
    let decoded = disk.as_ref().and_then(|disk| disk.load());
    timing("decoded_cache_read", &build.product, started.elapsed());
    if let Some(decoded) = decoded {
        if std::env::var_os("SELU_SIM_PROFILE").is_some() {
            eprintln!("TEXT_CACHE disk hit {}", build.product);
        }
        let text = decoded.map(Arc::new);
        if cache.len() >= 32 {
            cache.clear();
        }
        cache.insert(key, text.clone());
        return text;
    }
    if std::env::var_os("SELU_SIM_PROFILE").is_some() {
        eprintln!("TEXT_CACHE miss {}", build.product);
    }
    let started = std::time::Instant::now();
    let storage = selu_gamedata::Storage::open(root, &build.product, None);
    timing("storage_open", &build.product, started.elapsed());
    let started = std::time::Instant::now();
    let text = storage
        .ok()
        .and_then(|storage| match storage.global_strings() {
            Ok(strings) => {
                if let (Some(disk), Ok(hash)) = (&disk, storage.global_strings_content_hash()) {
                    disk.save(Some((hash, strings)));
                }
                Some(Arc::new(strings.to_vec()))
            }
            Err(error) => {
                // Only persist build-stable encryption refusal. Missing downloads
                // and transient I/O failures must be retried on a later start.
                if error.to_string().starts_with("encrypted with key")
                    && let Some(disk) = &disk
                {
                    disk.save(None);
                }
                None
            }
        });
    timing("db2_read_decode", &build.product, started.elapsed());
    // Keep memory bounded, including failed candidates.
    if cache.len() >= 32 {
        cache.clear();
    }
    cache.insert(key, text.clone());
    text
}
/// One concern/table: nearest-family preference, including cross-family ties.
/// Channels inherit their base product row; builds break ties within a family.
pub const TEXT_FAMILIES: &[(&str, &str)] = &[
    ("wow", "Mainline"),
    ("wowf", "Mainline"),
    ("wow_classic", "Classic"),
    ("wow_classic_era", "Classic"),
    ("wow_anniversary", "Classic"),
    ("wow_classic_titan", "Classic"),
];
fn family(product: &str) -> &str {
    let p = selu_manifest::base_product(selu_manifest::product(product));
    let p = p
        .strip_suffix("_ptr")
        .or_else(|| p.strip_suffix("_beta"))
        .unwrap_or(p);
    TEXT_FAMILIES
        .iter()
        .find(|(name, _)| *name == p)
        .map(|(_, f)| *f)
        .unwrap_or(p)
}
fn build_number(version: &str) -> u64 {
    version
        .rsplit('.')
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}
pub fn candidates<'a>(
    own: &str,
    version: &str,
    builds: &'a [selu_gamedata::BuildInfo],
) -> Vec<&'a selu_gamedata::BuildInfo> {
    let mut out: Vec<_> = builds
        .iter()
        .filter(|b| b.active && b.product != own)
        .collect();
    out.sort_by_key(|b| {
        (
            family(&b.product) != family(own),
            build_number(&b.version).abs_diff(build_number(version)),
            b.product.clone(),
            b.version.clone(),
        )
    });
    out
}
pub(crate) fn borrow(
    lua: &mlua::Lua,
    ctx: &crate::Ctx,
    root: &Path,
    own: &selu_gamedata::Storage,
    reason: &str,
) -> Option<Vec<(String, String)>> {
    let started = std::time::Instant::now();
    let builds =
        selu_gamedata::parse_build_info(&std::fs::read_to_string(root.join(".build.info")).ok()?)
            .ok()?;
    let mut selected = None;
    let tags = lua.create_table().ok()?;
    for build in candidates(&own.build.product, &own.build.version, &builds) {
        // Donors are readable-only snapshots. User keys apply to the own client.
        let Some(strings) = readable(root, build) else {
            continue;
        };
        let registration = std::time::Instant::now();
        for (key, _) in strings.iter() {
            let _ = tags.raw_set(key.as_str(), true);
        }
        timing("register_tags", &build.product, registration.elapsed());
        if selected.is_none() {
            let description = format!(
                "Interface text borrowed from {} {} ({}'s is unreadable: {reason})",
                build.product, build.version, own.build.product
            );
            ctx.log(description.clone());
            let _ = lua.set_named_registry_value("selu.interface_text_source", description);
            selected = Some(strings.to_vec());
        }
    }
    let _ = lua.globals().raw_set("__selu_string_tags", tags);
    timing("borrow_total", &own.build.product, started.elapsed());
    selected
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_borrowed_tags_are_synthetic_but_unknown_values_stay_nil() {
        let s = crate::Session::new(Default::default()).unwrap();
        s.lua()
            .set_named_registry_value("selu.synthetic_strings", true)
            .unwrap();
        s.eval("__selu_string_tags={SELF_WRITTEN_TEXT=true}; assert(SELF_WRITTEN_TEXT=='SELF WRITTEN TEXT'); assert(SELF_WRITTEN_UNKNOWN_NUMBER==nil); assert(SELF_WRITTEN_UNKNOWN_FRAME==nil)").unwrap();
    }
    #[test]
    fn family_then_build_then_name_orders_all_channels() {
        let b = |p: &str, v: &str| selu_gamedata::BuildInfo {
            product: p.into(),
            version: v.into(),
            build_key: String::new(),
            active: true,
        };
        let builds = [
            b("wow", "12.1.0.60000"),
            b("wow_anniversary", "2.5.0.70790"),
            b("wow_classic_era", "1.15.0.70789"),
        ];
        assert_eq!(
            candidates("wowf", "1.60.0.70790", &builds)
                .iter()
                .map(|b| b.product.as_str())
                .collect::<Vec<_>>(),
            vec!["wow", "wow_anniversary", "wow_classic_era"]
        );
        for p in [
            "wow_classic_ptr",
            "wow_classic_era_ptr",
            "wow_anniversary_ptr",
            "wow_classic_titan_beta",
        ] {
            assert_eq!(family(p), "Classic");
        }
        assert_eq!(family("wowxptr"), "Mainline");
    }
}

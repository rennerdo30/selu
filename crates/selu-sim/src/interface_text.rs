//! Readable sibling text is a compatibility fallback, never a UI substitute.
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

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
        return text.clone();
    }
    let text = selu_gamedata::Storage::open(root, &build.product, None)
        .ok()
        .and_then(|storage| storage.global_strings().ok().map(|s| Arc::new(s.to_vec())));
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
        for (key, _) in strings.iter() {
            let _ = tags.raw_set(key.as_str(), true);
        }
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

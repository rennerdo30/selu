//! Target manifests: the API surface of one exact game build.
//!
//! A target is a product or an exact `product@major.minor.patch.build` pin.
//! Transitional flavour aliases retain partial-version resolution; errors list the
//! closest known builds (DESIGN.md section 2).

pub mod format;
pub mod lock;
pub mod resolution;
pub mod store;
use selu_diagnostics::Diagnostic;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

/// `major.minor.patch.build`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Build {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub build: u32,
}

impl Build {
    pub fn parse(s: &str) -> Result<Build, Diagnostic> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 4 {
            return Err(Diagnostic::error(
                "SELU0201",
                format!("build `{s}` must be major.minor.patch.build"),
            ));
        }
        let n = |i: usize| {
            parts[i].parse::<u32>().map_err(|_| {
                Diagnostic::error("SELU0201", format!("`{}` is not a number", parts[i]))
            })
        };
        Ok(Build {
            major: n(0)?,
            minor: n(1)?,
            patch: n(2)?,
            build: n(3)?,
        })
    }
}

impl fmt::Display for Build {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.major, self.minor, self.patch, self.build
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Evidence {
    Documented,
    Source,
    Verified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FunctionEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Declaration>,
    pub name: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub available: bool,
    /// Restriction flags, e.g. `protected`, `outOfCombatOnly`, `hardwareClick`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restrictions: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub returns_secret: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    pub evidence: Evidence,
    #[serde(
        rename = "source",
        alias = "provenance",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub provenance: Option<FunctionSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionSource {
    pub file: String,
    pub kind: String,
}

fn is_true(v: &bool) -> bool {
    *v
}
fn is_false(v: &bool) -> bool {
    !v
}
fn yes() -> bool {
    true
}

/// A global value that is not a function, e.g. a frame like `UIParent` or
/// a table like `SlashCmdList`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    pub name: String,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Declaration>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<EventParam>,
    pub evidence: Evidence,
}

/// A numbered widget suffix, inferred from typed named globals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameFamily {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    pub prefix: String,
    pub suffix: String,
    pub min_index: u32,
    pub max_index: u32,
    pub widget: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frame_families: Vec<FrameFamily>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<DataEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_globals: Vec<DataEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predicates: Vec<Declaration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<Declaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enums: Vec<Declaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub widgets: Vec<Declaration>,
    pub flavour: String,
    pub build: Build,
    pub interface: u32,
    /// Lua dialect features present on this client, e.g. `lua51`, `bit`.
    #[serde(default)]
    pub lua_dialects: Vec<String>,
    #[serde(default)]
    pub functions: Vec<FunctionEntry>,
    #[serde(default)]
    pub events: Vec<EventEntry>,
    #[serde(default, deserialize_with = "read_globals")]
    pub globals: Vec<GlobalEntry>,
    /// True for hand-made test fixtures, never for generated manifests.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fixture: bool,
}

impl Manifest {
    pub fn target_string(&self) -> String {
        format!("{}@{}", product(&self.flavour), self.build)
    }
    pub fn function(&self, name: &str) -> Option<&FunctionEntry> {
        self.functions
            .iter()
            .find(|f| f.name == name && f.available)
    }
    pub fn event(&self, name: &str) -> Option<&EventEntry> {
        self.events.iter().find(|e| e.name == name)
    }
    /// Whether `name` is a global the game defines on this target: a
    /// function, a global value, or a namespace such as `C_Timer` (the root
    /// of at least one `C_Timer.X` function).
    pub fn has_global(&self, name: &str) -> bool {
        self.function(name).is_some()
            || self.globals.iter().any(|g| g.name == name)
            || self.functions.iter().any(|f| {
                f.available
                    && f.name.len() > name.len()
                    && f.name.starts_with(name)
                    && f.name.as_bytes()[name.len()] == b'.'
            })
    }
}

pub fn load_manifest(path: &Path) -> Result<Manifest, Diagnostic> {
    let bytes = std::fs::read(path).map_err(|e| {
        Diagnostic::error(
            "SELU0202",
            format!("cannot read manifest {}: {e}", path.display()),
        )
    })?;
    format::decode(&bytes).map_err(|e| {
        Diagnostic::error(
            "SELU0202",
            format!("invalid manifest {}: {e}", path.display()),
        )
    })
}

/// Load history stores and legacy compiled/JSON manifests under `dir`.
pub fn load_manifest_dir(dir: &Path) -> Result<Vec<Manifest>, Diagnostic> {
    let mut out = Vec::new();
    let store_path = if dir.join("store").is_dir() {
        dir.join("store")
    } else {
        dir.to_path_buf()
    };
    if store_path.join("..").is_dir()
        && std::fs::read_dir(&store_path).is_ok_and(|mut entries| {
            entries.any(|e| e.is_ok_and(|e| e.path().extension().is_some_and(|x| x == "selus")))
        })
    {
        let workspace = if store_path.file_name().is_some_and(|name| name == "store") {
            store_path.parent().and_then(Path::parent)
        } else {
            store_path.parent()
        }
        .unwrap_or(&store_path);
        let cache_root = workspace.join("target/selu-manifest-cache");
        out.extend(
            store::load_cached(&store_path, &cache_root)
                .map_err(|e| Diagnostic::error("SELU0202", e.to_string()))?,
        );
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = std::fs::read_dir(&d).map_err(|e| {
            Diagnostic::error(
                "SELU0202",
                format!("cannot read manifest dir {}: {e}", d.display()),
            )
        })?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "json" || x == "selum") {
                out.push(load_manifest(&p)?);
            }
        }
    }
    out.sort_by(|a, b| a.flavour.cmp(&b.flavour).then(a.build.cmp(&b.build)));
    Ok(out)
}

/// A parsed `flavour@version` target. Version parts are partial:
/// `retail`, `retail@12`, `retail@12.0.1`, `retail@12.0.1.64914`,
/// or `retail@installed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    pub flavour: String,
    pub parts: Vec<u32>,
    pub installed: bool,
}

/// Transitional project aliases; branch mapping belongs exclusively to the updater.
pub fn product(name: &str) -> &str {
    match name {
        "retail" => "wow",
        "forever" => "wowf",
        "classic" => "wow_classic",
        "classic-era" => "wow_classic_era",
        "classic-anniversary" => "wow_anniversary",
        "classic-titan" => "wow_classic_titan",
        _ => name,
    }
}
/// How a client of one product selects and filters addon files: the TOC
/// `AllowLoadGameType` token it answers to, the `[Game]` and `[Family]`
/// folder names Blizzard's TOCs use, and the TOC file suffixes it tries, in
/// order, before the plain `Name.toc`. This is the one table for every
/// product; loaders and tools never match product codes themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientUi {
    pub game_type: &'static str,
    pub game_folder: &'static str,
    pub family: &'static str,
    pub toc_suffixes: &'static [&'static str],
}

/// The `wow-ui-source` branch that mirrors each product's interface code,
/// verified against the exporter's TACT product set and each branch's
/// workflow. The manifest updater and Studio's source-checkout check share it.
pub const SOURCE_BRANCHES: &[(&str, &str)] = &[
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

/// The source branch for a product code or alias.
pub fn source_branch(product_or_alias: &str) -> Option<&'static str> {
    let product = product(product_or_alias);
    SOURCE_BRANCHES
        .iter()
        .find(|(_, p)| *p == product)
        .map(|(branch, _)| *branch)
}

/// The live product a PTR or beta channel tests (`wowt` → `wow`).
pub fn base_product(product: &str) -> &str {
    match product {
        "wowt" | "wowxptr" | "wow_beta" => "wow",
        "wow_classic_ptr" | "wow_classic_beta" => "wow_classic",
        "wow_classic_era_ptr" => "wow_classic_era",
        "wow_anniversary_ptr" => "wow_anniversary",
        other => other,
    }
}

/// [`ClientUi`] for a product (code or transitional alias) at a client major
/// version. Classic products follow the expansion their build replays, so a
/// channel that moves to the next expansion needs no code change. `None` when
/// the product or its expansion is unknown.
pub fn client_ui(product_or_alias: &str, major: Option<u32>) -> Option<ClientUi> {
    const fn ui(
        game_type: &'static str,
        game_folder: &'static str,
        family: &'static str,
        toc_suffixes: &'static [&'static str],
    ) -> ClientUi {
        ClientUi {
            game_type,
            game_folder,
            family,
            toc_suffixes,
        }
    }
    let product = base_product(product(product_or_alias));
    Some(match product {
        "wow" => ui("mainline", "Mainline", "Mainline", &["Mainline"]),
        // Forever answers to `camelot`; its addons often still ship Mainline
        // or Vanilla TOCs, which the client accepts after its own suffix.
        "wowf" => ui(
            "camelot",
            "Camelot",
            "Mainline",
            &["Camelot", "Mainline", "Vanilla"],
        ),
        "wow_classic" | "wow_classic_era" | "wow_anniversary" | "wow_classic_titan" => {
            // Without a build, each channel's current expansion (2026-11):
            // the build, when known, always decides.
            let major = major.unwrap_or(match product {
                "wow_classic_era" => 1,
                "wow_anniversary" => 2,
                "wow_classic_titan" => 3,
                _ => 5,
            });
            match major {
                1 => ui("vanilla", "Vanilla", "Classic", &["Vanilla", "Classic"]),
                2 => ui("tbc", "TBC", "Classic", &["TBC", "BCC", "Classic"]),
                3 => ui("wrath", "Wrath", "Classic", &["Wrath", "WOTLKC", "Classic"]),
                4 => ui("cata", "Cata", "Classic", &["Cata", "Classic"]),
                5 => ui("mists", "Mists", "Classic", &["Mists", "Classic"]),
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// [`client_ui`] for a target string such as `wow_classic@5.5.4.70631`.
pub fn client_ui_for_target(target: &str) -> Option<ClientUi> {
    let (product, version) = target.split_once('@').unwrap_or((target, ""));
    let major = version.split('.').next().and_then(|m| m.parse().ok());
    client_ui(product, major)
}

/// The expansion a Classic client's major version replays, for display only.
fn classic_expansion(major: u32) -> Option<&'static str> {
    Some(match major {
        2 => "Burning Crusade",
        3 => "Wrath of the Lich King",
        4 => "Cataclysm",
        5 => "Mists of Pandaria",
        6 => "Warlords of Draenor",
        _ => return None,
    })
}

/// Display text is derived metadata, never a project target identity: it is
/// computed from the product code and the build, so it follows Blizzard's
/// channels when they move to a new expansion.
pub fn display_name(product: &str, build: &Build) -> String {
    let classic = |suffix: &str| match classic_expansion(build.major) {
        Some(expansion) => format!("{expansion} Classic{suffix}"),
        None => format!("Classic{suffix}"),
    };
    let name = match product {
        "wow" => "World of Warcraft".to_string(),
        "wowt" => "World of Warcraft PTR".to_string(),
        "wowxptr" => "World of Warcraft PTR 2".to_string(),
        "wow_beta" => "World of Warcraft Beta".to_string(),
        "wowf" => "World of Warcraft: Forever".to_string(),
        "wow_classic" => classic(""),
        "wow_classic_ptr" => classic(" PTR"),
        "wow_classic_beta" => classic(" Beta"),
        "wow_classic_era" => "Classic Era".to_string(),
        "wow_classic_era_ptr" => "Classic Era PTR".to_string(),
        "wow_anniversary" => classic(" Anniversary"),
        "wow_anniversary_ptr" => classic(" Anniversary PTR"),
        "wow_classic_titan" => "Titan Reforged".to_string(),
        _ => product.to_string(),
    };
    format!("{name} {}.{}.{}", build.major, build.minor, build.patch)
}
impl TargetSpec {
    pub fn parse(s: &str) -> Result<TargetSpec, Diagnostic> {
        let (flavour, version) = match s.split_once('@') {
            Some((f, v)) => (f, Some(v)),
            None => (s, None),
        };
        if flavour.is_empty() {
            return Err(
                Diagnostic::error("SELU0203", format!("target `{s}` has no flavour"))
                    .with_help("use a product like `wow`, or an exact full build pin"),
            );
        }
        let canonical = product(flavour);
        if canonical != flavour {
            eprintln!("warning: target `{flavour}` is deprecated; use `{canonical}`");
        }
        if canonical == flavour
            && flavour.starts_with("wow")
            && version.is_some_and(|v| v.split('.').count() != 4)
        {
            return Err(Diagnostic::error(
                "SELU0203",
                "product targets require an exact major.minor.patch.build pin",
            ));
        }
        match version {
            None => Ok(TargetSpec {
                flavour: flavour.into(),
                parts: vec![],
                installed: false,
            }),
            Some("installed") => Ok(TargetSpec {
                flavour: flavour.into(),
                parts: vec![],
                installed: true,
            }),
            Some(v) => {
                let raw: Vec<&str> = v.split('.').collect();
                if raw.is_empty() || raw.len() > 4 {
                    return Err(Diagnostic::error(
                        "SELU0203",
                        format!("target version `{v}` must have 1 to 4 numbers"),
                    ));
                }
                let mut parts = Vec::new();
                for p in raw {
                    parts.push(p.parse::<u32>().map_err(|_| {
                        Diagnostic::error("SELU0203", format!("`{p}` in `{s}` is not a number"))
                    })?);
                }
                Ok(TargetSpec {
                    flavour: flavour.into(),
                    parts,
                    installed: false,
                })
            }
        }
    }

    fn matches(&self, b: &Build) -> bool {
        let full = [b.major, b.minor, b.patch, b.build];
        self.parts.iter().zip(full.iter()).all(|(a, b)| a == b)
    }
}

impl fmt::Display for TargetSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.installed {
            write!(f, "{}@installed", self.flavour)
        } else if self.parts.is_empty() {
            write!(f, "{}", self.flavour)
        } else {
            let v: Vec<String> = self.parts.iter().map(|n| n.to_string()).collect();
            write!(f, "{}@{}", self.flavour, v.join("."))
        }
    }
}

/// Resolve to the newest matching manifest. Unknown builds are an error
/// listing the closest known builds for that flavour.
pub fn resolve<'a>(
    spec: &TargetSpec,
    manifests: &'a [Manifest],
) -> Result<&'a Manifest, Diagnostic> {
    if spec.installed {
        return Err(Diagnostic::error(
            "SELU0204",
            format!("target `{spec}`: reading the installed client build is not implemented yet"),
        )
        .with_help("pin an exact build, e.g. `wow@12.1.5.70783`"));
    }
    let same_flavour: Vec<&Manifest> = manifests
        .iter()
        .filter(|m| product(&m.flavour) == product(&spec.flavour))
        .collect();
    if same_flavour.is_empty() {
        let known: Vec<String> = {
            let mut f: Vec<&str> = manifests.iter().map(|m| m.flavour.as_str()).collect();
            f.sort_unstable();
            f.dedup();
            f.into_iter().map(str::to_string).collect()
        };
        return Err(Diagnostic::error(
            "SELU0204",
            format!("unknown flavour `{}` in target `{spec}`", spec.flavour),
        )
        .with_help(format!("known flavours: {}", known.join(", "))));
    }
    same_flavour
        .iter()
        .filter(|m| spec.matches(&m.build))
        .max_by_key(|m| (m.build.build, m.build))
        .copied()
        .ok_or_else(|| {
            let mut builds: Vec<String> =
                same_flavour.iter().map(|m| m.build.to_string()).collect();
            builds.sort();
            Diagnostic::error(
                "SELU0204",
                format!(
                    "unknown build for target `{spec}`; Selu never silently uses another build"
                ),
            )
            .with_help(format!(
                "closest known {} builds: {}",
                spec.flavour,
                builds.join(", ")
            ))
        })
}

#[cfg(test)]
mod tests {
    #[test]
    fn client_ui_covers_every_product() {
        let ui = |t: &str| {
            client_ui_for_target(t).map(|u| (u.game_type, u.game_folder, u.toc_suffixes[0]))
        };
        assert_eq!(
            ui("wow@12.0.5.1"),
            Some(("mainline", "Mainline", "Mainline"))
        );
        assert_eq!(ui("retail"), Some(("mainline", "Mainline", "Mainline")));
        assert_eq!(
            ui("wowt@12.1.0.1"),
            Some(("mainline", "Mainline", "Mainline"))
        );
        assert_eq!(
            ui("wowf@1.60.1.70790"),
            Some(("camelot", "Camelot", "Camelot"))
        );
        assert_eq!(ui("forever"), Some(("camelot", "Camelot", "Camelot")));
        assert_eq!(
            ui("wow_classic_era@1.15.8.1"),
            Some(("vanilla", "Vanilla", "Vanilla"))
        );
        assert_eq!(
            ui("wow_classic_era"),
            Some(("vanilla", "Vanilla", "Vanilla"))
        );
        assert_eq!(
            ui("wow_classic_era_ptr@1.15.9.1"),
            Some(("vanilla", "Vanilla", "Vanilla"))
        );
        assert_eq!(ui("wow_anniversary@2.5.5.1"), Some(("tbc", "TBC", "TBC")));
        assert_eq!(
            ui("wow_classic_titan@3.80.0.1"),
            Some(("wrath", "Wrath", "Wrath"))
        );
        assert_eq!(ui("wow_classic@4.4.2.1"), Some(("cata", "Cata", "Cata")));
        assert_eq!(
            ui("wow_classic@5.5.4.70631"),
            Some(("mists", "Mists", "Mists"))
        );
        assert_eq!(
            ui("wow_classic_beta@5.5.5.1"),
            Some(("mists", "Mists", "Mists"))
        );
        // Without a build, a Classic channel uses its current expansion.
        assert_eq!(ui("wow_classic"), Some(("mists", "Mists", "Mists")));
        assert_eq!(ui("classic-anniversary@test"), Some(("tbc", "TBC", "TBC")));
        assert_eq!(ui("wow_classic@9.0.0.1"), None);
        assert_eq!(ui("unknown@1.0.0.1"), None);
        for (_, product) in SOURCE_BRANCHES {
            let major = if product.starts_with("wow_") {
                "1"
            } else {
                "12"
            };
            assert!(
                client_ui_for_target(&format!("{product}@{major}.0.0.1")).is_some(),
                "{product} has no client UI entry"
            );
        }
        for suffixes in [&client_ui("wow_classic", Some(5)).unwrap().toc_suffixes] {
            assert_eq!(suffixes.last(), Some(&"Classic"));
        }
    }

    #[test]
    fn source_branches_map_products_and_aliases() {
        assert_eq!(source_branch("wow"), Some("live"));
        assert_eq!(source_branch("forever"), Some("forever"));
        assert_eq!(
            source_branch("wow_anniversary"),
            Some("classic_anniversary")
        );
        assert_eq!(source_branch("classic-era"), Some("classic_era"));
        assert_eq!(source_branch("nope"), None);
    }

    use super::*;

    fn fixture() -> Manifest {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json");
        load_manifest(&p).expect("fixture loads")
    }

    #[test]
    fn fixture_manifest_loads() {
        let m = fixture();
        assert!(m.fixture);
        assert_eq!(m.flavour, "forever");
        assert!(m.function("UnitHealth").unwrap().returns_secret);
        assert!(m.event("PLAYER_LOGIN").is_some());
        assert_eq!(m.event("UNIT_HEALTH").unwrap().params[0].name, "unit");
    }

    #[test]
    fn partial_versions_resolve_to_newest_match() {
        let older = fixture();
        let mut newer = fixture();
        newer.build = Build {
            major: 1,
            minor: 0,
            patch: 1,
            build: 2000,
        };
        let all = vec![older, newer];
        let r = resolve(&TargetSpec::parse("forever@1.0").unwrap(), &all).unwrap();
        assert_eq!(r.build.to_string(), "1.0.1.2000");
        let r = resolve(&TargetSpec::parse("forever").unwrap(), &all).unwrap();
        assert_eq!(r.build.to_string(), "1.0.1.2000");
        let r = resolve(&TargetSpec::parse("forever@1.0.0.1000").unwrap(), &all).unwrap();
        assert_eq!(r.build.to_string(), "1.0.0.1000");
    }

    #[test]
    fn product_latest_follows_build_revision() {
        // PTR history: 9.2.0 (43340) follows 9.2.5 (43254).
        let mut older = fixture();
        older.flavour = "wowt".into();
        older.build = Build::parse("9.2.5.43254").unwrap();
        let mut newer = older.clone();
        newer.build = Build::parse("9.2.0.43340").unwrap();
        let all = [older, newer];
        assert_eq!(
            resolve(&TargetSpec::parse("wowt").unwrap(), &all)
                .unwrap()
                .build
                .build,
            43340
        );
        assert_eq!(
            resolve(&TargetSpec::parse("wowt@9.2.5.43254").unwrap(), &all)
                .unwrap()
                .build
                .build,
            43254
        );
    }
    #[test]
    fn unknown_build_lists_closest() {
        let all = vec![fixture()];
        let e = resolve(&TargetSpec::parse("forever@9.9").unwrap(), &all).unwrap_err();
        assert_eq!(e.code, "SELU0204");
        assert!(format!("{:?}", e.help).contains("1.0.0.1000"), "{e:?}");
    }
}

/// Only structural API metadata; documentation and source text are excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overloads: Vec<Declaration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predicates: Vec<Declaration>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub values: std::collections::BTreeMap<String, serde_json::Value>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<Declaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub returns: Vec<Declaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Declaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<Declaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inherits: Vec<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub attributes: std::collections::BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Source {
    /// Non-fatal extraction limitations retained with this build revision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub commit: String,
    pub version: String,
    pub interface_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface_basis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface_evidence: Option<Evidence>,
}

/// Compact source-only metadata; source indexes Manifest.source_files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DataEntry {
    pub name: String,
    pub source: usize,
    pub evidence: Evidence,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inherits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mixins: Vec<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub values: std::collections::BTreeMap<String, serde_json::Value>,
}

/// Generated manifests may group globals with identical metadata into name lists.
fn read_globals<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<GlobalEntry>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Single(GlobalEntry),
        Group {
            names: Vec<String>,
            #[serde(flatten)]
            metadata: GlobalMetadata,
        },
    }
    #[derive(Deserialize)]
    struct GlobalMetadata {
        #[serde(rename = "type", default)]
        ty: Option<String>,
        evidence: Evidence,
        #[serde(default)]
        templates: Vec<String>,
    }
    let mut globals = Vec::new();
    for entry in Vec::<Entry>::deserialize(deserializer)? {
        match entry {
            Entry::Single(g) => globals.push(g),
            Entry::Group { names, metadata } => {
                globals.extend(names.into_iter().map(|name| GlobalEntry {
                    name,
                    ty: metadata.ty.clone(),
                    evidence: metadata.evidence,
                    templates: metadata.templates.clone(),
                    value: None,
                }))
            }
        }
    }
    globals.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(globals)
}

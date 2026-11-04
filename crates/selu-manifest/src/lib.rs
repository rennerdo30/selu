//! Target manifests: the API surface of one exact game build.
//!
//! A target is `flavour@version`. Partial versions resolve to the newest
//! matching known manifest; an unknown build is an error listing the
//! closest known builds (DESIGN.md section 2).

pub mod format;
pub mod resolution;
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
        format!("{}@{}", self.flavour, self.build)
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

/// Load every `*.json` or `*.selum` manifest under `dir`, recursively.
pub fn load_manifest_dir(dir: &Path) -> Result<Vec<Manifest>, Diagnostic> {
    let mut out = Vec::new();
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

impl TargetSpec {
    pub fn parse(s: &str) -> Result<TargetSpec, Diagnostic> {
        let (flavour, version) = match s.split_once('@') {
            Some((f, v)) => (f, Some(v)),
            None => (s, None),
        };
        if flavour.is_empty() {
            return Err(
                Diagnostic::error("SELU0203", format!("target `{s}` has no flavour"))
                    .with_help("use a target like `retail@12.0`"),
            );
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
        .with_help("pin an exact build for now, e.g. `retail@12.0`"));
    }
    let same_flavour: Vec<&Manifest> = manifests
        .iter()
        .filter(|m| m.flavour == spec.flavour)
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
        .max_by_key(|m| m.build)
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

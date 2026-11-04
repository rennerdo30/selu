//! Completion and hover data: the game API of the project's target (from
//! its manifest) plus the names in the project's own files.

use selu_manifest::{Declaration, Manifest};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ItemKind {
    Keyword,
    Local,
    Function,
    Namespace,
    Method,
    Event,
    Global,
}

impl ItemKind {
    pub fn tag(self) -> &'static str {
        match self {
            ItemKind::Keyword => "kw",
            ItemKind::Local => "abc",
            ItemKind::Function => "fn",
            ItemKind::Namespace => "ns",
            ItemKind::Method => "m",
            ItemKind::Event => "ev",
            ItemKind::Global => "g",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub label: String,
    pub kind: ItemKind,
    /// Signature or short description.
    pub detail: String,
}

/// The game API of one target, indexed for the editor.
#[derive(Default)]
pub struct ApiIndex {
    pub target: String,
    /// Global functions without a namespace: `CreateFrame`, `print`...
    functions: BTreeMap<String, String>,
    /// `C_Timer` -> `After` -> signature.
    namespaces: BTreeMap<String, BTreeMap<String, String>>,
    /// Every widget method name (all widget types) -> `Widget:Method(...)`.
    methods: BTreeMap<String, String>,
    events: BTreeMap<String, String>,
    globals: BTreeMap<String, String>,
}

fn arg_list(d: &Declaration) -> String {
    d.arguments
        .iter()
        .map(|a| {
            let optional = a
                .attributes
                .get("Nilable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let ty = a.ty.as_deref().unwrap_or("any");
            format!("{}{}: {ty}", a.name, if optional { "?" } else { "" })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn returns(d: &Declaration) -> String {
    match d.returns.len() {
        0 => String::new(),
        1 => format!(": {}", d.returns[0].ty.as_deref().unwrap_or("any")),
        _ => format!(
            ": [{}]",
            d.returns
                .iter()
                .map(|r| format!("{}: {}", r.name, r.ty.as_deref().unwrap_or("any")))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

impl ApiIndex {
    pub fn from_manifest(m: &Manifest) -> ApiIndex {
        let mut index = ApiIndex {
            target: format!("{}@{}", m.flavour, m.build),
            ..ApiIndex::default()
        };
        for f in &m.functions {
            if !f.available {
                continue;
            }
            let sig = match &f.signature {
                Some(d) => format!("{}({}){}", f.name, arg_list(d), returns(d)),
                None => format!("{}({})", f.name, f.params.join(", ")),
            };
            let sig = if f.restrictions.is_empty() {
                sig
            } else {
                format!("{sig}\nrestricted: {}", f.restrictions.join(", "))
            };
            match f.name.split_once('.') {
                Some((ns, member)) => {
                    index
                        .namespaces
                        .entry(ns.to_string())
                        .or_default()
                        .insert(member.to_string(), sig);
                }
                None => {
                    index.functions.insert(f.name.clone(), sig);
                }
            }
        }
        for w in &m.widgets {
            for method in &w.methods {
                index.methods.entry(method.name.clone()).or_insert_with(|| {
                    format!(
                        "{}:{}({}){}",
                        w.name,
                        method.name,
                        arg_list(method),
                        returns(method)
                    )
                });
            }
        }
        for e in &m.events {
            let params = e
                .params
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            index
                .events
                .insert(e.name.clone(), format!("on {}({params})", e.name));
        }
        for g in &m.globals {
            let ty = g.ty.as_deref().unwrap_or("global");
            index
                .globals
                .insert(g.name.clone(), format!("{}: {ty}", g.name));
        }
        index
    }

    /// Is `word` a top-level API name (for highlighting)?
    pub fn is_api(&self, word: &str) -> bool {
        self.functions.contains_key(word) || self.namespaces.contains_key(word)
    }

    /// Hover text for a dotted name such as `C_Timer.After`, `CreateFrame`,
    /// `frame.SetPoint` or an event name.
    pub fn hover(&self, dotted: &str) -> Option<String> {
        let mut parts = dotted.rsplitn(2, '.');
        let last = parts.next()?;
        if let Some(object) = parts.next() {
            let object = object.rsplit('.').next().unwrap_or(object);
            if let Some(sig) = self.namespaces.get(object).and_then(|m| m.get(last)) {
                return Some(sig.clone());
            }
            return self
                .methods
                .get(last)
                .map(|s| format!("{s}\n(method; the receiver's type isn't checked here)"));
        }
        if let Some(sig) = self.functions.get(last) {
            return Some(sig.clone());
        }
        if let Some(members) = self.namespaces.get(last) {
            return Some(format!("namespace {last} ({} functions)", members.len()));
        }
        self.events
            .get(last)
            .or_else(|| self.globals.get(last))
            .cloned()
    }
}

/// What the cursor is completing.
#[derive(Debug, Clone, PartialEq)]
pub enum Context {
    /// After `object.`
    Member(String),
    /// After `on `
    Event,
    Plain,
}

/// The identifier prefix before byte offset `at`, and the context it sits in.
pub fn context_at(text: &str, at: usize) -> (usize, String, Context) {
    let b = text.as_bytes();
    let mut s = at;
    while s > 0 && (b[s - 1].is_ascii_alphanumeric() || b[s - 1] == b'_' || b[s - 1] == b'$') {
        s -= 1;
    }
    let prefix = text[s..at].to_string();
    if s > 0 && b[s - 1] == b'.' {
        let mut k = s - 1;
        while k > 0 && (b[k - 1].is_ascii_alphanumeric() || b[k - 1] == b'_' || b[k - 1] == b'$') {
            k -= 1;
        }
        return (s, prefix, Context::Member(text[k..s - 1].to_string()));
    }
    let before = text[..s].trim_end_matches([' ', '\t']);
    if before.ends_with("on")
        && before.len() < s
        && before[..before.len() - 2]
            .chars()
            .last()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_')
    {
        return (s, prefix, Context::Event);
    }
    (s, prefix, Context::Plain)
}

/// Identifiers declared or used in the project's files, for completion.
pub fn local_words(texts: &[&str]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for text in texts {
        for (range, kind) in super::highlight::spans(text, &|_| false) {
            if matches!(
                kind,
                super::highlight::Kind::Plain | super::highlight::Kind::Call
            ) {
                // Spans of one kind are merged, so split them into words.
                for w in text[range].split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                {
                    if w.len() >= 3
                        && w.chars()
                            .next()
                            .is_some_and(|c| c.is_alphabetic() || c == '_')
                    {
                        out.insert(w.to_string());
                    }
                }
            }
        }
    }
    out
}

fn rank(label: &str, prefix: &str) -> Option<(u8, usize)> {
    if prefix.is_empty() {
        return Some((2, label.len()));
    }
    if label.starts_with(prefix) {
        Some((0, label.len()))
    } else if label.to_lowercase().starts_with(&prefix.to_lowercase()) {
        Some((1, label.len()))
    } else if label.to_lowercase().contains(&prefix.to_lowercase()) {
        Some((3, label.len()))
    } else {
        None
    }
}

/// Completion items for `prefix` in `context`, best first, at most `limit`.
pub fn items(
    api: Option<&ApiIndex>,
    locals: &BTreeSet<String>,
    context: &Context,
    prefix: &str,
    limit: usize,
) -> Vec<Item> {
    let mut found: Vec<((u8, usize), Item)> = Vec::new();
    let mut add = |label: &str, kind: ItemKind, detail: &str| {
        if label == prefix {
            return;
        }
        if let Some(r) = rank(label, prefix) {
            found.push((
                r,
                Item {
                    label: label.to_string(),
                    kind,
                    detail: detail.to_string(),
                },
            ));
        }
    };
    match context {
        Context::Member(object) => {
            if let Some(api) = api {
                if let Some(members) = api.namespaces.get(object) {
                    for (name, sig) in members {
                        add(name, ItemKind::Function, sig);
                    }
                } else {
                    for (name, sig) in &api.methods {
                        add(name, ItemKind::Method, sig);
                    }
                }
            }
            if prefix.len() >= 2 {
                for w in locals {
                    add(w, ItemKind::Local, "");
                }
            }
        }
        Context::Event => {
            if let Some(api) = api {
                for (name, sig) in &api.events {
                    add(name, ItemKind::Event, sig);
                }
            }
        }
        Context::Plain => {
            if prefix.is_empty() {
                return Vec::new();
            }
            for k in super::highlight::keywords() {
                add(k, ItemKind::Keyword, "");
            }
            for w in locals {
                add(w, ItemKind::Local, "");
            }
            if let Some(api) = api {
                for (name, sig) in &api.functions {
                    add(name, ItemKind::Function, sig);
                }
                for (name, members) in &api.namespaces {
                    add(
                        name,
                        ItemKind::Namespace,
                        &format!("{} functions", members.len()),
                    );
                }
                if prefix.len() >= 2 {
                    for (name, detail) in &api.globals {
                        add(name, ItemKind::Global, detail);
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
    let mut seen = HashMap::new();
    found
        .into_iter()
        .filter(|(_, item)| seen.insert(item.label.clone(), ()).is_none())
        .take(limit)
        .map(|(_, item)| item)
        .collect()
}

/// The manifests folder for a project: the nearest `manifests/` walking up
/// from it, or `$SELU_MANIFEST_DIR` (the same search as `selu build`).
pub fn find_manifests(project: &Path) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SELU_MANIFEST_DIR") {
        return Some(PathBuf::from(p));
    }
    let mut dir = std::fs::canonicalize(project).ok();
    while let Some(d) = dir {
        if d.join("manifests").is_dir() {
            return Some(d.join("manifests"));
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

/// The first target in the project's `selu.toml`, e.g. `retail@12.0`.
pub fn project_target(project: &Path) -> Option<String> {
    let text = std::fs::read_to_string(project.join("selu.toml")).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    let target = value
        .get("addon")?
        .get("targets")?
        .as_array()?
        .first()?
        .as_str()
        .map(String::from)?;
    selu_manifest::lock::Lock::read(project)
        .ok()?
        .discovery_targets(&[target])
        .ok()?
        .pop()
}

/// Load the API index for `target` from the manifests folder. Slow (the
/// manifests are large); call it on a background thread.
pub fn load_api(manifests: &Path, target: &str) -> Result<ApiIndex, String> {
    let spec = selu_manifest::TargetSpec::parse(target).map_err(|d| d.to_string())?;
    let flavour_dir = manifests.join(&spec.flavour);
    let dir = if flavour_dir.is_dir() && !manifests.join("store").is_dir() {
        flavour_dir
    } else {
        manifests.to_path_buf()
    };
    let all = selu_manifest::load_manifest_dir(&dir).map_err(|d| d.to_string())?;
    let manifest = selu_manifest::resolve(&spec, &all).map_err(|d| d.to_string())?;
    Ok(ApiIndex::from_manifest(manifest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ApiIndex {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json");
        ApiIndex::from_manifest(&selu_manifest::load_manifest(&path).unwrap())
    }

    #[test]
    fn contexts() {
        assert_eq!(
            context_at("x = C_Timer.Af", 14),
            (12, "Af".into(), Context::Member("C_Timer".into()))
        );
        assert_eq!(context_at("on PLAY", 7), (3, "PLAY".into(), Context::Event));
        assert_eq!(context_at("const ab", 8), (6, "ab".into(), Context::Plain));
        assert_eq!(context_at("tion PL", 7).2, Context::Plain);
    }

    #[test]
    fn completes_from_the_manifest_and_the_project() {
        let api = fixture();
        let locals = local_words(&["const myCounter = 1; myCounter += 1;"]);
        assert!(locals.contains("myCounter"));
        let got = items(Some(&api), &locals, &Context::Plain, "Crea", 10);
        assert_eq!(got.first().map(|i| i.label.as_str()), Some("CreateFrame"));
        assert!(got[0].detail.starts_with("CreateFrame("), "{:?}", got[0]);
        let got = items(Some(&api), &locals, &Context::Plain, "myC", 10);
        assert_eq!(got[0].label, "myCounter");
        let events = items(Some(&api), &locals, &Context::Event, "PLAYER_LO", 10);
        assert!(
            events.iter().any(|i| i.label == "PLAYER_LOGIN"),
            "{events:?}"
        );
        assert!(
            api.hover("CreateFrame")
                .unwrap()
                .starts_with("CreateFrame(")
        );
    }
}

//! Inventory statically resolved engine receivers, excluding unresolved/dynamic values.
use selu_manifest::Manifest;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
pub struct WidgetCall {
    pub receiver: String,
    pub local: bool,
    pub ty: Option<String>,
    pub templates: Vec<String>,
    pub method: String,
    pub file: String,
    pub line: usize,
}
fn member(m: &Manifest, ty: &str, name: &str, path: &mut BTreeSet<String>) -> bool {
    if !path.insert(ty.into()) {
        return false;
    }
    if let Some(w) = m.widgets.iter().find(|w| w.name == ty) {
        if w.methods.iter().any(|d| d.name == name) {
            return true;
        }
        for base in &w.inherits {
            if member(m, base, name, path) {
                return true;
            }
        }
    }
    if let Some(d) = m
        .templates
        .iter()
        .chain(&m.data_globals)
        .find(|d| d.name == ty)
    {
        if m.function(&format!("{ty}.{name}"))
            .is_some_and(|f| f.signature.is_some())
        {
            return true;
        }
        for b in d.inherits.iter().chain(&d.mixins) {
            if member(m, b, name, path) {
                return true;
            }
        }
    }
    false
}
pub fn inventory(
    m: &Manifest,
    calls: &[WidgetCall],
    source_methods: &BTreeSet<String>,
) -> serde_json::Value {
    let source_names: BTreeSet<_> = source_methods
        .iter()
        .cloned()
        .chain(
            m.functions
                .iter()
                .filter(|f| f.name.contains('.') && f.signature.is_some())
                .map(|f| f.name.rsplit('.').next().unwrap().to_owned())
                .chain(
                    m.widgets
                        .iter()
                        .filter(|w| w.name.starts_with("SourceObject_"))
                        .flat_map(|w| w.methods.iter().map(|d| d.name.clone())),
                ),
        )
        .collect();
    let mut source_pairs: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut gaps: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut resolved = 0;
    let mut unresolved = 0;
    for c in calls {
        let global = if !c.local {
            m.globals.iter().find(|g| g.name == c.receiver)
        } else {
            None
        };
        let Some(ty) = c.ty.as_ref().or_else(|| global.and_then(|g| g.ty.as_ref())) else {
            unresolved += 1;
            continue;
        };
        if !m.widgets.iter().any(|w| &w.name == ty) {
            unresolved += 1;
            continue;
        }
        resolved += 1;
        let templates = global
            .map(|g| g.templates.as_slice())
            .unwrap_or(&c.templates);
        if !std::iter::once(ty)
            .chain(templates)
            .any(|t| member(m, t, &c.method, &mut BTreeSet::new()))
        {
            (if source_names.contains(&c.method) {
                &mut source_pairs
            } else {
                &mut gaps
            })
            .entry(ty.clone())
            .or_default()
            .entry(c.method.clone())
            .or_default()
            .push(format!("{}:{} ({})", c.file, c.line, c.receiver));
        }
    }
    let count: usize = gaps.values().map(BTreeMap::len).sum();
    serde_json::json!({"pin":m.source.as_ref().map(|s|&s.commit),"resolvedCalls":resolved,"unresolvedCalls":unresolved,"remainingEnginePairs":count,"widgets":gaps,"sourceImplementedOrAmbiguousPairs":source_pairs,"scope":"Direct unshadowed typed globals and lexical literal CreateFrame receivers; dynamic receivers, fields, self and aliases without a proven constructor are unresolved, not evidence of absence. Includes source variants, not TOC loading."})
}

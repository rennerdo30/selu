//! Index immutable manifest names once per target, retaining first-match lookup order.
use selu_manifest::{FunctionEntry, Manifest};
use std::collections::{HashMap, HashSet};
#[derive(Default)]
pub(crate) struct ManifestIndex {
    functions: HashMap<String, usize>,
    pub globals: HashMap<String, usize>,
    names: HashSet<String>,
}
impl ManifestIndex {
    pub fn new(m: &Manifest) -> Self {
        let mut out = Self::default();
        for (i, f) in m.functions.iter().enumerate().filter(|(_, f)| f.available) {
            out.functions.entry(f.name.clone()).or_insert(i);
            out.names.insert(f.name.clone());
            for (i, _) in f.name.match_indices('.') {
                out.names.insert(f.name[..i].to_string());
            }
        }
        for (i, g) in m.globals.iter().enumerate() {
            out.globals.entry(g.name.clone()).or_insert(i);
            out.names.insert(g.name.clone());
        }
        out
    }
    pub fn function<'a>(&self, m: &'a Manifest, name: &str) -> Option<&'a FunctionEntry> {
        self.functions.get(name).map(|i| &m.functions[*i])
    }
    pub fn has_global(&self, name: &str) -> bool {
        self.names.contains(name)
    }
}

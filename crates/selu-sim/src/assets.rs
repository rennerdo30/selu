//! Bounded asset sources shared by simulator probes, fonts and future client storage.
use std::{cell::RefCell, collections::HashMap, path::PathBuf, sync::Arc};
#[derive(Clone, Debug)]
pub enum Lookup {
    Found(Arc<[u8]>),
    Unavailable,
    Missing,
}
pub struct Resolver {
    sources: Vec<Source>,
    cache: RefCell<HashMap<String, Lookup>>,
}
pub enum Source {
    AddonMedia(PathBuf),
    /// Reserved for SIM4. No client storage is opened by SIM3.
    ClientStorage,
}
impl Resolver {
    pub fn add_addon(&mut self, root: PathBuf) {
        if !self
            .sources
            .iter()
            .any(|s| matches!(s, Source::AddonMedia(p) if p == &root))
        {
            self.sources.push(Source::AddonMedia(root));
            self.cache.get_mut().clear();
        }
    }
    pub fn lookup(&self, path: &str) -> Lookup {
        let normalized = path.replace('\\', "/");
        let key = normalized.to_ascii_lowercase();
        if let Some(value) = self.cache.borrow().get(&key) {
            return value.clone();
        }
        let invalid = normalized
            .split('/')
            .any(|p| p.is_empty() || p == ".." || p == "." || p.contains(':'));
        let value = if invalid {
            Lookup::Missing
        } else {
            let mut result = Lookup::Missing;
            for source in &self.sources {
                match source.lookup(&normalized) {
                    found @ Lookup::Found(_) => {
                        result = found;
                        break;
                    }
                    Lookup::Unavailable => result = Lookup::Unavailable,
                    Lookup::Missing => {}
                }
            }
            result
        };
        if self.cache.borrow().len() >= 128 {
            self.cache.borrow_mut().clear();
        }
        self.cache.borrow_mut().insert(key, value.clone());
        value
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self {
            sources: vec![Source::ClientStorage],
            cache: RefCell::default(),
        }
    }
}
impl Source {
    fn lookup(&self, name: &str) -> Lookup {
        let key = name.to_ascii_lowercase();
        let client = (key.starts_with("interface/") && !key.starts_with("interface/addons/"))
            || key.starts_with("fonts/")
            || key.parse::<u32>().is_ok();
        match self {
            Self::ClientStorage => {
                if client {
                    Lookup::Unavailable
                } else {
                    Lookup::Missing
                }
            }
            Self::AddonMedia(root) => {
                if client {
                    return Lookup::Missing;
                }
                for candidate in std::iter::once(name.to_owned())
                    .chain(["tga", "blp", "png"].iter().map(|e| format!("{name}.{e}")))
                {
                    if let Ok(path) = crate::fonts::resolve(std::slice::from_ref(root), &candidate)
                    {
                        use std::io::Read;
                        if let Ok(file) = std::fs::File::open(path)
                            && file.metadata().is_ok_and(|m| m.len() <= 16 * 1024 * 1024)
                        {
                            let mut bytes = Vec::new();
                            if file
                                .take(16 * 1024 * 1024 + 1)
                                .read_to_end(&mut bytes)
                                .is_ok()
                                && bytes.len() <= 16 * 1024 * 1024
                            {
                                return Lookup::Found(bytes.into());
                            }
                        }
                    }
                }
                Lookup::Missing
            }
        }
    }
}

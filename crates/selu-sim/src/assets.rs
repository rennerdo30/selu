//! Bounded asset sources shared by simulator probes, fonts and future client storage.
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, Weak},
};
#[derive(Clone, Debug)]
pub enum Lookup {
    Found(Arc<[u8]>),
    Unavailable,
    Missing,
}
pub struct Resolver {
    sources: Vec<Source>,
    atlases: RefCell<HashMap<String, Option<selu_gamedata::db2::Atlas>>>,
    cache: RefCell<HashMap<String, Lookup>>,
}
pub enum Source {
    AddonMedia(PathBuf),
    /// Local, read-only client storage; None preserves explicit offline placeholders.
    ClientStorage(Option<Arc<selu_gamedata::Storage>>),
}
impl Resolver {
    pub fn open_client(&mut self, root: &std::path::Path, product: &str) -> Result<(), String> {
        // Studio and its worker share one read-only index; decoded images remain lazy.
        type Key = (PathBuf, String);
        type Index = HashMap<Key, Weak<selu_gamedata::Storage>>;
        static INDEXES: OnceLock<Mutex<Index>> = OnceLock::new();
        let key = (
            root.canonicalize().map_err(|e| e.to_string())?,
            product.to_owned(),
        );
        let mut indexes = INDEXES
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|e| e.to_string())?;
        indexes.retain(|_, value| value.strong_count() > 0);
        let storage = if let Some(storage) = indexes.get(&key).and_then(Weak::upgrade) {
            storage
        } else {
            let storage = Arc::new(
                selu_gamedata::Storage::open(root, product, None).map_err(|e| e.to_string())?,
            );
            indexes.insert(key, Arc::downgrade(&storage));
            storage
        };
        self.sources
            .retain(|s| !matches!(s, Source::ClientStorage(_)));
        self.sources.insert(0, Source::ClientStorage(Some(storage)));
        self.cache.get_mut().clear();
        self.atlases.get_mut().clear();
        Ok(())
    }
    pub fn file_id(&self, name: &str) -> Option<u32> {
        if let Ok(id) = name.parse() {
            return Some(id);
        }
        self.sources.iter().find_map(|source| match source {
            Source::ClientStorage(Some(storage)) => storage
                .resolve(name)
                .or_else(|_| storage.resolve(&format!("{name}.blp")))
                .ok(),
            _ => None,
        })
    }
    pub fn atlas(&self, name: &str) -> Option<selu_gamedata::db2::Atlas> {
        if let Some(value) = self.atlases.borrow().get(name) {
            return value.clone();
        }
        let result = self.sources.iter().find_map(|s| match s {
            Source::ClientStorage(Some(storage)) => storage.atlas(name).ok(),
            _ => None,
        });
        self.atlases
            .borrow_mut()
            .insert(name.into(), result.clone());
        result
    }
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
            atlases: RefCell::default(),
            sources: vec![Source::ClientStorage(None)],
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
            Self::ClientStorage(storage) => {
                if client && let Some(storage) = storage {
                    let read = |path: &str| {
                        if let Ok(id) = path.parse::<u32>() {
                            storage.read(id)
                        } else {
                            storage.read_path(path)
                        }
                    };
                    for candidate in [name.to_owned(), format!("{name}.blp")] {
                        if let Ok(bytes) = read(&candidate) {
                            return Lookup::Found(bytes.into());
                        }
                    }
                    return Lookup::Missing;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_and_invalid_paths() {
        let r = Resolver::default();
        assert!(matches!(
            r.lookup("Interface/Buttons/WHITE8X8"),
            Lookup::Unavailable
        ));
        assert!(matches!(
            r.lookup("Fonts/FRIZQT__.TTF"),
            Lookup::Unavailable
        ));
        assert!(matches!(r.lookup("Interface/../WTF/file"), Lookup::Missing));
        assert!(matches!(r.lookup("WTF/Config.wtf"), Lookup::Missing));
    }
    #[test]
    #[ignore = "requires owner's Forever install"]
    fn client_assets() {
        let mut r = Resolver::default();
        r.open_client(
            std::path::Path::new("E:/Games/Battle.net/World of Warcraft"),
            "wowf",
        )
        .unwrap();
        for path in [
            "Fonts/FRIZQT__.TTF",
            "Fonts/ARIALN.TTF",
            "Interface/Buttons/WHITE8X8",
        ] {
            assert!(matches!(r.lookup(path), Lookup::Found(_)), "{path}");
        }
        assert!(matches!(
            r.lookup("Interface/RennersAddons/ProbeMissingTexture"),
            Lookup::Missing
        ));
        use ab_glyph::Font;
        let mut fonts = crate::fonts::FontBook::default();
        fonts
            .open_client(
                std::path::Path::new("E:/Games/Battle.net/World of Warcraft"),
                "wowf",
            )
            .unwrap();
        let face = fonts.face("Fonts/FRIZQT__.TTF");
        let fallback = crate::fonts::fallback();
        assert_ne!(
            face.h_advance_unscaled(face.glyph_id('W')),
            fallback.h_advance_unscaled(fallback.glyph_id('W'))
        );
        let a = r.atlas("UI-Character-Info-Title").unwrap();
        assert_eq!((a.width, a.height), (196, 40));
        assert!(matches!(
            r.lookup(&a.file_data_id.to_string()),
            Lookup::Found(_)
        ));
    }
}

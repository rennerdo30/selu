//! Font assets and the simulator/Studio text backend. No GUI or system fonts.
pub use crate::sim_globals::create_font;
use ab_glyph::FontArc;
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

#[derive(Default)]
pub struct FontBook {
    pub assets: crate::assets::Resolver,
    faces: RefCell<HashMap<String, FontArc>>,
    layouts: RefCell<HashMap<String, crate::text::TextLayout>>,
    asset_presence: RefCell<HashMap<String, bool>>,
    named_colors: HashMap<String, u32>,
}
impl FontBook {
    pub(crate) fn set_colors(&mut self, colors: &[(String, u32)]) {
        self.named_colors = colors.iter().cloned().collect();
        self.layouts.get_mut().clear();
    }
    pub(crate) fn color(&self, name: &str) -> Option<u32> {
        self.named_colors.get(name).copied()
    }
    pub fn open_client(&mut self, root: &std::path::Path, product: &str) -> Result<(), String> {
        self.open_client_with_keys(root, product, None)
    }
    pub fn open_client_with_keys(
        &mut self,
        root: &Path,
        product: &str,
        key_file: Option<&Path>,
    ) -> Result<(), String> {
        self.assets.open_client_with_keys(root, product, key_file)?;
        self.faces.get_mut().clear();
        self.layouts.get_mut().clear();
        self.asset_presence.get_mut().clear();
        self.named_colors.clear();
        Ok(())
    }

    pub fn asset(&self, name: &str) -> bool {
        let key = name.replace('\\', "/").to_ascii_lowercase();
        if let Some(found) = self.asset_presence.borrow().get(&key) {
            return *found;
        }
        let found = matches!(self.assets.lookup(name), crate::assets::Lookup::Found(_));
        let mut cache = self.asset_presence.borrow_mut();
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(key, found);
        found
    }
    pub fn face(&self, name: &str) -> FontArc {
        if let Some(face) = self.faces.borrow().get(name) {
            return face.clone();
        }
        let face = match self.assets.lookup(name) {
            crate::assets::Lookup::Found(bytes) => FontArc::try_from_vec(bytes.to_vec()).ok(),
            _ => None,
        }
        .unwrap_or_else(fallback);
        self.faces.borrow_mut().insert(name.into(), face.clone());
        face
    }
    pub fn layout(
        &self,
        text: &str,
        style: &crate::core::VisualStyle,
        width: Option<f64>,
        height: Option<f64>,
    ) -> crate::text::TextLayout {
        let key = format!(
            "{text}\0{width:?}\0{height:?}\0{}",
            serde_json::to_string(style).unwrap_or_default()
        );
        if let Some(layout) = self.layouts.borrow().get(&key) {
            return layout.clone();
        }
        let layout = crate::text::layout_with_resources(
            &self.face(&style.font),
            text,
            style,
            width,
            height,
            Some(self),
        );
        let mut cache = self.layouts.borrow_mut();
        if cache.len() >= 2048 {
            cache.clear();
        }
        cache.insert(key, layout.clone());
        layout
    }
}
pub fn fallback() -> FontArc {
    static FACE: std::sync::OnceLock<FontArc> = std::sync::OnceLock::new();
    FACE.get_or_init(|| {
        FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).expect("bundled font")
    })
    .clone()
}
/// Resolve native font templates even in manifest-free simulator sessions.
/// This is a private template registry, not a source of untyped Lua globals.
pub fn template(lua: &mlua::Lua, name: &str) -> mlua::Result<mlua::Table> {
    use mlua::{Table, Value};
    match lua.globals().raw_get::<Value>(name)? {
        Value::Table(t) => return Ok(t),
        Value::Nil => {}
        _ => return Err(mlua::Error::runtime("font template is not a Font object")),
    }
    if let Some(shapes) = lua
        .globals()
        .raw_get::<Option<Table>>("__selu_global_shapes")?
        && shapes
            .raw_get::<Option<String>>(name)?
            .is_some_and(|s| s.eq_ignore_ascii_case("font"))
    {
        return lua.globals().get(name);
    }
    Err(mlua::Error::runtime(format!(
        "unknown font template: {name}"
    )))
}
/// Called before addon chunks, including on reload and load-on-demand.
pub fn register_addon_root(lua: &mlua::Lua, dir: &Path) -> mlua::Result<()> {
    let root = dir.canonicalize().map_err(mlua::Error::external)?;
    let store = crate::frames::arena::store(lua);
    let mut arena = store.borrow_mut();
    arena.fonts.assets.add_addon(root);
    // Adding a source can resolve a prior miss; readable assets stay readable.
    arena
        .fonts
        .asset_presence
        .get_mut()
        .retain(|_, found| *found);
    arena.fonts.faces.borrow_mut().clear();
    arena.fonts.layouts.borrow_mut().clear();
    arena.dirty = true;
    arena.full_layout = true;
    arena.dirty_regions.clear();
    Ok(())
}
/// Refuse absolute paths, traversal, cross-addon prefixes and symlink escapes.
pub fn resolve(roots: &[PathBuf], name: &str) -> Result<PathBuf, String> {
    let name = name.replace('\\', "/");
    let prefix = "interface/addons/";
    let qualified = name.to_ascii_lowercase().starts_with(prefix);
    let name = if qualified {
        &name[prefix.len()..]
    } else {
        &name
    };
    let path = Path::new(name);
    if path
        .components()
        .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("font path escapes addon sandbox".into());
    }
    for root in roots {
        let local = if qualified {
            let Some((addon, rest)) = name.split_once('/') else {
                continue;
            };
            if !root
                .file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(addon))
            {
                continue;
            }
            Path::new(rest)
        } else {
            path
        };
        if let Ok(p) = root.join(local).canonicalize()
            && p.starts_with(root)
            && p.is_file()
        {
            return Ok(p);
        }
    }
    Err("unavailable addon font; using bundled fallback".into())
}

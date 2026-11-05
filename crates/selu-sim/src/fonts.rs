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
}
impl FontBook {
    pub fn open_client(&mut self, root: &std::path::Path, product: &str) -> Result<(), String> {
        self.assets.open_client(root, product)?;
        self.faces.get_mut().clear();
        self.layouts.get_mut().clear();
        Ok(())
    }

    pub fn asset(&self, name: &str) -> bool {
        matches!(self.assets.lookup(name), crate::assets::Lookup::Found(_))
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
        let layout = crate::text::layout(&self.face(&style.font), text, style, width, height);
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
    if native_style(name).is_none() {
        return Err(mlua::Error::runtime(format!(
            "unknown font template: {name}"
        )));
    }
    let registry = match lua.named_registry_value::<Option<Table>>("selu:font-templates")? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            lua.set_named_registry_value("selu:font-templates", t.clone())?;
            t
        }
    };
    if let Some(t) = registry.raw_get::<Option<Table>>(name)? {
        return Ok(t);
    }
    let font = create_font(lua, name)?;
    registry.raw_set(name, font.clone())?;
    Ok(font)
}
/// Visual profiles from Blizzard_Fonts_Shared/Shared/FontStyles.xml and
/// Fonts.xml (Roman GameFont Normal/Highlight shadow faces). These describe
/// native templates; they do not introduce compiler declarations or globals.
pub(crate) fn native_style(name: &str) -> Option<(f64, [f64; 4])> {
    let (size, highlight) = match name {
        "GameFontNormal" => (12.0, false),
        "GameFontNormalSmall" => (10.0, false),
        "GameFontNormalLarge" => (16.0, false),
        "GameFontHighlight" => (12.0, true),
        "GameFontHighlightSmall" => (10.0, true),
        "GameFontHighlightLarge" => (16.0, true),
        _ => return None,
    };
    Some((
        size,
        if highlight {
            [1.0; 4]
        } else {
            [1.0, 0.82, 0.0, 1.0]
        },
    ))
}
/// Called before addon chunks, including on reload and load-on-demand.
pub fn register_addon_root(lua: &mlua::Lua, dir: &Path) -> mlua::Result<()> {
    let root = dir.canonicalize().map_err(mlua::Error::external)?;
    let store = crate::frames::arena::store(lua);
    let mut arena = store.borrow_mut();
    arena.fonts.assets.add_addon(root);
    arena.fonts.faces.borrow_mut().clear();
    arena.fonts.layouts.borrow_mut().clear();
    arena.dirty = true;
    arena.full_layout = true;
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

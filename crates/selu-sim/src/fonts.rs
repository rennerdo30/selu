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
    roots: Vec<PathBuf>,
    faces: RefCell<HashMap<String, FontArc>>,
}
impl FontBook {
    pub fn face(&self, name: &str) -> FontArc {
        if let Some(face) = self.faces.borrow().get(name) {
            return face.clone();
        }
        let face = resolve(&self.roots, name)
            .ok()
            .and_then(|p| {
                let file = std::fs::File::open(p).ok()?;
                if file.metadata().ok()?.len() > 8 * 1024 * 1024 {
                    return None;
                }
                use std::io::Read;
                let mut bytes = Vec::new();
                file.take(8 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .ok()?;
                if bytes.len() > 8 * 1024 * 1024 {
                    return None;
                }
                FontArc::try_from_vec(bytes).ok()
            })
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
        crate::text::layout(&self.face(&style.font), text, style, width, height)
    }
}
pub fn fallback() -> FontArc {
    static FACE: std::sync::OnceLock<FontArc> = std::sync::OnceLock::new();
    FACE.get_or_init(|| {
        FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).expect("bundled font")
    })
    .clone()
}
/// Called before addon chunks, including on reload and load-on-demand.
pub fn register_addon_root(lua: &mlua::Lua, dir: &Path) -> mlua::Result<()> {
    let root = dir.canonicalize().map_err(mlua::Error::external)?;
    let store = crate::frames::arena::store(lua);
    let mut arena = store.borrow_mut();
    if !arena.fonts.roots.contains(&root) {
        arena.fonts.roots.push(root);
        arena.fonts.faces.borrow_mut().clear();
        arena.dirty = true;
    }
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

//! Rust-owned region storage. Lua tables contain only identity, scripts and addon fields.
use crate::core::{DrawLayer, Rect, SourcePos, Strata, TextureFill};
use mlua::Table;
use std::{cell::RefCell, rc::Rc};
pub type Store = Rc<RefCell<Arena>>;
#[derive(Clone)]
pub struct Point {
    pub point: String,
    pub relative: usize,
    pub anchor: String,
    pub x: f64,
    pub y: f64,
}
#[derive(Clone)]
pub struct Region {
    pub table: Table,
    pub kind: String,
    pub name: Option<String>,
    pub parent: Option<usize>,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub alpha: f64,
    pub shown: bool,
    pub strata: Strata,
    pub level: i32,
    pub layer: (DrawLayer, i8),
    pub points: Vec<Point>,
    pub rect: Option<Rect>,
    pub visible: bool,
    pub effective_scale: f64,
    pub effective_alpha: f64,
    pub text: Option<String>,
    pub texture: Option<TextureFill>,
    pub font_size: f64,
    pub font_path: String,
    pub font_flags: String,
    pub text_value: mlua::Value,
    pub strata_explicit: bool,
    pub level_explicit: bool,
    pub mouse: bool,
    pub enabled: bool,
    pub moving: bool,
    pub checked: bool,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub created_at: Option<SourcePos>,
}
#[derive(Default)]
pub struct Arena {
    pub regions: Vec<Region>,
    pub dirty: bool,
    pub cursor: (f64, f64),
    pub hover: Option<usize>,
    pub focus: Option<usize>,
    pub capture: Option<usize>,
    pub moving: Option<usize>,
}
pub fn store(lua: &mlua::Lua) -> Store {
    lua.app_data_ref::<Store>()
        .expect("frame arena installed")
        .clone()
}
pub fn id(t: &Table) -> mlua::Result<usize> {
    t.raw_get(identity_key())
}

static IDENTITY_KEY: u8 = 0;
pub fn identity_key() -> mlua::LightUserData {
    mlua::LightUserData((&IDENTITY_KEY as *const u8).cast_mut().cast())
}

pub fn is_region(kind: &str) -> bool {
    matches!(kind, "Texture" | "FontString" | "MaskTexture")
}
pub fn focus(a: &mut Arena, ctx: &crate::Ctx, target: Option<usize>) {
    let target = target.filter(|&i| a.regions[i].kind == "EditBox" && a.regions[i].visible);
    if a.focus == target {
        return;
    }
    if let Some(i) = a.focus.take() {
        super::widgets::callback(ctx, a.regions[i].table.clone(), "OnEditFocusLost", vec![]);
    }
    a.focus = target;
    if let Some(i) = target {
        super::widgets::callback(ctx, a.regions[i].table.clone(), "OnEditFocusGained", vec![]);
    }
}

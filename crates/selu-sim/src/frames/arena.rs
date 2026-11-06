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
    pub id: i32,
    pub layer: (DrawLayer, i8),
    pub points: Vec<Point>,
    pub rect: Option<Rect>,
    pub visible: bool,
    pub effective_scale: f64,
    pub effective_alpha: f64,
    pub text: Option<String>,
    pub texture: Option<TextureFill>,
    pub atlas_uv: Option<[f32; 4]>,
    pub visual: crate::core::VisualStyle,
    pub font_overrides: u8,
    pub clips_children: bool,
    pub scroll_child: Option<usize>,
    pub scroll: (f64, f64),
    pub animation_offset: (f64, f64),
    pub font_size: f64,
    pub multi_line: bool,
    pub font_path: String,
    pub font_flags: String,
    pub text_value: mlua::Value,
    pub strata_explicit: bool,
    pub level_explicit: bool,
    pub mouse: bool,
    pub enabled: bool,
    pub keyboard: bool,
    pub propagate_keyboard: bool,
    pub wheel: bool,
    pub thumb: Option<usize>,
    pub step: f64,
    pub obey_step: bool,
    pub orientation: String,
    pub movable: bool,
    pub clamped: bool,
    pub drag_buttons: Vec<String>,
    pub motion_disabled: bool,
    pub auto_focus: bool,
    pub cursor_position: usize,
    pub selection: (usize, usize),
    pub moving: bool,
    pub checked: bool,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub created_at: Option<SourcePos>,
}
#[derive(Default)]
pub struct Arena {
    pub fonts: crate::fonts::FontBook,
    pub regions: Vec<Region>,
    pub dirty: bool,
    pub full_layout: bool,
    pub styles_dirty: bool,
    pub dirty_regions: std::collections::HashSet<usize>,
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

impl Region {
    pub fn text_style(&self) -> crate::core::VisualStyle {
        crate::core::VisualStyle {
            font: self.font_path.clone(),
            size: self.font_size,
            flags: self.font_flags.clone(),
            ..self.visual.clone()
        }
    }
    /// Display placeholders never stringify or expose opaque secret payloads.
    pub fn display_text(&self) -> Option<&str> {
        if matches!(self.text_value, mlua::Value::LightUserData(_)) {
            Some("<secret>")
        } else {
            self.text.as_deref()
        }
    }
}

/// Intersection of every clipping ancestor, in screen coordinates.
pub fn clip(a: &Arena, i: usize) -> Option<crate::Rect> {
    let mut parent = a.regions[i].parent;
    let mut result: Option<crate::Rect> = None;
    for _ in 0..a.regions.len() {
        let Some(p) = parent else { break };
        let r = &a.regions[p];
        if (r.clips_children || r.kind == "ScrollFrame")
            && let Some(q) = r.rect
        {
            result = Some(match result {
                None => q,
                Some(c) => {
                    let left = c.left.max(q.left);
                    let bottom = c.bottom.max(q.bottom);
                    crate::Rect {
                        left,
                        bottom,
                        width: ((c.left + c.width).min(q.left + q.width) - left).max(0.0),
                        height: ((c.bottom + c.height).min(q.bottom + q.height) - bottom).max(0.0),
                    }
                }
            });
        }
        parent = r.parent;
    }
    result
}

pub fn animation_offset(a: &Arena, index: usize) -> (f64, f64) {
    let mut current = Some(index);
    let mut offset = (0.0, 0.0);
    for _ in 0..a.regions.len() {
        let Some(i) = current else { break };
        let r = &a.regions[i];
        offset.0 += r.animation_offset.0 * r.effective_scale;
        offset.1 += r.animation_offset.1 * r.effective_scale;
        current = r.parent;
    }
    offset
}

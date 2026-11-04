//! Frames, textures, font strings and layout. Owner: package S1 (round 8,
//! docs/SIMULATOR.md decisions 3, 4, 15, 16 and 19). S1 adds `arena.rs`,
//! `layout.rs`, `widgets.rs` and `input.rs` here and replaces the Lua-table
//! region model (`../regions.rs` and `CreateFrame` in `../lib.rs`).
//!
//! Groundwork: [`LuaTableModel`] implements the core's [`RegionModel`] over
//! today's Lua tables, so snapshots and Selu Studio work against the final
//! interface now. It covers frames from `CreateFrame` (plus `UIParent`),
//! textures and font strings, parents, size, points with a basic layout
//! (one point plus size, or edges from several points, `SetAllPoints`),
//! colour and file textures, text, alpha and shown/visible. No scale,
//! strata, levels or text wrapping; font strings without a size get a rough
//! width of half the font size (12) per character. S1 replaces all of this.

use crate::core::{Ctx, Rect, RegionId, RegionModel, RegionSnapshot, TextureFill};
use mlua::{Lua, Table, Value};
use std::collections::HashMap;

/// The round-8 region model: regions are the Lua tables in
/// `__selu_sim_frames` and `__selu_sim_regions` (plus `UIParent` as index 0).
pub struct LuaTableModel {
    /// Copied into every [`RegionId`] so ids from an older VM don't match.
    pub generation: u32,
    /// UIParent's size.
    pub ui_size: (f64, f64),
}

impl LuaTableModel {
    fn regions(&self, lua: &Lua) -> mlua::Result<Vec<Table>> {
        let mut out = Vec::new();
        if let Value::Table(ui) = lua.globals().raw_get::<Value>("UIParent")? {
            out.push(ui);
        }
        for registry in ["__selu_sim_frames", "__selu_sim_regions"] {
            if let Value::Table(list) = lua.globals().raw_get::<Value>(registry)? {
                for region in list.sequence_values::<Table>() {
                    out.push(region?);
                }
            }
        }
        Ok(out)
    }
}

/// One anchor, normalised from the forms `SetPoint` accepts.
struct Point {
    point: String,
    /// Region index of the relative frame; `None` means the parent.
    relative: Option<usize>,
    relative_point: String,
    x: f64,
    y: f64,
}

struct Raw {
    parent: Option<usize>,
    size: (Option<f64>, Option<f64>),
    points: Vec<Point>,
    text_width: Option<f64>,
}

const FONT_SIZE: f64 = 12.0;

/// `(horizontal, vertical)` fractions of a point name: LEFT = 0, CENTER =
/// 0.5, RIGHT = 1; BOTTOM = 0, TOP = 1.
fn fractions(point: &str) -> Option<(f64, f64)> {
    let p = point.to_ascii_uppercase();
    let h = if p.contains("LEFT") {
        0.0
    } else if p.contains("RIGHT") {
        1.0
    } else {
        0.5
    };
    let v = if p.contains("TOP") {
        1.0
    } else if p.contains("BOTTOM") {
        0.0
    } else {
        0.5
    };
    let known = matches!(
        p.as_str(),
        "TOPLEFT"
            | "TOP"
            | "TOPRIGHT"
            | "LEFT"
            | "CENTER"
            | "RIGHT"
            | "BOTTOMLEFT"
            | "BOTTOM"
            | "BOTTOMRIGHT"
    );
    known.then_some((h, v))
}

struct Layout<'a> {
    raw: &'a [Raw],
    ui_size: (f64, f64),
    rects: Vec<Option<Option<Rect>>>,
    visiting: Vec<bool>,
}

impl Layout<'_> {
    fn rect(&mut self, i: usize) -> Option<Rect> {
        if let Some(done) = self.rects[i] {
            return done;
        }
        if self.visiting[i] {
            return None; // anchor cycle: no rectangle, as in the client
        }
        self.visiting[i] = true;
        let r = self.compute(i);
        self.visiting[i] = false;
        self.rects[i] = Some(r);
        r
    }

    fn compute(&mut self, i: usize) -> Option<Rect> {
        if i == 0 {
            return Some(Rect {
                left: 0.0,
                bottom: 0.0,
                width: self.ui_size.0,
                height: self.ui_size.1,
            });
        }
        let raw = &self.raw[i];
        if raw.points.is_empty() {
            return None;
        }
        let width = raw.size.0.or(raw.text_width);
        let height = raw.size.1.or(raw.text_width.map(|_| FONT_SIZE));
        // Edge constraints from every point.
        let (mut left, mut right, mut cx) = (None, None, None);
        let (mut bottom, mut top, mut cy) = (None, None, None);
        for p in &raw.points {
            let (h, v) = fractions(&p.point)?;
            let (rh, rv) = fractions(&p.relative_point)?;
            let rel = p.relative.or(raw.parent).unwrap_or(0);
            let r = self.rect(rel)?;
            let tx = r.left + rh * r.width + p.x;
            let ty = r.bottom + rv * r.height + p.y;
            if h < 0.25 {
                left = Some(tx);
            } else if h > 0.75 {
                right = Some(tx);
            } else {
                cx = Some(tx);
            }
            if v < 0.25 {
                bottom = Some(ty);
            } else if v > 0.75 {
                top = Some(ty);
            } else {
                cy = Some(ty);
            }
        }
        let span = |lo: Option<f64>, hi: Option<f64>, mid: Option<f64>, size: Option<f64>| {
            let size = size.unwrap_or(0.0);
            match (lo, hi, mid) {
                (Some(lo), Some(hi), _) => Some((lo, hi - lo)),
                (Some(lo), None, _) => Some((lo, size)),
                (None, Some(hi), _) => Some((hi - size, size)),
                (None, None, Some(mid)) => Some((mid - size / 2.0, size)),
                (None, None, None) => None,
            }
        };
        let (l, w) = span(left, right, cx, width)?;
        let (b, h) = span(bottom, top, cy, height)?;
        Some(Rect {
            left: l,
            bottom: b,
            width: w,
            height: h,
        })
    }
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Number(n) => Some(*n),
        _ => None,
    }
}

impl RegionModel for LuaTableModel {
    fn resolve_layout(&self, _lua: &Lua, _ctx: &Ctx) -> mlua::Result<()> {
        // Resolved on demand in `snapshot_regions`; S1 caches with dirty flags.
        Ok(())
    }

    fn snapshot_regions(&self, lua: &Lua, ctx: &Ctx) -> mlua::Result<Vec<RegionSnapshot>> {
        self.resolve_layout(lua, ctx)?;
        let tables = self.regions(lua)?;
        let index: HashMap<usize, usize> = tables
            .iter()
            .enumerate()
            .map(|(i, t)| (t.to_pointer() as usize, i))
            .collect();
        let find = |v: &Value| -> Option<usize> {
            match v {
                Value::Table(t) => index.get(&(t.to_pointer() as usize)).copied(),
                Value::String(s) => {
                    match lua.globals().raw_get::<Value>(s.to_str().ok()?.to_string()) {
                        Ok(Value::Table(t)) => index.get(&(t.to_pointer() as usize)).copied(),
                        _ => None,
                    }
                }
                _ => None,
            }
        };
        let mut raw = Vec::with_capacity(tables.len());
        let mut out = Vec::with_capacity(tables.len());
        for (i, t) in tables.iter().enumerate() {
            let parent = match t.raw_get::<Value>("__parent")? {
                Value::Nil => None,
                v => find(&v),
            };
            let width = t.raw_get::<Option<f64>>("__width")?;
            let height = t.raw_get::<Option<f64>>("__height")?;
            let mut points = Vec::new();
            if let Value::Table(list) = t.raw_get::<Value>("__points")? {
                for p in list.sequence_values::<Table>() {
                    let p = p?;
                    let point: Option<String> = p.raw_get(1)?;
                    let Some(point) = point else { continue };
                    let (rel, rel_point, x, y): (Value, Value, Value, Value) =
                        (p.raw_get(2)?, p.raw_get(3)?, p.raw_get(4)?, p.raw_get(5)?);
                    // SetPoint(point, x, y)
                    let (relative, relative_point, x, y) = if let Some(x0) = number(&rel) {
                        (None, point.clone(), x0, number(&rel_point).unwrap_or(0.0))
                    } else {
                        let rp = match &rel_point {
                            Value::String(s) => s.to_str()?.to_string(),
                            _ => point.clone(),
                        };
                        (
                            find(&rel),
                            rp,
                            number(&x).unwrap_or(0.0),
                            number(&y).unwrap_or(0.0),
                        )
                    };
                    points.push(Point {
                        point,
                        relative,
                        relative_point,
                        x,
                        y,
                    });
                }
            }
            let widget = t
                .raw_get::<Option<String>>("__kind")?
                .unwrap_or_else(|| "Frame".into());
            // Secret values (round 7) are opaque: show a marker, never the value.
            let text = match t.raw_get::<Value>("__text")? {
                Value::String(s) => Some(s.to_str()?.to_string()),
                Value::Nil => None,
                _ => Some("<secret>".to_string()),
            };
            let text_width = (widget == "FontString")
                .then(|| {
                    text.as_ref()
                        .map(|s| s.chars().count() as f64 * FONT_SIZE * 0.5)
                })
                .flatten();
            let mut scripts: Vec<String> = Vec::new();
            if let Value::Table(s) = t.raw_get::<Value>("__scripts")? {
                for pair in s.pairs::<String, Value>() {
                    scripts.push(pair?.0);
                }
            }
            scripts.sort();
            let texture = if let Value::Table(c) = t.raw_get::<Value>("__color")? {
                let c: Vec<f32> = (1..=4)
                    .map(|k| c.raw_get::<Option<f64>>(k).ok().flatten().unwrap_or(1.0) as f32)
                    .collect();
                Some(TextureFill::Color([c[0], c[1], c[2], c[3]]))
            } else {
                match t.raw_get::<Value>("__texture")? {
                    Value::String(s) => Some(TextureFill::File(s.to_str()?.to_string())),
                    v => number(&v).map(|n| TextureFill::File(n.to_string())),
                }
            };
            out.push(RegionSnapshot {
                id: RegionId {
                    index: i as u32,
                    generation: self.generation,
                },
                parent: parent.map(|p| RegionId {
                    index: p as u32,
                    generation: self.generation,
                }),
                widget,
                name: t.raw_get::<Option<String>>("__name")?,
                rect: None,
                size: width.zip(height),
                shown: t.raw_get::<Option<bool>>("__shown")?.unwrap_or(true),
                visible: false,
                strata: None,
                level: None,
                layer: None,
                alpha: t.raw_get::<Option<f64>>("__alpha")?.unwrap_or(1.0),
                texture,
                text,
                scripts,
                created_at: None,
            });
            raw.push(Raw {
                parent,
                size: (width, height),
                points,
                text_width,
            });
        }
        let mut layout = Layout {
            raw: &raw,
            ui_size: self.ui_size,
            rects: vec![None; raw.len()],
            visiting: vec![false; raw.len()],
        };
        for (i, region) in out.iter_mut().enumerate() {
            region.rect = layout.rect(i);
        }
        // Visible = shown and every ancestor visible; effective alpha
        // multiplies down. Parents come first.
        for i in 0..out.len() {
            let (parent_visible, parent_alpha) = match out[i].parent {
                Some(p) if (p.index as usize) < i => {
                    let p = &out[p.index as usize];
                    (p.visible, p.alpha)
                }
                _ => (true, 1.0),
            };
            out[i].visible = out[i].shown && parent_visible;
            out[i].alpha *= parent_alpha;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use crate::Sim;

    #[test]
    fn snapshot_lists_uiparent_and_created_frames() {
        let sim = Sim::new().unwrap();
        sim.eval(
            r#"
            local f = CreateFrame("Frame", nil, UIParent)
            f:SetSize(100, 20)
            f:SetScript("OnEvent", function() end)
            local child = CreateFrame("Button", nil, f)
            f:Hide()
            "#,
        )
        .unwrap();
        let snap = sim.snapshot().unwrap();
        let widgets: Vec<_> = snap.regions.iter().map(|r| r.widget.as_str()).collect();
        assert_eq!(widgets, ["Frame", "Frame", "Button"]);
        let (ui, frame, button) = (&snap.regions[0], &snap.regions[1], &snap.regions[2]);
        assert!(ui.visible);
        assert_eq!(frame.parent, Some(ui.id));
        assert_eq!(frame.size, Some((100.0, 20.0)));
        assert_eq!(frame.scripts, ["OnEvent"]);
        assert!(!frame.shown && !frame.visible);
        assert_eq!(button.parent, Some(frame.id));
        assert!(button.shown && !button.visible, "hidden parent hides child");
    }

    #[test]
    fn basic_layout_from_points_and_size() {
        let sim = Sim::new().unwrap();
        sim.eval(
            r#"
            local f = CreateFrame("Frame", nil, UIParent)
            f:SetSize(200, 100)
            f:SetPoint("CENTER", UIParent, "CENTER", 10, -20)
            local t = f:CreateTexture(nil, "BACKGROUND")
            t:SetAllPoints()
            t:SetColorTexture(1, 0, 0, 0.5)
            local fs = f:CreateFontString()
            fs:SetPoint("TOPLEFT", 4, -4)
            fs:SetText("abcd")
            local inset = CreateFrame("Frame", nil, f)
            inset:SetPoint("TOPLEFT", f, "TOPLEFT", 5, -5)
            inset:SetPoint("BOTTOMRIGHT", f, "BOTTOMRIGHT", -5, 5)
            "#,
        )
        .unwrap();
        let snap = sim.snapshot().unwrap();
        let rect = |i: usize| {
            snap.regions[i]
                .rect
                .map(|r| (r.left, r.bottom, r.width, r.height))
        };
        // UIParent 1920 x 1080; centre (960, 540) + (10, -20), 200 x 100.
        assert_eq!(rect(1), Some((870.0, 470.0, 200.0, 100.0)));
        // Inset frame: 5 px inside on every side.
        assert_eq!(rect(2), Some((875.0, 475.0, 190.0, 90.0)));
        // Texture fills the frame.
        assert_eq!(rect(3), Some((870.0, 470.0, 200.0, 100.0)));
        assert_eq!(
            snap.regions[3].texture,
            Some(crate::TextureFill::Color([1.0, 0.0, 0.0, 0.5]))
        );
        // Font string: top left at (874, 566), 4 chars x 6 px wide, 12 tall.
        assert_eq!(rect(4), Some((874.0, 554.0, 24.0, 12.0)));
    }

    #[test]
    fn secret_text_is_shown_as_a_marker() {
        let sim = Sim::new().unwrap();
        sim.set_secret_global("SECRET").unwrap();
        sim.eval(
            "local f = CreateFrame('Frame'); local t = f:CreateFontString(); t:SetText(SECRET)",
        )
        .unwrap();
        let snap = sim.snapshot().unwrap();
        assert!(
            snap.regions
                .iter()
                .any(|r| r.text.as_deref() == Some("<secret>")),
            "{:?}",
            snap.regions
        );
    }

    #[test]
    fn anchor_cycles_get_no_rectangle() {
        let sim = Sim::new().unwrap();
        sim.eval(
            r#"
            local a = CreateFrame("Frame"); local b = CreateFrame("Frame")
            a:SetSize(10, 10); b:SetSize(10, 10)
            a:SetPoint("LEFT", b, "RIGHT"); b:SetPoint("LEFT", a, "RIGHT")
            "#,
        )
        .unwrap();
        let snap = sim.snapshot().unwrap();
        assert_eq!(snap.regions[1].rect, None);
        assert_eq!(snap.regions[2].rect, None);
    }
}

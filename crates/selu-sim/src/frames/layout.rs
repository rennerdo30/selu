//! Anchor constraints in UIParent coordinates; no Lua calls during resolution.
use super::arena::*;
use crate::core::{Ctx, Rect};
pub fn factor(p: &str) -> (f64, f64) {
    (
        if p.contains("LEFT") {
            0.0
        } else if p.contains("RIGHT") {
            1.0
        } else {
            0.5
        },
        if p.contains("BOTTOM") {
            0.0
        } else if p.contains("TOP") {
            1.0
        } else {
            0.5
        },
    )
}
pub fn resolve(a: &mut Arena, ctx: &Ctx) {
    if !a.dirty {
        return;
    }
    let before = a
        .regions
        .iter()
        .map(|r| (r.visible, r.rect, r.effective_scale))
        .collect::<Vec<_>>();
    let mut inherited = vec![0u8; a.regions.len()];
    for i in 0..a.regions.len() {
        inherit(i, a, &mut inherited, ctx);
    }
    let mut marks = vec![0u8; a.regions.len()];
    for i in 0..a.regions.len() {
        visit(i, a, &mut marks, ctx);
    }
    a.dirty = false;
    for (i, r) in a.regions.iter().enumerate() {
        let (visible, rect, old_scale) = before[i];
        if visible != r.visible {
            super::widgets::callback(
                ctx,
                r.table.clone(),
                if r.visible { "OnShow" } else { "OnHide" },
                vec![],
            );
        }
        let size = r
            .rect
            .map(|q| (q.width / r.effective_scale, q.height / r.effective_scale));
        if rect.map(|q| (q.width / old_scale, q.height / old_scale)) != size
            && let Some((w, h)) = size
        {
            super::widgets::callback(
                ctx,
                r.table.clone(),
                "OnSizeChanged",
                vec![mlua::Value::Number(w), mlua::Value::Number(h)],
            );
        }
    }
    if a.focus.is_some_and(|i| !a.regions[i].visible) {
        focus(a, ctx, None);
    }
}
pub fn text_height(r: &Region, width: f64) -> f64 {
    r.text
        .as_deref()
        .unwrap_or("")
        .split('\n')
        .map(|line| {
            let m = crate::core::measure_text(line, r.font_size);
            r.font_size
                * if width > 0.0 {
                    (m.width / width).ceil().max(1.0)
                } else {
                    1.0
                }
        })
        .sum()
}
fn inherit(i: usize, a: &mut Arena, marks: &mut [u8], ctx: &Ctx) {
    if marks[i] == 2 {
        return;
    }
    if marks[i] == 1 {
        ctx.not_simulated("parent cycle");
        return;
    }
    marks[i] = 1;
    let r = a.regions[i].clone();
    let (mut scale, mut alpha, mut visible) = (r.scale, r.alpha, r.shown);
    if let Some(p) = r.parent {
        inherit(p, a, marks, ctx);
        let parent = &a.regions[p];
        scale *= parent.effective_scale;
        alpha *= parent.effective_alpha;
        visible &= parent.visible;
        let strata = parent.strata;
        let level = parent.level;
        if is_region(&r.kind) || !r.strata_explicit {
            a.regions[i].strata = strata;
        }
        if is_region(&r.kind) || !r.level_explicit {
            a.regions[i].level = level + if is_region(&r.kind) { 0 } else { 1 };
        }
    }
    a.regions[i].effective_scale = scale;
    a.regions[i].effective_alpha = alpha;
    a.regions[i].visible = visible;
    marks[i] = 2;
}
fn visit(i: usize, a: &mut Arena, marks: &mut [u8], ctx: &Ctx) -> bool {
    if marks[i] == 2 {
        return true;
    }
    if marks[i] == 1 {
        ctx.not_simulated("anchor cycle");
        return false;
    }
    marks[i] = 1;
    let r = a.regions[i].clone();
    let mut valid = true;
    let scale = r.effective_scale;
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for p in &r.points {
        if p.relative >= a.regions.len() {
            ctx.not_simulated("unresolvable anchor");
            valid = false;
            continue;
        }
        if !visit(p.relative, a, marks, ctx) {
            valid = false;
            continue;
        }
        if let Some(rect) = a.regions[p.relative].rect {
            let (rx, ry) = factor(&p.anchor);
            let (x, y) = factor(&p.point);
            xs.push((x, rect.left + rx * rect.width + p.x * scale));
            ys.push((y, rect.bottom + ry * rect.height + p.y * scale));
        } else {
            valid = false;
        }
    }
    fn axis(points: &[(f64, f64)], size: f64) -> Option<(f64, f64)> {
        let &(f, pos) = points.first()?;
        for &(g, q) in points.iter().skip(1) {
            if f != g {
                let s = (q - pos) / (g - f);
                return Some((pos - f * s, s));
            }
        }
        Some((pos - f * size, size))
    }
    let metric = crate::core::measure_text(r.text.as_deref().unwrap_or(""), r.font_size);
    let intrinsic = r.kind == "FontString";
    let width = if intrinsic && r.width == 0.0 {
        metric.width
    } else {
        r.width
    };
    let horizontal = axis(&xs, width * scale);
    let logical_width = horizontal.map(|(_, w)| w / scale).unwrap_or(width);
    let height = if intrinsic && r.height == 0.0 {
        text_height(&r, logical_width)
    } else {
        r.height
    };
    let rect = if i == 0 {
        Some(Rect {
            left: 0.0,
            bottom: 0.0,
            width: r.width,
            height: r.height,
        })
    } else if valid {
        horizontal
            .zip(axis(&ys, height * scale))
            .map(|((left, width), (bottom, height))| Rect {
                left,
                bottom,
                width,
                height,
            })
    } else {
        None
    };
    let out = &mut a.regions[i];
    out.rect = rect;

    marks[i] = 2;
    valid
}

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
        .map(|r| (r.visible, r.rect))
        .collect::<Vec<_>>();
    let mut marks = vec![0u8; a.regions.len()];
    for i in 0..a.regions.len() {
        visit(i, a, &mut marks, ctx);
    }
    a.dirty = false;
    for (i, r) in a.regions.iter().enumerate() {
        let (visible, rect) = before[i];
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
        if rect.map(|q| (q.width / r.effective_scale, q.height / r.effective_scale)) != size
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
    let mut scale = r.scale;
    let mut alpha = r.alpha;
    let mut visible = r.shown;
    if let Some(p) = r.parent {
        valid = visit(p, a, marks, ctx);
        scale *= a.regions[p].effective_scale;
        alpha *= a.regions[p].effective_alpha;
        visible &= a.regions[p].visible;
    }
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
    let rect = if i == 0 {
        Some(Rect {
            left: 0.0,
            bottom: 0.0,
            width: r.width,
            height: r.height,
        })
    } else if valid {
        axis(&xs, r.width * scale)
            .zip(axis(&ys, r.height * scale))
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
    out.visible = visible && valid;
    out.effective_scale = scale;
    out.effective_alpha = alpha;
    marks[i] = 2;
    valid
}

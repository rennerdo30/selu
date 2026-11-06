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
    if !a.dirty || a.layout_batches > 0 {
        return;
    }
    let _profile = crate::profile::Span::new("layout.resolve");
    let full = a.full_layout || (a.dirty_regions.is_empty() && !a.styles_dirty);
    let previous_count = a.layout_dependencies.len();
    a.layout_dependencies.resize_with(a.regions.len(), Vec::new);
    a.layout_dependents
        .resize_with(a.regions.len(), Default::default);
    let mut structural = if a.full_layout {
        (0..a.regions.len()).collect::<Vec<_>>()
    } else {
        a.dirty_regions
            .iter()
            .copied()
            .chain(previous_count..a.regions.len())
            .collect()
    };
    structural.sort_unstable();
    structural.dedup();
    for index in structural {
        let region = &a.regions[index];
        let mut dependencies = region
            .parent
            .into_iter()
            .chain(region.points.iter().map(|point| point.relative))
            .filter(|relative| *relative < a.regions.len())
            .collect::<Vec<_>>();
        dependencies.sort_unstable();
        dependencies.dedup();
        if a.layout_dependencies[index] != dependencies {
            for relative in &a.layout_dependencies[index] {
                a.layout_dependents[*relative].remove(&index);
            }
            for relative in &dependencies {
                a.layout_dependents[*relative].insert(index);
            }
            a.layout_dependencies[index] = dependencies;
        }
    }
    let style_regions = if a.full_layout || a.styles_dirty || a.dirty_regions.is_empty() {
        (0..a.regions.len()).collect::<Vec<_>>()
    } else {
        a.dirty_regions.iter().copied().collect()
    };
    for index in style_regions {
        let r = &mut a.regions[index];
        let mut object = r
            .table
            .raw_get::<Option<mlua::Table>>("__font_object")
            .ok()
            .flatten();
        if object.is_none() {
            continue;
        }
        let original_style = r.visual.clone();
        let mut style_read = false;
        for _ in 0..64 {
            let Some(t) = object else { break };
            if !style_read {
                for (key, bit) in [
                    ("TextColor", 1),
                    ("ShadowColor", 2),
                    ("ShadowOffset", 4),
                    ("JustifyH", 8),
                    ("JustifyV", 16),
                    ("Spacing", 32),
                ] {
                    if r.font_overrides & bit != 0 {
                        continue;
                    }
                    let mut current = Some(t.clone());
                    for _ in 0..64 {
                        let Some(object) = current else { break };
                        let own = object
                            .raw_get::<Option<mlua::Table>>(format!("_{key}"))
                            .ok()
                            .flatten();
                        let defaults = if object
                            .raw_get::<Option<mlua::Table>>("_parent")
                            .ok()
                            .flatten()
                            .is_none()
                        {
                            object
                                .raw_get::<mlua::Table>("_defaults")
                                .ok()
                                .and_then(|t| t.raw_get::<Option<mlua::Table>>(key).ok().flatten())
                        } else {
                            None
                        };
                        if let Some(value) = own.or(defaults) {
                            match key {
                                "TextColor" | "ShadowColor" => {
                                    let c = [
                                        value.raw_get(1).unwrap_or(1.0),
                                        value.raw_get(2).unwrap_or(1.0),
                                        value.raw_get(3).unwrap_or(1.0),
                                        value.raw_get(4).unwrap_or(1.0),
                                    ];
                                    if key == "TextColor" {
                                        r.visual.color = c;
                                    } else {
                                        r.visual.shadow = c;
                                    }
                                }
                                "ShadowOffset" => {
                                    r.visual.shadow_offset = (
                                        value.raw_get(1).unwrap_or(0.0),
                                        value.raw_get(2).unwrap_or(0.0),
                                    )
                                }
                                "JustifyH" => {
                                    r.visual.justify_h =
                                        value.raw_get(1).unwrap_or_else(|_| "LEFT".into())
                                }
                                "Spacing" => r.visual.spacing = value.raw_get(1).unwrap_or(0.0),
                                _ => {
                                    r.visual.justify_v =
                                        value.raw_get(1).unwrap_or_else(|_| "TOP".into())
                                }
                            }
                            break;
                        }
                        current = object
                            .raw_get::<Option<mlua::Table>>("_parent")
                            .ok()
                            .flatten();
                    }
                }
                style_read = true;
            }
            if !t
                .raw_get::<Option<bool>>("_font_explicit")
                .ok()
                .flatten()
                .unwrap_or(false)
                && let Ok(Some(parent)) = t.raw_get::<Option<mlua::Table>>("_parent")
            {
                object = Some(parent);
                continue;
            }
            if r.font_overrides & 64 == 0
                && let Ok(font) = t.raw_get::<mlua::Table>("_font")
            {
                let path = font.raw_get::<String>(1).unwrap_or_default();
                let size = font.raw_get::<f64>(2).unwrap_or(12.0);
                let flags = font.raw_get::<String>(3).unwrap_or_default();
                if (r.font_path.as_str(), r.font_size, r.font_flags.as_str())
                    != (path.as_str(), size, flags.as_str())
                {
                    r.font_path = path;
                    r.font_size = size;
                    r.font_flags = flags;
                    a.dirty_regions.insert(index);
                    a.dirty = true;
                }
            }
            break;
        }
        if r.visual != original_style {
            a.dirty_regions.insert(index);
            a.dirty = true;
        }
    }
    a.styles_dirty = false;
    // Seed the dependency closure from mutations; external bulk invalidations
    // (root resize, input, font roots) retain conservative full resolution.
    if !full && a.dirty_regions.is_empty() {
        a.dirty = false;
        return;
    }
    a.layout_epoch = a.layout_epoch.wrapping_add(1);
    if a.layout_epoch == 0 {
        a.layout_seen.fill(0);
        a.layout_epoch = 1;
    }
    let epoch = a.layout_epoch;
    a.layout_seen.resize(a.regions.len(), 0);
    crate::profile::count(if full {
        "layout.full"
    } else {
        "layout.partial"
    });
    let mut active = if full {
        (0..a.regions.len()).collect::<Vec<_>>()
    } else {
        a.dirty_regions.iter().copied().collect()
    };
    a.dirty_regions.clear();
    for index in &active {
        a.layout_seen[*index] = epoch;
    }
    let mut cursor = 0;
    while cursor < active.len() {
        for dependent in &a.layout_dependents[active[cursor]] {
            if a.layout_seen[*dependent] != epoch {
                a.layout_seen[*dependent] = epoch;
                active.push(*dependent);
            }
        }
        cursor += 1;
    }
    active.sort_unstable();
    let before = active
        .iter()
        .map(|index| {
            let r = &a.regions[*index];
            (r.visible, r.rect, r.effective_scale)
        })
        .collect::<Vec<_>>();
    // Unchanged nodes stay resolved. Reset only the dependency closure rather
    // than scanning every region for each single-widget mutation.
    let mut inherited = std::mem::take(&mut a.layout_inherit_marks);
    let mut marks = std::mem::take(&mut a.layout_geometry_marks);
    inherited.resize(a.regions.len(), 2);
    marks.resize(a.regions.len(), 2);
    for &index in &active {
        inherited[index] = 0;
        marks[index] = 0;
    }
    for &i in &active {
        inherit(i, a, &mut inherited, ctx);
    }
    for &i in &active {
        visit(i, a, &mut marks, ctx);
    }
    for &index in &active {
        inherited[index] = 2;
        marks[index] = 2;
    }
    a.layout_inherit_marks = inherited;
    a.layout_geometry_marks = marks;
    for &i in &active {
        let r = &a.regions[i];
        if let (Some(index), Some(mut rect)) = (r.status_texture, r.rect) {
            let fraction = if r.max > r.min {
                ((r.value - r.min) / (r.max - r.min)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            if r.orientation == "VERTICAL" {
                rect.height *= fraction;
            } else {
                rect.width *= fraction;
            }
            a.regions[index].rect = Some(rect);
        }
        let r = &a.regions[i];
        if let (Some(index), Some(q)) = (r.thumb, r.rect) {
            let fraction = if r.max > r.min {
                (r.value - r.min) / (r.max - r.min)
            } else {
                0.0
            };
            let horizontal = r.orientation == "HORIZONTAL";
            let scale = r.effective_scale;
            let thumb = &mut a.regions[index];
            let width = thumb.width * scale;
            let height = thumb.height * scale;
            thumb.rect = Some(Rect {
                left: q.left
                    + if horizontal {
                        fraction * (q.width - width)
                    } else {
                        (q.width - width) / 2.0
                    },
                bottom: q.bottom
                    + if horizontal {
                        (q.height - height) / 2.0
                    } else {
                        (1.0 - fraction) * (q.height - height)
                    },
                width,
                height,
            });
        }
    }
    a.dirty = false;
    a.full_layout = false;
    for (&i, &(visible, rect, old_scale)) in active.iter().zip(&before) {
        let r = &a.regions[i];
        if visible != r.visible {
            super::widgets::callback(
                ctx,
                r.table.get(),
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
                r.table.get(),
                "OnSizeChanged",
                vec![mlua::Value::Number(w), mlua::Value::Number(h)],
            );
        }
    }
    let auto_focus = active
        .iter()
        .zip(&before)
        .rev()
        .find(|(i, before)| {
            let r = &a.regions[**i];
            r.kind == "EditBox" && r.auto_focus && r.visible && !before.0
        })
        .map(|(i, _)| *i);
    if let Some(i) = auto_focus {
        focus(a, ctx, Some(i));
    }
    if a.focus.is_some_and(|i| !a.regions[i].visible) {
        focus(a, ctx, None);
    }
}
pub fn text_height(r: &Region, width: f64, fonts: &crate::fonts::FontBook) -> f64 {
    fonts
        .layout(
            r.display_text().unwrap_or(""),
            &r.text_style(),
            Some(width),
            None,
        )
        .height
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
    let r = &a.regions[i];
    let (mut scale, mut alpha, mut visible) = (r.scale, r.alpha, r.shown);
    let (parent, region, strata_explicit, level_explicit) = (
        r.parent,
        is_region(&r.kind),
        r.strata_explicit,
        r.level_explicit,
    );
    if let Some(p) = parent {
        inherit(p, a, marks, ctx);
        let parent = &a.regions[p];
        scale *= parent.effective_scale;
        alpha *= parent.effective_alpha;
        visible &= parent.visible;
        let strata = parent.strata;
        let level = parent.level;
        if region || !strata_explicit {
            a.regions[i].strata = strata;
        }
        if region || !level_explicit {
            a.regions[i].level = level + if region { 0 } else { 1 };
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
    let points = a.regions[i].points.clone();
    let mut valid = true;
    let scale = a.regions[i].effective_scale;
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for p in &points {
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
    let r = &a.regions[i];
    let parent = r.parent;
    let intrinsic = r.kind == "FontString" || (r.kind == "EditBox" && r.multi_line);
    let width = if intrinsic && r.width == 0.0 {
        a.fonts
            .layout(r.display_text().unwrap_or(""), &r.text_style(), None, None)
            .width
    } else {
        r.width
    };
    let horizontal = axis(&xs, width * scale);
    let logical_width = horizontal.map(|(_, w)| w / scale).unwrap_or(width);
    let height = if intrinsic && r.height == 0.0 {
        text_height(
            r,
            (logical_width - r.visual.text_insets[0] - r.visual.text_insets[1]).max(0.0),
            &a.fonts,
        ) + r.visual.text_insets[2]
            + r.visual.text_insets[3]
    } else {
        r.height
    };
    let rect = if i == 0 {
        Some(Rect {
            left: 0.0,
            bottom: 0.0,
            width: r.width * scale,
            height: r.height * scale,
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
    if let Some(p) = parent
        && a.regions[p].scroll_child == Some(i)
    {
        let (x, y) = a.regions[p].scroll;
        if let Some(q) = &mut a.regions[i].rect {
            q.left -= x * scale;
            q.bottom += y * scale;
        }
    }

    marks[i] = 2;
    valid
}

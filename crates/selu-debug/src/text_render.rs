//! Draw the simulator's glyph masks at its exact shaped positions.
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use selu_sim::{RegionSnapshot, text::Content};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::PathBuf,
};
pub type GlyphCache = HashMap<u64, egui::TextureHandle>;
pub struct TextPainter<'a> {
    pub ctx: &'a egui::Context,
    pub roots: &'a [PathBuf],
    pub glyphs: &'a mut GlyphCache,
    pub textures: &'a mut HashMap<String, Result<egui::TextureHandle, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn painted_glyph_quads_are_the_simulator_positions() {
        let sim = selu_sim::Sim::new().unwrap();
        sim.eval("local t=UIParent:CreateFontString();t:SetFont('fallback',20);t:SetScale(2);t:SetText('AVATAR');t:SetPoint('TOPLEFT')").unwrap();
        let snapshot = sim.snapshot().unwrap();
        let region = snapshot.regions.last().unwrap();
        let layout = region.text_layout.as_ref().unwrap();
        let ctx = egui::Context::default();
        ctx.begin_pass(Default::default());
        let painter = ctx.layer_painter(egui::LayerId::background());
        let mut glyphs = GlyphCache::new();
        let mut textures = HashMap::new();
        let rect = Rect::from_min_size(Pos2::new(30.0, 40.0), Vec2::splat(500.0));
        TextPainter {
            ctx: &ctx,
            roots: &[],
            glyphs: &mut glyphs,
            textures: &mut textures,
        }
        .paint(&painter, region, rect, 0.75);
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
        let meshes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| {
                if let egui::Shape::Mesh(m) = &s.shape {
                    Some(m)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(meshes.len(), layout.placements.len());
        for (mesh, p) in meshes.iter().zip(&layout.placements) {
            let Content::Glyph(id) = p.content else {
                panic!("glyph")
            };
            let mask = layout.masks.iter().find(|m| m.id == id).unwrap();
            let top_left = rect.left_top() + Vec2::new(p.x + mask.left, p.y + mask.top) * 1.5;
            let bottom_right = top_left + Vec2::new(mask.width as f32, mask.height as f32) * 1.5;
            assert!(mesh.vertices.iter().any(|v| v.pos == top_left));
            assert!(mesh.vertices.iter().any(|v| v.pos == bottom_right));
        }
    }
}
impl TextPainter<'_> {
    pub fn paint(
        &mut self,
        painter: &egui::Painter,
        region: &RegionSnapshot,
        rect: Rect,
        canvas_scale: f32,
    ) {
        let Some(layout) = &region.text_layout else {
            return;
        };
        let scale = canvas_scale * (region.visual.size / layout.font_size) as f32;
        let opacity = region.alpha.clamp(0.0, 1.0) as f32 * if region.visible { 1.0 } else { 0.3 };
        let tint = |c: [f32; 4]| {
            Color32::from_rgba_unmultiplied(
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
                (c[3] * 255.0) as u8,
            )
            .gamma_multiply(opacity)
        };
        let mut masks = HashMap::new();
        for mask in &layout.masks {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            mask.width.hash(&mut hash);
            mask.height.hash(&mut hash);
            mask.alpha.hash(&mut hash);
            let key = hash.finish();
            let texture = self.glyphs.entry(key).or_insert_with(|| {
                let pixels = mask
                    .alpha
                    .iter()
                    .map(|a| Color32::from_white_alpha(*a))
                    .collect();
                self.ctx.load_texture(
                    format!("glyph:{key}"),
                    egui::ColorImage::new([mask.width, mask.height], pixels),
                    egui::TextureOptions::LINEAR,
                )
            });
            masks.insert(mask.id, (texture.id(), mask));
        }
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        for p in &layout.placements {
            let origin = rect.left_top() + Vec2::new(p.x, p.y) * scale;
            match &p.content {
                Content::Glyph(id) => {
                    let Some((texture, mask)) = masks.get(id) else {
                        continue;
                    };
                    let area = Rect::from_min_size(
                        origin + Vec2::new(mask.left, mask.top) * scale,
                        Vec2::new(mask.width as f32, mask.height as f32) * scale,
                    );
                    if region.visual.shadow[3] > 0.0 {
                        painter.image(
                            *texture,
                            area.translate(
                                Vec2::new(
                                    region.visual.shadow_offset.0 as f32,
                                    -region.visual.shadow_offset.1 as f32,
                                ) * scale,
                            ),
                            uv,
                            tint(region.visual.shadow),
                        );
                    }
                    if region.visual.flags.contains("OUTLINE") {
                        let d = if region.visual.flags.contains("THICKOUTLINE") {
                            2.0
                        } else {
                            1.0
                        };
                        for offset in [
                            Vec2::new(-d, 0.0),
                            Vec2::new(d, 0.0),
                            Vec2::new(0.0, -d),
                            Vec2::new(0.0, d),
                        ] {
                            painter.image(
                                *texture,
                                area.translate(offset * scale),
                                uv,
                                tint([0.0, 0.0, 0.0, p.color[3]]),
                            );
                        }
                    }
                    painter.image(*texture, area, uv, tint(p.color));
                }
                Content::Texture {
                    path,
                    width,
                    height,
                    uv,
                } => {
                    let area = Rect::from_min_size(origin, Vec2::new(*width, *height) * scale);
                    let texture = self.textures.entry(path.clone()).or_insert_with(|| {
                        crate::textures::load(self.roots, path).map(|image| {
                            self.ctx
                                .load_texture(path, image, egui::TextureOptions::LINEAR)
                        })
                    });
                    if let Ok(texture) = texture {
                        painter.image(
                            texture.id(),
                            area,
                            Rect::from_min_max(Pos2::new(uv[0], uv[2]), Pos2::new(uv[1], uv[3])),
                            tint(p.color),
                        );
                    } else {
                        painter.rect_filled(area, 0.0, tint([0.3, 0.1, 0.3, 1.0]));
                        painter
                            .with_clip_rect(area.intersect(painter.clip_rect()))
                            .text(
                                area.left_top(),
                                egui::Align2::LEFT_TOP,
                                "Unavailable texture",
                                egui::FontId::proportional(8.0),
                                tint([1.0; 4]),
                            );
                    }
                }
            }
        }
    }
}

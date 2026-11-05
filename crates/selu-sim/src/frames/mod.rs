//! Rust arena adapter, retaining the S0 model name for existing wiring.
pub mod arena;
pub mod input;
pub mod layout;
pub mod widgets;
use crate::core::{Ctx, RegionId, RegionModel, RegionSnapshot};
use mlua::Lua;
pub struct LuaTableModel {
    pub generation: u32,
    /// UIParent's size.
    pub ui_size: (f64, f64),
}
impl RegionModel for LuaTableModel {
    fn resolve_layout(&self, lua: &Lua, ctx: &Ctx) -> mlua::Result<()> {
        let store = arena::store(lua);
        let mut a = store.borrow_mut();
        let logical_size = (
            self.ui_size.0 / a.regions[0].scale,
            self.ui_size.1 / a.regions[0].scale,
        );
        if (a.regions[0].width, a.regions[0].height) != logical_size {
            a.regions[0].width = logical_size.0;
            a.regions[0].height = logical_size.1;
            a.dirty = true;
            a.full_layout = true;
            a.dirty_regions.clear();
        }
        layout::resolve(&mut a, ctx);
        Ok(())
    }
    fn snapshot_regions(&self, lua: &Lua, ctx: &Ctx) -> mlua::Result<Vec<RegionSnapshot>> {
        self.resolve_layout(lua, ctx)?;
        let a = arena::store(lua);
        let a = a.borrow();
        a.regions
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut scripts = r
                    .table
                    .raw_get::<mlua::Table>("__scripts")?
                    .pairs::<String, mlua::Value>()
                    .map(|p| p.map(|p| p.0))
                    .collect::<mlua::Result<Vec<_>>>()?;
                scripts.sort();
                Ok(RegionSnapshot {
                    id: RegionId {
                        index: i as u32,
                        generation: self.generation,
                    },
                    parent: r.parent.map(|p| RegionId {
                        index: p as u32,
                        generation: self.generation,
                    }),
                    widget: r.kind.clone(),
                    name: r.name.clone(),
                    rect: r.rect.map(|mut q| {
                        let (x, y) = arena::animation_offset(&a, i);
                        q.left += x;
                        q.bottom += y;
                        q
                    }),
                    size: Some((r.width, r.height)),
                    shown: r.shown,
                    visible: r.visible,
                    strata: Some(r.strata),
                    level: Some(r.level),
                    layer: Some(r.layer),
                    alpha: r.effective_alpha,
                    visual: crate::core::VisualStyle {
                        uv: if let Some(b) = r.atlas_uv {
                            let u = r.visual.uv;
                            [
                                b[0] + u[0] * (b[1] - b[0]),
                                b[0] + u[1] * (b[1] - b[0]),
                                b[2] + u[2] * (b[3] - b[2]),
                                b[2] + u[3] * (b[3] - b[2]),
                            ]
                        } else {
                            r.visual.uv
                        },
                        font: r.font_path.clone(),
                        size: r.font_size * r.effective_scale,
                        flags: r.font_flags.clone(),
                        text_scroll: if r.kind == "EditBox" {
                            r.scroll
                        } else {
                            (0.0, 0.0)
                        },
                        ..r.visual.clone()
                    },
                    clip: arena::clip(&a, i),
                    texture: r.texture.clone(),
                    text: r.display_text().map(str::to_owned),
                    text_layout: r.display_text().map(|text| {
                        let style = r.text_style();
                        let mut shaped = a.fonts.layout(
                            text,
                            &style,
                            r.rect.map(|q| {
                                (q.width / r.effective_scale
                                    - r.visual.text_insets[0]
                                    - r.visual.text_insets[1])
                                    .max(0.0)
                            }),
                            r.rect.filter(|_| r.kind != "EditBox").map(|q| {
                                (q.height / r.effective_scale
                                    - r.visual.text_insets[2]
                                    - r.visual.text_insets[3])
                                    .max(0.0)
                            }),
                        );
                        crate::text::rasterize(&a.fonts.face(&style.font), style.size, &mut shaped);
                        shaped
                    }),
                    scripts,
                    created_at: r.created_at.clone(),
                })
            })
            .collect()
    }
}
pub fn install(lua: &Lua, ctx: &Ctx) -> mlua::Result<()> {
    widgets::install(lua, ctx)
}

mod manifest;

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
        let parent = snap
            .regions
            .iter()
            .find(|r| r.widget == "Frame" && r.parent == Some(snap.regions[0].id))
            .unwrap();
        let inset = snap
            .regions
            .iter()
            .find(|r| r.widget == "Frame" && r.parent == Some(parent.id))
            .unwrap();
        let texture = snap
            .regions
            .iter()
            .find(|r| r.widget == "Texture" && r.parent == Some(parent.id))
            .unwrap();
        let font = snap
            .regions
            .iter()
            .find(|r| r.widget == "FontString" && r.parent == Some(parent.id))
            .unwrap();
        let rect =
            |r: &crate::RegionSnapshot| r.rect.map(|r| (r.left, r.bottom, r.width, r.height));
        // UIParent 1920 x 1080; centre (960, 540) + (10, -20), 200 x 100.
        assert_eq!(rect(parent), Some((870.0, 470.0, 200.0, 100.0)));
        // Inset frame: 5 px inside on every side.
        assert_eq!(rect(inset), Some((875.0, 475.0, 190.0, 90.0)));
        // Texture fills the frame.
        assert_eq!(rect(texture), Some((870.0, 470.0, 200.0, 100.0)));
        assert_eq!(
            texture.texture,
            Some(crate::TextureFill::Color([1.0, 0.0, 0.0, 0.5]))
        );
        // Font-backed advances and ascent/descent replace the old 24x12 guess.
        let (w, h) = crate::text::expected_fallback("abcd", 12.0);
        let q = font.rect.unwrap();
        assert!((q.left - 874.0).abs() < 1e-5 && (q.bottom - (566.0 - h)).abs() < 1e-5);
        assert!((q.width - w).abs() < 1e-5 && (q.height - h).abs() < 1e-5);
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

/// Flattened widget method names (including inheritance), with empty return
/// shapes: b=false, d=0, s=empty string, t=empty table, n=nil.
pub type MethodList = std::collections::HashMap<String, std::collections::HashMap<String, String>>;
/// S2 may set this Lua app datum when SessionConfig supplies a manifest.
#[derive(Default)]
pub struct MethodCatalog(pub Option<MethodList>);
pub struct Fallback {
    pub target: String,
    pub empty_shape: String,
}
/// Missing addon fields read nil. Only methods found in a supplied or bundled
/// widget catalog receive a callable fallback.
pub fn missing_method(widget: &str, key: &str, manifest: Option<&MethodList>) -> Option<Fallback> {
    if !key.chars().next().is_some_and(char::is_uppercase) {
        return None;
    }
    let shape = if let Some(manifest) = manifest {
        manifest.get(widget)?.get(key)?.clone()
    } else {
        manifest::methods(widget)
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, shape)| (*shape).to_owned())?
    };
    Some(Fallback {
        target: format!("{widget}:{key}"),
        empty_shape: shape,
    })
}
#[cfg(test)]
mod fallback_tests {
    #[test]
    fn manifest_filters_and_lowercase_fields() {
        let mut catalog = super::MethodList::new();
        catalog.insert("Button".into(), [("Unmodelled".into(), "b".into())].into());
        assert!(super::missing_method("Button", "Wiggle", Some(&catalog)).is_none());
        assert!(super::missing_method("Button", "Unmodelled", Some(&catalog)).is_some());
        assert!(super::missing_method("Button", "addonField", None).is_none());
    }
}

/// Initialize the viewport for a newly created VM, including reloads. S2's
/// VM setup must preserve this call before loading any addon code.
pub fn configure_ui_size(lua: &Lua, size: (f64, f64)) -> mlua::Result<()> {
    if !size.0.is_finite() || !size.1.is_finite() || size.0 <= 0.0 || size.1 <= 0.0 {
        return Err(mlua::Error::runtime("invalid UI size"));
    }
    let store = arena::store(lua);
    let mut a = store.borrow_mut();
    if (a.regions[0].width, a.regions[0].height) != size {
        a.regions[0].width = size.0;
        a.regions[0].height = size.1;
        a.dirty = true;
        a.full_layout = true;
    }
    Ok(())
}

#[cfg(test)]
mod fidelity_tests {
    use crate::Sim;
    #[test]
    fn scroll_offsets_and_nested_clip_reach_snapshot() {
        let sim = Sim::new().unwrap();
        sim.eval(
            r#"
            local v = CreateFrame('ScrollFrame', 'Viewport', UIParent)
            v:SetSize(100, 50); v:SetPoint('BOTTOMLEFT', 20, 30)
            local c = CreateFrame('Frame', 'Content', v); c:SetSize(200, 150)
            v:SetScrollChild(c); v:SetHorizontalScroll(10); v:SetVerticalScroll(25)
            local t = c:CreateTexture('Overflow'); t:SetAllPoints(); t:SetColorTexture(1,0,0,1)
            assert(v:GetHorizontalScrollRange() == 100)
            assert(v:GetVerticalScrollRange() == 100)
        "#,
        )
        .unwrap();
        let snap = sim.snapshot().unwrap();
        let content = snap
            .regions
            .iter()
            .find(|r| r.name.as_deref() == Some("Content"))
            .unwrap();
        let q = content.rect.unwrap();
        assert_eq!(
            (q.left, q.bottom, q.width, q.height),
            (10.0, -45.0, 200.0, 150.0)
        );
        let overflow = snap
            .regions
            .iter()
            .find(|r| r.name.as_deref() == Some("Overflow"))
            .unwrap();
        assert_eq!(overflow.clip, snap.regions[1].rect);
        assert_eq!(overflow.rect, content.rect);
    }
    #[test]
    fn overlapping_frames_raise_lower_and_visual_state() {
        let sim = Sim::new().unwrap();
        sim.eval(r#"
            local a = CreateFrame('Frame', 'A'); a:SetSize(50,50); a:SetPoint('CENTER'); a:EnableMouse(true)
            local b = CreateFrame('Frame', 'B'); b:SetSize(50,50); b:SetPoint('CENTER'); b:EnableMouse(true)
            a:Raise(); assert(a:GetFrameLevel() > b:GetFrameLevel())
            a:Lower(); assert(a:GetFrameLevel() < b:GetFrameLevel())
            b:SetFrameLevel(50)
            local t = b:CreateTexture('Tint', 'OVERLAY', nil, -3)
            t:SetVertexColor(0.2,0.4,0.6,0.8); t:SetTexCoord(0.1,0.9,0.2,0.8)
            local f = CreateFont('FixtureFont'); f:SetFont('Media/font.ttf', 18, 'OUTLINE')
            local label = b:CreateFontString('Label'); label:SetFontObject(f); label:SetTextColor(1,0,0,1)
        "#).unwrap();
        let snap = sim.snapshot().unwrap();
        let tint = snap
            .regions
            .iter()
            .find(|r| r.name.as_deref() == Some("Tint"))
            .unwrap();
        assert_eq!(tint.visual.uv, [0.1, 0.9, 0.2, 0.8]);
        assert_eq!(tint.layer, Some((crate::DrawLayer::Overlay, -3)));
        let label = snap
            .regions
            .iter()
            .find(|r| r.name.as_deref() == Some("Label"))
            .unwrap();
        assert_eq!(label.visual.size, 18.0);
        assert_eq!(label.visual.color, [1.0, 0.0, 0.0, 1.0]);
    }
}

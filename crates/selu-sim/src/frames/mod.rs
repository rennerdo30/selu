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
        if (a.regions[0].width, a.regions[0].height) != self.ui_size {
            a.regions[0].width = self.ui_size.0;
            a.regions[0].height = self.ui_size.1;
            a.dirty = true;
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
                    rect: r.rect,
                    size: Some((r.width, r.height)),
                    shown: r.shown,
                    visible: r.visible,
                    strata: Some(r.strata),
                    level: Some(r.level),
                    layer: Some(r.layer),
                    alpha: r.effective_alpha,
                    texture: r.texture.clone(),
                    text: r.display_text().map(str::to_owned),
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
        // Font string: top left at (874, 566), 4 chars x 6 px wide, 12 tall.
        assert_eq!(rect(font), Some((874.0, 554.0, 24.0, 12.0)));
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
/// Missing lowercase addon fields always read nil. Without a manifest,
/// uppercase keys temporarily retain the historical callable fallback.
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
            .map(|(_, shape)| (*shape).to_owned())
            .unwrap_or_default()
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
    }
    Ok(())
}

//! Rust arena adapter, retaining the S0 model name for existing wiring.
pub mod arena;
pub mod input;
pub mod layout;
pub mod widgets;
use crate::core::{Ctx, RegionId, RegionModel, RegionSnapshot};
use mlua::Lua;
pub struct LuaTableModel {
    pub generation: u32,
}
impl RegionModel for LuaTableModel {
    fn resolve_layout(&self, lua: &Lua, ctx: &Ctx) -> mlua::Result<()> {
        layout::resolve(&mut arena::store(lua).borrow_mut(), ctx);
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
                    text: r.text.clone(),
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

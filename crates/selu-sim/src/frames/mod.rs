//! Frames, textures, font strings and layout. Owner: package S1 (round 8,
//! docs/SIMULATOR.md decisions 3, 4, 15, 16 and 19). S1 adds `arena.rs`,
//! `layout.rs`, `widgets.rs` and `input.rs` here and replaces the Lua-table
//! region model (`../regions.rs` and `CreateFrame` in `../lib.rs`).
//!
//! Groundwork (S0): [`LuaTableModel`] implements the core's
//! [`RegionModel`] over today's Lua tables, so snapshots, Studio's headless
//! mode and tests can be written against the final interface now. It knows
//! frames created with `CreateFrame` and `UIParent`; textures and font
//! strings aren't registered anywhere yet, and there is no layout, so
//! `rect`, `strata`, `level` and `layer` are `None`.

use crate::core::{Ctx, RegionId, RegionModel, RegionSnapshot};
use mlua::{Lua, Table, Value};
use std::collections::HashMap;

/// The round-8 region model: regions are the Lua tables in
/// `__selu_sim_frames` (plus `UIParent` as index 0).
pub struct LuaTableModel {
    /// Copied into every [`RegionId`] so ids from an older VM don't match.
    pub generation: u32,
}

impl LuaTableModel {
    fn regions(&self, lua: &Lua) -> mlua::Result<Vec<Table>> {
        let mut out = Vec::new();
        if let Value::Table(ui) = lua.globals().raw_get::<Value>("UIParent")? {
            out.push(ui);
        }
        let registry: Table = lua.globals().raw_get("__selu_sim_frames")?;
        for frame in registry.sequence_values::<Table>() {
            out.push(frame?);
        }
        Ok(out)
    }
}

impl RegionModel for LuaTableModel {
    fn resolve_layout(&self, _lua: &Lua, _ctx: &Ctx) -> mlua::Result<()> {
        // No layout in the Lua-table model; S1 resolves anchors here.
        Ok(())
    }

    fn snapshot_regions(&self, lua: &Lua, ctx: &Ctx) -> mlua::Result<Vec<RegionSnapshot>> {
        self.resolve_layout(lua, ctx)?;
        let tables = self.regions(lua)?;
        let ids: HashMap<usize, RegionId> = tables
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    t.to_pointer() as usize,
                    RegionId {
                        index: i as u32,
                        generation: self.generation,
                    },
                )
            })
            .collect();
        let mut out = Vec::with_capacity(tables.len());
        for (i, t) in tables.iter().enumerate() {
            let parent = match t.raw_get::<Value>("__parent")? {
                Value::Table(p) => ids.get(&(p.to_pointer() as usize)).copied(),
                _ => None,
            };
            let width = t.raw_get::<Option<f64>>("__width")?;
            let height = t.raw_get::<Option<f64>>("__height")?;
            let mut scripts: Vec<String> = Vec::new();
            if let Value::Table(s) = t.raw_get::<Value>("__scripts")? {
                for pair in s.pairs::<String, Value>() {
                    scripts.push(pair?.0);
                }
            }
            scripts.sort();
            out.push(RegionSnapshot {
                id: RegionId {
                    index: i as u32,
                    generation: self.generation,
                },
                parent,
                widget: t
                    .raw_get::<Option<String>>("__kind")?
                    .unwrap_or_else(|| "Frame".into()),
                name: t.raw_get::<Option<String>>("__name")?,
                rect: None,
                size: width.zip(height),
                shown: t.raw_get::<Option<bool>>("__shown")?.unwrap_or(true),
                visible: false,
                strata: None,
                level: None,
                layer: None,
                alpha: 1.0,
                texture: None,
                text: t.raw_get::<Option<String>>("__text")?,
                scripts,
                created_at: None,
            });
        }
        // Visible = shown and every ancestor visible; parents come first.
        for i in 0..out.len() {
            let parent_visible = match out[i].parent {
                Some(p) => out[p.index as usize].visible,
                None => true,
            };
            out[i].visible = out[i].shown && parent_visible;
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
}

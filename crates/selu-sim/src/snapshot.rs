//! Snapshot assembly (docs/SIMULATOR.md decision 8). Owner: package S2.
//! Groundwork (S0): [`take`] builds a [`Snapshot`] from any
//! [`RegionModel`]; S2 adds snapshots at trace marks and the bounded store
//! Studio scrubs through.

use crate::SimResult;
use crate::core::{Ctx, Generation, RegionModel, Snapshot};
use mlua::Lua;

/// UIParent's size when no session config says otherwise (decision 4).
pub const DEFAULT_UI_SIZE: (f64, f64) = (1920.0, 1080.0);

pub fn take(
    lua: &Lua,
    ctx: &Ctx,
    model: &dyn RegionModel,
    generation: Generation,
    ui_size: (f64, f64),
) -> SimResult<Snapshot> {
    Ok(Snapshot {
        generation,
        time: ctx.now(),
        ui_width: ui_size.0,
        ui_height: ui_size.1,
        regions: model.snapshot_regions(lua, ctx)?,
    })
}

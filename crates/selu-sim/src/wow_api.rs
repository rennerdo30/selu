//! Simulated WoW API functions beyond frames and the clock (package D4 in
//! docs/WORK-PACKAGES.md adds `hooksecurefunc`, `SlashCmdList` and the
//! `SLASH_*` globals here).

use mlua::Lua;

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let _ = lua;
    Ok(())
}

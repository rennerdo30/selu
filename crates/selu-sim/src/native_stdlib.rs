//! Bootstrap extensions supplied by the WoW Lua host rather than Lua 5.1.
use mlua::{Lua, MultiValue, Value};
fn word(lua: &Lua, value: Value) -> mlua::Result<u32> {
    let n = lua
        .coerce_number(value)?
        .ok_or_else(|| mlua::Error::runtime("bit operation expects a number"))?;
    if !n.is_finite() {
        return Err(mlua::Error::runtime("finite bit operand expected"));
    }
    Ok(n.trunc().rem_euclid(4294967296.0) as u32)
}
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    if !lua.globals().raw_get::<Value>("bit")?.is_nil() {
        return Ok(());
    }
    let bit = lua.create_table()?;
    for name in ["band", "bor", "bxor"] {
        bit.raw_set(
            name,
            lua.create_function(move |lua, args: MultiValue| {
                let mut out = if name == "band" { u32::MAX } else { 0 };
                for value in args {
                    let value = word(lua, value)?;
                    out = match name {
                        "band" => out & value,
                        "bor" => out | value,
                        _ => out ^ value,
                    };
                }
                Ok(out)
            })?,
        )?;
    }
    bit.raw_set(
        "bnot",
        lua.create_function(|lua, value: Value| Ok(!word(lua, value)?))?,
    )?;
    for name in ["lshift", "rshift", "arshift", "rol", "ror"] {
        bit.raw_set(
            name,
            lua.create_function(move |lua, (value, shift): (Value, Value)| {
                let value = word(lua, value)?;
                let shift = word(lua, shift)? & 31;
                Ok(match name {
                    "lshift" => value.wrapping_shl(shift),
                    "rshift" => value.wrapping_shr(shift),
                    "arshift" => ((value as i32) >> shift) as u32,
                    "rol" => value.rotate_left(shift),
                    _ => value.rotate_right(shift),
                })
            })?,
        )?;
    }
    bit.raw_set(
        "bswap",
        lua.create_function(|lua, value: Value| Ok(word(lua, value)?.swap_bytes()))?,
    )?;
    bit.raw_set(
        "tobit",
        lua.create_function(|lua, value: Value| Ok(word(lua, value)? as i32))?,
    )?;
    bit.raw_set(
        "tohex",
        lua.create_function(|lua, (value, digits): (Value, Option<i32>)| {
            let digits = digits.unwrap_or(8);
            let count = digits.unsigned_abs().min(8) as usize;
            let text = format!("{:08x}", word(lua, value)?);
            let text = &text[8 - count..];
            Ok(if digits < 0 {
                text.to_ascii_uppercase()
            } else {
                text.to_string()
            })
        })?,
    )?;
    lua.globals().raw_set("bit", bit)?;
    Ok(())
}

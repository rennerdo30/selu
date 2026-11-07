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
#[cfg(test)]
mod tests {
    #[test]
    fn removevalue_releases_every_array_reference_and_keeps_hash_fields() {
        let s = crate::Session::new(Default::default()).unwrap();
        super::install(s.lua()).unwrap();
        s.eval("local released={GetElementData=function() return 1 end};local live={GetElementData=function() return 2 end};local active={released,live,released,tag='keep'};assert(table.removevalue(active,released)==2);released.GetElementData=nil;assert(#active==1 and active[1]:GetElementData()==2 and active.tag=='keep');assert(table.removevalue(active,{})==0);assert(table.removevalue({},nil)==0);local remove=table.removevalue;table={};assert(remove(active,live)==1 and #active==0)").unwrap();
    }
}
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    // Ordered array compaction. Pool releases depend on actually removing all
    // matching entries before invalidating accessors on the released object.
    let remove = crate::host_lua::factory(
        lua,
        "selu.table.removevalue",
        r#"
        return function(values,needle)
            local size,write,removed=#values,1,0
            for read=1,size do
                local value=rawget(values,read)
                if value==needle then removed=removed+1
                else rawset(values,write,value);write=write+1 end
            end
            for index=write,size do rawset(values,index,nil) end
            return removed
        end
    "#,
    )?;
    lua.globals()
        .raw_get::<mlua::Table>("table")?
        .raw_set("removevalue", remove)?;
    // StringUtilDocumentation.lua: preserve printable UTF-8, escape controls
    // (except tab/CR/LF) and invalid UTF-8 bytes as decimal byte escapes.
    let strings = lua
        .globals()
        .raw_get::<Option<mlua::Table>>("C_StringUtil")?
        .unwrap_or(lua.create_table()?);
    strings.raw_set(
        "EscapeDecimalNonPrintables",
        lua.create_function(|lua, value: mlua::String| {
            let bytes = value.as_bytes();
            let mut remaining = bytes.as_ref();
            let mut result = String::new();
            while !remaining.is_empty() {
                let (valid, invalid) = match std::str::from_utf8(remaining) {
                    Ok(text) => (text, 0),
                    Err(error) => (
                        std::str::from_utf8(&remaining[..error.valid_up_to()])
                            .expect("valid prefix"),
                        error
                            .error_len()
                            .unwrap_or(remaining.len() - error.valid_up_to()),
                    ),
                };
                for ch in valid.chars() {
                    if ch.is_ascii_control() && !matches!(ch, '\t' | '\r' | '\n') {
                        result.push_str(&format!("\\{:03}", ch as u8));
                    } else {
                        result.push(ch);
                    }
                }
                for byte in &remaining[valid.len()..valid.len() + invalid] {
                    result.push_str(&format!("\\{byte:03}"));
                }
                remaining = &remaining[valid.len() + invalid..];
            }
            lua.create_string(&result)
        })?,
    )?;
    lua.globals().raw_set("C_StringUtil", strings)?;
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

//! Simulator code gets private snapshots of Lua builtins. Addon code keeps
//! the real mutable globals; this is not an addon sandbox or language API.
use mlua::{Chunk, Function, Lua, Table, Value};
const ENV: &str = "selu.host_environment";
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let environment = lua.create_table()?;
    for name in [
        "assert",
        "collectgarbage",
        "error",
        "getfenv",
        "getmetatable",
        "ipairs",
        "load",
        "loadstring",
        "next",
        "pairs",
        "pcall",
        "rawequal",
        "rawget",
        "rawset",
        "select",
        "setfenv",
        "setmetatable",
        "tonumber",
        "tostring",
        "type",
        "unpack",
        "xpcall",
        "newproxy",
    ] {
        environment.raw_set(name, globals.raw_get::<Value>(name)?)?;
    }
    for name in ["table", "string", "math", "coroutine", "debug"] {
        if let Value::Table(original) = globals.raw_get::<Value>(name)? {
            let copy = lua.create_table()?;
            for pair in original.pairs::<Value, Value>() {
                let (key, value) = pair?;
                copy.raw_set(key, value)?;
            }
            environment.raw_set(name, copy)?;
        }
    }
    environment.raw_set("_G", globals.clone())?;
    let mt = lua.create_table()?;
    mt.raw_set("__index", globals.clone())?;
    mt.raw_set("__newindex", globals)?;
    environment.set_metatable(Some(mt));
    lua.set_named_registry_value(ENV, environment)
}
pub(crate) fn seal(lua: &Lua) -> mlua::Result<()> {
    let environment: Table = lua.named_registry_value(ENV)?;
    for pair in lua.globals().pairs::<String, Value>() {
        let (name, value) = pair?;
        if matches!(value, Value::Function(_))
            && (name.starts_with("__selu_")
                || matches!(
                    name.as_str(),
                    "GetTime" | "CreateFrame" | "CreateFont" | "print"
                ))
        {
            environment.raw_set(name, value)?;
        }
    }
    Ok(())
}
pub(crate) fn load<'a>(lua: &'a Lua, source: &'a str) -> Chunk<'a> {
    const UPVALUES: &str = "local _G,assert,collectgarbage,error,getfenv,getmetatable,ipairs,load,loadstring,next,pairs,pcall,print,rawequal,rawget,rawset,select,setfenv,setmetatable,tonumber,tostring,type,unpack,xpcall,table,string,math,coroutine = _G,assert,collectgarbage,error,getfenv,getmetatable,ipairs,load,loadstring,next,pairs,pcall,print,rawequal,rawget,rawset,select,setfenv,setmetatable,tonumber,tostring,type,unpack,xpcall,table,string,math,coroutine";
    lua.load(format!("{UPVALUES}\n{source}")).set_environment(
        lua.named_registry_value::<Table>(ENV)
            .expect("host environment installed before helpers"),
    )
}
pub(crate) fn factory(lua: &Lua, key: &str, source: &str) -> mlua::Result<Function> {
    if let Some(function) = lua.named_registry_value::<Option<Function>>(key)? {
        return Ok(function);
    }
    let function: Function = load(lua, source).eval()?;
    lua.set_named_registry_value(key, function.clone())?;
    Ok(function)
}
pub(crate) fn builtin(lua: &Lua, name: &str) -> mlua::Result<Function> {
    lua.named_registry_value::<Table>(ENV)?.raw_get(name)
}

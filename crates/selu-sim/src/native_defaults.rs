//! Defaults are restricted to manifest-declared fields and methods. Unknown
//! members remain nil; there is no dynamic catch-all object or language bridge.
use mlua::{Lua, MultiValue, Value};
use selu_manifest::Declaration;
use std::{collections::HashMap, sync::Arc};
pub(crate) type Schemas = Arc<HashMap<String, Declaration>>;
#[derive(Default)]
pub(crate) struct CallCounts(std::cell::RefCell<HashMap<String, u64>>);
pub(crate) fn record(lua: &Lua, name: &str) {
    if let Some(counts) = lua.app_data_ref::<CallCounts>() {
        let mut counts = counts.0.borrow_mut();
        let count = counts.entry(name.to_owned()).or_default();
        *count = count.saturating_add(1);
    }
}
pub(crate) fn counts(lua: &Lua) -> Vec<(String, u64)> {
    let mut result: Vec<_> = lua
        .app_data_ref::<CallCounts>()
        .map(|counts| {
            counts
                .0
                .borrow()
                .iter()
                .map(|(name, count)| (name.clone(), *count))
                .collect()
        })
        .unwrap_or_default();
    result.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    result
}

pub(crate) fn value(
    lua: &Lua,
    ty: &str,
    schemas: &Schemas,
    ctx: &crate::Ctx,
    depth: usize,
) -> mlua::Result<Value> {
    let ty = ty
        .split('|')
        .map(str::trim)
        .find(|s| !matches!(*s, "nil" | "null"))
        .unwrap_or("nil")
        .trim_end_matches('?');
    Ok(match ty {
        "boolean" | "bool" => Value::Boolean(false),
        "number" | "integer" | "float" | "double" | "luaIndex" | "uiUnit" | "WOWMONEY"
        | "fileID" | "Seconds" | "BigUInteger" | "BigInteger" | "time_t" | "Milliseconds"
        | "normalizedValue" | "SingleColorValue" | "uiFontHeight" | "size" => Value::Number(0.0),
        "string"
        | "cstring"
        | "WOWGUID"
        | "UnitToken"
        | "UnitTokenVariant"
        | "FramePoint"
        | "DrawLayer"
        | "textureAtlas"
        | "textureKit"
        | "stringView"
        | "UnitTokenNamePlate"
        | "UnitTokenPvPRestrictedForAddOns"
        | "UnitTokenRestrictedForAddOns"
        | "mouseButton"
        | "MouseButton"
        | "JustifyHorizontal"
        | "JustifyVertical"
        | "FrameStrata"
        | "ScriptTypeName"
        | "BlendMode"
        | "FontAlphabet"
        | "HTMLTextType"
        | "Orientation"
        | "InsertMode"
        | "LoopType"
        | "FilterMode"
        | "SimpleButtonStateToken"
        | "ClickButton" => Value::String(lua.create_string("")?),
        "colorRGB" | "colorRGBA" => {
            let create: mlua::Function = lua.globals().get("CreateColor")?;
            create.call::<Value>((0.0, 0.0, 0.0, 1.0))?
        }
        "nil" | "void" => Value::Nil,
        "LuaFunctionReference" => Value::Function(lua.create_function(|_, _: MultiValue| Ok(()))?),
        _ => {
            let table = lua.create_table()?;
            if depth < 8 {
                if let Some(schema) = schemas.get(ty) {
                    if schema.ty.as_deref() == Some("enum") {
                        return Ok(Value::Number(0.0));
                    }
                    for parent in &schema.inherits {
                        if let Value::Table(base) = value(lua, parent, schemas, ctx, depth + 1)? {
                            for pair in base.pairs::<String, Value>() {
                                let (key, v) = pair?;
                                table.raw_set(key, v)?;
                            }
                        }
                    }
                    for field in &schema.fields {
                        table.raw_set(
                            field.name.as_str(),
                            value(
                                lua,
                                field.ty.as_deref().unwrap_or("table"),
                                schemas,
                                ctx,
                                depth + 1,
                            )?,
                        )?;
                    }
                    for method in &schema.methods {
                        let returns = method.returns.clone();
                        let schemas = schemas.clone();
                        let ctx = ctx.clone();
                        let name = format!("{ty}:{}", method.name);
                        table.raw_set(
                            method.name.as_str(),
                            lua.create_function(move |lua, _: MultiValue| {
                                ctx.not_simulated(&name);
                                values(lua, &returns, &schemas, &ctx, depth + 1)
                            })?,
                        )?;
                    }
                }
            }
            Value::Table(table)
        }
    })
}
pub(crate) fn values(
    lua: &Lua,
    returns: &[Declaration],
    schemas: &Schemas,
    ctx: &crate::Ctx,
    depth: usize,
) -> mlua::Result<MultiValue> {
    let mut out = MultiValue::new();
    for declaration in returns {
        out.push_back(value(
            lua,
            declaration.ty.as_deref().unwrap_or("table"),
            schemas,
            ctx,
            depth,
        )?);
    }
    Ok(out)
}

//! Empty map geometry for the synthetic world, not a map-data export.
use mlua::{Lua, Table, Value};

pub(crate) fn install(lua: &Lua, ctx: &crate::Ctx) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(map) = globals.raw_get::<Option<Table>>("C_Map")? else {
        return Ok(());
    };
    let standins = globals.raw_get::<Option<Table>>("__selu_native_standins")?;
    // No known map/fog records in the synthetic world. Preserve a richer
    // provider rather than overriding actual runtime data supplied by one.
    for (namespace, names) in [
        ("C_Map", &["GetMapInfo", "GetMapInfoAtPosition"][..]),
        ("C_FogOfWar", &["GetFogOfWarForMap"][..]),
        (
            "C_DeathInfo",
            &["GetCorpseMapPosition", "GetDeathReleasePosition"][..],
        ),
        ("C_AreaPoiInfo", &["GetAreaPOIInfo"][..]),
        ("C_GossipInfo", &["GetPoiForUiMapID", "GetPoiInfo"][..]),
        (
            "C_InvasionInfo",
            &["GetInvasionForUiMapID", "GetInvasionInfo"][..],
        ),
    ] {
        let Some(owner) = globals.raw_get::<Option<Table>>(namespace)? else {
            continue;
        };
        for name in names {
            let current = owner.raw_get::<Value>(*name)?;
            if !current.is_nil()
                && !standins.as_ref().is_some_and(|s| {
                    s.raw_get::<Value>(format!("{namespace}.{name}"))
                        .ok()
                        .is_some_and(|v| v == current)
                })
            {
                continue;
            }
            let ctx = ctx.clone();
            let namespace = namespace.to_owned();
            let name = (*name).to_owned();
            owner.raw_set(
                name.clone(),
                lua.create_function(move |lua, id: Option<i64>| {
                    ctx.not_simulated("map database records unavailable; synthetic world only");
                    if namespace == "C_Map" && name == "GetMapInfo" && id == Some(0) {
                        let info = lua.create_table()?;
                        info.raw_set("mapID", 0)?;
                        info.raw_set("parentMapID", 0)?;
                        info.raw_set("flags", 0)?;
                        info.raw_set(
                            "name",
                            lua.globals()
                                .raw_get::<Table>("__selu_zone")?
                                .raw_get::<String>("zone")?,
                        )?;
                        let enums: Table = lua.globals().raw_get("Enum")?;
                        info.raw_set(
                            "mapType",
                            enums.get::<Table>("UIMapType")?.get::<Value>("World")?,
                        )?;
                        return Ok(Value::Table(info));
                    }
                    Ok(Value::Nil)
                })?,
            )?;
        }
    }
    for name in ["GetMapArtLayers", "GetMapArtLayerTextures"] {
        let current = map.raw_get::<Value>(name)?;
        if !current.is_nil()
            && !standins.as_ref().is_some_and(|s| {
                s.raw_get::<Value>(format!("C_Map.{name}"))
                    .ok()
                    .is_some_and(|v| v == current)
            })
        {
            continue;
        }
        let ctx = ctx.clone();
        map.raw_set(
            name,
            lua.create_function(move |lua, _: mlua::MultiValue| {
                ctx.not_simulated("map database geometry unavailable; using an empty canvas");
                if name == "GetMapArtLayerTextures" {
                    return lua.create_table();
                }
                // One untextured canvas sized to this simulated viewport. These
                // are simulator dimensions, not a claim about any client map.
                let width = lua
                    .globals()
                    .raw_get::<Option<f64>>("__selu_screen_width")?
                    .unwrap_or(1920.)
                    .max(1.);
                let height = lua
                    .globals()
                    .raw_get::<Option<f64>>("__selu_screen_height")?
                    .unwrap_or(1080.)
                    .max(1.);
                let layer = lua.create_table()?;
                for (key, value) in [
                    ("layerWidth", width),
                    ("layerHeight", height),
                    ("tileWidth", width),
                    ("tileHeight", height),
                    ("minScale", 1.),
                    ("maxScale", 1.),
                    ("additionalZoomSteps", 0.),
                ] {
                    layer.raw_set(key, value)?;
                }
                lua.create_sequence_from([layer])
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_canvas_has_coherent_dimensions_and_no_texture_ids() {
        let s = crate::Session::new(Default::default()).unwrap();
        super::install(s.lua(), s.ctx()).unwrap();
        s.eval("local layers=C_Map.GetMapArtLayers(0);assert(#layers==1 and layers[1].layerWidth>0 and layers[1].minScale==layers[1].maxScale);assert(#C_Map.GetMapArtLayerTextures(0,1)==0)").unwrap();
        s.eval("Enum={UIMapType={World=11}};local info=C_Map.GetMapInfo(0);assert(info.mapID==0 and info.parentMapID==0 and info.flags==0 and info.mapType==11);assert(C_Map.GetMapInfo(12345)==nil);C_Map.GetMapArtLayers=function() return 'custom provider' end").unwrap();
        super::install(s.lua(), s.ctx()).unwrap();
        s.eval("assert(C_Map.GetMapArtLayers()=='custom provider')")
            .unwrap();
    }
}

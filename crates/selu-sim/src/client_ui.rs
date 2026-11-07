//! Read-only client metadata required by Blizzard's own UI initialization.
use mlua::{Lua, Table, Value};
pub(crate) fn install(lua: &Lua, ctx: &crate::Ctx) -> mlua::Result<()> {
    let globals = lua.globals();
    // The baseline has no guild, queued battlegrounds, learned specializations,
    // pets, or equipped Azerite item. These are empty world collections, not
    // client constants or UI policies. Preserve any richer native provider.
    let standins = globals.raw_get::<Option<Table>>("__selu_native_standins")?;
    for (path, shape, capability) in [
        ("GuildControlGetNumRanks", "count", None),
        ("GetMaxBattlefieldID", "count", None),
        (
            "GetNumSpecializations",
            "count",
            Some("C_SpecializationInfo"),
        ),
        (
            "C_PetJournal.GetNumPetsNeedingFanfare",
            "count",
            Some("C_PetJournal"),
        ),
        (
            "C_PetBattles.GetAllEffectNames",
            "empty",
            Some("C_PetBattles"),
        ),
        ("C_PetBattles.GetAllStates", "empty", Some("C_PetBattles")),
        ("C_LFGList.GetAvailableRoles", "roles", Some("C_LFGList")),
        (
            "C_AzeriteItem.FindActiveAzeriteItem",
            "empty",
            Some("C_AzeriteItem"),
        ),
        (
            "C_AccountServices.GetPreferredLocales",
            "empty",
            Some("C_Club"),
        ),
        (
            "C_AccountServices.GetPreferredDatacenterLocalities",
            "empty",
            Some("C_Club"),
        ),
    ] {
        // Use the selected client's capabilities rather than making modern
        // systems appear on products whose documentation lacks them.
        if let Some(capability) = capability
            && globals.raw_get::<Value>(capability)?.is_nil()
        {
            continue;
        }
        let (owner, key) = if let Some((namespace, key)) = path.split_once('.') {
            let owner = globals
                .raw_get::<Option<Table>>(namespace)?
                .unwrap_or(lua.create_table()?);
            globals.raw_set(namespace, owner.clone())?;
            (owner, key)
        } else {
            (globals.clone(), path)
        };
        let existing = owner.raw_get::<Value>(key)?;
        let standin = standins
            .as_ref()
            .map(|s| s.raw_get::<Value>(path))
            .transpose()?
            .unwrap_or(Value::Nil);
        if !existing.is_nil() && existing != standin {
            continue;
        }
        let context = ctx.clone();
        owner.raw_set(
            key,
            lua.create_function(move |_, _: mlua::MultiValue| {
                context.not_simulated(path);
                Ok(match shape {
                    "count" => mlua::MultiValue::from_vec(vec![Value::Integer(0)]),
                    "roles" => mlua::MultiValue::from_vec(vec![Value::Boolean(false); 3]),
                    _ => mlua::MultiValue::new(),
                })
            })?,
        )?;
    }
    lua.set_named_registry_value("selu.default_ui_strings", true)?;
    let logging = globals
        .raw_get::<Option<Table>>("C_Log")?
        .unwrap_or(lua.create_table()?);
    let context = ctx.clone();
    logging.raw_set(
        "LogErrorMessage",
        lua.create_function(move |lua, message: String| {
            // Explicit calls to Blizzard's installed error handler (assertsafe,
            // CallErrorHandler) must reach the captured list too. Boundaries
            // already own their original error and must not record it twice.
            if !lua
                .named_registry_value::<Option<bool>>("selu.notifying_native_error")?
                .unwrap_or(false)
            {
                context.log(format!("error: {message}"));
            }
            Ok(())
        })?,
    )?;
    globals.raw_set("C_Log", logging)?;
    let texture = globals
        .raw_get::<Option<Table>>("C_Texture")?
        .unwrap_or(lua.create_table()?);
    let existing = texture.raw_get::<Value>("GetAtlasInfo")?;
    let standins = globals.raw_get::<Option<Table>>("__selu_native_standins")?;
    let standin = standins
        .and_then(|table| table.raw_get::<Value>("C_Texture.GetAtlasInfo").ok())
        .unwrap_or(Value::Nil);
    if existing.is_nil() || existing == standin {
        let ctx = ctx.clone();
        texture.raw_set("GetAtlasInfo",lua.create_function(move |lua,name:String| {
            let atlas=crate::frames::arena::store(lua).borrow().fonts.assets.atlas(&name);
            let Some(atlas)=atlas else {
                ctx.not_simulated(&format!("unavailable atlas metadata: {name}"));
                return Ok(None);
            };
            let info=lua.create_table()?;
            info.raw_set("elementName",atlas.name)?;
            info.raw_set("width",atlas.width)?;
            info.raw_set("height",atlas.height)?;
            info.raw_set("file",atlas.file_data_id)?;
            info.raw_set("leftTexCoord",f64::from(atlas.left)/f64::from(atlas.sheet_width))?;
            info.raw_set("rightTexCoord",f64::from(atlas.right)/f64::from(atlas.sheet_width))?;
            info.raw_set("topTexCoord",f64::from(atlas.top)/f64::from(atlas.sheet_height))?;
            info.raw_set("bottomTexCoord",f64::from(atlas.bottom)/f64::from(atlas.sheet_height))?;
            info.raw_set("tilesHorizontally",atlas.tiles_horizontally)?;
            info.raw_set("tilesVertically",atlas.tiles_vertically)?;
            let vector=crate::host_lua::factory(lua,"selu.helper.atlas_vector",r#"
                local methods={GetXY=function(self) return self.x,self.y end}
                return function(x,y,mixin) return setmetatable({x=x,y=y},{__index=mixin or methods}) end
            "#)?;
            info.raw_set("rawSize",vector.call::<Table>((atlas.raw_width,atlas.raw_height,lua.globals().raw_get::<Option<Table>>("Vector2DMixin")?))?)?;
            Ok(Some(info))
        })?)?;
    }
    globals.raw_set("C_Texture", texture)?;
    for kind in ["Frame", "Button", "FontString", "Texture", "StatusBar"] {
        globals.raw_set(
            format!("Get{kind}Metatable"),
            lua.create_function(move |lua, ()| {
                let methods = lua
                    .named_registry_value::<Table>(&format!("selu.methods.{kind}"))
                    .or_else(|_| lua.named_registry_value::<Table>("selu.methods.Frame"))?;
                let metatable = lua.create_table()?;
                metatable.raw_set("__index", methods)?;
                Ok(metatable)
            })?,
        )?;
    }
    let storage = crate::frames::arena::store(lua)
        .borrow()
        .fonts
        .assets
        .client_storage();
    if let Some(storage) = storage {
        let colors = (|| -> selu_gamedata::Result<Vec<(String, u32)>> {
            let bytes = storage.read(4328468)?;
            let table = selu_gamedata::db2::Table::parse(&bytes)?;
            if table.layout != 0x760e0516 {
                return Err("unsupported GlobalColor layout".into());
            }
            (0..table.count())
                .map(|row| Ok((table.text(row, 0)?.to_owned(), table.value(row, 1)?)))
                .collect()
        })();
        match colors {
            Ok(colors) => {
                let namespace = globals.raw_get::<Table>("C_UIColor")?;
                namespace.raw_set(
                    "GetColors",
                    lua.create_function(move |lua, ()| {
                        let result = lua.create_table()?;
                        for (index, (name, argb)) in colors.iter().enumerate() {
                            let entry = lua.create_table()?;
                            entry.raw_set("baseTag", name.as_str())?;
                            let rgba = lua.create_table()?;
                            for (key, shift) in [("r", 16), ("g", 8), ("b", 0), ("a", 24)] {
                                rgba.raw_set(key, f64::from((argb >> shift) & 255) / 255.0)?;
                            }
                            entry.raw_set("color", rgba)?;
                            result.raw_set(index + 1, entry)?;
                        }
                        Ok(result)
                    })?,
                )?;
            }
            Err(error) => ctx.not_simulated(&format!("client color palette unavailable: {error}")),
        }
    }
    Ok(())
}

/// Prefer this client's strings; only permit synthetic labels if neither the
/// client payload nor a source export exists. No account files are inspected.
pub(crate) fn load_strings(
    lua: &Lua,
    ctx: &crate::Ctx,
    source: &std::path::Path,
) -> mlua::Result<()> {
    let storage = crate::frames::arena::store(lua)
        .borrow()
        .fonts
        .assets
        .client_storage();
    let source = source.canonicalize().map_err(mlua::Error::external)?;
    let mut found = false;
    let mut count = 0;
    if let Some(storage) = storage.as_deref() {
        let bytes = storage
            .read(1394440)
            .or_else(|_| storage.read_path("DBFilesClient/GlobalStrings.db2"));
        match bytes {
            Ok(bytes) => {
                found = true;
                match selu_gamedata::db2::Table::parse(&bytes) {
                    Ok(table) => {
                        for row in 0..table.count() {
                            let (Ok(key), Ok(text)) = (table.text(row, 0), table.text(row, 1))
                            else {
                                continue;
                            };
                            if !key.is_empty() {
                                lua.globals().raw_set(key, text)?;
                                count += 1;
                            }
                        }
                    }
                    Err(error) => ctx.not_simulated(&format!("GlobalStrings DB2 decode: {error}")),
                }
            }
            Err(error) => ctx.log(format!("GlobalStrings DB2 unavailable: {error}")),
        }
    }
    let mut paths = vec![
        "Interface/GlobalStrings.lua".to_owned(),
        "Interface/FrameXML/GlobalStrings.lua".into(),
        "Interface/AddOns/Blizzard_GlobalStrings/GlobalStrings.lua".into(),
        "Interface/AddOns/Blizzard_GlobalStrings/Blizzard_GlobalStrings.lua".into(),
    ];
    let export = source.join("Interface/AddOns/Blizzard_GlobalStrings");
    if let Ok(entries) = std::fs::read_dir(export) {
        for entry in entries.flatten() {
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "lua")
            {
                paths.push(format!(
                    "Interface/AddOns/Blizzard_GlobalStrings/{}",
                    entry.file_name().to_string_lossy()
                ));
            }
        }
    }
    paths.sort();
    paths.dedup();
    for path in paths {
        let bytes = storage
            .as_deref()
            .and_then(|storage| storage.read_path(&path).ok())
            .or_else(|| {
                source
                    .join(&path)
                    .canonicalize()
                    .ok()
                    .filter(|path| path.starts_with(&source))
                    .and_then(|path| std::fs::read(path).ok())
            });
        if let Some(bytes) = bytes {
            found = true;
            let text = String::from_utf8(bytes).map_err(mlua::Error::external)?;
            crate::core::call_lua::<()>(
                lua,
                lua.load(text.trim_start_matches('\u{feff}'))
                    .set_name(&path)
                    .into_function()?,
                (),
            )?;
            ctx.log(format!("GlobalStrings source: {path}"));
        }
    }
    if !found {
        ctx.not_simulated("GlobalStrings absent from local CASC and source export; synthetic localized labels enabled");
    } else {
        ctx.log(format!(
            "GlobalStrings: {count} DB2 strings loaded; synthetic labels disabled"
        ));
    }
    lua.set_named_registry_value("selu.synthetic_strings", !found)?;
    lua.globals().raw_set("__selu_synthetic_strings", !found)
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_collections_are_native_world_results_and_existing_providers_win() {
        let s = crate::Session::new(Default::default()).unwrap();
        super::install(s.lua(), s.sim().ctx()).unwrap();
        s.eval("assert(C_PetJournal==nil and C_PetBattles==nil and C_AzeriteItem==nil and C_AccountServices==nil and GetNumSpecializations==nil); C_PetJournal={};C_PetBattles={};C_AzeriteItem={};C_LFGList={};C_Club={};C_SpecializationInfo={}").unwrap();
        s.eval("function GetNumSpecializations() return 7 end")
            .unwrap();
        super::install(s.lua(), s.sim().ctx()).unwrap();
        s.eval("assert(GetNumSpecializations()==7);assert(GetMaxBattlefieldID()==0);assert(GuildControlGetNumRanks()==0);assert(C_PetJournal.GetNumPetsNeedingFanfare()==0);assert(C_AzeriteItem.FindActiveAzeriteItem()==nil);assert(select('#',C_PetBattles.GetAllEffectNames())==0); local a,b,c=C_LFGList.GetAvailableRoles();assert(a==false and b==false and c==false)").unwrap();
    }
    #[test]
    fn animation_restart_rewinds_and_model_properties_are_per_region() {
        let s = crate::Session::new(Default::default()).unwrap();
        s.eval("f=CreateFrame('PlayerModel');other=CreateFrame('PlayerModel');f:SetRotation(.75);f:SetPortraitZoom(.4);assert(f.__selu_rotation==.75 and other.__selu_rotation==nil);assert(f.__selu_portrait_zoom==.4);local t=f:CreateTexture();t:SetRotation(.25);assert(t:GetRotation()==.25);g=f:CreateAnimationGroup();a=g:CreateAnimation('Alpha');a:SetFromAlpha(0);a:SetToAlpha(1);a:SetDuration(1);plays=0;g:SetScript('OnPlay',function()plays=plays+1 end);g:Play()").unwrap();
        s.advance_time(0.5);
        s.eval("assert(f:GetAlpha()>.4 and f:GetAlpha()<.6);g:Restart();assert(g:IsPlaying() and plays==2)").unwrap();
        s.advance_time(0.1);
        s.eval("assert(f:GetAlpha()<.2)").unwrap();
    }
}

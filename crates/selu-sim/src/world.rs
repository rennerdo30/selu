//! Session-local world data. Sources: read-only E:/Development/wow-ui-source,
//! forever build 1.60.1.70790, Interface/AddOns/Blizzard_APIDocumentationGenerated:
//! UnitDocumentation.lua (unit events/APIs), CurrencyInfoDocumentation.lua (money),
//! ContainerDocumentation.lua (bags), MapDocumentation.lua (zone events/map).
use crate::{SimResult, session::Vm, value_string};
use mlua::{Lua, ObjectLike, Table, Value};
/// Simulated fresh-character preference; layout data remains client-owned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditModePreset {
    #[default]
    Modern,
    Classic,
}
impl EditModePreset {
    fn key(self) -> &'static str {
        match self {
            Self::Modern => "modern",
            Self::Classic => "classic",
        }
    }
    fn enum_key(self) -> &'static str {
        match self {
            Self::Modern => "Modern",
            Self::Classic => "Classic",
        }
    }
    fn layout(self) -> u8 {
        match self {
            Self::Modern => 1,
            Self::Classic => 2,
        }
    }
}
pub(crate) fn configure_edit_mode(vm: &Vm, available: bool) -> SimResult<()> {
    vm.lua
        .set_named_registry_value("selu.edit_mode_available", available)?;
    if !available {
        vm.ctx()
            .not_simulated("Edit Mode preset ignored: this UI source does not ship Edit Mode");
    }
    let api: Table = vm.lua.globals().raw_get("C_EditMode")?;
    api.raw_set(
        "GetLayouts",
        vm.lua.create_function(move |lua, ()| {
            let preset = if available {
                read_edit_mode_preset(lua)?
            } else {
                EditModePreset::Modern
            };
            let info = lua.create_table()?;
            info.raw_set("activeLayout", preset.layout())?;
            info.raw_set("layouts", lua.create_table()?)?;
            Ok(info)
        })?,
    )?;
    api.raw_set(
        "GetEditModeDefaultLayout",
        vm.lua.create_function(move |lua, ()| {
            let enums: Table = lua.globals().get("Enum")?;
            let presets: Table = enums.get("EditModePresetLayouts")?;
            let preset = if available {
                read_edit_mode_preset(lua)?
            } else {
                EditModePreset::Modern
            };
            presets.get::<Value>(preset.enum_key())
        })?,
    )
}
fn read_edit_mode_preset(lua: &Lua) -> SimResult<EditModePreset> {
    Ok(
        match lua
            .globals()
            .raw_get::<Option<String>>("__selu_edit_mode_preset")?
            .as_deref()
        {
            Some("classic") => EditModePreset::Classic,
            _ => EditModePreset::Modern,
        },
    )
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BagSlots {
    pub ordinary: u8,
    pub reagent: u8,
}
/// Owner-selected simulator defaults, not exported client data. Channels use
/// their base product; an unselected/unknown product makes no capacity claim.
pub const BAG_SLOT_DEFAULTS: &[(&str, BagSlots)] = &[
    (
        "wow",
        BagSlots {
            ordinary: 4,
            reagent: 1,
        },
    ),
    (
        "wowf",
        BagSlots {
            ordinary: 4,
            reagent: 1,
        },
    ),
    (
        "wow_classic",
        BagSlots {
            ordinary: 4,
            reagent: 0,
        },
    ),
    (
        "wow_classic_era",
        BagSlots {
            ordinary: 4,
            reagent: 0,
        },
    ),
    (
        "wow_anniversary",
        BagSlots {
            ordinary: 4,
            reagent: 0,
        },
    ),
    (
        "wow_classic_titan",
        BagSlots {
            ordinary: 4,
            reagent: 0,
        },
    ),
];
pub fn bag_slot_defaults(target: Option<&str>) -> BagSlots {
    let product = target.unwrap_or("").split('@').next().unwrap_or("");
    let product = selu_manifest::base_product(selu_manifest::product(product));
    let product = product
        .strip_suffix("_ptr")
        .or_else(|| product.strip_suffix("_beta"))
        .unwrap_or(product);
    BAG_SLOT_DEFAULTS
        .iter()
        .find(|(name, _)| *name == product)
        .map(|(_, slots)| *slots)
        .unwrap_or_default()
}
pub(crate) fn configure_bag_slots(lua: &Lua, target: Option<&str>) -> SimResult<()> {
    let slots = bag_slot_defaults(target);
    let state = lua.create_table()?;
    state.raw_set("default_ordinary", slots.ordinary)?;
    state.raw_set("default_reagent", slots.reagent)?;
    state.raw_set("ordinary", slots.ordinary)?;
    state.raw_set("reagent", slots.reagent)?;
    lua.globals().raw_set("__selu_bag_slots", state)
}
fn update_bag_slots(lua: &Lua, ordinary: Option<u8>, reagent: Option<u8>) -> SimResult<()> {
    let state: Table = lua.globals().raw_get("__selu_bag_slots")?;
    state.raw_set("requested_ordinary", ordinary)?;
    state.raw_set("requested_reagent", reagent)?;
    for (key, requested) in [("ordinary", ordinary), ("reagent", reagent)] {
        let value = state
            .raw_get::<Option<u8>>(format!("documented_{key}"))?
            .or(requested)
            .unwrap_or(state.raw_get(format!("default_{key}"))?);
        state.raw_set(key, value)?;
    }
    Ok(())
}
pub(crate) fn documented_bag_slots(
    lua: &Lua,
    ordinary: Option<u8>,
    reagent: Option<u8>,
) -> SimResult<()> {
    let state: Table = lua.globals().raw_get("__selu_bag_slots")?;
    state.raw_set("documented_ordinary", ordinary)?;
    state.raw_set("documented_reagent", reagent)?;
    update_bag_slots(
        lua,
        state.raw_get("requested_ordinary")?,
        state.raw_get("requested_reagent")?,
    )
}
pub(crate) fn bag_slots(lua: &Lua) -> SimResult<BagSlots> {
    let state: Table = lua.globals().raw_get("__selu_bag_slots")?;
    Ok(BagSlots {
        ordinary: state.raw_get("ordinary")?,
        reagent: state.raw_get("reagent")?,
    })
}
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Unit {
    pub classification: String,
    pub creature_type: String,
    pub reaction: u8,
    pub dead: bool,
    pub guid: String,
    pub name: String,
    pub class: String,
    /// One-based shapeshift/stance bar index; zero means the normal form.
    pub form: u32,
    pub level: u32,
    pub health: u32,
    pub max_health: u32,
    pub power: u32,
    pub max_power: u32,
    pub auras: Vec<String>,
    pub combat: bool,
}
/// Deliver the original value before the host boundary renders it. The default
/// simulator sink is already represented by existing failure logging.
pub(crate) fn notify_native_error(lua: &Lua, error: Value) -> SimResult<bool> {
    let _profile = crate::profile::Span::new("errors.notify_handler");
    if !lua
        .globals()
        .raw_get::<bool>("__selu_sim_custom_error_handler")?
    {
        return Ok(false);
    }
    const ACTIVE: &str = "selu.notifying_native_error";
    if !lua
        .named_registry_value::<Option<bool>>(ACTIVE)?
        .unwrap_or(false)
    {
        lua.set_named_registry_value(ACTIVE, true)?;
        let handler = lua
            .globals()
            .raw_get::<mlua::Function>("__selu_sim_native_error_handler")?;
        // A broken handler cannot replace the original failure or recursively
        // report itself. The owning dispatch still receives its original error.
        let _ = handler.call::<()>(error);
        lua.set_named_registry_value(ACTIVE, false)?;
    }
    Ok(true)
}

pub(crate) fn install_restrictions(lua: &Lua, ctx: &crate::Ctx) -> SimResult<()> {
    let globals = lua.globals();
    globals.raw_set("__selu_sim_custom_error_handler", false)?;
    {
        let logs = ctx.logs.clone();
        let handler = lua.create_function(move |_, error: Value| {
            logs.lock()
                .unwrap()
                .push(format!("error: {}", value_string(&error)));
            Ok(())
        })?;
        globals.set("__selu_sim_native_error_handler", handler)?;
        globals.set(
            "seterrorhandler",
            lua.create_function(|lua, handler: mlua::Function| {
                lua.globals()
                    .raw_set("__selu_sim_custom_error_handler", true)?;
                lua.globals()
                    .set("__selu_sim_native_error_handler", handler)
            })?,
        )?;
        globals.set(
            "geterrorhandler",
            lua.create_function(move |lua, ()| {
                lua.globals()
                    .get::<mlua::Function>("__selu_sim_native_error_handler")
            })?,
        )?;
    }
    globals.set("__selu_sim_combat", false)?;
    globals.set(
        "InCombatLockdown",
        lua.create_function(|lua, ()| lua.globals().get::<bool>("__selu_sim_combat"))?,
    )?;
    globals.set(
        "canaccessvalue",
        lua.create_function(|_, value: Value| Ok(!matches!(value, Value::LightUserData(_))))?,
    )?;
    globals.set(
        "issecretvalue",
        lua.create_function(|_, value: Value| Ok(matches!(value, Value::LightUserData(_))))?,
    )?;

    Ok(())
}
impl Vm {
    pub fn edit_mode_preset(&self) -> SimResult<EditModePreset> {
        read_edit_mode_preset(&self.lua)
    }
    pub fn edit_mode_available(&self) -> bool {
        self.lua
            .named_registry_value::<Option<bool>>("selu.edit_mode_available")
            .ok()
            .flatten()
            .unwrap_or(false)
    }
    pub fn bag_slot_counts(&self) -> SimResult<BagSlots> {
        bag_slots(&self.lua)
    }
    /// Toggle combat and dispatch the matching transition event once.
    pub fn set_combat(&self, combat: bool) -> SimResult<()> {
        let old = self.lua.globals().get::<bool>("__selu_sim_combat")?;
        self.lua.globals().set("__selu_sim_combat", combat)?;
        let units: Table = self.lua.globals().get("__selu_world_units")?;
        if let Some(player) = units.raw_get::<Option<Table>>("player")? {
            player.raw_set("combat", combat)?;
        }
        if old != combat {
            self.fire(
                if combat {
                    "PLAYER_REGEN_DISABLED"
                } else {
                    "PLAYER_REGEN_ENABLED"
                },
                vec![],
            )?;
        }
        Ok(())
    }

    /// Make a named test value inaccessible to the simulated client predicates.
    pub fn set_secret_global(&self, name: &str) -> SimResult<()> {
        self.lua.globals().set(name, self.secret_value())
    }

    /// Opaque sentinel; only the secret predicates may inspect it.
    pub fn secret_value(&self) -> Value {
        Value::LightUserData(mlua::LightUserData(std::ptr::null_mut()))
    }

    pub fn set_unit(&self, token: &str, unit: Unit) -> SimResult<()> {
        if !valid_token(token) {
            return Err(mlua::Error::runtime("unsupported unit token"));
        }
        let units: Table = self.lua.globals().raw_get("__selu_world_units")?;
        let old: Option<Table> = units.raw_get(token)?;
        let next = self.lua.create_table()?;
        // Baseline players and older world snapshots predate GUIDs. Preserve
        // their name-based identity semantics instead of refusing the edit.
        let identity_changed = match &old {
            Some(old) => match old.raw_get::<Option<String>>("guid")? {
                Some(guid) if !guid.is_empty() && !unit.guid.is_empty() => guid != unit.guid,
                _ => old.raw_get::<String>("name")? != unit.name,
            },
            None => true,
        };
        let old_auras = old
            .as_ref()
            .map(|old| {
                old.raw_get::<Table>("auras")?
                    .sequence_values::<String>()
                    .collect::<mlua::Result<Vec<_>>>()
            })
            .transpose()?;
        let aura_changed = old_auras.as_ref() != Some(&unit.auras);
        next.set("classification", unit.classification)?;
        next.set("creature_type", unit.creature_type)?;
        next.set("reaction", unit.reaction)?;
        next.set("dead", unit.dead)?;
        next.set("guid", unit.guid)?;
        next.set("name", unit.name)?;
        next.set("class", unit.class)?;
        next.set("form", unit.form)?;
        next.set("level", unit.level)?;
        next.set("health", unit.health)?;
        next.set("max_health", unit.max_health)?;
        next.set("power", unit.power)?;
        next.set("max_power", unit.max_power)?;
        next.set("combat", unit.combat)?;
        next.set("auras", self.lua.create_sequence_from(unit.auras)?)?;
        let changes = [
            ("classification", "UNIT_CLASSIFICATION_CHANGED"),
            ("reaction", "UNIT_FACTION"),
            ("dead", "UNIT_FLAGS"),
            ("name", "UNIT_NAME_UPDATE"),
            ("level", "UNIT_LEVEL"),
            ("health", "UNIT_HEALTH"),
            ("max_health", "UNIT_MAXHEALTH"),
            ("power", "UNIT_POWER_UPDATE"),
            ("max_power", "UNIT_MAXPOWER"),
        ];
        if token.starts_with("nameplate") && identity_changed && old.is_some() {
            self.clear_unit(token)?;
        }
        units.set(token, next.clone())?;
        if token == "player"
            && old
                .as_ref()
                .map(|old| old.raw_get::<u32>("form").unwrap_or(0))
                != Some(unit.form)
        {
            self.fire("UPDATE_SHAPESHIFT_FORM", vec![])?;
        }
        if token == "player" {
            self.set_combat(unit.combat)?;
        }
        for (field, event) in changes {
            let changed = match &old {
                Some(old) => old.raw_get::<Value>(field)? != next.raw_get::<Value>(field)?,
                None => true,
            };
            if changed {
                let mut args = vec![Value::String(self.lua.create_string(token)?)];
                if field == "power" || field == "max_power" {
                    args.push(Value::String(self.lua.create_string("MANA")?));
                }
                self.fire(event, args)?;
            }
        }
        if (old.is_none() || identity_changed) && token.starts_with("nameplate") {
            let f: mlua::Function = self.lua.globals().get("__selu_add_plate")?;
            f.call::<()>(token)?;
            self.fire(
                "NAME_PLATE_UNIT_ADDED",
                vec![Value::String(self.lua.create_string(token)?)],
            )?;
        }
        if identity_changed {
            match token {
                "target" => {
                    self.fire("PLAYER_TARGET_CHANGED", vec![])?;
                }
                "focus" => {
                    self.fire("PLAYER_FOCUS_CHANGED", vec![])?;
                }
                "mouseover" => {
                    self.fire("UPDATE_MOUSEOVER_UNIT", vec![])?;
                }
                "pet" => {
                    self.fire(
                        "UNIT_PET",
                        vec![Value::String(self.lua.create_string("player")?)],
                    )?;
                }
                _ => {}
            }
        }
        if aura_changed {
            let update = self.lua.create_table()?;
            update.set("isFullUpdate", true)?;
            self.fire(
                "UNIT_AURA",
                vec![
                    Value::String(self.lua.create_string(token)?),
                    Value::Table(update),
                ],
            )?;
        }
        Ok(())
    }
    pub fn set_money(&self, copper: u64) -> SimResult<()> {
        self.lua.globals().set("__selu_world_money", copper)?;
        self.fire("PLAYER_MONEY", vec![])?;
        Ok(())
    }
    pub fn set_bag(&self, bag: i32, items: &[u32]) -> SimResult<()> {
        if items.iter().any(|id| *id > 0) {
            self.ctx().not_simulated(
                "scenario item metadata: unit stacks, synthetic names and generic client icons",
            );
        }
        let bags: Table = self.lua.globals().raw_get("__selu_world_bags")?;
        bags.set(bag, self.lua.create_sequence_from(items.iter().copied())?)?;
        self.fire("BAG_UPDATE", vec![Value::Integer(bag as i64)])?;
        self.fire("BAG_UPDATE_DELAYED", vec![])?;
        Ok(())
    }
}
fn valid_token(t: &str) -> bool {
    matches!(t, "player" | "target" | "focus" | "pet" | "mouseover")
        || t.strip_prefix("nameplate")
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|n| n > 0)
        || t.strip_prefix("party")
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|n| (1..=4).contains(&n))
        || t.strip_prefix("raid")
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|n| (1..=40).contains(&n))
}
pub(crate) fn install(lua: &Lua) -> SimResult<()> {
    configure_bag_slots(lua, None)?;
    crate::host_lua::load(lua,r#"
__selu_world_units = {}; __selu_world_money = 0; __selu_world_bags = {}
local units = __selu_world_units
__selu_vignettes = {}; __selu_plates = {}; __selu_zone = {zone='',subzone='',map=0}
function __selu_add_plate(u) local f=CreateFrame('Frame'); f.namePlateUnitToken=u; __selu_plates[u]=f end
C_NamePlate = {}
function C_NamePlate.GetNamePlateForUnit(u) return __selu_plates[u] end
function C_NamePlate.GetNamePlates() local r={} for _,f in pairs(__selu_plates) do r[#r+1]=f end return r end
C_VignetteInfo = {}
function C_VignetteInfo.GetVignettes() local r={} for g in pairs(__selu_vignettes) do r[#r+1]=g end table.sort(r); return r end
function C_VignetteInfo.GetVignetteInfo(g) return __selu_vignettes[g] end
function C_VignetteInfo.GetVignettePosition(g,m) local v=__selu_vignettes[g]; if v and v.map==m then return {x=v.x,y=v.y,GetXY=function(p)return p.x,p.y end},0 end end
function GetZoneText() return __selu_zone.zone end
function GetRealZoneText() return __selu_zone.zone end
function GetSubZoneText() return __selu_zone.subzone end
C_Map = {GetBestMapForUnit=function() return __selu_zone.map end}
function UnitClassification(u) return units[u] and units[u].classification or 'normal' end
function UnitCreatureType(u) return units[u] and units[u].creature_type end
function UnitReaction(u,other) return units[u] and units[u].reaction end
function UnitCanAttack(a,b) return units[b] ~= nil and units[b].reaction <= 3 end
function UnitIsEnemy(a,b) return UnitCanAttack(a,b) end
function UnitIsDead(u) return units[u] and units[u].dead or false end
function UnitIsDeadOrGhost(u) return UnitIsDead(u) end
function UnitGUID(u) return units[u] and units[u].guid end
function UnitExists(u) return units[u] ~= nil end
function UnitName(u) return units[u] and units[u].name end
local classes={WARRIOR={'Warrior',1},PALADIN={'Paladin',2},HUNTER={'Hunter',3},ROGUE={'Rogue',4},PRIEST={'Priest',5},DEATHKNIGHT={'Death Knight',6},SHAMAN={'Shaman',7},MAGE={'Mage',8},WARLOCK={'Warlock',9},MONK={'Monk',10},DRUID={'Druid',11},DEMONHUNTER={'Demon Hunter',12},EVOKER={'Evoker',13}}
function UnitClass(u) local c=units[u] and units[u].class; local info=classes[c]; if info then return info[1],c,info[2] end end
function GetShapeshiftForm() return units.player and units.player.form or 0 end
function UnitLevel(u) return units[u] and units[u].level or 0 end
function UnitHealth(u) return units[u] and units[u].health or 0 end
function UnitHealthMax(u) return units[u] and units[u].max_health or 0 end
function UnitPower(u) return units[u] and units[u].power or 0 end
function UnitPowerMax(u) return units[u] and units[u].max_power or 0 end
function UnitAffectingCombat(u) if u=="player" then return InCombatLockdown() end; return units[u] and units[u].combat or false end
function GetMoney() return __selu_world_money end
C_Container = {}
function C_Container.GetContainerNumSlots(b) return #(__selu_world_bags[b] or {}) end
function C_Container.GetContainerItemID(b,s) return (__selu_world_bags[b] or {})[s] end
local randomState = 1
function math.randomseed(seed) randomState=math.floor(seed)%2147483647; if randomState==0 then randomState=1 end end
function math.random(a,b)
 randomState=(randomState*16807)%2147483647
 local n=(randomState-1)/2147483647
 if a==nil then return n end
 if b==nil then b=a; a=1 end
 assert(a<=b, 'interval is empty'); return a+math.floor(n*(b-a+1))
end
function UnitAura(u,i) return units[u] and units[u].auras[i] end
function time() return 946684800 + math.floor(GetTime()) end
function date(format, timestamp)
 local t = timestamp or time()
 local days = math.floor(t / 86400)
 local y = 1970
 local function leap(n) return n%4==0 and (n%100~=0 or n%400==0) end
 for _=1,10000 do local n=leap(y) and 366 or 365; if days<n then break end; days=days-n; y=y+1 end
 local months={31,leap(y) and 29 or 28,31,30,31,30,31,31,30,31,30,31}; local m=1
 for _=1,11 do if days<months[m] then break end; days=days-months[m]; m=m+1 end
 local v={year=y,month=m,day=days+1,hour=math.floor(t/3600)%24,min=math.floor(t/60)%60,sec=t%60,isdst=false}
 if format=='*t' or format=='!*t' then return v end
 local fields={Y=string.format('%04d',y),m=string.format('%02d',m),d=string.format('%02d',v.day),H=string.format('%02d',v.hour),M=string.format('%02d',v.min),S=string.format('%02d',v.sec),['%']='%'}
 return (format or '%Y-%m-%d %H:%M:%S'):gsub('^!',''):gsub('%%(.)',function(k) assert(fields[k], 'unsupported date format'); return fields[k] end)
end
os = {time=time,date=date,clock=GetTime,difftime=function(a,b)return a-b end}
-- No host filesystem, environment, process or native-module escape paths.
io=nil; package=nil; require=nil; loadfile=nil; dofile=nil
"#).exec()
}

impl Default for Unit {
    fn default() -> Self {
        Self {
            name: "Rare creature".into(),
            class: "WARRIOR".into(),
            form: 0,
            level: 60,
            health: 100,
            max_health: 100,
            power: 0,
            max_power: 0,
            auras: vec![],
            combat: false,
            classification: "normal".into(),
            creature_type: "Beast".into(),
            reaction: 3,
            dead: false,
            guid: "Creature-0-0-0-0-1-1".into(),
        }
    }
}

/// Shapes sourced from read-only wow-ui-source forever 1.60.1.70790,
/// Interface/AddOns/Blizzard_APIDocumentationGenerated/VignetteInfoDocumentation.lua.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vignette {
    pub guid: String,
    pub object_guid: String,
    pub name: String,
    pub map: u32,
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scenario {
    pub edit_mode_preset: EditModePreset,
    /// Omitted counts select the session product's simulator defaults.
    pub bag_slots: Option<u8>,
    pub reagent_bag_slots: Option<u8>,
    pub units: std::collections::BTreeMap<String, Unit>,
    pub vignettes: Vec<Vignette>,
    pub combat: bool,
    pub money: u64,
    #[serde(with = "bag_keys")]
    pub bags: std::collections::BTreeMap<i32, Vec<u32>>,
    pub zone: String,
    pub subzone: String,
    pub map: u32,
}
impl Scenario {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        toml::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let data = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, data).map_err(|e| e.to_string())
    }
    pub fn apply(&self, vm: &Vm) -> SimResult<()> {
        // Validate before mutation, including values which TOML permits but the game does not.
        for (token, u) in &self.units {
            if !valid_token(token)
                || !["normal", "elite", "rare", "rareelite", "worldboss", "minus"]
                    .contains(&u.classification.as_str())
                || !(1..=8).contains(&u.reaction)
                || u.health > u.max_health
            {
                return Err(mlua::Error::runtime("invalid scenario unit"));
            }
        }
        for v in &self.vignettes {
            if !v.x.is_finite()
                || !v.y.is_finite()
                || !(0.0..=1.0).contains(&v.x)
                || !(0.0..=1.0).contains(&v.y)
            {
                return Err(mlua::Error::runtime("invalid vignette position"));
            }
        }
        let units: Table = vm.lua.globals().get("__selu_world_units")?;
        let tokens = units
            .clone()
            .pairs::<String, Table>()
            .map(|p| p.map(|p| p.0))
            .collect::<mlua::Result<Vec<_>>>()?;
        for t in tokens {
            if !self.units.contains_key(&t) {
                vm.clear_unit(&t)?;
            }
        }
        for (t, u) in &self.units {
            vm.set_unit(t, u.clone())?;
        }
        vm.set_combat(self.combat)?;
        vm.lua
            .globals()
            .raw_set("__selu_edit_mode_preset", self.edit_mode_preset.key())?;
        if self.edit_mode_preset != EditModePreset::Modern
            && vm
                .lua
                .named_registry_value::<Option<bool>>("selu.edit_mode_available")?
                == Some(false)
        {
            vm.ctx()
                .not_simulated("Edit Mode preset ignored: this UI source does not ship Edit Mode");
        }
        update_bag_slots(&vm.lua, self.bag_slots, self.reagent_bag_slots)?;
        if vm.lua.globals().get::<u64>("__selu_world_money")? != self.money {
            vm.set_money(self.money)?;
        }
        let bags: Table = vm.lua.globals().get("__selu_world_bags")?;
        let old = bags
            .clone()
            .pairs::<i32, Table>()
            .map(|p| p.map(|p| p.0))
            .collect::<mlua::Result<Vec<_>>>()?;
        for b in old {
            if !self.bags.contains_key(&b) {
                vm.set_bag(b, &[])?;
            }
        }
        for (b, items) in &self.bags {
            let prior: Option<Table> = bags.get(*b)?;
            let prior = prior
                .map(|t| t.sequence_values::<u32>().collect::<mlua::Result<Vec<_>>>())
                .transpose()?
                .unwrap_or_default();
            if prior != *items {
                vm.set_bag(*b, items)?;
            }
        }
        let zone: Table = vm.lua.globals().get("__selu_zone")?;
        let map_changed = zone.get::<u32>("map")? != self.map;
        let zone_changed = zone.get::<String>("zone")? != self.zone;
        let sub_changed = zone.get::<String>("subzone")? != self.subzone;
        zone.set("zone", self.zone.clone())?;
        zone.set("subzone", self.subzone.clone())?;
        zone.set("map", self.map)?;
        // MapDocumentation.lua: ZONE_CHANGED*, no payload.
        if map_changed {
            vm.fire("ZONE_CHANGED_NEW_AREA", vec![])?;
        }
        if zone_changed {
            vm.fire("ZONE_CHANGED", vec![])?;
        }
        if sub_changed {
            vm.fire("ZONE_CHANGED_INDOORS", vec![])?;
        }
        let list: Table = vm.lua.globals().get("__selu_vignettes")?;
        let old = list
            .clone()
            .pairs::<String, Table>()
            .map(|p| p.map(|p| p.0))
            .collect::<mlua::Result<Vec<_>>>()?;
        for g in old {
            if !self.vignettes.iter().any(|v| v.guid == g) {
                list.set(g.clone(), Value::Nil)?;
                vm.fire(
                    "VIGNETTE_MINIMAP_UPDATED",
                    vec![
                        Value::String(vm.lua.create_string(g)?),
                        Value::Boolean(false),
                    ],
                )?;
                vm.fire("VIGNETTES_UPDATED", vec![])?;
            }
        }
        for v in &self.vignettes {
            let next = vm.lua.create_table()?;
            for (k, value) in [
                ("vignetteGUID", &v.guid),
                ("objectGUID", &v.object_guid),
                ("name", &v.name),
            ] {
                next.set(k, value.clone())?;
            }
            for k in ["isDead", "zoneInfiniteAOI", "isUnique", "inFogOfWar"] {
                next.set(k, false)?;
            }
            for k in ["onWorldMap", "onMinimap", "hasTooltip"] {
                next.set(k, true)?;
            }
            next.set("atlasName", "VignetteKill")?;
            for k in ["vignetteID", "type", "rewardQuestID"] {
                next.set(k, 0)?;
            }
            next.set("map", v.map)?;
            next.set("x", v.x)?;
            next.set("y", v.y)?;
            let prior: Option<Table> = list.get(v.guid.clone())?;
            let changed = prior
                .map(|t| -> SimResult<bool> {
                    for pair in next.clone().pairs::<String, Value>() {
                        let (k, val) = pair?;
                        if t.get::<Value>(k)? != val {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                })
                .transpose()?
                .unwrap_or(true);
            if changed {
                list.set(v.guid.clone(), next)?;
                vm.fire(
                    "VIGNETTE_MINIMAP_UPDATED",
                    vec![
                        Value::String(vm.lua.create_string(&v.guid)?),
                        Value::Boolean(true),
                    ],
                )?;
                vm.fire("VIGNETTES_UPDATED", vec![])?;
            }
        }
        Ok(())
    }
}
impl Vm {
    pub fn clear_unit(&self, token: &str) -> SimResult<()> {
        let units: Table = self.lua.globals().get("__selu_world_units")?;
        if units.get::<Option<Table>>(token)?.is_none() {
            return Ok(());
        }
        // NamePlateManagerDocumentation.lua: payload is unitToken.
        if token.starts_with("nameplate") {
            self.fire(
                "NAME_PLATE_UNIT_REMOVED",
                vec![Value::String(self.lua.create_string(token)?)],
            )?;
            let plates: Table = self.lua.globals().get("__selu_plates")?;
            if let Some(p) = plates.get::<Option<Table>>(token)? {
                p.call_method::<()>("Hide", ())?;
            }
            plates.set(token, Value::Nil)?;
        }
        units.set(token, Value::Nil)?;
        let event = match token {
            "target" => Some("PLAYER_TARGET_CHANGED"),
            "focus" => Some("PLAYER_FOCUS_CHANGED"),
            "mouseover" => Some("UPDATE_MOUSEOVER_UNIT"),
            _ => None,
        };
        if let Some(e) = event {
            self.fire(e, vec![])?;
        }
        Ok(())
    }
}

impl Scenario {
    pub fn capture(vm: &Vm) -> SimResult<Self> {
        let globals = vm.lua.globals();
        let units: Table = globals.get("__selu_world_units")?;
        let mut result = Self {
            edit_mode_preset: vm.edit_mode_preset()?,
            ..Self::default()
        };
        let slots = vm.bag_slot_counts()?;
        result.bag_slots = Some(slots.ordinary);
        result.reagent_bag_slots = Some(slots.reagent);
        for pair in units.pairs::<String, Table>() {
            let (t, u) = pair?;
            result.units.insert(
                t,
                Unit {
                    name: u.get("name")?,
                    class: u.get("class")?,
                    form: u.get::<Option<u32>>("form")?.unwrap_or(0),
                    level: u.get("level")?,
                    health: u.get("health")?,
                    max_health: u.get("max_health")?,
                    power: u.get("power")?,
                    max_power: u.get("max_power")?,
                    auras: u
                        .get::<Table>("auras")?
                        .sequence_values()
                        .collect::<mlua::Result<_>>()?,
                    combat: u.get("combat")?,
                    classification: u
                        .get::<Option<String>>("classification")?
                        .unwrap_or_else(|| "normal".into()),
                    creature_type: u
                        .get::<Option<String>>("creature_type")?
                        .unwrap_or_else(|| "Humanoid".into()),
                    reaction: u.get::<Option<u8>>("reaction")?.unwrap_or(5),
                    dead: u.get::<Option<bool>>("dead")?.unwrap_or(false),
                    guid: u.get::<Option<String>>("guid")?.unwrap_or_default(),
                },
            );
        }
        result.combat = globals.get("__selu_sim_combat")?;
        result.money = globals.get("__selu_world_money")?;
        let bags: Table = globals.get("__selu_world_bags")?;
        for pair in bags.pairs::<i32, Table>() {
            let (b, t) = pair?;
            result
                .bags
                .insert(b, t.sequence_values().collect::<mlua::Result<_>>()?);
        }
        let zone: Table = globals.get("__selu_zone")?;
        result.zone = zone.get("zone")?;
        result.subzone = zone.get("subzone")?;
        result.map = zone.get("map")?;
        let list: Table = globals.get("__selu_vignettes")?;
        for pair in list.pairs::<String, Table>() {
            let (guid, v) = pair?;
            result.vignettes.push(Vignette {
                guid,
                object_guid: v.get("objectGUID")?,
                name: v.get("name")?,
                map: v.get("map")?,
                x: v.get("x")?,
                y: v.get("y")?,
            });
        }
        result.vignettes.sort_by(|a, b| a.guid.cmp(&b.guid));
        Ok(result)
    }
}

// TOML table keys are strings; retain numeric bag IDs in the runtime model.
mod bag_keys {
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;
    pub fn serialize<S: serde::Serializer>(
        bags: &BTreeMap<i32, Vec<u32>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        bags.iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<_, _>>()
            .serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<i32, Vec<u32>>, D::Error> {
        BTreeMap::<String, Vec<u32>>::deserialize(deserializer)?
            .into_iter()
            .map(|(k, v)| Ok((k.parse().map_err(serde::de::Error::custom)?, v)))
            .collect()
    }
}

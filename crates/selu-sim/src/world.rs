//! Session-local world data. Event payloads follow the target manifests.
use crate::{SimResult, session::Vm, value_string};
use mlua::{Lua, Table, Value};
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Unit {
    pub name: String,
    pub class: String,
    pub level: u32,
    pub health: u32,
    pub max_health: u32,
    pub power: u32,
    pub max_power: u32,
    pub auras: Vec<String>,
    pub combat: bool,
}
pub(crate) fn install_restrictions(lua: &Lua, ctx: &crate::Ctx) -> SimResult<()> {
    let globals = lua.globals();
    {
        let logs = ctx.logs.clone();
        let handler = lua.create_function(move |_, error: Value| {
            logs.lock()
                .unwrap()
                .push(format!("error: {}", value_string(&error)));
            Ok(())
        })?;
        globals.set(
            "geterrorhandler",
            lua.create_function(move |_, ()| Ok(handler.clone()))?,
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
        let identity_changed = old
            .as_ref()
            .map(|old| old.raw_get::<String>("name"))
            .transpose()?
            .as_deref()
            != Some(unit.name.as_str());
        let old_auras = old
            .as_ref()
            .map(|old| {
                old.raw_get::<Table>("auras")?
                    .sequence_values::<String>()
                    .collect::<mlua::Result<Vec<_>>>()
            })
            .transpose()?;
        let aura_changed = old_auras.as_ref() != Some(&unit.auras);
        next.set("name", unit.name)?;
        next.set("class", unit.class)?;
        next.set("level", unit.level)?;
        next.set("health", unit.health)?;
        next.set("max_health", unit.max_health)?;
        next.set("power", unit.power)?;
        next.set("max_power", unit.max_power)?;
        next.set("combat", unit.combat)?;
        next.set("auras", self.lua.create_sequence_from(unit.auras)?)?;
        let changes = [
            ("name", "UNIT_NAME_UPDATE"),
            ("level", "UNIT_LEVEL"),
            ("health", "UNIT_HEALTH"),
            ("max_health", "UNIT_MAXHEALTH"),
            ("power", "UNIT_POWER_UPDATE"),
            ("max_power", "UNIT_MAXPOWER"),
        ];
        units.set(token, next.clone())?;
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
        if identity_changed {
            match token {
                "target" => {
                    self.fire("PLAYER_TARGET_CHANGED", vec![])?;
                }
                "focus" => {
                    self.fire("PLAYER_FOCUS_CHANGED", vec![])?;
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
        let bags: Table = self.lua.globals().raw_get("__selu_world_bags")?;
        bags.set(bag, self.lua.create_sequence_from(items.iter().copied())?)?;
        self.fire("BAG_UPDATE", vec![Value::Integer(bag as i64)])?;
        self.fire("BAG_UPDATE_DELAYED", vec![])?;
        Ok(())
    }
}
fn valid_token(t: &str) -> bool {
    matches!(t, "player" | "target" | "focus" | "pet")
        || t.strip_prefix("party")
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|n| (1..=4).contains(&n))
        || t.strip_prefix("raid")
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|n| (1..=40).contains(&n))
}
pub(crate) fn install(lua: &Lua) -> SimResult<()> {
    // Table::metatable bypasses __metatable, without exposing the table
    // itself. Lua's native tostring still invokes the metamethod in Lua.
    lua.globals().set(
        "__selu_has_tostring",
        lua.create_function(|_, table: Table| {
            Ok(table
                .metatable()
                .map(|mt| mt.raw_get::<Value>("__tostring"))
                .transpose()?
                .is_some_and(|v| !matches!(v, Value::Nil)))
        })?,
    )?;
    lua.load(r#"
__selu_world_units = {}; __selu_world_money = 0; __selu_world_bags = {}
local units = __selu_world_units
function UnitExists(u) return units[u] ~= nil end
function UnitName(u) return units[u] and units[u].name end
local classes={WARRIOR={'Warrior',1},PALADIN={'Paladin',2},HUNTER={'Hunter',3},ROGUE={'Rogue',4},PRIEST={'Priest',5},DEATHKNIGHT={'Death Knight',6},SHAMAN={'Shaman',7},MAGE={'Mage',8},WARLOCK={'Warlock',9},MONK={'Monk',10},DRUID={'Druid',11},DEMONHUNTER={'Demon Hunter',12},EVOKER={'Evoker',13}}
function UnitClass(u) local c=units[u] and units[u].class; local info=classes[c]; if info then return info[1],c,info[2] end end
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
local native_tostring = tostring
function tostring(value)
 local kind=type(value)
 if kind=='function' or kind=='thread' then return kind end
 if kind=='table' then
  if not __selu_has_tostring(value) then return 'table' end
 end
 return native_tostring(value)
end
"#).exec()
}

//! VM-independent SavedVariables snapshots. Only serializable Lua data survives logout.
use crate::{Sim, SimResult};
use mlua::{Lua, Value};
use std::collections::{BTreeMap, HashSet};
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SavedValue {
    Nil,
    Boolean(bool),
    Number(f64),
    String(Vec<u8>),
    Table(Vec<(SavedValue, SavedValue)>),
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SavedSnapshot(pub BTreeMap<String, SavedValue>);
pub(crate) fn capture(v: Value, active: &mut HashSet<usize>) -> SimResult<SavedValue> {
    Ok(match v {
        Value::Nil => SavedValue::Nil,
        Value::Boolean(b) => SavedValue::Boolean(b),
        Value::Integer(n) => SavedValue::Number(n as f64),
        Value::Number(n) if n.is_finite() => SavedValue::Number(n),
        Value::String(s) => SavedValue::String(s.as_bytes().to_vec()),
        Value::Table(t) => {
            let id = t.to_pointer() as usize;
            if !active.insert(id) {
                return Err(mlua::Error::RuntimeError("cyclic saved data".into()));
            }
            let mut pairs = Vec::new();
            for pair in t.pairs::<Value, Value>() {
                let (k, v) = pair?;
                if !matches!(
                    k,
                    Value::String(_) | Value::Number(_) | Value::Integer(_) | Value::Boolean(_)
                ) {
                    return Err(mlua::Error::RuntimeError("invalid saved table key".into()));
                }
                pairs.push((capture(k, active)?, capture(v, active)?));
            }
            active.remove(&id);
            pairs.sort_by_key(|(k, _)| format!("{k:?}"));
            SavedValue::Table(pairs)
        }
        _ => {
            return Err(mlua::Error::RuntimeError(
                "saved data contains an unsupported value".into(),
            ));
        }
    })
}
pub(crate) fn restore(lua: &Lua, v: &SavedValue) -> SimResult<Value> {
    Ok(match v {
        SavedValue::Nil => Value::Nil,
        SavedValue::Boolean(b) => Value::Boolean(*b),
        SavedValue::Number(n) => Value::Number(*n),
        SavedValue::String(s) => Value::String(lua.create_string(s)?),
        SavedValue::Table(pairs) => {
            let t = lua.create_table()?;
            for (k, v) in pairs {
                t.raw_set(restore(lua, k)?, restore(lua, v)?)?;
            }
            Value::Table(t)
        }
    })
}
impl Sim {
    pub fn logout_saved(&self, names: &[&str]) -> SimResult<SavedSnapshot> {
        self.fire("PLAYER_LOGOUT", vec![])?;
        let mut saved = BTreeMap::new();
        for name in names {
            saved.insert(
                (*name).into(),
                capture(self.lua.globals().raw_get(*name)?, &mut HashSet::new())?,
            );
        }
        Ok(SavedSnapshot(saved))
    }
    pub fn login_saved(
        addon: &str,
        files: &[(String, String)],
        saved: &SavedSnapshot,
    ) -> SimResult<Self> {
        let sim = Self::new()?;
        for (name, value) in &saved.0 {
            sim.lua
                .globals()
                .raw_set(name.as_str(), restore(&sim.lua, value)?)?;
        }
        sim.load_addon(addon, files)?;
        Ok(sim)
    }
}
/// Account-wide data is shared; each character has an independent persisted tree.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SavedSessions {
    pub account: SavedSnapshot,
    pub characters: BTreeMap<String, SavedSnapshot>,
}
impl Sim {
    pub fn login_character(
        addon: &str,
        files: &[(String, String)],
        sessions: &SavedSessions,
        character: &str,
    ) -> SimResult<Self> {
        let mut snapshot = sessions.account.clone();
        if let Some(saved) = sessions.characters.get(character) {
            snapshot.0.extend(saved.0.clone());
        }
        Self::login_saved(addon, files, &snapshot)
    }
    pub fn logout_character(
        &self,
        sessions: &mut SavedSessions,
        character: &str,
        account_names: &[&str],
        character_names: &[&str],
    ) -> SimResult<()> {
        if account_names
            .iter()
            .any(|name| character_names.contains(name))
        {
            return Err(mlua::Error::RuntimeError(
                "a saved global cannot have both account and character scope".into(),
            ));
        }
        let names = account_names
            .iter()
            .chain(character_names)
            .copied()
            .collect::<Vec<_>>();
        let mut snapshot = self.logout_saved(&names)?;
        let mut account = SavedSnapshot::default();
        for name in account_names {
            account.0.insert(
                (*name).into(),
                snapshot.0.remove(*name).expect("captured saved name"),
            );
        }
        sessions.account.0.extend(account.0);
        sessions
            .characters
            .entry(character.into())
            .or_default()
            .0
            .extend(snapshot.0);
        Ok(())
    }
}

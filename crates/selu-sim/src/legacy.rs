//! Shared engine identities for undocumented enums and their legacy aliases.
//! Only runtime client values and simulator-generated identities live here.
use mlua::{Lua, Table, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
struct Catalog {
    values: BTreeMap<String, i64>,
    counts: BTreeMap<String, String>,
    families: BTreeMap<String, BTreeSet<i64>>,
    enum_values: BTreeMap<(String, String), i64>,
}
fn normalized(name: &str) -> String {
    name.split('_')
        .map(|part| if part == "ERR" { "ERROR" } else { part })
        .flat_map(str::bytes)
        .filter(u8::is_ascii_alphanumeric)
        .map(|b| char::from(b.to_ascii_uppercase()))
        .collect()
}
fn hash(name: &str) -> i64 {
    let mut hash = 2166136261u32;
    for byte in name.bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(16777619);
    }
    -1 - i64::from(hash & 0x3fff_ffff)
}
fn count_prefix<'a>(key: &str, names: impl Iterator<Item = &'a String>) -> String {
    let names: Vec<_> = names.collect();
    let count = key.trim_start_matches("NUM_");
    let stem = count
        .strip_suffix("IES")
        .map(|p| format!("{p}Y"))
        .unwrap_or_else(|| count.strip_suffix('S').unwrap_or(count).to_owned());
    let mut candidate = stem.as_str();
    loop {
        if names
            .iter()
            .any(|name| name.starts_with(&format!("{candidate}_")))
        {
            return normalized(candidate);
        }
        let Some((prefix, _)) = candidate.rsplit_once('_') else {
            break;
        };
        if prefix == "LE" {
            break;
        }
        candidate = prefix;
    }
    normalized(&stem)
}
impl Catalog {
    fn assign(&mut self, key: &str) -> i64 {
        self.assign_in(key, None)
    }
    fn assign_in(&mut self, key: &str, scope: Option<&str>) -> i64 {
        let key = normalized(key);
        if let Some(value) = self.values.get(&key) {
            return *value;
        }
        let family = self
            .families
            .keys()
            .filter(|prefix| {
                scope.map_or_else(
                    || key.starts_with(prefix.as_str()) && key.len() > prefix.len(),
                    |s| prefix.as_str() == s,
                )
            })
            .max_by_key(|s| s.len())
            .cloned();
        let mut value = if family.is_some() { 1 } else { hash(&key) };
        while family.as_ref().map_or_else(
            || self.values.values().any(|v| *v == value),
            |f| self.families[f].contains(&value),
        ) {
            value += if family.is_some() { 1 } else { -1 };
        }
        for (family, members) in &mut self.families {
            if scope.map_or_else(
                || key.starts_with(family.as_str()) && key.len() > family.len(),
                |s| family == s,
            ) {
                members.insert(value);
            }
        }
        self.values.insert(key, value);
        value
    }
}
/// Discover counted families before executing source. Synthetic members are
/// dense and registered in sorted order; actual runtime enum values win.
pub(crate) fn prepare(lua: &Lua, names: Vec<String>) -> mlua::Result<()> {
    let _profile = crate::profile::Span::new("source.legacy_refs");
    let names: BTreeSet<_> = names.into_iter().collect();
    let mut catalog = Catalog::default();
    for count in names.iter().filter(|name| name.starts_with("NUM_LE_")) {
        let prefix = count_prefix(count, names.iter());
        catalog.families.entry(prefix.clone()).or_default();
        catalog.counts.insert(count.clone(), prefix);
    }
    // Existing enum identities (including own-client documented values) are
    // preserved. Index once rather than comparing every field to every alias.
    if let Some(enums) = lua.globals().raw_get::<Option<Table>>("Enum")? {
        for pair in enums.pairs::<String, Table>() {
            let (table, fields) = pair?;
            if table.ends_with("Meta") {
                continue;
            }
            for field in fields.pairs::<String, Value>() {
                let (member, value) = field?;
                let value = match value {
                    Value::Integer(n) => Some(n),
                    Value::Number(n) if n.is_finite() && n.fract() == 0. => Some(n as i64),
                    _ => None,
                };
                if let Some(value) = value {
                    catalog
                        .enum_values
                        .insert((table.clone(), member.clone()), value);
                    let key = normalized(&format!("LE_{table}_{member}"));
                    for (family, members) in &mut catalog.families {
                        if normalized(&format!("LE_{table}")) == *family {
                            members.insert(value);
                        }
                    }
                    catalog.values.insert(key, value);
                }
            }
        }
    }
    for key in names.iter().filter(|name| name.starts_with("LE_")) {
        catalog.assign(key);
    }
    lua.set_app_data(catalog);
    Ok(())
}
pub(crate) fn global(lua: &Lua, key: &str, ctx: &crate::Ctx) -> mlua::Result<Option<Value>> {
    if !key.starts_with("LE_") && !key.starts_with("NUM_LE_") {
        return Ok(None);
    }
    let Some(mut catalog) = lua.app_data_mut::<Catalog>() else {
        return Ok(None);
    };
    let value = if key.starts_with("NUM_LE_") {
        let prefix = if let Some(prefix) = catalog.counts.get(key) {
            prefix.clone()
        } else {
            let raw = key.trim_start_matches("NUM_").trim_end_matches('S');
            let prefix = normalized(raw);
            let members = catalog
                .values
                .iter()
                .filter(|(name, _)| name.starts_with(&prefix) && name.len() > prefix.len())
                .map(|(_, value)| *value)
                .collect();
            catalog.families.insert(prefix.clone(), members);
            catalog.counts.insert(key.into(), prefix.clone());
            prefix
        };
        catalog.families[&prefix].len() as i64
    } else {
        catalog.assign(key)
    };
    ctx.not_simulated(&format!("legacy client identity: {key}"));
    Ok(Some(Value::Integer(value)))
}
pub(crate) fn enum_member(lua: &Lua, table: &str, member: &str) -> Option<i64> {
    let mut catalog = lua.app_data_mut::<Catalog>()?;
    let pair = (table.to_owned(), member.to_owned());
    if let Some(value) = catalog.enum_values.get(&pair) {
        return Some(*value);
    }
    let key = normalized(&format!("LE_{table}_{member}"));
    let scope = normalized(&format!("LE_{table}"));
    let mut value = catalog.assign_in(&key, Some(&scope));
    if catalog
        .enum_values
        .iter()
        .any(|((owner, _), v)| owner == table && *v == value)
    {
        let counted = catalog.families.contains_key(&scope);
        value = if counted {
            1
        } else {
            hash(&format!("{table}:{member}"))
        };
        while if counted {
            catalog.families[&scope].contains(&value)
        } else {
            catalog.values.values().any(|v| *v == value)
        } {
            value += if counted { 1 } else { -1 };
        }
        catalog
            .values
            .insert(format!("!enum:{table}:{member}"), value);
        for (family, members) in &mut catalog.families {
            if family == &scope {
                members.insert(value);
            }
        }
    }
    catalog.enum_values.insert(pair, value);
    Some(value)
}
pub(crate) fn native(lua: &Lua, ty: &str) -> Option<i64> {
    let catalog = lua.app_data_ref::<Catalog>()?;
    let wanted = normalized(&format!(
        "LE_{}",
        ty.trim_start_matches("Enum.").trim_start_matches("LE_")
    ));
    if let Some(value) = catalog
        .enum_values
        .iter()
        .filter(|((table, _), _)| normalized(&format!("LE_{table}")) == wanted)
        .map(|(_, value)| *value)
        .min()
    {
        return Some(value);
    }
    if let Some(members) = catalog.families.get(&wanted) {
        return members.first().copied();
    }
    catalog
        .values
        .range(wanted.clone()..)
        .take_while(|(key, _)| key.starts_with(&wanted))
        .map(|(_, value)| *value)
        .min()
}
#[cfg(test)]
mod tests {
    #[test]
    fn aliases_counts_and_native_results_share_session_identities() {
        for reverse in [false, true] {
            let s = crate::Session::new(Default::default()).unwrap();
            s.eval("Enum={SelfFamily={}}").unwrap();
            let mut names = vec![
                "LE_SELF_FAMILY_FIRST".into(),
                "LE_SELF_FAMILY_SECOND".into(),
                "NUM_LE_SELF_FAMILYS".into(),
            ];
            if reverse {
                names.reverse();
            }
            super::prepare(s.lua(), names).unwrap();
            s.eval("assert(LE_SELF_FAMILY_FIRST==1 and LE_SELF_FAMILY_SECOND==2 and NUM_LE_SELF_FAMILYS==2);assert(LE_SELF_FAMILY_THIRD==3 and NUM_LE_SELF_FAMILYS==3)").unwrap();
            assert_eq!(super::enum_member(s.lua(), "SelfFamily", "First"), Some(1));
            assert_eq!(super::enum_member(s.lua(), "SelfFamily", "Fourth"), Some(4));
            s.eval("assert(LE_SELF_FAMILY_FOURTH==4 and NUM_LE_SELF_FAMILYS==4)")
                .unwrap();
            assert_ne!(super::enum_member(s.lua(), "SelfFamily", "First_"), Some(1));
            let schema =
                serde_json::from_value(serde_json::json!({"name":"SelfFamily","type":"enum"}))
                    .unwrap();
            let schemas = std::sync::Arc::new(std::collections::HashMap::from([(
                "Enum.SelfFamily".into(),
                schema,
            )]));
            for ty in ["Enum.SelfFamily", "LE_SELF_FAMILY"] {
                assert_eq!(
                    crate::native_defaults::value(s.lua(), ty, &schemas, s.sim().ctx(), 0).unwrap(),
                    mlua::Value::Integer(1)
                );
            }
            let other = crate::Session::new(Default::default()).unwrap();
            assert!(super::enum_member(other.lua(), "SelfFamily", "First").is_none());
        }
    }
    #[test]
    fn documented_values_and_non_counted_unknowns_remain_stable() {
        let s = crate::Session::new(Default::default()).unwrap();
        s.eval("Enum={SelfFamily={First=1},SelfFamilyOther={Other=2}};LE_SELF_DEFINED=23")
            .unwrap();
        super::prepare(
            s.lua(),
            vec![
                "LE_SELF_FAMILY_SECOND".into(),
                "NUM_LE_SELF_FAMILYS".into(),
                "LE_SELF_UNDOCUMENTED".into(),
            ],
        )
        .unwrap();
        s.eval("assert(LE_SELF_FAMILY_FIRST==1 and LE_SELF_FAMILY_SECOND==2 and NUM_LE_SELF_FAMILYS==2 and LE_SELF_DEFINED==23);local first=LE_SELF_UNDOCUMENTED;assert(first==LE_SELF_UNDOCUMENTED and first~=LE_SELF_OTHER)").unwrap();
        super::enum_member(s.lua(), "SelfFamilyOther", "Next").unwrap();
        s.eval("assert(NUM_LE_SELF_FAMILYS==2)").unwrap();
        assert_eq!(
            s.logs()
                .iter()
                .filter(|s| s.contains("legacy client identity: LE_SELF_UNDOCUMENTED"))
                .count(),
            1
        );
        let value = super::enum_member(s.lua(), "OtherFamily", "First").unwrap();
        assert_eq!(
            super::global(s.lua(), "LE_OTHER_FAMILY_FIRST", s.sim().ctx()).unwrap(),
            Some(mlua::Value::Integer(value))
        );
    }
}

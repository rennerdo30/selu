pub mod coverage;
pub mod factory;
pub mod xml;
use mlua::{Lua, LuaOptions, StdLib, Table, Value};
use selu_manifest::{Declaration, EventEntry, EventParam, Evidence, FunctionEntry, Manifest};
use std::collections::{BTreeMap, BTreeSet};
pub mod source;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Execute data-only chunks in an empty environment, with instruction and memory limits.
pub fn read_documentation(source: &str) -> Result<Vec<Declaration>> {
    read_documents(&[source.to_owned()])
}

pub fn read_documents(sources: &[String]) -> Result<Vec<Declaration>> {
    let lua = Lua::new_with(StdLib::NONE, LuaOptions::default())?;
    lua.set_memory_limit(32 * 1024 * 1024)?;
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(1_000_000),
        |_, _| {
            Err(mlua::Error::RuntimeError(
                "documentation instruction limit exceeded".into(),
            ))
        },
    );
    let env = lua.create_table()?;
    let docs = lua.create_table()?;
    let captured = lua.create_table()?;
    let sink = captured.clone();
    let enums = lua.create_table()?;
    env.set("Enum", enums.clone())?;
    let constants = lua.create_table()?;
    env.set("Constants", constants.clone())?;
    // Engine-only values remain symbolic; documented enums are resolved on retry.
    namespace_fallback(&lua, &constants, "Constants")?;
    docs.set(
        "AddDocumentationTable",
        lua.create_function(move |_lua, (_self, table): (Table, Table)| {
            if let Some(tables) = table.get::<Option<Table>>("Tables")? {
                for t in tables.sequence_values::<Table>() {
                    let t = t?;
                    if t.get::<Option<String>>("Type")?.as_deref() == Some("Enumeration") {
                        let values = _lua.create_table()?;
                        if let Some(fields) = t.get::<Option<Table>>("Fields")? {
                            for f in fields.sequence_values::<Table>() {
                                let f = f?;
                                values
                                    .set(f.get::<String>("Name")?, f.get::<Value>("EnumValue")?)?;
                            }
                        }
                        enums.set(t.get::<String>("Name")?, values)?;
                    } else if t.get::<Option<String>>("Type")?.as_deref() == Some("Constants") {
                        let values = _lua.create_table()?;
                        if let Some(fields) = t.get::<Option<Table>>("Values")? {
                            for f in fields.sequence_values::<Table>() {
                                let f = f?;
                                values.set(f.get::<String>("Name")?, f.get::<Value>("Value")?)?;
                            }
                        }
                        constants.set(t.get::<String>("Name")?, values)?;
                    }
                }
            }
            sink.set(sink.raw_len() + 1, table)
        })?,
    )?;
    env.set("APIDocumentation", docs)?;
    let mut pending: Vec<&String> = sources.iter().collect();
    let mut symbolic_enums = false;
    while !pending.is_empty() {
        let before = pending.len();
        let mut error = String::new();
        pending.retain(|source| {
            match lua
                .load(source.as_str())
                .set_environment(env.clone())
                .exec()
            {
                Ok(()) => false,
                Err(e) => {
                    error = e.to_string();
                    true
                }
            }
        });
        if pending.len() == before {
            if !symbolic_enums {
                namespace_fallback(&lua, &env.get::<Table>("Enum")?, "Enum")?;
                symbolic_enums = true;
            } else {
                return Err(format!("unresolved documentation dependencies: {error}").into());
            }
        }
    }
    let mut predicates = BTreeSet::new();
    for t in captured.clone().sequence_values::<Table>() {
        if let Some(list) = t?.get::<Option<Table>>("Predicates")? {
            for p in list.sequence_values::<Table>() {
                predicates.insert(p?.get::<String>("Name")?);
            }
        }
    }
    captured
        .sequence_values::<Table>()
        .map(|t| project(&t?, &predicates).map_err(Into::into))
        .collect()
}

fn namespace_fallback(lua: &Lua, root: &Table, prefix: &str) -> mlua::Result<()> {
    let prefix = prefix.to_owned();
    let mt = lua.create_table()?;
    mt.set(
        "__index",
        lua.create_function(move |lua, (_t, namespace): (Table, String)| {
            let prefix = prefix.clone();
            let t = lua.create_table()?;
            let mt = lua.create_table()?;
            mt.set(
                "__index",
                lua.create_function(move |lua, (_t, key): (Table, String)| {
                    symbolic(lua, format!("{prefix}.{namespace}.{key}"))
                })?,
            )?;
            t.set_metatable(Some(mt));
            Ok(t)
        })?,
    )?;
    root.set_metatable(Some(mt));
    Ok(())
}
fn symbolic(lua: &Lua, expression: String) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("__symbol", expression)?;
    let mt = lua.create_table()?;
    for (name, op) in [
        ("__add", "+"),
        ("__sub", "-"),
        ("__mul", "*"),
        ("__div", "/"),
        ("__mod", "%"),
        ("__pow", "^"),
    ] {
        mt.set(
            name,
            lua.create_function(move |lua, (a, b): (Value, Value)| {
                fn expr(v: Value) -> mlua::Result<String> {
                    match v {
                        Value::Table(t) => t.get("__symbol"),
                        Value::Integer(n) => Ok(n.to_string()),
                        Value::Number(n) => Ok(n.to_string()),
                        _ => Err(mlua::Error::RuntimeError(
                            "invalid symbolic arithmetic".into(),
                        )),
                    }
                }
                symbolic(lua, format!("({}{}{})", expr(a)?, op, expr(b)?))
            })?,
        )?;
    }
    t.set_metatable(Some(mt));
    Ok(t)
}
fn scalar(v: Value) -> mlua::Result<serde_json::Value> {
    Ok(match v {
        Value::Boolean(v) => v.into(),
        Value::Integer(v) => v.into(),
        Value::Number(v) => serde_json::Number::from_f64(v)
            .ok_or_else(|| mlua::Error::RuntimeError("nonfinite number".into()))?
            .into(),
        Value::String(v) => v.to_str()?.to_string().into(),
        Value::Table(t) if t.contains_key("__symbol")? => {
            serde_json::json!({"reference": t.get::<String>("__symbol")?})
        }
        Value::Table(t) => {
            if t.raw_len() > 0 {
                serde_json::Value::Array(
                    t.sequence_values::<Value>()
                        .map(|v| scalar(v?))
                        .collect::<mlua::Result<_>>()?,
                )
            } else {
                let mut map = serde_json::Map::new();
                for key in ["Argument", "Aspect", "Value", "Name", "Type"] {
                    let v = t.get::<Value>(key)?;
                    if !matches!(v, Value::Nil) {
                        map.insert(key.into(), scalar(v)?);
                    }
                }
                serde_json::Value::Object(map)
            }
        }
        _ => {
            return Err(mlua::Error::RuntimeError(
                "unsupported metadata value".into(),
            ));
        }
    })
}
fn children(t: &Table, key: &str, predicates: &BTreeSet<String>) -> mlua::Result<Vec<Declaration>> {
    match t.get::<Option<Table>>(key)? {
        Some(v) => v
            .sequence_values::<Table>()
            .map(|v| project(&v?, predicates))
            .collect(),
        None => Ok(vec![]),
    }
}
fn project(t: &Table, predicates: &BTreeSet<String>) -> mlua::Result<Declaration> {
    let mut attributes = BTreeMap::new();
    for pair in t.clone().pairs::<String, Value>() {
        let (k, v) = pair?;
        if matches!(
            k.as_str(),
            "Nilable"
                | "NilableContents"
                | "NeverSecretContents"
                | "Default"
                | "InnerType"
                | "KeyType"
                | "Mixin"
                | "EnumValue"
                | "Value"
                | "MinValue"
                | "MaxValue"
                | "NumValues"
                | "MayReturnNothing"
                | "LiteralName"
                | "Environment"
                | "ObjectType"
                | "StrideIndex"
                | "SynchronousEvent"
                | "UniqueEvent"
                | "CallbackEvent"
                | "FailureMode"
                | "Argument"
                | "Aspect"
        ) || predicates.contains(&k)
            || k.contains("Secret")
            || k.starts_with("Requires")
            || k.starts_with("RequireN")
            || k.starts_with("Check")
            || matches!(
                k.as_str(),
                "HasRestrictions"
                    | "IsProtectedFunction"
                    | "SecureHooksAllowed"
                    | "AddsForbiddenAspects"
                    | "RestrictedForMacroChatMessages"
                    | "MouseFocusValidForLimitedInput"
            )
        {
            attributes.insert(k, scalar(v)?);
        }
    }
    let mut fields = children(t, "Fields", predicates)?;
    fields.extend(children(t, "Tables", predicates)?);
    fields.extend(children(t, "Values", predicates)?);
    // Events retain payload order, just like function argument/return order.
    let mut arguments = children(t, "Arguments", predicates)?;
    arguments.extend(children(t, "Payload", predicates)?);
    let mut methods = children(t, "Functions", predicates)?;
    methods.extend(children(t, "Events", predicates)?);
    let mut values = BTreeMap::new();
    if t.get::<Option<String>>("Type")?.as_deref() == Some("Enumeration") {
        for f in &fields {
            if let Some(v) = f.attributes.get("EnumValue") {
                values.insert(f.name.clone(), v.clone());
            }
        }
        let enum_name = t.get::<String>("Name")?;
        // Compact conventional members; retain exceptional member metadata losslessly.
        fields.retain(|f| {
            f.ty.as_deref() != Some(enum_name.as_str())
                || f.attributes.len() != 1
                || !f.attributes.contains_key("EnumValue")
        });
    }
    Ok(Declaration {
        overloads: Vec::new(),
        evidence: None,
        predicates: children(t, "Predicates", predicates)?,
        values,
        name: t.get::<Option<String>>("Name")?.unwrap_or_default(),
        namespace: t.get("Namespace")?,
        ty: t.get("Type")?,
        arguments,
        returns: children(t, "Returns", predicates)?,
        fields,
        methods,
        inherits: vec![],
        attributes,
    })
}

pub fn merge_system(m: &mut Manifest, mut system: Declaration) {
    m.predicates.append(&mut system.predicates);
    let environment = system.attributes.get("Environment").cloned();
    if let Some(e) = &environment {
        for f in &mut system.methods {
            f.attributes
                .entry("Environment".into())
                .or_insert(e.clone());
        }
        for f in &mut system.fields {
            f.attributes
                .entry("Environment".into())
                .or_insert(e.clone());
        }
    }
    let widget = system.ty.as_deref() == Some("ScriptObject");
    for f in &system.methods {
        if f.ty.as_deref() == Some("Event") {
            let name = f
                .attributes
                .get("LiteralName")
                .and_then(|v| v.as_str())
                .unwrap_or(&f.name)
                .to_owned();
            m.events.push(EventEntry {
                name,
                params: f
                    .arguments
                    .iter()
                    .map(|p| EventParam {
                        name: p.name.clone(),
                        ty: p.ty.clone().unwrap_or_else(|| "unknown".into()),
                    })
                    .collect(),
                evidence: Evidence::Documented,
                signature: Some(f.clone()),
            });
        } else if !widget {
            let name = system
                .namespace
                .as_ref()
                .map(|n| format!("{n}.{}", f.name))
                .unwrap_or_else(|| f.name.clone());
            let available =
                f.attributes.get("Environment").and_then(|e| e.as_str()) != Some("SecureOnly");
            m.functions.push(FunctionEntry {
                name,
                available,
                restrictions: vec![],
                returns_secret: false,
                params: f.arguments.iter().map(|p| p.name.clone()).collect(),
                evidence: Evidence::Documented,
                signature: Some(f.clone()),
                provenance: None,
            });
        }
    }
    for t in &system.fields {
        if t.ty.as_deref() == Some("Enumeration") {
            m.enums.push(t.clone());
        } else {
            m.tables.push(t.clone());
        }
    }
    if widget {
        m.widgets.push(system);
    }
    summarize(m);
}
fn active(v: &serde_json::Value) -> bool {
    !v.is_null()
        && v != &serde_json::Value::Bool(false)
        && v.as_array().is_none_or(|a| !a.is_empty())
}
fn conditional(d: &Declaration) -> bool {
    d.attributes.iter().any(|(k, v)| {
        active(v)
            && (k.starts_with("SecretWhen")
                || k == "ConditionalSecret"
                || k == "SecretInChatMessagingLockdown"
                || k == "SecretValue"
                || k == "SecretInActivePvPMatch")
    }) || d.returns.iter().any(conditional)
}
pub fn summarize(m: &mut Manifest) {
    let predicates: BTreeSet<_> = m.predicates.iter().map(|p| p.name.as_str()).collect();
    for f in &mut m.functions {
        if let Some(s) = &f.signature {
            f.returns_secret = s
                .attributes
                .iter()
                .any(|(k, v)| k.starts_with("SecretReturns") && active(v))
                || conditional(s);
            f.restrictions = s
                .attributes
                .iter()
                .filter(|(k, v)| {
                    active(v)
                        && (predicates.contains(k.as_str())
                            || k.as_str() == "IsProtectedFunction"
                            || k.as_str() == "HasRestrictions"
                            || k.starts_with("Requires")
                            || k.starts_with("Check"))
                })
                .map(|(k, _)| k.clone())
                .collect();
            if s.attributes.get("Environment").and_then(|v| v.as_str()) == Some("SecureOnly") {
                f.available = false;
                f.restrictions.push("SecureOnly".into());
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overlay {
    pub interface: Option<InterfaceOverlay>,
    #[serde(default)]
    pub functions: Vec<FunctionOverlay>,
    #[serde(default)]
    pub widgets: Vec<WidgetOverlay>,
    #[serde(default)]
    pub globals: Vec<GlobalOverlay>,
    #[serde(default)]
    pub methods: Vec<MethodOverlay>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterfaceOverlay {
    pub basis: String,
    pub value: u32,
    pub note: String,
    pub evidence: Evidence,
    #[serde(default)]
    pub live_probe: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionOverlay {
    pub entry: FunctionEntry,
    pub note: String,
    pub evidence: Evidence,
    #[serde(default)]
    pub live_probe: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetOverlay {
    pub entry: Declaration,
    pub note: String,
    pub evidence: Evidence,
    #[serde(default)]
    pub live_probe: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodOverlay {
    pub widget: String,
    pub entry: Declaration,
    pub note: String,
    pub evidence: Evidence,
    #[serde(default)]
    pub live_probe: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalOverlay {
    #[serde(rename = "type", default)]
    pub ty: Option<String>,
    pub name: String,
    pub note: String,
    pub evidence: Evidence,
    #[serde(default)]
    pub live_probe: Option<String>,
}
fn evidence(e: Evidence, probe: &Option<String>) -> Result<()> {
    if e == Evidence::Verified && probe.as_ref().is_none_or(|p| p.trim().is_empty()) {
        return Err("verified overlay evidence requires a live_probe record".into());
    }
    Ok(())
}
fn note(s: &str) -> Result<()> {
    if s.trim().is_empty() {
        Err("overlay note must identify its source".into())
    } else {
        Ok(())
    }
}
pub fn apply_overlay(m: &mut Manifest, text: &str) -> Result<()> {
    let overlay: Overlay = toml::from_str(text)?;
    if let Some(i) = overlay.interface {
        note(&i.note)?;
        evidence(i.evidence, &i.live_probe)?;
        if !matches!(i.basis.as_str(), "convention" | "source" | "probe") {
            return Err("unknown interface basis".into());
        }
        m.interface = i.value;
        if let Some(s) = &mut m.source {
            s.interface_source = format!("overlay: {}", i.note);
            s.interface_basis = Some(i.basis);
            s.interface_evidence = Some(i.evidence);
        }
    }
    for mut f in overlay.functions {
        f.entry.evidence = f.evidence;
        note(&f.note)?;
        evidence(f.evidence, &f.live_probe)?;
        m.functions.retain(|v| v.name != f.entry.name);
        m.functions.push(f.entry);
    }
    for mut w in overlay.widgets {
        w.entry.evidence = Some(w.evidence);
        note(&w.note)?;
        evidence(w.evidence, &w.live_probe)?;
        m.widgets.retain(|v| v.name != w.entry.name);
        m.widgets.push(w.entry);
    }
    for mut method in overlay.methods {
        method.entry.evidence = Some(method.evidence);
        note(&method.note)?;
        evidence(method.evidence, &method.live_probe)?;
        let widget = m
            .widgets
            .iter_mut()
            .find(|w| w.name == method.widget)
            .ok_or("method overlay widget does not exist")?;
        widget.methods.retain(|f| f.name != method.entry.name);
        widget.methods.push(method.entry);
        widget.methods.sort_by(|a, b| a.name.cmp(&b.name));
    }
    for g in overlay.globals {
        note(&g.note)?;
        evidence(g.evidence, &g.live_probe)?;
        let templates = m
            .globals
            .iter()
            .find(|v| v.name == g.name)
            .map(|v| v.templates.clone())
            .unwrap_or_default();
        m.globals.retain(|v| v.name != g.name);
        m.globals.push(selu_manifest::GlobalEntry {
            value: None,
            templates,
            name: g.name,
            ty: g.ty,
            evidence: g.evidence,
        });
    }
    Ok(())
}
pub fn encode(m: &mut Manifest) -> Result<Vec<u8>> {
    detect_frame_families(m);
    summarize(m);
    m.predicates.sort_by(|a, b| a.name.cmp(&b.name));
    m.predicates.dedup_by(|a, b| a == b);
    m.functions.sort_by(|a, b| a.name.cmp(&b.name));
    m.events.sort_by(|a, b| a.name.cmp(&b.name));
    m.globals.sort_by(|a, b| a.name.cmp(&b.name));
    for list in [&mut m.tables, &mut m.enums, &mut m.widgets] {
        list.sort_by(|a, b| a.name.cmp(&b.name));
    }
    m.templates.sort_by(|a, b| a.name.cmp(&b.name));
    m.data_globals.sort_by(|a, b| a.name.cmp(&b.name));
    selu_manifest::format::encode_manifest(m)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structural_projection_and_determinism() {
        let source = r#"APIDocumentation:AddDocumentationTable({Name="Sample", Namespace="C_Test", Functions={{Name="Run",Type="Function",MayReturnNothing=true,Documentation={"DO NOT COPY"},Arguments={{Name="index",Type="luaIndex",Nilable=true,Default=1}},Returns={{Name="ok",Type="bool",Nilable=false}}}},Events={{Name="Login",LiteralName="PLAYER_LOGIN",Payload={{Name="unit",Type="UnitToken",Nilable=true}}}},Tables={{Name="Choice",Type="Enumeration",Fields={{Name="A",Type="Choice",EnumValue=1}}},{Name="Point",Type="Structure",Fields={{Name="x",Type="number"}}}}})"#;
        let a = super::read_documentation(source).unwrap();
        let b = super::read_documentation(source).unwrap();
        assert_eq!(a, b);
        assert_eq!(a[0].methods[0].arguments[0].ty.as_deref(), Some("luaIndex"));
        assert_eq!(a[0].methods[0].arguments[0].attributes["Default"], 1);
        assert_eq!(a[0].methods[0].returns[0].ty.as_deref(), Some("bool"));
        let json = serde_json::to_string(&a).unwrap();
        assert!(!json.contains("DO NOT COPY"));
        assert!(json.contains("luaIndex"));
        assert!(json.contains("MayReturnNothing"));
        assert_eq!(
            serde_json::from_str::<Vec<selu_manifest::Declaration>>(&json).unwrap(),
            a
        );
    }
    #[test]
    fn overlay_validation_and_merge() {
        let mut m: Manifest = serde_json::from_str(
            r#"{"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1}"#,
        )
        .unwrap();
        apply_overlay(
            &mut m,
            "[interface]\nvalue=42\nnote='test source'\nevidence='source'\nbasis='source'",
        )
        .unwrap();
        assert_eq!(m.interface, 42);
        assert!(apply_overlay(&mut m, "surprise=1").is_err());
        assert!(apply_overlay(&mut m, "[interface]\nvalue=1\nnote=''").is_err());
        assert!(apply_overlay(&mut m, "[[globals]]\nname='Test'").is_err());
        let a = encode(&mut m).unwrap();
        assert_eq!(a, encode(&mut m).unwrap());
    }
    #[test]
    fn sandbox_rejects_host_access_and_loops() {
        assert!(read_documentation("os.execute('echo forbidden')").is_err());
        assert!(read_documentation("while true do end").is_err());
    }
    #[test]
    fn committed_manifests_smoke() {
        for (flavour, build) in [
            ("wow", "12.1.5.70391"),
            ("wowf", "1.60.1.70532"),
            ("wow_classic_era", "1.15.9.70576"),
            ("wow_anniversary", "2.5.6.70577"),
            ("wow_classic_titan", "3.80.2.70522"),
            ("wow_classic", "5.5.4.70631"),
        ] {
            let path =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store");
            let m = selu_manifest::store::load_build(&path, &format!("{flavour}@{build}")).unwrap();
            assert!(!m.fixture);
            assert!(m.function("C_Timer.After").is_some());
            assert!(m.function("CreateFrame").is_some());
            assert!(m.event("PLAYER_LOGIN").is_some());
            assert!(
                m.widgets
                    .iter()
                    .any(|w| w.methods.iter().any(|f| f.name == "SetPoint"))
            );
            assert_eq!(m.source.as_ref().unwrap().commit.len(), 40);
            let value = serde_json::to_value(&m).unwrap();
            assert_eq!(serde_json::from_value::<Manifest>(value).unwrap(), m);
        }
    }
}

/// Detect only repeated, consistently typed numbered names. Sparse ranges are
/// safe: lookups remain nullable, including indices inside observed bounds.
pub fn detect_frame_families(m: &mut Manifest) {
    let mut groups = BTreeMap::<(String, String), BTreeMap<u32, BTreeSet<String>>>::new();
    for g in &m.globals {
        let Some(ty) = &g.ty else { continue };
        if !m
            .widgets
            .iter()
            .any(|w| w.name == *ty || w.name == format!("{ty}API"))
        {
            continue;
        }
        let Some(start) = g.name.find(|c: char| c.is_ascii_digit()) else {
            continue;
        };
        if start == 0 {
            continue;
        }
        let end = g.name[start..]
            .find(|c: char| !c.is_ascii_digit())
            .map_or(g.name.len(), |n| start + n);
        let digits = &g.name[start..end];
        if digits.len() > 1 && digits.starts_with('0') {
            continue;
        }
        let Ok(index) = digits.parse::<u32>() else {
            continue;
        };
        groups
            .entry((g.name[..start].into(), g.name[end..].into()))
            .or_default()
            .entry(index)
            .or_default()
            .insert(ty.clone());
    }
    let runtime = m.frame_families.clone();
    m.frame_families = groups
        .into_iter()
        .filter_map(|((prefix, suffix), entries)| {
            let widget = entries.values().next()?.iter().next()?.clone();
            if entries.len() < 2
                || entries
                    .values()
                    .any(|types| types.len() != 1 || !types.contains(&widget))
            {
                return None;
            }
            Some(selu_manifest::FrameFamily {
                sources: runtime
                    .iter()
                    .filter(|f| f.prefix == prefix && f.suffix == suffix)
                    .flat_map(|f| f.sources.clone())
                    .collect(),
                prefix,
                suffix,
                min_index: *entries.keys().next()?,
                max_index: *entries.keys().next_back()?,
                widget,
            })
        })
        .collect();
}

#[cfg(test)]
mod family_tests {
    #[test]
    fn runtime_numeric_loop_is_typed_and_bounded() {
        let mut m: selu_manifest::Manifest = serde_json::from_str(r#"{"flavour":"fixture","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1}"#).unwrap();
        m.widgets
            .push(serde_json::from_str(r#"{"name":"Button"}"#).unwrap());
        let mut sources = crate::source::Sources::default();
        sources.parse("Frames.lua", "COUNT = 3; function init() for i = 1, COUNT do CreateFrame(\"Button\", \"Example\"..i) end end").unwrap();
        sources.extend_manifest(&mut m);
        super::detect_frame_families(&mut m);
        assert_eq!(m.frame_families.len(), 1);
        assert_eq!(m.frame_families[0].min_index, 1);
        assert_eq!(m.frame_families[0].max_index, 3);
    }
}

#[cfg(test)]
mod review_family_regressions {
    fn generate(src: &str) -> selu_manifest::Manifest {
        let mut m: selu_manifest::Manifest = serde_json::from_str(r#"{"flavour":"fixture","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1,"widgets":[{"name":"Button"},{"name":"Frame"}]}"#).unwrap();
        let mut sources = crate::source::Sources::default();
        sources.parse("review.lua", src).unwrap();
        sources.extend_manifest(&mut m);
        super::detect_frame_families(&mut m);
        m
    }
    #[test]
    fn conflicting_creation_types_never_choose_the_first() {
        let m = generate(
            r#"function a() for i=1,3 do CreateFrame("Button","Example"..i) end end function b() for i=1,3 do CreateFrame("Frame","Example"..i) end end"#,
        );
        assert!(m.frame_families.is_empty(), "{:?}", m.frame_families);
        assert!(
            m.globals
                .iter()
                .any(|g| g.name == "Example1" && g.ty.as_deref() == Some("Button"))
        );
        assert!(
            m.globals
                .iter()
                .any(|g| g.name == "Example1" && g.ty.as_deref() == Some("Frame"))
        );
    }
    #[test]
    fn index_binding_must_be_the_unchanged_numeric_loop_binding() {
        for src in [
            r#"for i=1,3 do local i="Other"; CreateFrame("Button","Example"..i) end"#,
            r#"for i=1,3 do i=9; CreateFrame("Button","Example"..i) end"#,
            r#"for i=1,3 do function i() end; CreateFrame("Button","Example"..i) end"#,
            r#"for i=1,3 do local function f(i) CreateFrame("Button","Example"..i) end end"#,
        ] {
            assert!(generate(src).frame_families.is_empty(), "{src}");
        }
        let m = generate(r#"for i=1,3 do for i=5,7 do CreateFrame("Button","Example"..i) end end"#);
        assert_eq!(m.frame_families[0].min_index, 5);
        assert_eq!(m.frame_families[0].max_index, 7);
        assert!(generate(r#"local CreateFrame=function() end; for i=1,3 do CreateFrame("Button","Example"..i) end"#).frame_families.is_empty());
        assert!(
            generate(r#"local N=3; for i=1,N do CreateFrame("Button","Example"..i) end"#)
                .frame_families
                .is_empty()
        );
    }
    #[test]
    fn single_zero_is_not_padding() {
        let m = generate(r#"for i=0,2 do CreateFrame("Button","Example"..i) end"#);
        assert_eq!(m.frame_families[0].min_index, 0);
    }
}

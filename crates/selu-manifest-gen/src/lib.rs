use mlua::{Lua, LuaOptions, StdLib, Table, Value};
use selu_manifest::{Declaration, EventEntry, EventParam, Evidence, FunctionEntry, Manifest};
use std::collections::BTreeMap;

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
    captured
        .sequence_values::<Table>()
        .map(|t| project(&t?).map_err(Into::into))
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
fn children(t: &Table, key: &str) -> mlua::Result<Vec<Declaration>> {
    match t.get::<Option<Table>>(key)? {
        Some(v) => v.sequence_values::<Table>().map(|v| project(&v?)).collect(),
        None => Ok(vec![]),
    }
}
fn project(t: &Table) -> mlua::Result<Declaration> {
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
        ) || k.contains("Secret")
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
    let mut fields = children(t, "Fields")?;
    fields.extend(children(t, "Tables")?);
    fields.extend(children(t, "Values")?);
    // Events retain payload order, just like function argument/return order.
    let mut arguments = children(t, "Arguments")?;
    arguments.extend(children(t, "Payload")?);
    let mut methods = children(t, "Functions")?;
    methods.extend(children(t, "Events")?);
    Ok(Declaration {
        name: t.get::<Option<String>>("Name")?.unwrap_or_default(),
        namespace: t.get("Namespace")?,
        ty: t.get("Type")?,
        arguments,
        returns: children(t, "Returns")?,
        fields,
        methods,
        inherits: vec![],
        attributes,
    })
}

pub fn merge_system(m: &mut Manifest, system: Declaration) {
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
            m.functions.push(FunctionEntry {
                name,
                available: true,
                restrictions: vec![],
                returns_secret: f.attributes.keys().any(|k| k.starts_with("SecretReturns")),
                params: f.arguments.iter().map(|p| p.name.clone()).collect(),
                evidence: Evidence::Documented,
                signature: Some(f.clone()),
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
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterfaceOverlay {
    pub value: u32,
    pub note: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionOverlay {
    pub entry: FunctionEntry,
    pub note: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetOverlay {
    pub entry: Declaration,
    pub note: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalOverlay {
    pub name: String,
    pub note: String,
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
        m.interface = i.value;
        if let Some(s) = &mut m.source {
            s.interface_source = format!("overlay: {}", i.note);
        }
    }
    for f in overlay.functions {
        note(&f.note)?;
        m.functions.retain(|v| v.name != f.entry.name);
        m.functions.push(f.entry);
    }
    for w in overlay.widgets {
        note(&w.note)?;
        m.widgets.retain(|v| v.name != w.entry.name);
        m.widgets.push(w.entry);
    }
    for g in overlay.globals {
        note(&g.note)?;
        m.globals.retain(|v| v.name != g.name);
        m.globals.push(selu_manifest::GlobalEntry {
            name: g.name,
            evidence: Evidence::Verified,
        });
    }
    Ok(())
}
pub fn encode(m: &mut Manifest) -> Result<Vec<u8>> {
    m.functions.sort_by(|a, b| a.name.cmp(&b.name));
    m.events.sort_by(|a, b| a.name.cmp(&b.name));
    m.globals.sort_by(|a, b| a.name.cmp(&b.name));
    for list in [&mut m.tables, &mut m.enums, &mut m.widgets] {
        list.sort_by(|a, b| a.name.cmp(&b.name));
    }
    let mut bytes = serde_json::to_vec(m)?;
    bytes.push(b'\n');
    Ok(bytes)
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
        apply_overlay(&mut m, "[interface]\nvalue=42\nnote='verified test source'").unwrap();
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
        for (flavour, build) in [("retail", "12.1.5.70391"), ("forever", "1.60.1.70532")] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../manifests/{flavour}/{build}.json"));
            let m = selu_manifest::load_manifest(&path).unwrap();
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

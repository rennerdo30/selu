//! Data-only reader for the user's generated API documentation. Never evaluates Lua.
use mlua::{Lua, Table, Value};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Debug)]
enum Node {
    Text(String),
    Number(f64),
    Bool(bool),
    Ref(String),
    Table(BTreeMap<String, Node>, Vec<Node>),
    Unknown,
}
#[derive(Clone, Debug)]
enum Token {
    Word(String),
    Text(String),
    Number(f64),
    Mark(u8),
}

fn long_literal(source: &str, start: usize) -> Option<(usize, usize, usize)> {
    let b = source.as_bytes();
    if b.get(start) != Some(&b'[') {
        return None;
    }
    let mut end = start + 1;
    while b.get(end) == Some(&b'=') {
        end += 1
    }
    if b.get(end) != Some(&b'[') {
        return None;
    }
    let begin = end + 1;
    let close = format!("]{}]", "=".repeat(end - start - 1));
    Some(match source[begin..].find(&close) {
        Some(n) => (begin, begin + n, begin + n + close.len()),
        None => (begin, source.len(), source.len()),
    })
}

fn tokens(source: &str) -> Vec<Token> {
    let b = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b.get(i..i + 2) == Some(b"--") {
            i += 2;
            if let Some((_, _, end)) = long_literal(source, i) {
                i = end;
            } else {
                while i < b.len() && b[i] != b'\n' {
                    i += 1
                }
            }
            continue;
        }
        if b[i] == b'\'' || b[i] == b'"' {
            let quote = b[i];
            i += 1;
            let mut text = Vec::new();
            let mut valid = true;
            while i < b.len() && b[i] != quote {
                let mut c = b[i];
                i += 1;
                if c == b'\\' && i < b.len() {
                    if b[i].is_ascii_digit() {
                        let mut n = 0u16;
                        let mut count = 0;
                        while count < 3 && i < b.len() && b[i].is_ascii_digit() {
                            n = n * 10 + (b[i] - b'0') as u16;
                            i += 1;
                            count += 1
                        }
                        if n > 255 {
                            valid = false
                        }
                        c = n as u8;
                    } else {
                        c = match b[i] {
                            b'n' => b'\n',
                            b'r' => b'\r',
                            b't' => b'\t',
                            b'a' => 7,
                            b'b' => 8,
                            b'f' => 12,
                            b'v' => 11,
                            c => c,
                        };
                        i += 1
                    }
                }
                text.push(c);
            }
            if i >= b.len() {
                valid = false
            }
            i = (i + 1).min(b.len());
            out.push(if valid {
                Token::Text(String::from_utf8_lossy(&text).into_owned())
            } else {
                Token::Word("nil".into())
            });
            continue;
        }
        if let Some((begin, end, next)) = long_literal(source, i) {
            out.push(Token::Text(source[begin..end].into()));
            i = next;
            continue;
        }
        if b[i].is_ascii_digit() {
            let start = i;
            i += 1;
            while i < b.len()
                && (b[i].is_ascii_alphanumeric()
                    || b[i] == b'.'
                    || b[i] == b'_'
                    || (matches!(b[i], b'+' | b'-') && matches!(b[i - 1], b'e' | b'E')))
            {
                i += 1
            }
            let raw = &source[start..i];
            let n: Option<f64> = if let Some(hex) = raw.strip_prefix("0x") {
                i64::from_str_radix(hex, 16).ok().map(|n| n as f64)
            } else {
                raw.parse().ok()
            };
            if let Some(n) = n.filter(|n| n.is_finite()) {
                out.push(Token::Number(n))
            } else {
                out.push(Token::Word(raw.into()))
            }
            continue;
        }
        if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let start = i;
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1
            }
            out.push(Token::Word(source[start..i].into()));
            continue;
        }
        out.push(Token::Mark(b[i]));
        i += 1;
    }
    out
}
struct Parser {
    tokens: Vec<Token>,
    at: usize,
}
impl Parser {
    fn mark(&self, c: u8) -> bool {
        matches!(self.tokens.get(self.at),Some(Token::Mark(v)) if *v==c)
    }
    fn value(&mut self, depth: usize) -> Node {
        if depth > 64 {
            self.at += 1;
            return Node::Unknown;
        }
        let Some(token) = self.tokens.get(self.at).cloned() else {
            return Node::Unknown;
        };
        self.at += 1;
        match token {
            Token::Text(s) => Node::Text(s),
            Token::Number(n) => Node::Number(n),
            Token::Mark(b'-') => match self.value(depth + 1) {
                Node::Number(n) => Node::Number(-n),
                _ => Node::Unknown,
            },
            Token::Word(s) if s == "true" || s == "false" => Node::Bool(s == "true"),
            Token::Word(mut s) => {
                while self.mark(b'.') {
                    self.at += 1;
                    if let Some(Token::Word(part)) = self.tokens.get(self.at) {
                        s.push('.');
                        s.push_str(part);
                        self.at += 1
                    } else {
                        break;
                    }
                }
                Node::Ref(s)
            }
            Token::Mark(b'{') => {
                let mut fields = BTreeMap::new();
                let mut items = Vec::new();
                while self.at < self.tokens.len() && !self.mark(b'}') {
                    let start = self.at;
                    if let (Some(Token::Word(key)), Some(Token::Mark(b'='))) =
                        (self.tokens.get(self.at), self.tokens.get(self.at + 1))
                    {
                        let key = key.clone();
                        self.at += 2;
                        let mut value = self.value(depth + 1);
                        // Arithmetic/calls are not literal data. Do not accept
                        // their first token as a valid partial constant.
                        if self.at < self.tokens.len()
                            && !self.mark(b',')
                            && !self.mark(b';')
                            && !self.mark(b'}')
                        {
                            let mut nesting = 0usize;
                            while self.at < self.tokens.len() {
                                if nesting == 0
                                    && (self.mark(b',') || self.mark(b';') || self.mark(b'}'))
                                {
                                    break;
                                }
                                match self.tokens[self.at] {
                                    Token::Mark(b'(' | b'[' | b'{') => nesting += 1,
                                    Token::Mark(b')' | b']' | b'}') => {
                                        nesting = nesting.saturating_sub(1)
                                    }
                                    _ => {}
                                }
                                self.at += 1;
                            }
                            value = Node::Unknown;
                        }
                        fields.insert(key, value);
                    } else {
                        items.push(self.value(depth + 1))
                    }
                    if self.at == start {
                        self.at += 1
                    }
                    if self.mark(b',') || self.mark(b';') {
                        self.at += 1
                    }
                }
                if !self.mark(b'}') {
                    return Node::Unknown;
                }
                self.at += 1;
                Node::Table(fields, items)
            }
            _ => Node::Unknown,
        }
    }
}
#[derive(Clone, Default)]
struct Metadata {
    tables: BTreeMap<String, BTreeMap<String, Node>>,
    functions: BTreeMap<String, selu_manifest::Declaration>,
    schemas: BTreeMap<String, selu_manifest::Declaration>,
}
fn declaration(node: &Node) -> Option<selu_manifest::Declaration> {
    let Node::Table(fields, _) = node else {
        return None;
    };
    let Node::Text(name) = fields.get("Name")? else {
        return None;
    };
    let mut data = serde_json::json!({"name": name});
    if let Some(Node::Text(ty)) = fields.get("Type") {
        data["type"] = ty.as_str().into();
    }
    if let Some(Node::Text(mixin)) = fields.get("Mixin") {
        data["attributes"]["Mixin"] = mixin.as_str().into();
    }
    for (source, dest) in [
        ("Fields", "fields"),
        ("Returns", "returns"),
        ("Functions", "methods"),
    ] {
        if let Some(Node::Table(_, list)) = fields.get(source) {
            data[dest] =
                serde_json::to_value(list.iter().filter_map(declaration).collect::<Vec<_>>())
                    .ok()?;
        }
    }
    serde_json::from_value(data).ok()
}
impl Metadata {
    fn expose_constant_aliases(&mut self, target: Option<&str>) {
        let Some(ui) = target.and_then(selu_manifest::client_ui_for_target) else {
            return;
        };
        let names: Vec<_> = self.tables.keys().cloned().collect();
        // Generated documentation names product variants with their game or
        // family suffix; native Constants expose the unsuffixed table. For
        // example the installed Camelot LevelConstants documentation and its
        // MicroMenu consumers use those two forms. Reuse the product mapping,
        // prefer game over family, and never replace an explicit declaration.
        for folder in [ui.game_folder, ui.family] {
            let suffix = format!("_{folder}");
            for name in &names {
                if name.starts_with("Constants.")
                    && let Some(base) = name.strip_suffix(&suffix)
                    && !self.tables.contains_key(base)
                {
                    self.tables.insert(base.into(), self.tables[name].clone());
                }
            }
        }
    }
    fn read(&mut self, source: &str) {
        if source.len() > 4 * 1024 * 1024 {
            return;
        }
        if !source.contains("Type") {
            return;
        }
        let mut p = Parser {
            tokens: tokens(source),
            at: 0,
        };
        while p.at < p.tokens.len() {
            if p.mark(b'{') {
                self.collect(&p.value(0));
            } else {
                p.at += 1
            }
        }
    }
    fn collect(&mut self, node: &Node) {
        let Node::Table(fields, items) = node else {
            return;
        };
        if let (Some(Node::Text(name)), Some(Node::Text(kind))) =
            (fields.get("Name"), fields.get("Type"))
        {
            if let Some(mut schema) = declaration(node) {
                match kind.as_str() {
                    "System" => {
                        let namespace = match fields.get("Namespace") {
                            Some(Node::Text(s)) => s.as_str(),
                            _ => "",
                        };
                        for mut function in schema.methods {
                            if !namespace.is_empty() {
                                function.name = format!("{namespace}.{}", function.name);
                            }
                            self.functions.insert(function.name.clone(), function);
                        }
                    }
                    "Structure" => {
                        self.schemas.insert(name.clone(), schema);
                    }
                    "Enumeration" => {
                        schema.ty = Some("enum".into());
                        if let Some(Node::Table(_, entries)) = fields.get("Fields") {
                            for entry in entries {
                                if let Some(field) = declaration(entry) {
                                    schema.values.insert(field.name, serde_json::Value::Null);
                                }
                            }
                        }
                        self.schemas.insert(name.clone(), schema.clone());
                        self.schemas.insert(format!("Enum.{name}"), schema);
                    }
                    _ => {}
                }
            }
            let (namespace, list, value) = match kind.as_str() {
                "Enumeration" => ("Enum", "Fields", "EnumValue"),
                "Constants" => ("Constants", "Values", "Value"),
                _ => ("", "", ""),
            };
            if let Some(Node::Table(_, entries)) = fields.get(list) {
                let mut values = BTreeMap::new();
                for entry in entries {
                    if let Node::Table(entry, _) = entry
                        && let (Some(Node::Text(key)), Some(v)) =
                            (entry.get("Name"), entry.get(value))
                    {
                        values.insert(key.clone(), v.clone());
                    }
                }
                if !namespace.is_empty() {
                    self.tables.insert(format!("{namespace}.{name}"), values);
                }
                if kind == "Enumeration" {
                    let meta = ["NumValues", "MinValue", "MaxValue"]
                        .into_iter()
                        .filter_map(|key| fields.get(key).map(|v| (key.into(), v.clone())))
                        .collect();
                    self.tables.insert(format!("Enum.{name}Meta"), meta);
                }
            }
        }
        for v in fields.values().chain(items) {
            self.collect(v)
        }
    }
}

pub(crate) fn install(
    lua: &Lua,
    ctx: &crate::Ctx,
    source: &Path,
    target: Option<&str>,
) -> mlua::Result<()> {
    let _profile = crate::profile::Span::new("source.api_metadata");
    let source = source.canonicalize().map_err(mlua::Error::external)?;
    let storage = crate::frames::arena::store(lua)
        .borrow()
        .fonts
        .assets
        .client_storage();
    let build = storage
        .as_ref()
        .map(|s| s.build.build_key.as_str())
        .unwrap_or("checkout");
    let key = format!("{}:{build}:{}", target.unwrap_or("none"), source.display());
    static CACHE: OnceLock<Mutex<BTreeMap<String, Arc<Metadata>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let shared = storage.as_ref().is_none_or(|s| !s.uses_user_keys());
    let cached = shared
        .then(|| cache.lock().unwrap().get(&key).cloned())
        .flatten();
    let data = if let Some(data) = cached {
        data
    } else {
        let cache_build = format!("{}:{build}", target.unwrap_or("none"));
        let _source_cache = crate::ui_source_cache::Scope::new(&source, Some(&cache_build));
        let mut data = Metadata::default();
        let dir = source.join("Interface/AddOns/Blizzard_APIDocumentationGenerated");
        let client = storage.as_deref().map(|s| (s, source.as_path()));
        let mut note = |s: String| ctx.not_simulated(&s);
        match crate::loader::read_toc_target_client(&dir, target, client, &mut note) {
            Ok(toc) => {
                for file in toc.files {
                    match crate::loader::read_ui_text(
                        &toc.dir.join(file.replace('\\', "/")),
                        client,
                        &mut note,
                    ) {
                        Ok(text) => data.read(&text),
                        Err(e) => ctx.not_simulated(&format!("client API documentation: {e}")),
                    }
                }
            }
            Err(e) => ctx.not_simulated(&format!("client API documentation: {e}")),
        }
        data.expose_constant_aliases(target);
        let data = Arc::new(data);
        if shared {
            cache.lock().unwrap().insert(key, data.clone());
        }
        data
    };
    apply(lua, ctx, &data)?;
    install_natives(lua, ctx, &data)
}
fn install_natives(lua: &Lua, ctx: &crate::Ctx, data: &Metadata) -> mlua::Result<()> {
    // Private/secure UI natives can be absent from the public checked API.
    // Read their return contracts from this client, without executing docs or
    // exposing any additional API to the Selu language.
    let schemas: crate::native_defaults::Schemas = Arc::new(
        data.schemas
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    );
    let standins = lua
        .globals()
        .raw_get::<Option<Table>>("__selu_native_standins")?
        .unwrap_or(lua.create_table()?);
    lua.globals()
        .raw_set("__selu_native_standins", standins.clone())?;
    'functions: for (path, function) in &data.functions {
        let parts: Vec<_> = path.split('.').collect();
        let mut owner = lua.globals();
        for part in &parts[..parts.len() - 1] {
            owner = match owner.raw_get::<Value>(*part)? {
                Value::Table(t) => t,
                Value::Nil => {
                    let t = lua.create_table()?;
                    owner.raw_set(*part, t.clone())?;
                    t
                }
                _ => continue 'functions,
            };
        }
        let key = parts[parts.len() - 1];
        if !owner.raw_get::<Value>(key)?.is_nil() {
            continue;
        }
        let name = path.clone();
        let returns = function.returns.clone();
        let schemas = schemas.clone();
        let context = ctx.clone();
        let native = lua.create_function(move |lua, _: mlua::MultiValue| {
            crate::native_defaults::record(lua, &name);
            context.not_simulated(&name);
            crate::native_defaults::values(lua, &returns, &schemas, &context, 0)
        })?;
        standins.raw_set(path.as_str(), native.clone())?;
        owner.raw_set(key, native)?;
    }
    Ok(())
}
fn value(lua: &Lua, node: &Node) -> mlua::Result<Value> {
    Ok(match node {
        Node::Number(n) => Value::Number(*n),
        Node::Text(s) => Value::String(lua.create_string(s)?),
        Node::Bool(b) => Value::Boolean(*b),
        Node::Ref(path) => {
            let mut current = Value::Table(lua.globals());
            for part in path.split('.') {
                current = match current {
                    Value::Table(t) => t.raw_get(part)?,
                    _ => Value::Nil,
                };
            }
            if matches!(
                current,
                Value::Boolean(_) | Value::Integer(_) | Value::Number(_) | Value::String(_)
            ) {
                current
            } else {
                Value::Nil
            }
        }
        _ => Value::Nil,
    })
}
fn missing(lua: &Lua, ctx: &crate::Ctx, table: &Table, path: &str) -> mlua::Result<()> {
    let mt = lua.create_table()?;
    let ctx = ctx.clone();
    let path = path.to_owned();
    mt.raw_set(
        "__index",
        lua.create_function(move |lua, (owner, key): (Table, String)| {
            if path == "Constants.InventoryConstants" {
                let slots = crate::world::bag_slots(lua)?;
                match key.as_str() {
                    "NumBagSlots" => return Ok(Value::Integer(i64::from(slots.ordinary))),
                    "NumReagentBagSlots" => return Ok(Value::Integer(i64::from(slots.reagent))),
                    _ => {}
                }
            }
            ctx.not_simulated(&format!("undocumented client value: {path}.{key}"));
            if path == "Enum" || path == "Constants" {
                let child = lua.create_table()?;
                missing(lua, &ctx, &child, &format!("{path}.{key}"))?;
                owner.raw_set(key, child.clone())?;
                return Ok(Value::Table(child));
            }
            if path.starts_with("Enum.") {
                // Synthetic engine identity, not a client value. Hash the name
                // so independent sessions agree regardless of lookup order.
                // Negative integers avoid ordinary documented enum ranges;
                // collision probing also checks every existing real value.
                let mut hash = 2166136261u32;
                for byte in key.bytes() {
                    hash = (hash ^ u32::from(byte)).wrapping_mul(16777619);
                }
                let mut number = -1 - i64::from(hash & 0x3fff_ffff);
                while owner
                    .clone()
                    .pairs::<Value, Value>()
                    .any(|entry| match entry {
                        Ok((_, Value::Integer(n))) => n == number,
                        Ok((_, Value::Number(n))) => n == number as f64,
                        _ => false,
                    })
                {
                    number -= 1;
                }
                owner.raw_set(key, number)?;
                return Ok(Value::Integer(number));
            }
            Ok(Value::Nil)
        })?,
    )?;
    table.set_metatable(Some(mt));
    Ok(())
}
fn apply(lua: &Lua, ctx: &crate::Ctx, data: &Metadata) -> mlua::Result<()> {
    for namespace in ["Enum", "Constants"] {
        // Default UI must use this client's documentation, not values from a
        // different/stale checked addon manifest or a former simulator export.
        let table = lua.create_table()?;
        missing(lua, ctx, &table, namespace)?;
        lua.globals().raw_set(namespace, table)?;
    }
    for (path, values) in &data.tables {
        let (namespace, name) = path.split_once('.').unwrap();
        let parent: Table = lua.globals().raw_get(namespace)?;
        let table = parent
            .raw_get::<Option<Table>>(name)?
            .unwrap_or(lua.create_table()?);
        for (key, node) in values {
            if !matches!(node, Node::Ref(_)) {
                table.raw_set(key.as_str(), value(lua, node)?)?;
            }
        }
        missing(lua, ctx, &table, path)?;
        parent.raw_set(name, table)?;
    }
    let mut pending = Vec::new();
    for (path, values) in &data.tables {
        let (namespace, name) = path.split_once('.').unwrap();
        let parent: Table = lua.globals().raw_get(namespace)?;
        let table: Table = parent.raw_get(name)?;
        for (key, node) in values {
            if matches!(node, Node::Ref(_)) {
                pending.push((table.clone(), format!("{path}.{key}"), key, node));
            }
        }
    }
    loop {
        let before = pending.len();
        let mut next = Vec::new();
        for (table, path, key, node) in pending {
            let v = value(lua, node)?;
            if v.is_nil() {
                next.push((table, path, key, node))
            } else {
                table.raw_set(key.as_str(), v)?;
            }
        }
        pending = next;
        if pending.len() == before {
            break;
        }
    }
    for (_, path, _, _) in pending {
        ctx.not_simulated(&format!("undocumented client value: {path}"));
    }
    let constants: Table = lua.globals().raw_get("Constants")?;
    let inventory: Table = constants.get("InventoryConstants")?;
    crate::world::documented_bag_slots(
        lua,
        inventory.raw_get("NumBagSlots")?,
        inventory.raw_get("NumReagentBagSlots")?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_documentation_supplies_private_natives_and_real_record_mixins() {
        let s = crate::Session::new(Default::default()).unwrap();
        let mut data = Metadata::default();
        data.read(r#"local fixture={Name="Fixture",Type="System",Namespace="C_Fixture",Functions={
            {Name="Read",Type="Function",Returns={{Name="result",Type="FixtureRecord",Mixin="FixtureMixin"},{Name="ok",Type="bool"}}},
            {Name="Existing",Type="Function",Returns={{Name="result",Type="number"}}}
        },Tables={{Name="FixtureRecord",Type="Structure",Fields={{Name="count",Type="number"}}}}}"#);
        s.eval("C_Fixture={Existing=function() return 19 end}; FixtureMixin={Count=function(self)return self.count end}").unwrap();
        install_natives(s.lua(), s.sim().ctx(), &data).unwrap();
        s.eval("local r,ok=C_Fixture.Read();assert(r.count==0 and r:Count()==0 and ok==false);assert(C_Fixture.Existing()==19);assert(C_Fixture.NotDocumented==nil);C_Fixture.Read()").unwrap();
        assert_eq!(
            s.logs()
                .iter()
                .filter(|s| s.contains("not simulated: C_Fixture.Read"))
                .count(),
            1
        );
    }
    #[test]
    fn documented_constant_variants_use_the_selected_game_and_family() {
        let source = r#"{Tables={
            {Name="Fixture_Camelot",Type="Constants",Values={{Name="Choice",Value=7}}},
            {Name="Fixture_Mainline",Type="Constants",Values={{Name="Choice",Value=29}}},
            {Name="Fixture_Classic",Type="Constants",Values={{Name="Choice",Value=41}}},
            {Name="Direct",Type="Constants",Values={{Name="Choice",Value=13}}},
            {Name="Direct_Mainline",Type="Constants",Values={{Name="Choice",Value=17}}},
            {Name="Unrelated_Other",Type="Constants",Values={{Name="Choice",Value=19}}}
        }}"#;
        for (target, expected) in [("wowf", 7), ("wow", 29), ("wow_classic_era", 41)] {
            let mut data = Metadata::default();
            data.read(source);
            data.expose_constant_aliases(Some(target));
            let s = crate::Session::new(Default::default()).unwrap();
            apply(s.lua(), s.sim().ctx(), &data).unwrap();
            s.eval(&format!("assert(Constants.Fixture.Choice=={expected});assert(Constants.Direct.Choice==13);assert(type(Constants.Unrelated)=='table' and Constants.Unrelated.Choice==nil)")).unwrap();
        }
    }
    #[test]
    fn synthetic_enums_are_stable_unique_and_constants_stay_unknown() {
        for reverse in [false, true] {
            let s = crate::Session::new(Default::default()).unwrap();
            apply(s.lua(), s.sim().ctx(), &Metadata::default()).unwrap();
            if reverse {
                s.eval("local x=Enum.Synthetic.Second").unwrap();
            }
            // Owner-approved change: missing Constant namespaces exist, while
            // their unknown members remain nil (no numeric value fallback).
            s.eval("assert(type(Enum.Synthetic.First)=='number'); assert(Enum.Synthetic.First==Enum.Synthetic.First); assert(Enum.Synthetic.First~=Enum.Synthetic.Second); assert(type(Constants.Missing)=='table' and Constants.Missing.Value==nil)").unwrap();
            let first: i64 = s.lua().load("return Enum.Synthetic.First").eval().unwrap();
            s.lua()
                .set_named_registry_value("selu.default_ui_strings", true)
                .unwrap();
            let schema = serde_json::from_value(
                serde_json::json!({"name":"Synthetic","type":"enum","values":{"First":7}}),
            )
            .unwrap();
            let schemas = Arc::new(std::collections::HashMap::from([(
                "Enum.Synthetic".into(),
                schema,
            )]));
            let native = crate::native_defaults::value(
                s.lua(),
                "Enum.Synthetic",
                &schemas,
                s.sim().ctx(),
                0,
            )
            .unwrap();
            assert_eq!(native, Value::Integer(first));
            let again = crate::Session::new(Default::default()).unwrap();
            apply(again.lua(), again.sim().ctx(), &Metadata::default()).unwrap();
            assert_eq!(
                first,
                again
                    .lua()
                    .load("return Enum.Synthetic.First")
                    .eval::<i64>()
                    .unwrap()
            );
            assert_eq!(
                s.logs()
                    .iter()
                    .filter(|l| l.contains("undocumented client value: Enum.Synthetic.First"))
                    .count(),
                1
            );
        }
    }
    #[test]
    fn comments_and_nonliteral_values_are_not_declarations() {
        let mut data = Metadata::default();
        data.read(r#"--[=[
        {Name="Poison",Type="Enumeration",Fields={{Name="Injected",EnumValue=888}}}
        ]=]
        local t={Name="Synthetic",Type="Constants",Values={{Name="Good",Value=1e-3},{Name="Bad",Value=os.execute('never')},{Name="Text",Value="a\098"}}}
        "#);
        assert!(!data.tables.contains_key("Enum.Poison"));
        let s = crate::Session::new(Default::default()).unwrap();
        apply(s.lua(), s.sim().ctx(), &data).unwrap();
        s.eval("assert(Constants.Synthetic.Good==0.001 and Constants.Synthetic.Text=='ab');assert(Constants.Synthetic.Bad==nil);assert(Constants.Synthetic.Bad==nil)").unwrap();
        assert_eq!(
            s.logs()
                .iter()
                .filter(|line| line.contains("undocumented client value: Constants.Synthetic.Bad"))
                .count(),
            1
        );
    }
    #[test]
    #[ignore = "requires read-only wowf, wow and wow_classic_era client storage"]
    fn installed_product_documentation() {
        let root = std::env::var_os("SELU_CLIENT_INSTALL").unwrap();
        let source = std::env::temp_dir().join(format!("selu-client-docs-{}", std::process::id()));
        std::fs::create_dir_all(&source).unwrap();
        let game_mode_id = selu_gamedata::Storage::open(Path::new(&root), "wow_classic_era", None)
            .unwrap()
            .resolve("DBFilesClient/GameMode.db2")
            .ok();
        for product in ["wowf", "wow", "wow_classic_era"] {
            let storage = selu_gamedata::Storage::open(Path::new(&root), product, None).unwrap();
            if let Some(id) = game_mode_id {
                match storage.read(id) {
                    Ok(bytes) => {
                        eprintln!(
                            "CLIENT_DATA_SEARCH {product} GameMode by runtime resolved ID: {} bytes",
                            bytes.len()
                        );
                        match selu_gamedata::db2::Table::parse(&bytes) {
                            Ok(table) => {
                                eprintln!(
                                    "GameMode records={} layout={}",
                                    table.count(),
                                    table.layout
                                );
                                for row in 0..table.count().min(8) {
                                    for column in 0..8 {
                                        if let Ok(text) = table.text(row, column)
                                            && !text.is_empty()
                                        {
                                            eprintln!(
                                                "GameMode string row={row} column={column}: {text}"
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => eprintln!("GameMode table decode unavailable: {e}"),
                        }
                    }
                    Err(e) => eprintln!("GameMode data unavailable: {e}"),
                }
            }
            for path in [
                "Interface/AddOns/Blizzard_SharedXML/Enum.lua",
                "Interface/AddOns/Blizzard_SharedXML/Enums.lua",
                "Interface/AddOns/Blizzard_SharedXMLBase/Enum.lua",
                "Interface/AddOns/Blizzard_APIDocumentationGenerated/GameModeConstantsDocumentation.lua",
                "Interface/AddOns/Blizzard_APIDocumentationGenerated/GameRuleConstantsDocumentation.lua",
                "DBFilesClient/GameMode.db2",
                "DBFilesClient/GameRule.db2",
                "DBFilesClient/GameRules.db2",
            ] {
                eprintln!(
                    "CLIENT_DATA_SEARCH {product} {path}: {:?}",
                    storage.read_path(path).map(|b| b.len())
                );
            }
            let path = "Interface/AddOns/Blizzard_APIDocumentationGenerated/Blizzard_APIDocumentationGenerated.toc";
            eprintln!(
                "PRODUCT_DOCUMENTATION {product} build={} toc={:?}",
                storage.build.version,
                storage.read_path(path).map(|b| b.len())
            );
            let s = crate::Session::new(crate::SessionConfig {
                target: Some(product.into()),
                client_install: Some(root.clone().into()),
                ..Default::default()
            })
            .unwrap();
            install(s.lua(), s.sim().ctx(), &source, Some(product)).unwrap();
            let mut expected = Metadata::default();
            for file in [
                "InputConstantsDocumentation.lua",
                "AuctionHouseConstantsDocumentation.lua",
            ] {
                expected.read(
                    &String::from_utf8(
                        storage
                            .read_path(&format!(
                                "Interface/AddOns/Blizzard_APIDocumentationGenerated/{file}"
                            ))
                            .unwrap(),
                    )
                    .unwrap(),
                );
            }
            // Variants use the selected client's own values, including the
            // unsuffixed native name when its game/family form is documented.
            if let Ok(bytes) = storage.read_path("Interface/AddOns/Blizzard_APIDocumentationGenerated/LevelConstantsDocumentation.lua") {
                expected.read(&String::from_utf8(bytes).unwrap());
            }
            expected.expose_constant_aliases(Some(product));
            assert!(expected.tables.keys().any(|p| p.starts_with("Enum.")));
            assert!(expected.tables.keys().any(|p| p.starts_with("Constants.")));
            for (path, fields) in &expected.tables {
                let (namespace, name) = path.split_once('.').unwrap();
                let namespace: Table = s.lua().globals().raw_get(namespace).unwrap();
                let table: Table = namespace.raw_get(name).unwrap();
                for (key, node) in fields {
                    assert_eq!(
                        table.raw_get::<Value>(key.as_str()).unwrap(),
                        value(s.lua(), node).unwrap(),
                        "{product}: {path}.{key}"
                    );
                }
            }
        }
    }
    #[test]
    fn documentation_is_data_and_sessions_do_not_share_values() {
        let root = std::env::temp_dir().join(format!("selu-doc-data-{}", std::process::id()));
        let dir = root.join("Interface/AddOns/Blizzard_APIDocumentationGenerated");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Blizzard_APIDocumentationGenerated.toc"),
            "Example.lua\n",
        )
        .unwrap();
        for expected in [7, 29] {
            let text=r#"local x={Tables={{Name="Example",Type="Enumeration",Fields={{Name="Choice",EnumValue=EXPECTED}}},{Name="Options",Type="Constants",Values={{Name="Enabled",Value=true},{Name="Expression",Value=2+3},{Name="AliasA",Value=Constants.Options.AliasB},{Name="AliasB",Value=Enum.Example.Choice}}}}}; error('must never execute')"#.replace("EXPECTED",&expected.to_string());
            std::fs::write(dir.join("Example.lua"), text).unwrap();
            let s = crate::Session::new(Default::default()).unwrap();
            install(
                s.lua(),
                s.sim().ctx(),
                &root,
                Some(&format!("fixture-{expected}@1")),
            )
            .unwrap();
            s.eval(&format!(
                "assert(Enum.Example.Choice=={expected});assert(Constants.Options.Enabled==true)"
            ))
            .unwrap();
            s.eval(&format!("assert(Constants.Options.AliasA=={expected})"))
                .unwrap();
            s.eval("assert(Constants.Options.Expression==nil)").unwrap();
        }
    }
}

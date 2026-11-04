//! Static Lua 5.1 AST discovery. Never executes addon code.
use full_moon::{
    ast::*,
    node::Node,
    tokenizer::{TokenReference, TokenType},
    visitors::Visitor,
};
use selu_manifest::{Evidence, FunctionEntry, FunctionSource, Manifest};
use std::collections::{BTreeMap, BTreeSet};
fn name(t: &TokenReference) -> String {
    match t.token().token_type() {
        TokenType::Identifier { identifier } => identifier.to_string(),
        _ => "...".into(),
    }
}
fn start(n: &impl Node) -> usize {
    n.start_position().map_or(0, |p| p.bytes())
}
fn end(n: &impl Node) -> usize {
    n.end_position().map_or(usize::MAX, |p| p.bytes())
}
#[derive(Default)]
struct Scan {
    blocks: Vec<(usize, usize)>,
    depth: usize,
    repeat_ends: BTreeMap<usize, usize>,
    locals: Vec<(String, usize, usize)>,
    definitions: Vec<(String, usize, Vec<String>, bool)>,
    calls: Vec<(String, usize)>,
    frame_calls: Vec<FunctionCall>,
    numeric_loops: Vec<NumericFor>,
    assignments: Vec<(String, usize)>,
    function_bodies: Vec<(usize, usize)>,
    tables: Vec<(String, usize, TableConstructor)>,
    members: Vec<(String, String, usize)>,
    signatures: Vec<(String, usize, FunctionBody)>,
    scalars: Vec<(String, usize, Expression)>,
    mixins: Vec<(String, usize, Vec<String>, String)>,
}
impl Scan {
    fn binding(&self, n: &str, pos: usize) -> Option<(usize, usize)> {
        self.locals
            .iter()
            .filter(|(v, a, b)| v == n && *a <= pos && pos <= *b)
            .max_by_key(|(_, a, _)| *a)
            .map(|(_, a, b)| (*a, *b))
    }

    fn bind(&mut self, n: String, from: usize, to: usize) {
        self.locals.push((n, from, to));
    }
    fn local(&self, n: &str, pos: usize) -> bool {
        self.locals
            .iter()
            .any(|(v, a, b)| v == n && *a <= pos && pos <= *b)
    }
}
impl Visitor for Scan {
    fn visit_block(&mut self, n: &Block) {
        self.blocks.push((
            start(n),
            self.repeat_ends
                .get(&start(n))
                .copied()
                .unwrap_or_else(|| end(n)),
        ));
    }
    fn visit_repeat(&mut self, n: &Repeat) {
        self.repeat_ends.insert(start(n.block()), end(n));
    }
    fn visit_block_end(&mut self, _: &Block) {
        self.blocks.pop();
    }
    fn visit_local_assignment(&mut self, n: &LocalAssignment) {
        let to = self.blocks.last().unwrap().1;
        for t in n.names() {
            self.bind(name(t), end(n), to);
        }
    }
    fn visit_local_function(&mut self, n: &LocalFunction) {
        self.bind(name(n.name()), start(n), self.blocks.last().unwrap().1);
    }
    fn visit_function_body(&mut self, n: &FunctionBody) {
        self.function_bodies.push((start(n), end(n)));
        self.depth += 1;
        for p in n.parameters() {
            if let Parameter::Name(t) = p {
                self.bind(name(t), start(n.block()), end(n.block()));
            }
        }
        // Method self is handled by the declaration; implicit self is never a global call.
    }
    fn visit_function_body_end(&mut self, _: &FunctionBody) {
        self.depth -= 1;
    }
    fn visit_function_declaration(&mut self, n: &FunctionDeclaration) {
        if self.depth == 0 {
            let names: Vec<_> = n.name().names().iter().map(name).collect();
            if let Some(method) = n.name().method_name() {
                if names.len() == 1 {
                    self.signatures.push((
                        format!("{}.{}", names[0], name(method)),
                        start(n),
                        n.body().clone(),
                    ));
                    self.members
                        .push((names[0].clone(), name(method), start(n)));
                }
            } else if names.len() == 2 {
                self.signatures
                    .push((names.join("."), start(n), n.body().clone()));
                self.members
                    .push((names[0].clone(), names[1].clone(), start(n)));
            }
        }
        if n.name().method_name().is_some() {
            self.bind(
                "self".into(),
                start(n.body().block()),
                end(n.body().block()),
            );
        } else if n.name().names().len() == 1 {
            let t = n.name().names().iter().next().unwrap();
            self.assignments.push((name(t), start(n)));
            self.definitions.push((
                name(t),
                start(n),
                n.body()
                    .parameters()
                    .iter()
                    .map(|p| match p {
                        Parameter::Name(t) => name(t),
                        _ => "...".into(),
                    })
                    .collect(),
                self.depth == 0,
            ));
        }
    }
    fn visit_assignment(&mut self, n: &Assignment) {
        for v in n.variables() {
            if let Var::Name(t) = v {
                self.assignments.push((name(t), start(n)));
            }
        }
        for (v, e) in n.variables().iter().zip(n.expressions().iter()) {
            if self.depth == 0
                && let Var::Name(t) = v
            {
                self.scalars.push((name(t), start(n), e.clone()));
                if let Expression::FunctionCall(call) = e
                    && let Some((_, parents)) = mixin_call(call, Some(name(t)))
                {
                    self.mixins.push((
                        name(t),
                        start(n),
                        parents,
                        match call.prefix() {
                            Prefix::Name(t) => name(t),
                            _ => unreachable!(),
                        },
                    ));
                }
            }
            if self.depth == 0
                && let (Var::Name(t), Expression::TableConstructor(table)) = (v, e)
            {
                self.tables.push((name(t), start(n), (**table).clone()));
            }
            if let (Var::Name(t), Expression::Function(f)) = (v, e) {
                self.definitions.push((
                    name(t),
                    start(n),
                    f.body()
                        .parameters()
                        .iter()
                        .map(|p| match p {
                            Parameter::Name(t) => name(t),
                            _ => "...".into(),
                        })
                        .collect(),
                    self.depth == 0,
                ));
            }
        }
    }
    fn visit_function_call(&mut self, n: &FunctionCall) {
        if matches!(n.prefix(), Prefix::Name(t) if name(t) == "CreateFrame") {
            self.frame_calls.push(n.clone());
        }
        if self.depth == 0
            && let Some((target, parents)) = mixin_call(n, None)
        {
            self.mixins.push((
                target,
                start(n),
                parents,
                match n.prefix() {
                    Prefix::Name(t) => name(t),
                    _ => unreachable!(),
                },
            ));
        }
        if let Prefix::Name(t) = n.prefix()
            && matches!(
                n.suffixes().next(),
                Some(Suffix::Call(Call::AnonymousCall(_)))
            )
        {
            self.calls.push((name(t), start(n)));
        }
    }
    fn visit_numeric_for(&mut self, n: &NumericFor) {
        self.numeric_loops.push(n.clone());
        self.bind(name(n.index_variable()), start(n.block()), end(n.block()));
    }
    fn visit_generic_for(&mut self, n: &GenericFor) {
        for t in n.names() {
            self.bind(name(t), start(n.block()), end(n.block()));
        }
    }
}
#[derive(Default)]
pub struct Sources {
    signatures: BTreeMap<String, selu_manifest::Declaration>,
    runtime_frames: Vec<(String, String, String, String, String)>,
    definitions: BTreeMap<String, (String, Vec<String>)>,
    all_definitions: BTreeSet<String>,
    calls: BTreeMap<String, String>,
    tables: BTreeMap<String, selu_manifest::Declaration>,
    scalars: BTreeMap<String, Vec<(Scalar, bool)>>,
}
impl Sources {
    pub fn parse(&mut self, file: &str, source: &str) -> Result<(), String> {
        let ast = full_moon::parse(source.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("{file}: Lua parse error: {e:?}"))?;
        let mut scan = Scan::default();
        scan.visit_ast(&ast);
        for call in &scan.frame_calls {
            if scan.local("CreateFrame", start(call)) {
                continue;
            }
            let Some(Suffix::Call(Call::AnonymousCall(call_args))) = call.suffixes().next() else {
                continue;
            };
            let FunctionArgs::Parentheses { arguments, .. } = call_args.as_ref() else {
                continue;
            };
            let args: Vec<_> = arguments.iter().collect();
            if args.len() < 2 {
                continue;
            }
            if !args[0].to_string().trim().starts_with(['"', '\'']) {
                continue;
            }
            let widget = args[0]
                .to_string()
                .trim()
                .trim_matches(['\"', '\''])
                .to_string();
            let key = args[1].to_string();
            let parts: Vec<_> = key.split("..").map(str::trim).collect();
            if parts.len() != 2 || !parts[0].starts_with(['\"', '\'']) {
                continue;
            }
            for lp in &scan.numeric_loops {
                if start(lp.block()) <= start(call)
                    && end(call) <= end(lp.block())
                    && parts[1] == name(lp.index_variable())
                    && scan.binding(parts[1], start(call))
                        == Some((start(lp.block()), end(lp.block())))
                    && !scan.assignments.iter().any(|(n, p)| {
                        n == parts[1]
                            && scan.binding(n, *p) == Some((start(lp.block()), end(lp.block())))
                    })
                    && !scan
                        .function_bodies
                        .iter()
                        .any(|(a, b)| *a > start(lp) && *a < start(call) && end(call) <= *b)
                    && lp.step().is_none()
                    && !scan.local(lp.start().to_string().trim(), start(lp))
                    && !scan.local(lp.end().to_string().trim(), start(lp))
                {
                    self.runtime_frames.push((
                        parts[0].trim_matches(['\"', '\'']).into(),
                        widget.clone(),
                        lp.start().to_string().trim().into(),
                        lp.end().to_string().trim().into(),
                        format!("{file}:{}", call.start_position().unwrap().line()),
                    ));
                }
            }
        }
        for (n, pos, e) in &scan.scalars {
            if !scan.local(n, *pos)
                && let Some(expr) = Scalar::from_ast(e, &scan, *pos)
            {
                self.scalars
                    .entry(n.clone())
                    .or_default()
                    .push((expr, !localisation(file)));
            }
        }
        for (target, pos, parents, callee) in &scan.mixins {
            if !scan.local(callee, *pos)
                && !scan.local(target, *pos)
                && parents.iter().all(|p| !scan.local(p, *pos))
            {
                let d = self
                    .tables
                    .entry(target.clone())
                    .or_insert_with(|| crate::xml::declaration(target, "table", file));
                for parent in parents {
                    if !d.inherits.contains(parent) {
                        d.inherits.push(parent.clone());
                    }
                }
            }
        }
        for (n, pos, table) in &scan.tables {
            let lower = file.to_ascii_lowercase();
            if table.fields().len() > 2048
                || ["questdata", "mapdata"]
                    .iter()
                    .any(|part| lower.contains(part))
            {
                continue;
            }
            if !scan.local(n, *pos) {
                let d = self
                    .tables
                    .entry(n.clone())
                    .or_insert_with(|| crate::xml::declaration(n, "table", file));
                for field in table.fields().iter() {
                    let (key, value) = match field {
                        Field::NameKey { key, value, .. } => (Some(name(key)), value.as_ref()),
                        Field::ExpressionKey { key, value, .. } => (
                            literal(key)
                                .filter(safe_value)
                                .and_then(|v| v.as_str().map(str::to_owned)),
                            value.as_ref(),
                        ),
                        _ => continue,
                    };
                    if let Some(key) = key {
                        if matches!(value, Expression::Function(_))
                            && !d.methods.iter().any(|f| f.name == key)
                        {
                            d.methods.push(crate::xml::declaration(&key, "", file));
                        }
                        if !d.fields.iter().any(|f| f.name == key) {
                            d.fields.push(crate::xml::declaration(&key, "", file));
                        }
                        if let Some(value) =
                            literal(value).filter(|v| !localisation(file) && safe_value(v))
                        {
                            d.values.insert(key, value);
                        }
                    }
                }
            }
        }
        for (qualified, pos, body) in &scan.signatures {
            let root = qualified.split('.').next().unwrap();
            if !scan.local(root, *pos) {
                let signature = source_signature(qualified, body, &source[..*pos], file);
                self.signatures
                    .entry(qualified.clone())
                    .or_insert(signature);
            }
        }
        for (root, member, pos) in &scan.members {
            if !scan.local(root, *pos) {
                let d = self
                    .tables
                    .entry(root.clone())
                    .or_insert_with(|| crate::xml::declaration(root, "table", file));
                if !d.methods.iter().any(|m| m.name == *member) {
                    d.methods.push(crate::xml::declaration(member, "", file));
                }
            }
        }
        for (n, pos, args, top) in &scan.definitions {
            if !scan.local(n, *pos) {
                self.all_definitions.insert(n.clone());
                if *top {
                    self.definitions
                        .entry(n.clone())
                        .or_insert((file.into(), args.clone()));
                }
            }
        }
        for (n, pos) in &scan.calls {
            if !scan.local(n, *pos) {
                self.calls.entry(n.clone()).or_insert(file.into());
            }
        }
        Ok(())
    }
    pub fn extend_manifest(self, m: &mut Manifest) -> (usize, usize) {
        let mut types = BTreeMap::new();
        for table in &m.tables {
            if table.ty.as_deref() == Some("Constants") {
                for field in &table.fields {
                    if let Some(ty) = &field.ty {
                        types.insert(
                            format!("Constants.{}.{}", table.name, field.name),
                            ty.clone(),
                        );
                    }
                }
            }
        }
        loop {
            let mut changed = false;
            for (name, expressions) in &self.scalars {
                if !types.contains_key(name)
                    && let Some(ty) = expressions.iter().find_map(|(e, _)| e.ty(&types))
                {
                    types.insert(name.clone(), ty);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for (name, expressions) in &self.scalars {
            if let Some(ty) = types
                .get(name)
                .filter(|t| t.as_str() == "number" || t.as_str() == "string")
                && !m.globals.iter().any(|g| &g.name == name)
            {
                let value = expressions.iter().find_map(|(e, values)| match e {
                    Scalar::Literal(v) if *values && safe_value(v) => Some(v.clone()),
                    _ => None,
                });
                m.globals.push(selu_manifest::GlobalEntry {
                    name: name.clone(),
                    ty: Some(ty.clone()),
                    value,
                    evidence: Evidence::Source,
                    templates: vec![],
                });
            }
        }
        for (prefix, widget, low, high, evidence) in &self.runtime_frames {
            let bound = |s: &str| {
                s.parse::<u32>().ok().or_else(|| {
                    m.globals
                        .iter()
                        .find(|g| g.name == s)
                        .and_then(|g| g.value.as_ref())
                        .and_then(|v| v.as_f64())
                        .filter(|v| v.fract() == 0.0 && *v >= 0.0 && *v <= u32::MAX as f64)
                        .map(|v| v as u32)
                })
            };
            if let (Some(lo), Some(hi)) = (bound(low), bound(high)) {
                if lo > hi || hi - lo > 1000 {
                    continue;
                }
                m.frame_families.push(selu_manifest::FrameFamily {
                    prefix: prefix.clone(),
                    suffix: String::new(),
                    widget: widget.clone(),
                    min_index: lo,
                    max_index: hi,
                    sources: vec![evidence.clone()],
                });
                for i in lo..=hi {
                    let name = format!("{prefix}{i}");
                    if !m
                        .globals
                        .iter()
                        .any(|g| g.name == name && g.ty.as_deref() == Some(widget.as_str()))
                    {
                        m.globals.push(selu_manifest::GlobalEntry {
                            name,
                            ty: Some(widget.clone()),
                            value: None,
                            evidence: Evidence::Source,
                            templates: vec![],
                        });
                    }
                }
            }
        }
        for (_, mut d) in self.tables {
            if d.fields.len() > 2048 {
                continue;
            }
            d.fields.sort_by(|a, b| a.name.cmp(&b.name));
            d.methods.sort_by(|a, b| a.name.cmp(&b.name));
            if !m.globals.iter().any(|g| g.name == d.name) {
                m.globals.push(selu_manifest::GlobalEntry {
                    value: None,
                    name: d.name.clone(),
                    ty: None,
                    evidence: Evidence::Source,
                    templates: vec![],
                });
            }
            // Large generated datasets are not API metadata.
            if d.fields.len() <= 2048 {
                if !d.name.chars().all(|c| !c.is_ascii_lowercase())
                    || serde_json::to_vec(&d.values).unwrap().len() > 65536
                {
                    d.values.clear();
                }
                let entry = crate::xml::compact(d, m);
                m.data_globals.push(entry);
            }
        }
        for (name, signature) in self.signatures {
            if !m.functions.iter().any(|f| f.name == name) {
                let mut f = entry(
                    name,
                    String::new(),
                    "defined",
                    signature.arguments.iter().map(|a| a.name.clone()).collect(),
                );
                f.signature = Some(signature);
                m.functions.push(f);
            }
        }
        let mut known: BTreeSet<String> = m.functions.iter().map(|f| f.name.clone()).collect();
        let mut counts = (0, 0);
        for (n, (file, args)) in self.definitions {
            if known.insert(n.clone()) {
                m.functions.push(entry(n, file, "defined", args));
                counts.0 += 1;
            }
        }
        for (n, file) in self.calls {
            if !self.all_definitions.contains(&n) && known.insert(n.clone()) {
                m.functions.push(entry(n, file, "called", vec![]));
                counts.1 += 1;
            }
        }
        counts
    }
}
// Source-only signatures preserve arity and primitive EmmyLua annotations.
// Unspecified types use the checker's existing untyped-game-value rule.
fn source_signature(
    qualified: &str,
    body: &FunctionBody,
    prefix: &str,
    file: &str,
) -> selu_manifest::Declaration {
    let mut d = crate::xml::declaration(qualified.rsplit('.').next().unwrap(), "Function", file);
    let mut annotations = BTreeMap::new();
    let mut returns = Vec::new();
    for line in prefix
        .lines()
        .rev()
        .skip_while(|line| line.trim().is_empty())
    {
        let line = line.trim();
        if !line.starts_with("--") {
            break;
        }
        if let Some(param) = line.strip_prefix("---@param ") {
            let mut words = param.split_whitespace();
            if let (Some(n), Some(t)) = (words.next(), words.next()) {
                annotations.insert(n.to_owned(), t.to_owned());
            }
        }
        if let Some(ret) = line.strip_prefix("---@return ")
            && let Some(t) = ret.split_whitespace().next()
        {
            returns.push(t.to_owned());
        }
    }
    for param in body.parameters() {
        let n = match param {
            Parameter::Name(t) => name(t),
            _ => "...".into(),
        };
        let mut a = crate::xml::declaration(
            &n,
            annotations.get(&n).map(String::as_str).unwrap_or(""),
            "",
        );
        a.attributes.clear();
        if a.ty.as_deref() == Some("") {
            a.attributes.insert("Nilable".into(), true.into());
        }
        d.arguments.push(a);
    }
    if returns.is_empty() {
        returns.push(String::new());
    }
    returns.reverse();
    for ty in returns {
        let mut r = crate::xml::declaration("result", &ty, "");
        r.attributes.clear();
        d.returns.push(r);
    }
    d
}
fn entry(name: String, file: String, kind: &str, params: Vec<String>) -> FunctionEntry {
    FunctionEntry {
        name,
        params,
        available: true,
        restrictions: vec![],
        returns_secret: false,
        evidence: Evidence::Source,
        signature: None,
        provenance: Some(FunctionSource {
            file: file
                .strip_prefix("Interface/AddOns/")
                .unwrap_or(&file)
                .into(),
            kind: kind.into(),
        }),
    }
}

fn localisation(file: &str) -> bool {
    let file = file.to_ascii_lowercase();
    ["localization", "localisation", "locale", "globalstrings"]
        .iter()
        .any(|p| file.contains(p))
}
fn safe_value(v: &serde_json::Value) -> bool {
    v.is_number()
        || v.as_str().is_some_and(|s| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
        })
}
fn mixin_call(call: &FunctionCall, destination: Option<String>) -> Option<(String, Vec<String>)> {
    let Prefix::Name(callee) = call.prefix() else {
        return None;
    };
    let mut suffixes = call.suffixes();
    let Suffix::Call(Call::AnonymousCall(args)) = suffixes.next()? else {
        return None;
    };
    let FunctionArgs::Parentheses { arguments, .. } = args.as_ref() else {
        return None;
    };
    if suffixes.next().is_some() {
        return None;
    }
    let args: Vec<_> = arguments.iter().collect();
    let (target, parents) = match (name(callee).as_str(), destination) {
        ("CreateFromMixins", Some(target)) => (target, args.as_slice()),
        ("Mixin", Some(target)) => match args.first()? {
            Expression::TableConstructor(t) if t.fields().is_empty() => (target, args.get(1..)?),
            Expression::Var(Var::Name(_)) => (target, args.as_slice()),
            _ => return None,
        },
        ("Mixin", None) => {
            let Expression::Var(Var::Name(target)) = args.first()? else {
                return None;
            };
            (name(target), args.get(1..)?)
        }
        _ => return None,
    };
    let parents: Option<Vec<_>> = parents
        .iter()
        .map(|e| match e {
            Expression::Var(Var::Name(n)) => Some(name(n)),
            _ => None,
        })
        .collect();
    Some((target, parents?))
}
#[derive(Clone)]
enum Scalar {
    Literal(serde_json::Value),
    Reference(String),
    Arithmetic(Box<Scalar>, Box<Scalar>),
    Concat,
}
impl Scalar {
    fn from_ast(e: &Expression, scan: &Scan, pos: usize) -> Option<Self> {
        if let Some(v) = literal(e) {
            return Some(Self::Literal(v));
        }
        match e {
            Expression::Parentheses { expression, .. } => Self::from_ast(expression, scan, pos),
            Expression::Var(v) => {
                let path = match v {
                    Var::Name(t) => name(t),
                    Var::Expression(v) => {
                        let Prefix::Name(root) = v.prefix() else {
                            return None;
                        };
                        let mut path = name(root);
                        for suffix in v.suffixes() {
                            let Suffix::Index(Index::Dot { name: t, .. }) = suffix else {
                                return None;
                            };
                            path.push('.');
                            path.push_str(&name(t));
                        }
                        path
                    }
                    _ => return None,
                };
                if scan.local(path.split('.').next()?, pos) {
                    None
                } else {
                    Some(Self::Reference(path))
                }
            }
            Expression::BinaryOperator { lhs, binop, rhs } => match binop.to_string().trim() {
                "+" | "-" | "*" | "/" | "%" | "^" => Some(Self::Arithmetic(
                    Box::new(Self::from_ast(lhs, scan, pos)?),
                    Box::new(Self::from_ast(rhs, scan, pos)?),
                )),
                ".." => Some(Self::Concat),
                _ => None,
            },
            _ => None,
        }
    }
    fn ty(&self, types: &BTreeMap<String, String>) -> Option<String> {
        match self {
            Self::Literal(v) => Some(if v.is_number() { "number" } else { "string" }.into()),
            Self::Reference(n) => types.get(n).cloned(),
            Self::Arithmetic(a, b)
                if a.ty(types).as_deref() == Some("number")
                    && b.ty(types).as_deref() == Some("number") =>
            {
                Some("number".into())
            }
            Self::Concat => Some("string".into()),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn member_signature_preserves_annotations_and_shadowing() {
        let mut sources = Sources::default();
        sources.parse("mixin.lua", "WidgetMixin = {};\n---@param value number\n---@return boolean\nfunction WidgetMixin:SetValue(value) return value > 0 end\ndo local Hidden = {}; function Hidden:Nope() end end").unwrap();
        let mut m: Manifest = serde_json::from_str(
            r#"{"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1}"#,
        )
        .unwrap();
        sources.extend_manifest(&mut m);
        let d = m
            .function("WidgetMixin.SetValue")
            .unwrap()
            .signature
            .as_ref()
            .unwrap();
        assert_eq!(d.arguments[0].ty.as_deref(), Some("number"));
        assert_eq!(d.returns[0].ty.as_deref(), Some("boolean"));
        assert!(m.function("Hidden.Nope").is_none());
    }
    #[test]
    fn lexical_globals_and_first_provenance() {
        let mut s = Sources::default();
        s.parse(
            "a.lua",
            r#"
  Native(); CrossFile(); Before(); local Before = function() end; Before();
  function Defined(x, ...) Native(); end
  Assigned = function(a,b) end
  local function Hidden() end; Hidden();
  local Alias = Native; Alias();
  object.Field(); object:Method();
  function object.Field() end
  function Wrapper(Native) Native(); end
  do local Scoped = function() end; Scoped(); end; Scoped();
  for Key in pairs({}) do Key(); end
  for Number=1,3 do Number(); end
  local Ghost = "TextFake()" -- CommentFake()
  "#,
        )
        .unwrap();
        s.parse("b.lua", "function CrossFile() end; Native()")
            .unwrap();
        let mut m: Manifest = serde_json::from_str(
            r#"{"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1}"#,
        )
        .unwrap();
        s.extend_manifest(&mut m);
        for n in [
            "Native",
            "CrossFile",
            "Defined",
            "Assigned",
            "Wrapper",
            "Scoped",
            "Before",
            "pairs",
        ] {
            assert!(m.function(n).is_some(), "{n}");
        }
        for n in [
            "Hidden",
            "Alias",
            "Field",
            "Method",
            "Key",
            "Number",
            "Ghost",
            "TextFake",
            "CommentFake",
        ] {
            assert!(m.function(n).is_none(), "{n}");
        }
        assert_eq!(m.function("Defined").unwrap().params, ["x", "..."]);
        assert_eq!(m.function("Assigned").unwrap().params, ["a", "b"]);
        assert_eq!(
            m.function("Native")
                .unwrap()
                .provenance
                .as_ref()
                .unwrap()
                .file,
            "a.lua"
        );
        assert_eq!(
            m.function("CrossFile")
                .unwrap()
                .provenance
                .as_ref()
                .unwrap()
                .kind,
            "defined"
        );
        assert!(Sources::default().parse("bad.lua", "function (").is_err());
    }
}

fn literal(e: &Expression) -> Option<serde_json::Value> {
    match e {
        Expression::Number(t) => mlua::Lua::new()
            .load(format!("return {}", t.token()))
            .eval::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(serde_json::Value::Number),
        Expression::String(t) => match t.token().token_type() {
            TokenType::StringLiteral { .. } => mlua::Lua::new()
                .load(format!("return {}", t.token()))
                .eval::<String>()
                .ok()
                .map(serde_json::Value::String),
            _ => None,
        },
        Expression::UnaryOperator { unop, expression } if unop.to_string().trim() == "-" => {
            literal(expression)?
                .as_f64()
                .and_then(|n| serde_json::Number::from_f64(-n))
                .map(serde_json::Value::Number)
        }
        _ => None,
    }
}

#[cfg(test)]
mod data_tests {
    use super::*;
    fn manifest() -> Manifest {
        serde_json::from_value(serde_json::json!({"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1})).unwrap()
    }
    #[test]
    fn review_mixin_inheritance() {
        let mut sources = Sources::default();
        sources.parse("mixins.lua", "A = {}; function A:Inherited() end; B = CreateFromMixins(A); C = {}; Mixin(C, B); function F() Hidden = CreateFromMixins(A) end; local Private = {}; LocalAlias = CreateFromMixins(Private)").unwrap();
        let mut m = manifest();
        sources.extend_manifest(&mut m);
        assert_eq!(
            m.data_globals
                .iter()
                .find(|d| d.name == "B")
                .unwrap()
                .inherits,
            ["A"]
        );
        assert_eq!(
            m.data_globals
                .iter()
                .find(|d| d.name == "C")
                .unwrap()
                .inherits,
            ["B"]
        );
        assert!(
            !m.data_globals
                .iter()
                .any(|d| d.name == "Hidden" || d.name == "LocalAlias")
        );
    }
    #[test]
    fn review_string_value_policy() {
        let mut sources = Sources::default();
        sources.parse("constants.lua", r#"HTML_START = "<html>"; SAFE = "Token:1-a.b"; CONSTANTS = { HTML = "<body>", PROSE = "PRIVATE SENTENCE never copy", SAFE = "Token" }"#).unwrap();
        sources.parse("Localization/enUS.lua", r#"LOCAL_TOKEN = "Token"; LOCAL_SENTENCE = "LOCALISATION SENTINEL sentence"; LOCALE_CONSTANTS = { TOKEN = "Token", SENTENCE = "TABLE LOCALISATION SENTINEL sentence" }"#).unwrap();
        let mut m = manifest();
        sources.extend_manifest(&mut m);
        let html = m.globals.iter().find(|g| g.name == "HTML_START").unwrap();
        assert_eq!(html.ty.as_deref(), Some("string"));
        assert_eq!(html.value, None);
        assert_eq!(
            m.globals.iter().find(|g| g.name == "SAFE").unwrap().value,
            Some(serde_json::json!("Token:1-a.b"))
        );
        assert_eq!(
            m.globals
                .iter()
                .find(|g| g.name == "LOCAL_TOKEN")
                .unwrap()
                .value,
            None
        );
        let constants = m
            .data_globals
            .iter()
            .find(|d| d.name == "CONSTANTS")
            .unwrap();
        assert!(!constants.values.contains_key("HTML"));
        assert!(!constants.values.contains_key("PROSE"));
        assert_eq!(constants.values["SAFE"], "Token");
        let encoded = String::from_utf8(crate::encode(&mut m).unwrap()).unwrap();
        for text in [
            "<html>",
            "<body>",
            "PRIVATE SENTENCE",
            "LOCALISATION SENTINEL",
        ] {
            assert!(!encoded.contains(text), "{text}");
        }
    }
    #[test]
    fn scalar_constants_and_documented_dependencies() {
        let mut s = Sources::default();
        s.parse("a.lua", r#"NUM_BAG_SLOTS = Constants.InventoryConstants.NumBagSlots; NUM_TOTAL_EQUIPPED_BAG_SLOTS = NUM_BAG_SLOTS + EXTRA; local LOCAL = 3; Hidden = LOCAL; function F() INNER = 4 end; object.FIELD = 5; LABEL = "bag\nslots"; HEX = 0x10; FLAG = true; EXTRA = 1; UNKNOWN = Mystery()"#).unwrap();
        let mut m: Manifest = serde_json::from_value(serde_json::json!({"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1,"tables":[{"name":"InventoryConstants","type":"Constants","fields":[{"name":"NumBagSlots","type":"number"}]}]})).unwrap();
        s.extend_manifest(&mut m);
        let total = m
            .globals
            .iter()
            .find(|g| g.name == "NUM_TOTAL_EQUIPPED_BAG_SLOTS")
            .unwrap();
        assert_eq!(total.ty.as_deref(), Some("number"));
        assert_eq!(total.value, None);
        assert_eq!(
            m.globals.iter().find(|g| g.name == "HEX").unwrap().value,
            Some(serde_json::json!(16.0))
        );
        assert_eq!(
            m.globals.iter().find(|g| g.name == "LABEL").unwrap().value,
            None
        );
        for name in ["LOCAL", "Hidden", "INNER", "FIELD", "FLAG", "UNKNOWN"] {
            assert!(!m.globals.iter().any(|g| g.name == name), "{name}");
        }
    }
    #[test]
    fn literal_data_and_mixin_members() {
        let mut s = Sources::default();
        s.parse("data.lua", r#"SOUNDKIT = { CLICK = 12, LABEL = "click", FLAG = true, DYNAMIC = Make(), ["OTHER"] = -2 }; local Hidden = {}; function Factory() Nested = {} end; Mixin = {}; function Mixin:Apply() end"#).unwrap();
        assert!(!s.tables.contains_key("Hidden"));
        assert!(!s.tables.contains_key("Nested"));
        let d = &s.tables["SOUNDKIT"];
        assert_eq!(d.fields.len(), 5);
        assert_eq!(d.values["CLICK"], 12.0);
        assert_eq!(d.values["OTHER"], -2.0);
        assert!(!d.values.contains_key("FLAG"));
        assert_eq!(s.tables["Mixin"].methods[0].name, "Apply");
    }
}

//! Bounded abstract interpretation of source constructors; never runs Lua.
//! Local bindings and closure environments are lexical. Unresolved operations stay untyped.
use full_moon::ast::*;
use selu_manifest::{Declaration, Manifest};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
#[derive(Clone, Default)]
enum Value {
    #[default]
    Unknown,
    Nil,
    Symbol(String),
    Object(Rc<RefCell<Object>>),
    Function(usize),
    Strings(Vec<String>),
    Widget(String),
    Number,
    Bool,
    Iterator(Vec<String>),
    Choice(Vec<Value>),
}
#[derive(Clone, Default)]
struct Object {
    members: BTreeMap<String, Value>,
    signatures: BTreeMap<String, Declaration>,
    proxy: Option<Value>,
    element: Option<String>,
}
#[derive(Clone)]
struct Function {
    body: FunctionBody,
    scope: usize,
    signature: Declaration,
    method: bool,
}
struct Scope {
    parent: Option<usize>,
    values: BTreeMap<String, Value>,
}
struct Analysis {
    scopes: Vec<Scope>,
    functions: Vec<Function>,
    exports: BTreeMap<String, Value>,
    fuel: usize,
    file: String,
}
// Invalidate branch-written bindings instead of choosing a dynamic branch.
fn branch_effects(b: &Block, names: &mut Vec<String>) -> bool {
    let mut returns = matches!(b.last_stmt(), Some(LastStmt::Return(_)));
    for stmt in b.stmts() {
        match stmt {
            Stmt::Assignment(a) => {
                for v in a.variables() {
                    if let Var::Name(n) = v {
                        names.push(n.token().to_string().trim().into());
                    } else if let Var::Expression(e) = v
                        && let Prefix::Name(n) = e.prefix()
                    {
                        names.push(format!(
                            "{}.{}",
                            n.token().to_string().trim(),
                            match e.suffixes().last() {
                                Some(Suffix::Index(Index::Dot { name, .. })) =>
                                    name.token().to_string().trim().to_owned(),
                                _ => "*".into(),
                            }
                        ));
                    }
                }
            }
            Stmt::NumericFor(f) => returns |= branch_effects(f.block(), names),
            Stmt::GenericFor(f) => returns |= branch_effects(f.block(), names),
            Stmt::While(f) => returns |= branch_effects(f.block(), names),
            Stmt::Repeat(f) => returns |= branch_effects(f.block(), names),
            Stmt::Do(d) => returns |= branch_effects(d.block(), names),
            Stmt::If(i) => {
                returns |= branch_effects(i.block(), names);
                if let Some(branches) = i.else_if() {
                    for e in branches {
                        returns |= branch_effects(e.block(), names);
                    }
                }
                if let Some(b) = i.else_block() {
                    returns |= branch_effects(b, names);
                }
            }
            _ => {}
        }
    }
    returns
}
impl Analysis {
    fn invalidate(&mut self, scope: usize, n: &str) {
        if let Some((root, key)) = n.split_once('.') {
            if let Value::Object(o) = self.get(scope, root) {
                let mut o = o.borrow_mut();
                if key == "*" {
                    for v in o.members.values_mut() {
                        *v = Value::Unknown;
                    }
                    o.signatures.clear();
                } else {
                    o.members.insert(key.into(), Value::Unknown);
                    o.signatures.remove(key);
                }
                o.element = None;
            }
        } else {
            self.assign(scope, n, Value::Unknown);
        }
    }
    fn invalidate_block(&mut self, b: &Block, scope: usize) -> bool {
        let mut writes = vec![];
        let returns = branch_effects(b, &mut writes);
        for n in writes {
            self.invalidate(scope, &n);
        }
        returns
    }
    fn scope(&mut self, parent: usize) -> usize {
        let id = self.scopes.len();
        self.scopes.push(Scope {
            parent: Some(parent),
            values: BTreeMap::new(),
        });
        id
    }
    fn get(&self, scope: usize, name: &str) -> Value {
        self.scopes[scope]
            .values
            .get(name)
            .cloned()
            .unwrap_or_else(|| {
                self.scopes[scope]
                    .parent
                    .map(|p| self.get(p, name))
                    .unwrap_or_else(|| Value::Symbol(name.into()))
            })
    }
    fn global(&self, scope: usize, name: &str) -> bool {
        if self.scopes[scope].values.contains_key(name) {
            scope == 0
        } else {
            self.scopes[scope]
                .parent
                .is_none_or(|p| self.global(p, name))
        }
    }
    fn assign(&mut self, scope: usize, name: &str, value: Value) {
        if self.scopes[scope].values.contains_key(name) || scope == 0 {
            self.scopes[scope].values.insert(name.into(), value);
        } else {
            self.assign(self.scopes[scope].parent.unwrap_or(0), name, value);
        }
    }
    fn member(&self, object: Value, key: &str) -> Value {
        match object {
            Value::Object(o) => o.borrow().members.get(key).cloned().unwrap_or_default(),
            Value::Symbol(s) => Value::Symbol(format!("{s}.{key}")),
            _ => Value::Unknown,
        }
    }
    fn function(&mut self, body: &FunctionBody, scope: usize, name: &str, method: bool) -> Value {
        let id = self.functions.len();
        let mut signature = super::source::source_signature(name, body, "", &self.file);
        // Factory methods obtain iterator proof from lexical evaluation below.
        for result in &mut signature.returns {
            if result.attributes.remove("SeluIterator").is_some() {
                result.ty = Some(String::new());
                signature
                    .attributes
                    .insert("SeluUntypedReturnTail".into(), true.into());
            }
        }
        if !method
            && signature
                .arguments
                .first()
                .is_some_and(|a| a.name == "self")
        {
            signature.arguments.remove(0);
        }
        self.functions.push(Function {
            body: body.clone(),
            scope,
            signature,
            method,
        });
        Value::Function(id)
    }
    fn expr(&mut self, e: &Expression, scope: usize, depth: usize) -> Value {
        if depth > 48 || self.fuel == 0 {
            return Value::Unknown;
        }
        self.fuel -= 1;
        match e {
            Expression::Var(Var::Name(t)) => self.get(scope, t.token().to_string().trim()),
            Expression::Var(Var::Expression(v)) => {
                self.suffix(v.prefix(), v.suffixes(), scope, depth)
            }
            Expression::FunctionCall(c) => self.suffix(c.prefix(), c.suffixes(), scope, depth),
            Expression::Function(f) => self.function(f.body(), scope, "callback", false),
            Expression::TableConstructor(t) => {
                let mut object = Object::default();
                let mut strings = vec![];
                for field in t.fields() {
                    match field {
                        Field::NameKey { key, value, .. } => {
                            let v = self.expr(value, scope, depth + 1);
                            object
                                .members
                                .insert(key.token().to_string().trim().into(), v);
                        }
                        Field::NoKey(v) => {
                            if let Some(s) = super::source::literal(v)
                                .and_then(|v| v.as_str().map(str::to_owned))
                            {
                                strings.push(s)
                            }
                        }
                        _ => {}
                    }
                }
                if strings.is_empty() {
                    Value::Object(Rc::new(RefCell::new(object)))
                } else {
                    Value::Strings(strings)
                }
            }
            Expression::Number(_) => Value::Number,
            Expression::Symbol(t) if t.token().to_string().trim() == "nil" => Value::Nil,
            Expression::Symbol(t) if ["true", "false"].contains(&t.token().to_string().trim()) => {
                Value::Bool
            }
            Expression::Parentheses { expression, .. } => self.expr(expression, scope, depth + 1),
            Expression::BinaryOperator { lhs, binop, rhs } if binop.to_string().trim() == "and" => {
                if matches!(self.expr(lhs, scope, depth + 1), Value::Nil) {
                    Value::Nil
                } else {
                    Value::Choice(vec![Value::Nil, self.expr(rhs, scope, depth + 1)])
                }
            }
            Expression::BinaryOperator { lhs, binop, rhs } if binop.to_string().trim() == "or" => {
                let a = self.expr(lhs, scope, depth + 1);
                let b = self.expr(rhs, scope, depth + 1);
                match a {
                    Value::Nil => b,
                    Value::Choice(choices) => Value::Choice(
                        choices
                            .into_iter()
                            .map(|v| {
                                if matches!(v, Value::Nil) {
                                    b.clone()
                                } else {
                                    v
                                }
                            })
                            .collect(),
                    ),
                    Value::Unknown | Value::Bool => Value::Choice(vec![a, b]),
                    a => a,
                }
            }
            Expression::BinaryOperator { binop, .. }
                if ["==", "~=", "<", ">", "<=", ">="].contains(&binop.to_string().trim()) =>
            {
                Value::Bool
            }
            _ => Value::Unknown,
        }
    }
    fn suffix<'a>(
        &mut self,
        prefix: &Prefix,
        suffixes: impl Iterator<Item = &'a Suffix>,
        scope: usize,
        depth: usize,
    ) -> Value {
        let mut v = match prefix {
            Prefix::Name(t) => self.get(scope, t.token().to_string().trim()),
            Prefix::Expression(e) => self.expr(e, scope, depth + 1),
            _ => Value::Unknown,
        };
        for suffix in suffixes {
            v = match suffix {
                Suffix::Index(Index::Dot { name, .. }) => {
                    self.member(v, name.token().to_string().trim())
                }
                Suffix::Call(Call::AnonymousCall(args)) => {
                    let args = self.args(args, scope, depth);
                    self.call(v, args, depth + 1)
                }
                Suffix::Call(Call::MethodCall(m)) => {
                    let args = self.args(m.args(), scope, depth);
                    self.method(v, m.name().token().to_string().trim(), args, depth + 1)
                }
                _ => Value::Unknown,
            }
        }
        v
    }
    fn args(&mut self, args: &FunctionArgs, scope: usize, depth: usize) -> Vec<Value> {
        match args {
            FunctionArgs::Parentheses { arguments, .. } => arguments
                .iter()
                .map(|e| self.expr(e, scope, depth + 1))
                .collect(),
            _ => vec![],
        }
    }
    fn call(&mut self, f: Value, args: Vec<Value>, depth: usize) -> Value {
        if depth > 48 || self.fuel == 0 {
            return Value::Unknown;
        }
        match f {
            Value::Choice(choices) => {
                let mut ty = None;
                for choice in choices {
                    let Value::Widget(next) = self.call(choice, args.clone(), depth + 1) else {
                        return Value::Unknown;
                    };
                    if ty.as_ref().is_some_and(|old| old != &next) {
                        return Value::Unknown;
                    }
                    ty = Some(next);
                }
                ty.map(Value::Widget).unwrap_or_default()
            }
            Value::Function(id) => {
                let f = self.functions[id].clone();
                let scope = self.scope(f.scope);
                for (i, p) in f.body.parameters().iter().enumerate() {
                    self.scopes[scope].values.insert(
                        p.to_string().trim().into(),
                        args.get(i).cloned().unwrap_or_default(),
                    );
                }
                self.block(f.body.block(), scope, depth + 1)
                    .unwrap_or_default()
            }
            Value::Symbol(name) => match name.as_str() {
                "Mixin" => {
                    if let Some(Value::Object(target)) = args.first() {
                        for v in args.iter().skip(1) {
                            if let Value::Object(source) = v {
                                if Rc::ptr_eq(target, source) {
                                    continue;
                                }
                                let source = source.borrow();
                                target.borrow_mut().members.extend(source.members.clone());
                                target
                                    .borrow_mut()
                                    .signatures
                                    .extend(source.signatures.clone());
                            } else {
                                let mut object = target.borrow_mut();
                                object.members.clear();
                                object.signatures.clear();
                                object.element = None;
                                return Value::Unknown;
                            }
                        }
                        return Value::Object(target.clone());
                    }
                    Value::Unknown
                }
                "CreateAndInitFromMixin" => {
                    let obj = self.call(
                        Value::Symbol("CreateFromMixins".into()),
                        args.first().cloned().into_iter().collect(),
                        depth + 1,
                    );
                    self.method(
                        obj.clone(),
                        "Init",
                        args.into_iter().skip(1).collect(),
                        depth + 1,
                    );
                    obj
                }
                "CreateFromMixins" => {
                    let mut obj = Object::default();
                    for a in args {
                        if let Value::Object(a) = a {
                            let a = a.borrow();
                            obj.members.extend(a.members.clone());
                            obj.signatures.extend(a.signatures.clone());
                        } else {
                            return Value::Unknown;
                        }
                    }
                    Value::Object(Rc::new(RefCell::new(obj)))
                }
                "ProxyUtil.CreateProxyMixin" => {
                    let mut obj = Object::default();
                    if let (Some(Value::Object(private)), Some(Value::Strings(names))) =
                        (args.get(1), args.get(2))
                    {
                        for n in names {
                            let p = private.borrow();
                            if let Some(v) = p.members.get(n) {
                                obj.members.insert(n.clone(), v.clone());
                            }
                            if let Some(v) = p.signatures.get(n) {
                                obj.signatures.insert(n.clone(), v.clone());
                            }
                        }
                    }
                    Value::Object(Rc::new(RefCell::new(obj)))
                }
                "ProxyUtil.CreateProxy" => {
                    if let (Some(Value::Object(private)), Some(Value::Object(prototype))) =
                        (args.first(), args.get(1))
                    {
                        let prototype = prototype.borrow();
                        let instance = Object {
                            members: prototype.members.clone(),
                            signatures: prototype.signatures.clone(),
                            ..Object::default()
                        };
                        let proxy = Value::Object(Rc::new(RefCell::new(instance)));
                        private.borrow_mut().proxy = Some(proxy.clone());
                        proxy
                    } else {
                        Value::Unknown
                    }
                }
                "SecureTypes.CreateSecureMap" => Value::Symbol("SecureMap".into()),
                "CreateFrame" | "CreateForbiddenFrame" => Value::Widget("Frame".into()),
                "pairs" => Value::Iterator(vec![String::new(), String::new()]),
                "ipairs" => Value::Iterator(vec!["number".into(), String::new()]),
                _ => Value::Unknown,
            },
            _ => Value::Unknown,
        }
    }
    fn method(&mut self, receiver: Value, name: &str, args: Vec<Value>, depth: usize) -> Value {
        if !matches!(receiver, Value::Object(_))
            && matches!(
                name,
                "CreateTexture" | "CreateFontString" | "CreateMaskTexture" | "CreateActor"
            )
        {
            return Value::Widget(
                match name {
                    "CreateTexture" => "Texture",
                    "CreateFontString" => "FontString",
                    "CreateMaskTexture" => "MaskTexture",
                    _ => "Actor",
                }
                .into(),
            );
        }
        if matches!(&receiver,Value::Symbol(s) if s=="SecureMap") && name == "Enumerate" {
            return Value::Iterator(vec![String::new(), "bool".into()]);
        }
        if let Value::Object(o) = &receiver {
            if name == "ToProxy" {
                let object = o.borrow();
                if let Some(Value::Object(p)) = &object.proxy {
                    p.borrow_mut().members.extend(
                        object
                            .members
                            .iter()
                            .filter(|(_, v)| !matches!(v, Value::Function(_)))
                            .map(|(n, v)| (n.clone(), v.clone())),
                    );
                }
                return object.proxy.clone().unwrap_or_default();
            }
            let method = self.member(receiver.clone(), name);
            let mut params = args;
            let result = if let Value::Function(id) = method
                && self.functions[id].method
            {
                self.scopes.push(Scope {
                    parent: Some(self.functions[id].scope),
                    values: BTreeMap::from([("self".into(), receiver.clone())]),
                });
                let mut f = self.functions[id].clone();
                f.scope = self.scopes.len() - 1;
                let next = self.functions.len();
                self.functions.push(f);
                self.call(Value::Function(next), params, depth + 1)
            } else {
                params.insert(0, receiver.clone());
                self.call(method, params, depth + 1)
            };
            // Follow the initialized object's allocator path. A reset callback
            // returning a widget must never become evidence of the element type.
            if name == "Init"
                && let Value::Widget(ty) =
                    self.method(receiver.clone(), "CallCreate", vec![], depth + 1)
            {
                o.borrow_mut().element = Some(ty.clone());
                if let Some(Value::Object(p)) = &o.borrow().proxy {
                    p.borrow_mut().element = Some(ty);
                }
            }
            return result;
        }
        Value::Unknown
    }
    fn block(&mut self, b: &Block, scope: usize, depth: usize) -> Option<Value> {
        if depth > 48 || self.fuel == 0 {
            return None;
        }
        for stmt in b.stmts() {
            self.fuel = self.fuel.saturating_sub(1);
            if self.fuel == 0 {
                return None;
            }
            match stmt {
                Stmt::LocalAssignment(a) => {
                    let vs: Vec<_> = a
                        .expressions()
                        .iter()
                        .map(|e| self.expr(e, scope, depth + 1))
                        .collect();
                    for (i, n) in a.names().iter().enumerate() {
                        self.scopes[scope].values.insert(
                            n.token().to_string().trim().into(),
                            vs.get(i).cloned().unwrap_or_default(),
                        );
                    }
                }
                Stmt::Assignment(a) => {
                    let vs: Vec<_> = a
                        .expressions()
                        .iter()
                        .map(|e| self.expr(e, scope, depth + 1))
                        .collect();
                    for (i, n) in a.variables().iter().enumerate() {
                        let v = vs.get(i).cloned().unwrap_or_default();
                        match n {
                            Var::Name(n) => {
                                let name = n.token().to_string();
                                let name = name.trim();
                                let global = self.global(scope, name);
                                self.assign(scope, name, v.clone());
                                if global {
                                    self.exports.insert(name.into(), v);
                                }
                            }
                            Var::Expression(n) => {
                                if let Some(Suffix::Index(Index::Dot { name, .. })) =
                                    n.suffixes().last()
                                {
                                    let receiver = match n.prefix() {
                                        Prefix::Name(t) => {
                                            self.get(scope, t.token().to_string().trim())
                                        }
                                        _ => Value::Unknown,
                                    };
                                    if let Value::Object(o) = receiver {
                                        o.borrow_mut()
                                            .members
                                            .insert(name.token().to_string().trim().into(), v);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Stmt::LocalFunction(f) => {
                    let v =
                        self.function(f.body(), scope, f.name().token().to_string().trim(), false);
                    self.scopes[scope]
                        .values
                        .insert(f.name().token().to_string().trim().into(), v);
                }
                Stmt::FunctionDeclaration(f) => {
                    let names: Vec<_> = f
                        .name()
                        .names()
                        .iter()
                        .map(|n| n.token().to_string().trim().to_owned())
                        .collect();
                    let method = f.name().method_name();
                    let key = method
                        .map(|m| m.token().to_string().trim().to_owned())
                        .or_else(|| names.get(1).cloned());
                    let v = self.function(
                        f.body(),
                        scope,
                        key.as_deref().unwrap_or(&names[0]),
                        method.is_some(),
                    );
                    if let Some(key) = key {
                        if let Value::Object(o) = self.get(scope, &names[0]) {
                            o.borrow_mut().members.insert(key, v);
                        }
                    } else {
                        let global = self.global(scope, &names[0]);
                        self.assign(scope, &names[0], v.clone());
                        if global {
                            self.exports.insert(names[0].clone(), v);
                        }
                    }
                }
                Stmt::Do(d) => {
                    let child = self.scope(scope);
                    if let Some(v) = self.block(d.block(), child, depth + 1) {
                        return Some(v);
                    }
                }
                Stmt::FunctionCall(c) => {
                    self.suffix(c.prefix(), c.suffixes(), scope, depth + 1);
                }
                Stmt::If(i) => {
                    let mut writes = vec![];
                    let mut returns = branch_effects(i.block(), &mut writes);
                    if let Some(branches) = i.else_if() {
                        for e in branches {
                            returns |= branch_effects(e.block(), &mut writes);
                        }
                    }
                    if let Some(b) = i.else_block() {
                        returns |= branch_effects(b, &mut writes);
                    }
                    for n in writes {
                        self.invalidate(scope, &n);
                    }
                    if returns {
                        return Some(Value::Unknown);
                    }
                }
                Stmt::NumericFor(f) if self.invalidate_block(f.block(), scope) => {
                    return Some(Value::Unknown);
                }
                Stmt::GenericFor(f) if self.invalidate_block(f.block(), scope) => {
                    return Some(Value::Unknown);
                }
                Stmt::While(f) if self.invalidate_block(f.block(), scope) => {
                    return Some(Value::Unknown);
                }
                Stmt::Repeat(f) if self.invalidate_block(f.block(), scope) => {
                    return Some(Value::Unknown);
                }
                _ => {}
            }
        }
        if let Some(LastStmt::Return(r)) = b.last_stmt() {
            r.returns()
                .iter()
                .next()
                .map(|e| self.expr(e, scope, depth + 1))
        } else {
            None
        }
    }
}
fn declaration(name: &str, ty: &str) -> Declaration {
    let mut d = super::xml::declaration(name, ty, "");
    d.attributes.clear();
    d
}
fn return_type(v: Value) -> Option<Declaration> {
    match v {
        Value::Widget(t) => Some(declaration("result", &t)),
        Value::Number => Some(declaration("result", "number")),
        Value::Bool => Some(declaration("result", "bool")),
        Value::Iterator(types) => {
            let mut d = declaration("result", "Function");
            d.attributes
                .insert("SeluIterator".into(), serde_json::json!(types));
            Some(d)
        }
        _ => None,
    }
}
/// Interpret only source files with constructors, with a fresh arena per file.
pub fn extend(m: &mut Manifest, files: &[(String, String)]) {
    // A forwarding method is an iterator only when every non-forwarding
    // source definition of that method proves the same iterator result.
    let mut shapes: BTreeMap<String, Option<serde_json::Value>> = BTreeMap::new();
    for f in &m.functions {
        if f.name.contains('.')
            && let Some(d) = &f.signature
        {
            if d.attributes.contains_key("SeluIteratorForward") {
                continue;
            }
            let shape = d
                .returns
                .first()
                .and_then(|r| r.attributes.get("SeluIterator"))
                .cloned();
            shapes
                .entry(d.name.clone())
                .and_modify(|old| {
                    if *old != shape {
                        *old = None;
                    }
                })
                .or_insert(shape);
        }
    }
    for f in &mut m.functions {
        if let Some(d) = &mut f.signature
            && d.attributes.remove("SeluIteratorForward").is_some()
            && let Some(Some(shape)) = shapes.get(&d.name)
        {
            let mut ret = declaration("result", "Function");
            ret.attributes.insert("SeluIterator".into(), shape.clone());
            d.returns = vec![ret];
        }
    }
    let functions: BTreeMap<_, _> = m.functions.iter().map(|f| (f.name.as_str(), f)).collect();
    let data: BTreeMap<_, _> = m
        .data_globals
        .iter()
        .map(|d| (d.name.as_str(), d))
        .collect();
    let mut seeds = BTreeMap::new();
    for d in &m.data_globals {
        let mut obj = Object::default();
        let mut pending = vec![d.name.as_str()];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(n) = pending.pop() {
            if !seen.insert(n) {
                continue;
            }
            if let Some(d) = data.get(n) {
                pending.extend(d.inherits.iter().rev().map(String::as_str));
                for method in &d.methods {
                    if let Some(sig) = functions
                        .get(format!("{n}.{method}").as_str())
                        .and_then(|f| f.signature.clone())
                    {
                        obj.signatures.entry(method.clone()).or_insert(sig);
                    }
                }
            }
        }
        seeds.insert(d.name.clone(), obj);
    }
    for (file, text) in files {
        if !text.contains("CreateFromMixins") || !text.contains("function Create") {
            continue;
        }
        let Ok(ast) = full_moon::parse(text.trim_start_matches('\u{feff}')) else {
            continue;
        };
        let mut a = Analysis {
            scopes: vec![Scope {
                parent: None,
                values: BTreeMap::new(),
            }],
            functions: vec![],
            exports: BTreeMap::new(),
            fuel: 100_000,
            file: file.clone(),
        };
        for (name, obj) in &seeds {
            a.scopes[0].values.insert(
                name.clone(),
                Value::Object(Rc::new(RefCell::new(obj.clone()))),
            );
        }
        for helper in ["ProxyUtil", "SecureTypes"] {
            a.scopes[0]
                .values
                .insert(helper.into(), Value::Symbol(helper.into()));
        }
        let scope = a.scope(0);
        a.block(ast.nodes(), scope, 0);
        let exports = a.exports.clone();
        for (name, f) in exports {
            if !name.starts_with("Create") || !matches!(f, Value::Function(_)) {
                continue;
            }
            a.fuel = 10_000;
            let result = a.call(f.clone(), vec![], 0);
            let Value::Object(obj) = result else {
                continue;
            };
            let obj = obj.borrow().clone();
            if obj.members.is_empty() && obj.signatures.is_empty() {
                continue;
            }
            let type_name = format!("SourceObject_{name}");
            let mut widget = declaration(&type_name, "ScriptObject");
            widget.attributes.insert("file".into(), file.clone().into());
            let mut methods = obj.signatures.clone();
            for (n, v) in &obj.members {
                if let Value::Function(id) = v {
                    let mut sig = a.functions[*id].signature.clone();
                    sig.name = n.clone();
                    a.fuel = 2000;
                    let value = a.method(
                        Value::Object(Rc::new(RefCell::new(obj.clone()))),
                        n,
                        vec![],
                        0,
                    );
                    if let Some(ret) = return_type(value) {
                        if sig.returns.is_empty() {
                            sig.returns.push(ret);
                        } else {
                            sig.returns[0] = ret;
                        }
                    }
                    methods.insert(n.clone(), sig);
                }
            }
            // The allocator was proven by the factory closure, not by its name.
            if let Some(ty) = obj.element {
                if let Some(acquire) = methods.get_mut("Acquire") {
                    let mut r = declaration("object", &ty);
                    r.attributes.insert("Nilable".into(), true.into());
                    acquire.returns = vec![r, declaration("new", "bool")];
                }
                if let Some(release) = methods
                    .get_mut("Release")
                    .and_then(|d| d.arguments.first_mut())
                {
                    release.ty = Some(ty.clone());
                    release.attributes.remove("Nilable");
                }
                if let Some(enumerate) = methods.get_mut("EnumerateActive") {
                    let mut r = declaration("result", "Function");
                    r.attributes
                        .insert("SeluIterator".into(), serde_json::json!([ty, "bool"]));
                    enumerate.returns = vec![r];
                }
            }
            widget.methods = methods.into_values().collect();
            let Value::Function(id) = f else {
                unreachable!()
            };
            let mut signature = a.functions[id].signature.clone();
            signature.name = name.clone();
            let first = declaration("result", &type_name);
            if signature.returns.is_empty() {
                signature.returns.push(first);
            } else {
                signature.returns[0] = first;
            }
            if let Some(entry) = m.functions.iter_mut().find(|f| f.name == name) {
                if entry.signature.is_none() {
                    entry.signature = Some(signature);
                    m.widgets.push(widget);
                }
            } else {
                let mut entry = super::source::entry(
                    name,
                    file.clone(),
                    "defined",
                    signature.arguments.iter().map(|a| a.name.clone()).collect(),
                );
                entry.signature = Some(signature);
                m.functions.push(entry);
                m.widgets.push(widget);
            }
        }
    }
}

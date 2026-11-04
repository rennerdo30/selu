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
    LiteralBool(bool),
    Iterator(Vec<String>),
    Choice(Vec<Value>),
}
#[derive(Clone, Default)]
struct Object {
    deferred: Option<String>,
    contents: Vec<Value>,
    members: Rc<BTreeMap<String, Value>>,
    signatures: Rc<BTreeMap<String, Rc<Declaration>>>,
    proxy: Option<Value>,
    element: Option<String>,
    public_methods: Option<Vec<String>>,
}
impl Object {
    fn members_mut(&mut self) -> &mut BTreeMap<String, Value> {
        Rc::make_mut(&mut self.members)
    }
    fn signatures_mut(&mut self) -> &mut BTreeMap<String, Rc<Declaration>> {
        Rc::make_mut(&mut self.signatures)
    }
}
#[derive(Clone)]
struct Function {
    file: String,
    body: FunctionBody,
    scope: usize,
    signature: Declaration,
    method: bool,
}
struct Scope {
    parent: Option<usize>,
    values: BTreeMap<String, Value>,
}
struct Catalog<'a> {
    files: &'a [(String, String)],
    definitions: BTreeMap<String, Option<usize>>,
    parsed: RefCell<BTreeMap<usize, Rc<full_moon::ast::Ast>>>,
}
impl<'a> Catalog<'a> {
    fn new(files: &'a [(String, String)]) -> Self {
        let mut definitions = BTreeMap::new();
        for (index, (_, text)) in files.iter().enumerate() {
            let Ok(ast) = full_moon::parse(text.trim_start_matches('\u{feff}')) else {
                continue;
            };
            for name in super::source::helper_names(&ast) {
                definitions
                    .entry(name)
                    .and_modify(|old| {
                        if *old != Some(index) {
                            *old = None;
                        }
                    })
                    .or_insert(Some(index));
            }
        }
        Self {
            files,
            definitions,
            parsed: RefCell::new(BTreeMap::new()),
        }
    }
    fn ast(&self, index: usize) -> Option<Rc<Ast>> {
        if let Some(ast) = self.parsed.borrow().get(&index) {
            return Some(ast.clone());
        }
        let ast =
            Rc::new(full_moon::parse(self.files[index].1.trim_start_matches('\u{feff}')).ok()?);
        self.parsed.borrow_mut().insert(index, ast.clone());
        Some(ast)
    }
}
struct Analysis<'a> {
    catalog: Rc<Catalog<'a>>,
    loaded: std::collections::BTreeSet<usize>,
    scopes: Vec<Scope>,
    functions: Vec<Rc<Function>>,
    exports: BTreeMap<String, Value>,
    export_files: BTreeMap<String, String>,
    fuel: usize,
    exhausted: bool,
    effects_only: bool,
    initializing: bool,
    unproven: std::collections::BTreeSet<String>,
    file: String,
    owner_file: String,
}
// Objects passed to calls or aliases in skipped code may be mutated through them.
#[derive(Default)]
struct Escapes {
    names: Vec<String>,
    aliases: Vec<Expression>,
    calls: Vec<FunctionCall>,
}
impl full_moon::visitors::Visitor for Escapes {
    fn visit_function_call(&mut self, c: &FunctionCall) {
        self.calls.push(c.clone());
    }
    fn visit_return(&mut self, r: &Return) {
        for e in r.returns() {
            self.aliases.push(e.clone());
        }
    }
    fn visit_local_assignment(&mut self, a: &LocalAssignment) {
        for e in a.expressions() {
            self.aliases.push(e.clone());
        }
    }
    fn visit_assignment(&mut self, a: &Assignment) {
        for e in a.expressions() {
            self.aliases.push(e.clone());
        }
    }
    fn visit_function_declaration(&mut self, f: &FunctionDeclaration) {
        if (f.name().method_name().is_some() || f.name().names().len() > 1)
            && let Some(n) = f.name().names().iter().next()
        {
            self.names.push(n.token().to_string().trim().into());
        }
    }
}
// Invalidate branch-written bindings instead of choosing a dynamic branch.
fn branch_effects(b: &Block, names: &mut Vec<Var>) -> bool {
    let mut returns = matches!(b.last_stmt(), Some(LastStmt::Return(_)));
    for stmt in b.stmts() {
        match stmt {
            Stmt::Assignment(a) => names.extend(a.variables().iter().cloned()),
            Stmt::FunctionDeclaration(f)
                if f.name().method_name().is_none() && f.name().names().len() == 1 =>
            {
                names.push(Var::Name(f.name().names().iter().next().unwrap().clone()));
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
impl Analysis<'_> {
    // Each proof simulation owns its graph, while aliases inside that graph remain aliases.
    fn fork(&self, extra: &Value) -> (Self, Value) {
        fn copy(v: &Value, objects: &mut BTreeMap<usize, Rc<RefCell<Object>>>) -> Value {
            match v {
                Value::Object(o) => {
                    let key = Rc::as_ptr(o) as usize;
                    if let Some(c) = objects.get(&key) {
                        return Value::Object(c.clone());
                    }
                    let c = Rc::new(RefCell::new(Object::default()));
                    objects.insert(key, c.clone());
                    let o = o.borrow();
                    *c.borrow_mut() = Object {
                        deferred: o.deferred.clone(),
                        contents: o.contents.iter().map(|v| copy(v, objects)).collect(),
                        members: if o
                            .members
                            .values()
                            .any(|v| matches!(v, Value::Object(_) | Value::Choice(_)))
                        {
                            Rc::new(
                                o.members
                                    .iter()
                                    .map(|(n, v)| (n.clone(), copy(v, objects)))
                                    .collect(),
                            )
                        } else {
                            o.members.clone()
                        },
                        signatures: o.signatures.clone(),
                        proxy: o.proxy.as_ref().map(|v| copy(v, objects)),
                        element: o.element.clone(),
                        public_methods: o.public_methods.clone(),
                    };
                    Value::Object(c)
                }
                Value::Choice(vs) => Value::Choice(vs.iter().map(|v| copy(v, objects)).collect()),
                v => v.clone(),
            }
        }
        let mut objects = BTreeMap::new();
        let scopes = self
            .scopes
            .iter()
            .map(|s| Scope {
                parent: s.parent,
                values: s
                    .values
                    .iter()
                    .map(|(n, v)| (n.clone(), copy(v, &mut objects)))
                    .collect(),
            })
            .collect();
        let exports = self
            .exports
            .iter()
            .map(|(n, v)| (n.clone(), copy(v, &mut objects)))
            .collect();
        let extra = copy(extra, &mut objects);
        (
            Self {
                catalog: self.catalog.clone(),
                loaded: self.loaded.clone(),
                scopes,
                functions: self.functions.clone(),
                exports,
                export_files: self.export_files.clone(),
                fuel: self.fuel,
                exhausted: self.exhausted,
                effects_only: self.effects_only,
                initializing: self.initializing,
                unproven: self.unproven.clone(),
                file: self.file.clone(),
                owner_file: self.owner_file.clone(),
            },
            extra,
        )
    }
    fn resolve(&mut self, name: &str, depth: usize) -> Value {
        if depth > 48 || self.fuel == 0 {
            self.exhausted = true;
            return Value::Unknown;
        }
        let mut parts = name.split('.');
        let mut known = self.get(0, parts.next().unwrap_or(""));
        for key in parts {
            known = self.member(known, key);
        }
        if matches!(known, Value::Function(_)) {
            return known;
        }
        let Some(Some(index)) = self.catalog.definitions.get(name).copied() else {
            return Value::Unknown;
        };
        if self.loaded.insert(index) {
            let Some(ast) = self.catalog.ast(index) else {
                return Value::Unknown;
            };
            let old_file = std::mem::replace(&mut self.file, self.catalog.files[index].0.clone());
            let scope = self.scope(0);
            let old_effects = std::mem::replace(&mut self.effects_only, false);
            let old_initializing = std::mem::replace(&mut self.initializing, true);
            self.block(ast.nodes(), scope, depth + 1);
            self.initializing = old_initializing;
            self.effects_only = old_effects;
            self.file = old_file;
        }
        let mut parts = name.split('.');
        let mut value = self.get(0, parts.next().unwrap_or(""));
        for key in parts {
            value = self.member(value, key);
        }
        if matches!(value,Value::Symbol(ref n) if n==name) {
            Value::Unknown
        } else {
            value
        }
    }
    fn invalidate_escapes(
        &mut self,
        node: &impl full_moon::visitors::Visit,
        scope: usize,
        depth: usize,
    ) {
        if depth > 48 || self.fuel == 0 {
            self.exhausted = true;
            return;
        }
        let mut escapes = Escapes::default();
        full_moon::visitors::Visit::visit(node, &mut escapes);
        let old = self.effects_only;
        self.effects_only = true;
        for n in escapes.names {
            self.invalidate(scope, &format!("{n}.*"), depth + 1);
        }
        for e in escapes.aliases {
            let value = self.expr(&e, scope, depth + 1);
            self.invalidate_values(&[value], depth + 1);
        }
        for call in escapes.calls {
            self.suffix(call.prefix(), call.suffixes(), scope, depth + 1);
        }
        self.effects_only = old;
    }
    fn invalidate_values(&mut self, values: &[Value], depth: usize) {
        if depth > 48 || self.fuel == 0 {
            self.exhausted = true;
            return;
        }
        let mut pending = values.to_vec();
        let mut seen = std::collections::BTreeSet::new();
        let mut functions = std::collections::BTreeSet::new();
        while let Some(value) = pending.pop() {
            if let Value::Choice(values) = &value {
                pending.extend(values.clone());
            }
            if let Value::Function(id) = &value
                && functions.insert(*id)
            {
                let f = self.functions[*id].clone();
                let mut writes = vec![];
                branch_effects(f.body.block(), &mut writes);
                for n in writes {
                    self.invalidate_lvalue(f.scope, &n, depth + 1);
                }
                self.invalidate_escapes(&f.body, f.scope, depth + 1);
            }
            if let Value::Object(o) = value
                && seen.insert(Rc::as_ptr(&o) as usize)
            {
                let mut o = o.borrow_mut();
                pending.extend(o.members.values().cloned());
                pending.extend(o.contents.clone());
                pending.extend(o.proxy.clone());
                o.members_mut().clear();
                o.signatures_mut().clear();
                o.element = None;
                o.proxy = None;
            }
        }
    }
    fn invalidate_lvalue(&mut self, scope: usize, var: &Var, depth: usize) {
        match var {
            Var::Name(n) => {
                let name = n.token().to_string();
                let name = name.trim();
                self.assign(
                    scope,
                    name,
                    Value::Choice(vec![self.get(scope, name), Value::Unknown]),
                );
            }
            Var::Expression(e) => {
                let suffixes: Vec<_> = e.suffixes().collect();
                let Some(last) = suffixes.last() else {
                    return;
                };
                let receiver = self.suffix(
                    e.prefix(),
                    suffixes[..suffixes.len() - 1].iter().copied(),
                    scope,
                    depth + 1,
                );
                let mut pending = vec![receiver];
                while let Some(receiver) = pending.pop() {
                    match receiver {
                        Value::Choice(values) => pending.extend(values),
                        Value::Object(o) => match last {
                            Suffix::Index(Index::Dot { name, .. }) => {
                                let key = name.token().to_string().trim().to_owned();
                                let mut o = o.borrow_mut();
                                let old = o.members.get(&key).cloned().unwrap_or_default();
                                o.members_mut()
                                    .insert(key.clone(), Value::Choice(vec![old, Value::Unknown]));
                                o.signatures_mut().remove(&key);
                                o.element = None;
                                if key == "proxy" {
                                    o.proxy = None;
                                }
                            }
                            _ => self.invalidate_values(&[Value::Object(o)], depth + 1),
                        },
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    fn invalidate(&mut self, scope: usize, n: &str, depth: usize) {
        if let Some((root, key)) = n.split_once('.') {
            if let Value::Object(o) = self.get(scope, root) {
                if key == "*" {
                    self.invalidate_values(&[Value::Object(o)], depth + 1);
                } else {
                    let mut o = o.borrow_mut();
                    o.members_mut().insert(key.into(), Value::Unknown);
                    o.signatures_mut().remove(key);
                    if key == "proxy" {
                        o.proxy = None;
                    }
                    o.element = None;
                }
            }
        } else {
            self.assign(scope, n, Value::Unknown);
        }
    }
    fn invalidate_control(
        &mut self,
        node: &impl full_moon::visitors::Visit,
        b: &Block,
        scope: usize,
        depth: usize,
    ) -> bool {
        let mut writes = vec![];
        let returns = branch_effects(b, &mut writes);
        self.invalidate_escapes(node, scope, depth + 1);
        for n in writes {
            self.invalidate_lvalue(scope, &n, depth + 1);
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
            Value::Object(o) => {
                let o = o.borrow();
                o.members.get(key).cloned().unwrap_or_else(|| {
                    o.deferred
                        .as_ref()
                        .map(|root| format!("{root}.{key}"))
                        .filter(|name| self.catalog.definitions.contains_key(name))
                        .map(Value::Symbol)
                        .unwrap_or_default()
                })
            }
            Value::Symbol(s) => Value::Symbol(format!("{s}.{key}")),
            Value::Choice(values) => {
                Value::Choice(values.into_iter().map(|v| self.member(v, key)).collect())
            }
            _ => Value::Unknown,
        }
    }
    fn indexed(&self, value: Value) -> Value {
        match value {
            Value::Object(o) => {
                let o = o.borrow();
                Value::Choice(
                    std::iter::once(Value::Nil)
                        .chain(o.members.values().cloned())
                        .chain(o.contents.iter().cloned())
                        .collect(),
                )
            }
            Value::Choice(values) => {
                Value::Choice(values.into_iter().map(|v| self.indexed(v)).collect())
            }
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
        self.functions.push(Rc::new(Function {
            file: self.file.clone(),
            body: body.clone(),
            scope,
            signature,
            method,
        }));
        Value::Function(id)
    }
    fn expr(&mut self, e: &Expression, scope: usize, depth: usize) -> Value {
        if depth > 48 || self.fuel == 0 {
            self.exhausted = true;
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
                                .members_mut()
                                .insert(key.token().to_string().trim().into(), v);
                        }
                        Field::NoKey(v) => {
                            if let Some(s) = super::source::literal(v)
                                .and_then(|v| v.as_str().map(str::to_owned))
                            {
                                strings.push(s)
                            } else {
                                object.contents.push(self.expr(v, scope, depth + 1));
                            }
                        }
                        Field::ExpressionKey { key, value, .. } => {
                            object.contents.push(self.expr(key, scope, depth + 1));
                            object.contents.push(self.expr(value, scope, depth + 1));
                        }
                        _ => {}
                    }
                }
                if strings.is_empty() || !object.contents.is_empty() || !object.members.is_empty() {
                    Value::Object(Rc::new(RefCell::new(object)))
                } else {
                    Value::Strings(strings)
                }
            }
            Expression::Number(_) => Value::Number,
            Expression::Symbol(t) if t.token().to_string().trim() == "nil" => Value::Nil,
            Expression::Symbol(t) if ["true", "false"].contains(&t.token().to_string().trim()) => {
                Value::LiteralBool(t.token().to_string().trim() == "true")
            }
            Expression::Parentheses { expression, .. } => self.expr(expression, scope, depth + 1),
            Expression::BinaryOperator { lhs, binop, rhs } if binop.to_string().trim() == "and" => {
                if matches!(
                    self.expr(lhs, scope, depth + 1),
                    Value::Nil | Value::LiteralBool(false)
                ) {
                    Value::Nil
                } else {
                    let old = self.effects_only;
                    self.effects_only = true;
                    let right = self.expr(rhs, scope, depth + 1);
                    self.effects_only = old;
                    Value::Choice(vec![Value::Nil, right])
                }
            }
            Expression::BinaryOperator { lhs, binop, rhs } if binop.to_string().trim() == "or" => {
                let a = self.expr(lhs, scope, depth + 1);
                let old = self.effects_only;
                if matches!(a, Value::Unknown | Value::Bool | Value::Choice(_)) {
                    self.effects_only = true;
                }
                let b = self.expr(rhs, scope, depth + 1);
                self.effects_only = old;
                match a {
                    Value::Nil | Value::LiteralBool(false) => b,
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
                Suffix::Index(Index::Brackets { expression, .. }) => {
                    self.expr(expression, scope, depth + 1);
                    self.indexed(v)
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
            FunctionArgs::TableConstructor(t) => {
                vec![self.expr(&Expression::TableConstructor(t.clone()), scope, depth + 1)]
            }
            FunctionArgs::String(_) => vec![Value::Unknown],
            _ => vec![],
        }
    }
    fn call(&mut self, f: Value, args: Vec<Value>, depth: usize) -> Value {
        if depth > 48 || self.fuel == 0 {
            self.exhausted = true;
            self.invalidate_values(&args, depth + 1);
            return Value::Unknown;
        }
        match f {
            Value::Choice(choices) => {
                let mut ty = None;
                for choice in &choices {
                    let Value::Widget(next) = self.call(choice.clone(), args.clone(), depth + 1)
                    else {
                        self.invalidate_values(&choices, depth + 1);
                        self.invalidate_values(&args, depth + 1);
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
                let offset = usize::from(f.method);
                if f.method {
                    self.scopes[scope]
                        .values
                        .insert("self".into(), args.first().cloned().unwrap_or_default());
                }
                for (i, p) in f.body.parameters().iter().enumerate() {
                    self.scopes[scope].values.insert(
                        p.to_string().trim().into(),
                        args.get(i + offset).cloned().unwrap_or_default(),
                    );
                }
                let old_file = std::mem::replace(&mut self.file, f.file.clone());
                let result = self
                    .block(f.body.block(), scope, depth + 1)
                    .unwrap_or_default();
                self.file = old_file;
                result
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
                                target.borrow_mut().members_mut().extend(
                                    source.members.iter().map(|(k, v)| (k.clone(), v.clone())),
                                );
                                target.borrow_mut().contents.extend(source.contents.clone());
                                target.borrow_mut().signatures_mut().extend(
                                    source
                                        .signatures
                                        .iter()
                                        .map(|(k, v)| (k.clone(), v.clone())),
                                );
                            } else {
                                let mut object = target.borrow_mut();
                                object.members_mut().clear();
                                object.signatures_mut().clear();
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
                            obj.members_mut()
                                .extend(a.members.iter().map(|(k, v)| (k.clone(), v.clone())));
                            obj.contents.extend(a.contents.clone());
                            obj.signatures_mut()
                                .extend(a.signatures.iter().map(|(k, v)| (k.clone(), v.clone())));
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
                        obj.public_methods = Some(names.clone());
                        for n in names {
                            let p = private.borrow();
                            if let Some(v) = p.members.get(n) {
                                obj.members_mut().insert(n.clone(), v.clone());
                            }
                            if let Some(v) = p.signatures.get(n) {
                                obj.signatures_mut().insert(n.clone(), v.clone());
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
                            public_methods: prototype.public_methods.clone(),
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
                _ => {
                    let resolved = self.resolve(&name, depth + 1);
                    if matches!(resolved, Value::Function(_))
                        || matches!(&resolved,Value::Symbol(n) if n!=&name)
                    {
                        self.call(resolved, args, depth + 1)
                    } else {
                        self.unproven
                            .insert(format!("unresolved {name} ({})", self.file));
                        self.invalidate_values(&args, depth + 1);
                        Value::Unknown
                    }
                }
            },
            _ => {
                self.unproven
                    .insert(format!("dynamic callee ({})", self.file));
                self.invalidate_values(&args, depth + 1);
                Value::Unknown
            }
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
                    p.borrow_mut().members_mut().extend(
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
            params.insert(0, receiver.clone());
            let result = self.call(method, params, depth + 1);
            // Follow the initialized object's allocator path. A reset callback
            // returning a widget must never become evidence of the element type.
            if name == "Init" && !self.effects_only {
                o.borrow_mut().element = None;
                if let Some(Value::Object(p)) = &o.borrow().proxy {
                    p.borrow_mut().element = None;
                }
                if o.borrow().members.contains_key("CallCreate")
                    || o.borrow().signatures.contains_key("CallCreate")
                {
                    let (mut probe, receiver) = self.fork(&receiver);
                    let value = probe.method(receiver, "CallCreate", vec![], depth + 1);
                    self.fuel = probe.fuel;
                    if !probe.exhausted
                        && let Value::Widget(ty) = value
                    {
                        o.borrow_mut().element = Some(ty.clone());
                        if let Some(Value::Object(p)) = &o.borrow().proxy {
                            p.borrow_mut().element = Some(ty);
                        }
                    }
                }
            }
            return result;
        }
        self.invalidate_values(&args, depth + 1);
        self.invalidate_values(&[receiver], depth + 1);
        Value::Unknown
    }
    fn block(&mut self, b: &Block, scope: usize, depth: usize) -> Option<Value> {
        if depth > 48 || self.fuel == 0 {
            self.exhausted = true;
            return None;
        }
        for stmt in b.stmts() {
            self.fuel = self.fuel.saturating_sub(1);
            if self.fuel == 0 {
                self.exhausted = true;
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
                        let mut v = vs.get(i).cloned().unwrap_or_default();
                        match n {
                            Var::Name(n) => {
                                let name = n.token().to_string();
                                let name = name.trim();
                                let global = self.global(scope, name);
                                if global
                                    && !self.effects_only
                                    && matches!(
                                        a.expressions().iter().nth(i),
                                        Some(Expression::TableConstructor(_))
                                    )
                                    && let (Value::Object(old), Value::Object(new)) =
                                        (self.get(scope, name), &v)
                                    && old.borrow().deferred.as_deref() == Some(name)
                                {
                                    let initialized = new.borrow().clone();
                                    *old.borrow_mut() = initialized;
                                    v = Value::Object(old);
                                }
                                if self.effects_only {
                                    self.assign(
                                        scope,
                                        name,
                                        Value::Choice(vec![self.get(scope, name), v.clone()]),
                                    );
                                } else {
                                    self.assign(scope, name, v.clone());
                                }
                                if global && !self.initializing {
                                    // Publishing an alias exposes it beyond the modelled local graph.
                                    self.invalidate_values(&[v.clone()], depth + 1);
                                }
                                if global {
                                    self.exports.insert(name.into(), v);
                                    self.export_files.insert(name.into(), self.file.clone());
                                }
                            }
                            Var::Expression(n) => {
                                let suffixes: Vec<_> = n.suffixes().collect();
                                let Some(last) = suffixes.last() else {
                                    continue;
                                };
                                let receiver = self.suffix(
                                    n.prefix(),
                                    suffixes[..suffixes.len() - 1].iter().copied(),
                                    scope,
                                    depth + 1,
                                );
                                if let Value::Object(o) = receiver {
                                    match last {
                                        Suffix::Index(Index::Dot { name, .. }) => {
                                            let key = name.token().to_string().trim().to_owned();
                                            let mut o = o.borrow_mut();
                                            if self.effects_only {
                                                let previous = o
                                                    .members
                                                    .get(&key)
                                                    .cloned()
                                                    .unwrap_or(Value::Nil);
                                                o.members_mut().insert(
                                                    key.clone(),
                                                    Value::Choice(vec![previous, v]),
                                                );
                                                o.signatures_mut().remove(&key);
                                                o.element = None;
                                                if key == "proxy" {
                                                    o.proxy = None;
                                                }
                                            } else {
                                                if key == "proxy" {
                                                    o.proxy = Some(v.clone());
                                                }
                                                if self.file == self.owner_file
                                                    && matches!(v, Value::Function(_))
                                                    && let Some(names) = &mut o.public_methods
                                                    && !names.contains(&key)
                                                {
                                                    names.push(key.clone());
                                                }
                                                o.members_mut().insert(key, v);
                                            }
                                        }
                                        Suffix::Index(Index::Brackets { expression, .. }) => {
                                            self.expr(expression, scope, depth + 1);
                                            let mut o = o.borrow_mut();
                                            let previous: Vec<_> =
                                                o.members.values().cloned().collect();
                                            o.contents.extend(previous);
                                            o.contents.push(v);
                                            o.members_mut().clear();
                                            o.signatures_mut().clear();
                                            o.element = None;
                                        }
                                        _ => self.invalidate_values(
                                            &[Value::Object(o.clone()), v],
                                            depth + 1,
                                        ),
                                    }
                                } else {
                                    self.invalidate_values(&[receiver, v], depth + 1);
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
                            o.borrow_mut().members_mut().insert(key, v);
                        }
                    } else {
                        let global = self.global(scope, &names[0]);
                        self.assign(scope, &names[0], v.clone());
                        if global {
                            self.exports.insert(names[0].clone(), v);
                            self.export_files
                                .insert(names[0].clone(), self.file.clone());
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
                    let condition = self.expr(i.condition(), scope, depth + 1);
                    if matches!(condition, Value::LiteralBool(true)) {
                        let child = self.scope(scope);
                        if let Some(v) = self.block(i.block(), child, depth + 1) {
                            return Some(v);
                        }
                        continue;
                    }
                    if matches!(condition, Value::Nil | Value::LiteralBool(false))
                        && i.else_if().is_none()
                    {
                        if let Some(b) = i.else_block() {
                            let child = self.scope(scope);
                            if let Some(v) = self.block(b, child, depth + 1) {
                                return Some(v);
                            }
                        }
                        continue;
                    }
                    self.invalidate_escapes(i, scope, depth + 1);
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
                        self.invalidate_lvalue(scope, &n, depth + 1);
                    }
                    if returns && !self.effects_only {
                        return Some(Value::Unknown);
                    }
                }
                Stmt::NumericFor(f)
                    if self.invalidate_control(f, f.block(), scope, depth + 1)
                        && !self.effects_only =>
                {
                    return Some(Value::Unknown);
                }
                Stmt::GenericFor(f)
                    if self.invalidate_control(f, f.block(), scope, depth + 1)
                        && !self.effects_only =>
                {
                    return Some(Value::Unknown);
                }
                Stmt::While(f)
                    if self.invalidate_control(f, f.block(), scope, depth + 1)
                        && !self.effects_only =>
                {
                    return Some(Value::Unknown);
                }
                Stmt::Repeat(f)
                    if self.invalidate_control(f, f.block(), scope, depth + 1)
                        && !self.effects_only =>
                {
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
        Value::Bool | Value::LiteralBool(_) => Some(declaration("result", "bool")),
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
        let mut obj = Object {
            deferred: Some(d.name.clone()),
            ..Object::default()
        };
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
                        obj.signatures_mut()
                            .entry(method.clone())
                            .or_insert(Rc::new(sig));
                        obj.members_mut()
                            .entry(method.clone())
                            .or_insert(Value::Symbol(format!("{n}.{method}")));
                    }
                }
            }
        }
        seeds.insert(d.name.clone(), obj);
    }
    let catalog = Rc::new(Catalog::new(files));
    for (file_index, (file, text)) in files.iter().enumerate() {
        if !text.contains("CreateFromMixins") || !text.contains("function Create") {
            continue;
        }
        let Ok(ast) = full_moon::parse(text.trim_start_matches('\u{feff}')) else {
            continue;
        };
        let mut a = Analysis {
            catalog: catalog.clone(),
            loaded: std::collections::BTreeSet::from([file_index]),
            scopes: vec![Scope {
                parent: None,
                values: BTreeMap::new(),
            }],
            functions: vec![],
            exports: BTreeMap::new(),
            export_files: BTreeMap::new(),
            fuel: 100_000,
            exhausted: false,
            effects_only: false,
            initializing: true,
            unproven: std::collections::BTreeSet::new(),
            file: file.clone(),
            owner_file: file.clone(),
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
        a.initializing = false;
        if a.exhausted {
            continue;
        }
        let exports = a.exports.clone();
        let base = a;
        for (name, f) in exports {
            if base.export_files.get(&name) != Some(file) {
                continue;
            }
            if base.catalog.definitions.get(&name) == Some(&None) {
                eprintln!(
                    "factory-trace {}",
                    serde_json::json!({"factory":name,"unproven":["ambiguous global factory binding"],"boundedOut":false})
                );
                continue;
            }
            let (mut a, _) = base.fork(&Value::Nil);
            if !name.starts_with("Create") || !matches!(f, Value::Function(_)) {
                continue;
            }
            a.fuel = 10_000;
            let result = a.call(f.clone(), vec![], 0);
            eprintln!(
                "factory-trace {}",
                serde_json::json!({"factory":name,"unproven":a.unproven,"boundedOut":a.exhausted})
            );
            if a.exhausted {
                eprintln!("factory {name}: proof withheld (depth or fuel bound)");
                continue;
            }
            let Value::Object(instance) = result else {
                continue;
            };
            let obj = instance.borrow().clone();
            if obj.members.is_empty() && obj.signatures.is_empty() {
                continue;
            }
            let type_name = format!("SourceObject_{name}");
            let mut widget = declaration(&type_name, "ScriptObject");
            widget.attributes.insert("file".into(), file.clone().into());
            let mut methods: BTreeMap<_, _> = obj
                .signatures
                .iter()
                .map(|(n, d)| (n.clone(), d.as_ref().clone()))
                .collect();
            for (n, v) in obj.members.iter() {
                if obj
                    .public_methods
                    .as_ref()
                    .is_some_and(|names| !names.contains(n))
                {
                    continue;
                }
                if let Value::Function(id) = v {
                    let mut sig = a.functions[*id].signature.clone();
                    sig.name = n.clone();
                    let (mut method_analysis, receiver) = a.fork(&Value::Object(instance.clone()));
                    method_analysis.fuel = 2000;
                    let value = method_analysis.method(receiver, n, vec![], 0);
                    if !method_analysis.exhausted
                        && let Some(ret) = return_type(value)
                    {
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

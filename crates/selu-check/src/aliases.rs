//! Named type declarations and explicit unknown narrowing.
use crate::{Ctx, Narrowings, Type, err};
use selu_syntax::{Expr, ExprKind, Stmt, StmtKind, TypeDecl, TypeExpr, TypeExprKind as K};
use std::collections::HashMap;
#[derive(Clone, Default)]
pub(crate) struct Aliases {
    pub(crate) declarations: HashMap<String, TypeDecl>,
    active: Vec<String>,
    shapes: HashMap<String, crate::types::NamedShape>,
    validated: std::collections::HashSet<String>,
    pub(crate) contexts: HashMap<String, HashMap<String, Type>>,
    context_stack: Vec<HashMap<String, Type>>,
}
impl Ctx<'_> {
    pub(crate) fn finish_alias_diagnostics(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.out.retain(|d| {
            let Some(span) = &d.span else {
                return true;
            };
            let declaration = self.aliases.declarations.values().any(|t| {
                span.file == t.name_span.file
                    && span.start >= t.name_span.start
                    && span.end <= t.body.span.end
            });
            !declaration || seen.insert((d.code.clone(), span.file.clone(), span.start, span.end))
        });
    }
    pub(crate) fn predeclare_aliases(&mut self, ss: &[Stmt]) {
        if self.scopes.len() != 1 {
            return;
        }
        for s in ss {
            let s = if let StmtKind::Export(s) = &s.kind {
                s
            } else {
                s
            };
            if let StmtKind::TypeDecl(d) = &s.kind
                && self
                    .aliases
                    .declarations
                    .insert(d.name.clone(), d.clone())
                    .is_some()
            {
                self.out.push(err(
                    "SELU0520",
                    format!("type `{}` is already declared", d.name),
                    &d.name_span,
                ));
            }
        }
        let imports = self.module.imports.clone();
        for s in ss {
            if let StmtKind::Import { names, from, .. } = &s.kind {
                for (remote, local, local_span, _) in names {
                    if let Some(export) = imports.get(from).and_then(|m| m.get(remote)) {
                        if !export.aliases.contains_key(remote) {
                            continue;
                        }
                        self.out.retain(|d| {
                            !(d.code == "SELU0304"
                                && !export.has_value
                                && d.span.as_ref() == Some(local_span))
                        });
                        self.aliases.contexts.extend(export.type_contexts.clone());
                        let renames: HashMap<_, _> = export
                            .aliases
                            .iter()
                            .map(|(n, d)| {
                                (n.clone(), format!("$module:{}:{}", d.name_span.file, n))
                            })
                            .collect();
                        for (n, original) in &export.aliases {
                            let mut d = original.clone();
                            let mut map = renames.clone();
                            for (p, _) in &d.params {
                                map.remove(p);
                            }
                            rename_types(&mut d.body, &map);
                            for base in &mut d.bases {
                                rename_types(base, &map);
                            }
                            for (_, bound) in &mut d.params {
                                if let Some(t) = bound {
                                    rename_types(t, &map);
                                }
                            }
                            self.aliases
                                .declarations
                                .insert(renames[n].clone(), d.clone());
                            if n == remote {
                                if let Some(existing) = self.aliases.declarations.get(local) {
                                    self.out.push(err(
                                        "SELU0520",
                                        format!("type `{local}` is already declared"),
                                        &existing.name_span,
                                    ));
                                } else {
                                    self.aliases.declarations.insert(local.clone(), d);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    pub(crate) fn check_alias(&mut self, d: &TypeDecl) {
        if !self.aliases.validated.insert(declaration_key(d)) {
            return;
        }
        if self.scopes.len() != 1 {
            self.out.push(err(
                "SELU0521",
                "type declarations belong at module scope",
                &d.name_span,
            ));
        }
        if d.name.starts_with("__selu") {
            self.out.push(err(
                "SELU0318",
                "type names starting with __selu are reserved",
                &d.name_span,
            ));
        }
        let mut params = std::collections::HashSet::new();
        for ((name, _), span) in d.params.iter().zip(&d.param_spans) {
            if !params.insert(name) {
                self.out.push(err(
                    "SELU0520",
                    format!("type parameter `{name}` is already declared"),
                    span,
                ));
            }
            if name.starts_with("__selu") {
                self.out.push(err(
                    "SELU0318",
                    "type parameter names starting with __selu are reserved",
                    &d.name_span,
                ));
            }
        }
        if let K::Object(fields) = &d.body.kind {
            let mut names = std::collections::HashSet::new();
            for field in fields {
                if !names.insert(&field.name) {
                    self.out.push(err(
                        "SELU0527",
                        format!("field `{}` is already declared", field.name),
                        &field.name_span,
                    ));
                }
            }
        }
        let mut env = HashMap::new();
        for (n, bound) in &d.params {
            let t = bound
                .as_ref()
                .map(|t| self.alias_resolve(t, &env))
                .unwrap_or(Type::Unknown);
            env.insert(n.clone(), t);
        }
        let t = TypeExpr {
            kind: K::Named {
                name: d.name.clone(),
                args: d
                    .params
                    .iter()
                    .map(|(n, _)| TypeExpr {
                        kind: K::Named {
                            name: n.clone(),
                            args: vec![],
                        },
                        span: d.name_span.clone(),
                    })
                    .collect(),
            },
            span: d.name_span.clone(),
        };
        let resolved = self.alias_resolve(&t, &env);
        if unguarded_cycle(&resolved) {
            self.out.push(err(
                "SELU0523",
                format!("type `{}` has no finite shape", d.name),
                &d.name_span,
            ));
        }
        validate_resolved(&resolved, &d.body.span, &mut Default::default(), self.out);
    }
    pub(crate) fn alias_annotation(&mut self, t: &TypeExpr) -> Type {
        self.alias_resolve(t, &HashMap::new())
    }
    fn alias_resolve(&mut self, t: &TypeExpr, env: &HashMap<String, Type>) -> Type {
        let mut rewritten = t.clone();
        let mut names = HashMap::new();
        self.alias_rewrite(&mut rewritten, env, &mut names);
        if let Some(context) = self.aliases.context_stack.last() {
            let mut all = context.clone();
            all.extend(names);
            names = all;
        }
        // Keep class resolution's lexical namespace and replace only named-type leaves.
        let ty = if names.is_empty() {
            self.class_resolve(&rewritten)
        } else {
            self.class_resolve_with_aliases(&rewritten, &names)
        };
        crate::restrictions::collapse_secrets(ty, self.secret_capable)
    }
    fn alias_rewrite(
        &mut self,
        t: &mut TypeExpr,
        env: &HashMap<String, Type>,
        names: &mut HashMap<String, Type>,
    ) {
        match &mut t.kind {
            K::Named { name, args } => {
                if let Some(value) = env.get(name) {
                    names.insert(name.clone(), value.clone());
                    return;
                }
                let Some(d) = self.aliases.declarations.get(name).cloned() else {
                    for a in args {
                        self.alias_rewrite(a, env, names);
                    }
                    return;
                };
                let supplied: Vec<_> = args.iter().map(|a| self.alias_resolve(a, env)).collect();
                let display = if name.starts_with("$module:") {
                    &d.name
                } else {
                    &*name
                };
                let label = if supplied.is_empty() {
                    display.clone()
                } else {
                    format!(
                        "{}<{}>",
                        display,
                        supplied
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                let key = format!(
                    "{}:{:?}",
                    declaration_key(&d),
                    supplied.iter().map(identity_key).collect::<Vec<_>>()
                );
                let ty = if supplied.len() != d.params.len() {
                    self.out.push(err(
                        "SELU0522",
                        format!(
                            "type `{name}` needs {} type arguments, got {}",
                            d.params.len(),
                            supplied.len()
                        ),
                        &t.span,
                    ));
                    Type::Error
                } else if self.aliases.active.contains(&key) {
                    Type::Named(label.clone(), self.aliases.shapes[&key].reference())
                } else if let Some(shape) = self.aliases.shapes.get(&key) {
                    Type::Named(label.clone(), shape.clone())
                } else if self.aliases.active.len() >= 64 {
                    self.out.push(err(
                        "SELU0523",
                        format!("type `{label}` expands without a finite shape"),
                        &t.span,
                    ));
                    Type::Error
                } else {
                    self.aliases.active.push(key.clone());
                    let shape = crate::types::NamedShape::new(key.clone());
                    self.aliases.shapes.insert(key.clone(), shape.clone());
                    self.types.named_roots.push(shape.clone());
                    let context = self
                        .aliases
                        .contexts
                        .get(&declaration_key(&d))
                        .cloned()
                        .unwrap_or_else(|| self.class_type_names());
                    self.aliases.context_stack.push(context);
                    let local: HashMap<_, _> = d
                        .params
                        .iter()
                        .map(|p| p.0.clone())
                        .zip(supplied.iter().cloned())
                        .collect();
                    let mut invalid = false;
                    for ((parameter, bound), value) in d.params.iter().zip(&supplied) {
                        if let Some(bound) = bound {
                            let expected = self.alias_resolve(bound, &local);
                            if !value.assignable_to(&expected) {
                                invalid = true;
                                self.out.push(err("SELU0524", format!("type argument `{value}` for `{parameter}` must satisfy `{expected}`"), &t.span));
                            }
                        }
                    }
                    let mut body = self.alias_resolve(&d.body, &local);
                    for base in &d.bases {
                        let inherited = self.alias_resolve(base, &local);
                        let inherited = self.alias_shape(inherited);
                        if let (Type::Object(fields), Type::Object(extra)) = (&mut body, inherited)
                        {
                            for (n, value) in extra {
                                if let Some(own) = fields.get(&n) {
                                    if !own.assignable_to(&value) {
                                        self.out.push(err(
                                            "SELU0525",
                                            format!(
                                                "inherited field `{n}` needs `{value}`, got `{own}`"
                                            ),
                                            &base.span,
                                        ));
                                    }
                                } else {
                                    fields.insert(n, value);
                                }
                            }
                        } else {
                            self.out.push(err(
                                "SELU0525",
                                "an interface can only extend an object type",
                                &base.span,
                            ));
                        }
                    }
                    self.aliases.context_stack.pop();
                    self.aliases.active.pop();
                    shape.set(body);
                    if invalid {
                        Type::Error
                    } else {
                        Type::Named(label.clone(), shape)
                    }
                };
                let key = format!("$alias{}", names.len());
                names.insert(key.clone(), ty);
                *name = key;
                args.clear();
            }
            K::Array(t) => {
                self.alias_rewrite(t, env, names);
                if let K::Named { name, .. } = &t.kind
                    && names.get(name).is_some_and(Type::is_optional)
                {
                    self.out.push(
                        err("SELU0350", "array elements cannot be null", &t.span)
                            .with_help("use a Map for gaps"),
                    );
                    names.insert(name.clone(), Type::Error);
                }
            }
            K::Optional(t) => self.alias_rewrite(t, env, names),
            K::Union(ts) | K::Tuple(ts) => {
                for t in ts {
                    self.alias_rewrite(t, env, names);
                }
            }
            K::StringLiteral(_) | K::NumberLiteral(_) | K::BooleanLiteral(_) => {}
            K::Object(fs) => {
                for f in fs {
                    self.alias_rewrite(&mut f.ty, env, names);
                }
            }
            K::Function { params, ret, .. } => {
                for p in params {
                    self.alias_rewrite(p, env, names);
                }
                self.alias_rewrite(ret, env, names);
            }
        }
    }
    pub(crate) fn alias_shape(&self, t: Type) -> Type {
        shape_of(&t)
    }
    pub(crate) fn alias_narrowings(&self, e: &Expr) -> Option<(Narrowings, Narrowings)> {
        let ExprKind::Binary { op, left, right } = &e.kind else {
            return None;
        };
        if op != "==" && op != "!=" {
            return None;
        }
        for (test, literal) in [(left, right), (right, left)] {
            let lit = match &literal.kind {
                ExprKind::Str(s) => Some(Type::StringLiteral(s.clone())),
                ExprKind::Number(s) => Some(Type::NumberLiteral(s.clone())),
                ExprKind::Bool(b) => Some(Type::BooleanLiteral(*b)),
                ExprKind::Null => Some(Type::Null),
                _ => None,
            };
            if let Some(lit) = lit {
                let (name, field) = match &test.kind {
                    ExprKind::Ident(n) => (Some(n), None),
                    ExprKind::Member { object, name } => match &object.kind {
                        ExprKind::Ident(n) => (Some(n), Some(name)),
                        _ => (None, None),
                    },
                    _ => (None, None),
                };
                if let Some(n) = name {
                    let original = self.type_of(n)?;
                    if matches!(shape_of(&original), Type::Union(_) | Type::Optional(_)) {
                        let ts = original.members();
                        let select = |equal| {
                            let members: Vec<_> = ts
                                .iter()
                                .filter_map(|t| {
                                    let value = if let Some(f) = field {
                                        match shape_of(t) {
                                            Type::Class(_, _, fs) | Type::Object(fs) => {
                                                match fs.get(f) {
                                                    Some(t) => t.clone(),
                                                    None => return Some(t.clone()),
                                                }
                                            }
                                            _ => return Some(t.clone()),
                                        }
                                    } else {
                                        t.clone()
                                    };
                                    if equal && lit.assignable_to(&value) {
                                        Some(if field.is_none() {
                                            lit.clone()
                                        } else {
                                            t.clone()
                                        })
                                    } else if !equal && !value.assignable_to(&lit) {
                                        Some(t.clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            Type::union(members)
                        };
                        let key = if field.is_some() {
                            self.field_fact_key(n)
                        } else {
                            self.fact_key(n)
                        };
                        let yes = vec![(key.clone(), select(true))];
                        let no = vec![(key, select(false))];
                        return Some(if op == "==" { (yes, no) } else { (no, yes) });
                    }
                }
            }

            if let (ExprKind::Unary { op: unary, expr }, ExprKind::Str(tag)) =
                (&test.kind, &literal.kind)
                && unary == "typeof"
                && let ExprKind::Ident(n) = &expr.kind
            {
                let original = self.type_of(n)?;
                let non_null = original.non_null();
                let shape = shape_of(&non_null);
                if matches!(shape_of(&original), Type::Union(_) | Type::Optional(_)) {
                    let matches = |t: &Type| -> Option<bool> {
                        let category = match shape_of(t) {
                            Type::String | Type::StringLiteral(_) => "string",
                            Type::Number | Type::NumberLiteral(_) => "number",
                            Type::Boolean | Type::BooleanLiteral(_) => "boolean",
                            Type::Null => "nil",
                            Type::Object(_)
                            | Type::Map(..)
                            | Type::Array(_)
                            | Type::Set(_)
                            | Type::WeakMap(..)
                            | Type::Tuple(_)
                            | Type::Class(..)
                            | Type::ClassObject(_)
                            | Type::Widget(..) => "table",
                            Type::Function { .. } => "function",
                            _ => return None,
                        };
                        Some(tag == category)
                    };
                    let members = original.members();
                    // Unknown runtime categories are retained on both branches.
                    let select = |yes| {
                        Type::union(
                            members
                                .iter()
                                .filter_map(|t| match matches(t) {
                                    Some(b) => (b == yes).then(|| t.clone()),
                                    None if yes
                                        && matches!(shape_of(t), Type::Unknown | Type::Untyped) =>
                                    {
                                        Some(match tag.as_str() {
                                            "nil" => Type::Null,
                                            "string" => Type::String,
                                            "number" => Type::Number,
                                            "boolean" => Type::Boolean,
                                            "table" => Type::Object(Default::default()),
                                            _ => t.clone(),
                                        })
                                    }
                                    None => Some(t.clone()),
                                })
                                .collect(),
                        )
                    };
                    let yes = vec![(self.fact_key(n), select(true))];
                    let no = vec![(self.fact_key(n), select(false))];
                    return Some(if op == "==" { (yes, no) } else { (no, yes) });
                }
                if let Type::SavedRepair(table) = &shape {
                    let narrowed = match tag.as_str() {
                        "table" => *table.clone(),
                        "string" => Type::String,
                        "number" => Type::Number,
                        "boolean" => Type::Boolean,
                        "nil" if original.is_optional() => Type::Null,
                        _ => return None,
                    };
                    let yes = vec![(self.fact_key(n), narrowed)];
                    // Keep the full union on the opposite branch: it must still
                    // guard or replace the value before accessing any members.
                    return Some(if op == "==" {
                        (yes, vec![])
                    } else {
                        (vec![], yes)
                    });
                }
                let category = match shape {
                    Type::Number => "number",
                    Type::String => "string",
                    Type::Boolean => "boolean",
                    Type::Function { .. } => "function",
                    Type::Object(_)
                    | Type::Array(_)
                    | Type::Map(..)
                    | Type::Set(_)
                    | Type::WeakMap(..)
                    | Type::Class(..)
                    | Type::Widget(..) => "table",
                    Type::Null => "nil",
                    _ => "",
                };
                let (yes, no) = if tag == "nil" {
                    (
                        vec![(self.fact_key(n), Type::Null)],
                        if original.is_optional() {
                            vec![(self.fact_key(n), non_null)]
                        } else {
                            vec![]
                        },
                    )
                } else {
                    let ty = if category == tag {
                        non_null
                    } else if shape == Type::Unknown || shape.is_open() {
                        match tag.as_str() {
                            "string" => Type::String,
                            "number" => Type::Number,
                            "boolean" => Type::Boolean,
                            "table" => Type::Object(Default::default()),
                            "function" => Type::Function {
                                multiple_results: false,
                                unspecified_callback: true,
                                api: Default::default(),
                                api_semantics: Default::default(),
                                params: vec![],
                                ret: Box::new(Type::Unknown),
                                required: 0,
                                rest: Some(Box::new(Type::Unknown)),
                            },
                            _ => return None,
                        }
                    } else {
                        return None;
                    };
                    (
                        vec![(self.fact_key(n), ty)],
                        if original.is_optional() && category == tag {
                            vec![(self.fact_key(n), Type::Null)]
                        } else {
                            vec![]
                        },
                    )
                };
                return Some(if op == "==" { (yes, no) } else { (no, yes) });
            }
        }
        None
    }
}

fn rename_types(t: &mut TypeExpr, names: &HashMap<String, String>) {
    match &mut t.kind {
        K::Named { name, args } => {
            if let Some(n) = names.get(name) {
                *name = n.clone();
            }
            for a in args {
                rename_types(a, names);
            }
        }
        K::Array(t) | K::Optional(t) => rename_types(t, names),
        K::Union(ts) | K::Tuple(ts) => {
            for t in ts {
                rename_types(t, names);
            }
        }
        K::StringLiteral(_) | K::NumberLiteral(_) | K::BooleanLiteral(_) => {}
        K::Object(fs) => {
            for f in fs {
                rename_types(&mut f.ty, names);
            }
        }
        K::Function { params, ret, .. } => {
            for t in params {
                rename_types(t, names);
            }
            rename_types(ret, names);
        }
    }
}

pub fn shape_of(t: &Type) -> Type {
    let mut t = t.clone();
    let mut seen = std::collections::HashSet::new();
    while let Type::Named(_, body) = t {
        if !seen.insert(body.key.clone()) {
            return Type::Error;
        }
        t = body.get().unwrap_or(Type::Error);
    }
    t
}

pub(crate) fn declaration_key(d: &TypeDecl) -> String {
    format!("{}:{}", d.name_span.file, d.name_span.start)
}
pub(crate) fn identity_key(t: &Type) -> String {
    let parts = match t {
        Type::Named(_, s) => return s.key.clone(),
        Type::StringLiteral(s) | Type::NumberLiteral(s) => vec![s.clone()],
        Type::BooleanLiteral(b) => vec![b.to_string()],
        Type::Class(n, ..) | Type::ClassObject(n) | Type::Widget(n, _) => vec![n.clone()],
        Type::Optional(t)
        | Type::SavedRepair(t)
        | Type::Secret(t)
        | Type::Array(t)
        | Type::Set(t) => {
            vec![identity_key(t)]
        }
        Type::Map(k, v) | Type::WeakMap(k, v) => vec![identity_key(k), identity_key(v)],
        Type::Object(fs) => fs
            .iter()
            .map(|(n, t)| format!("{n:?}:{}", identity_key(t)))
            .collect(),
        Type::Union(ts) | Type::Tuple(ts) | Type::ApiIterator(ts) => {
            ts.iter().map(identity_key).collect()
        }
        Type::ApiOpaque(n, ts) => std::iter::once(n.clone())
            .chain(ts.iter().map(identity_key))
            .collect(),
        Type::Function {
            params,
            ret,
            required,
            rest,
            multiple_results,
            unspecified_callback,
            ..
        } => vec![
            format!("{:?}", params.iter().map(identity_key).collect::<Vec<_>>()),
            identity_key(ret),
            format!(
                "{required}:{multiple_results}:{unspecified_callback}:{:?}",
                rest.as_deref().map(identity_key)
            ),
        ],
        _ => vec![],
    };
    format!("{:?}{parts:?}", std::mem::discriminant(t))
}

fn unguarded_cycle(t: &Type) -> bool {
    fn visit(t: &Type, seen: &mut std::collections::HashSet<String>) -> bool {
        match t {
            Type::Named(_, s) => {
                if !seen.insert(s.key.clone()) {
                    return true;
                }
                let result = s.get().is_some_and(|t| visit(&t, seen));
                seen.remove(&s.key);
                result
            }
            Type::Optional(t) => visit(t, seen),
            Type::Union(ts) => ts.iter().any(|t| visit(t, seen)),
            _ => false,
        }
    }
    visit(t, &mut Default::default())
}

fn validate_resolved(
    t: &Type,
    span: &selu_diagnostics::Span,
    seen: &mut std::collections::HashSet<String>,
    out: &mut Vec<selu_diagnostics::Diagnostic>,
) {
    if let Type::Named(_, s) = t
        && !seen.insert(s.key.clone())
    {
        return;
    }
    match shape_of(t) {
        Type::Array(t) => {
            if t.is_optional() {
                out.push(
                    err("SELU0350", "array elements cannot be null", span)
                        .with_help("use a Map for gaps"),
                );
            }
            validate_resolved(&t, span, seen, out);
        }
        Type::Optional(t) | Type::SavedRepair(t) | Type::Secret(t) | Type::Set(t) => {
            validate_resolved(&t, span, seen, out)
        }
        Type::Map(k, v) | Type::WeakMap(k, v) => {
            validate_resolved(&k, span, seen, out);
            validate_resolved(&v, span, seen, out);
        }
        Type::Object(fs) => {
            for t in fs.values() {
                validate_resolved(t, span, seen, out);
            }
        }
        Type::Union(ts) | Type::Tuple(ts) => {
            for t in ts {
                validate_resolved(&t, span, seen, out);
            }
        }
        Type::Function {
            params, ret, rest, ..
        } => {
            for t in params {
                validate_resolved(&t, span, seen, out);
            }
            validate_resolved(&ret, span, seen, out);
            if let Some(t) = rest {
                validate_resolved(&t, span, seen, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn non_null_of(t: &Type) -> Type {
    let mut t = t.clone();
    let mut seen = std::collections::HashSet::new();
    loop {
        match t {
            Type::Union(ts) => {
                let ts: Vec<_> = ts
                    .into_iter()
                    .filter(|t| !matches!(shape_of(t), Type::Null))
                    .map(|t| t.non_null())
                    .collect();
                return if ts.len() == 1 {
                    ts[0].clone()
                } else {
                    Type::Union(ts)
                };
            }
            Type::Optional(inner) => t = *inner,
            Type::Named(_, ref s) if matches!(shape_of(&t), Type::Optional(_) | Type::Null) => {
                if !seen.insert(s.key.clone()) {
                    return Type::Error;
                }
                t = s.get().unwrap_or(Type::Error);
            }
            _ => return t,
        }
    }
}

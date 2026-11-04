//! Named type declarations and explicit unknown narrowing.
use crate::{Ctx, Narrowings, Type, err};
use selu_syntax::{Expr, ExprKind, Stmt, StmtKind, TypeDecl, TypeExpr, TypeExprKind as K};
use std::collections::HashMap;
#[derive(Default)]
pub(crate) struct Aliases {
    pub(crate) declarations: HashMap<String, TypeDecl>,
    active: Vec<String>,
    shapes: HashMap<String, crate::types::NamedShape>,
}
impl Ctx<'_> {
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
                            !(d.code == "SELU0304" && d.span.as_ref() == Some(local_span))
                        });
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
                                self.aliases.declarations.insert(local.clone(), d);
                            }
                        }
                    }
                }
            }
        }
    }
    pub(crate) fn check_alias(&mut self, d: &TypeDecl) {
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
        for (name, _) in &d.params {
            if !params.insert(name) {
                self.out.push(err(
                    "SELU0520",
                    format!("type parameter `{name}` is already declared"),
                    &d.name_span,
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
                        &field.ty.span,
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
        self.alias_resolve(&t, &env);
    }
    pub(crate) fn alias_annotation(&mut self, t: &TypeExpr) -> Type {
        self.alias_resolve(t, &HashMap::new())
    }
    fn alias_resolve(&mut self, t: &TypeExpr, env: &HashMap<String, Type>) -> Type {
        let mut rewritten = t.clone();
        let mut names = HashMap::new();
        self.alias_rewrite(&mut rewritten, env, &mut names);
        // Keep class resolution's lexical namespace and replace only named-type leaves.
        if names.is_empty() {
            self.class_resolve(&rewritten)
        } else {
            self.class_resolve_with_aliases(&rewritten, &names)
        }
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
                let label = if supplied.is_empty() {
                    name.clone()
                } else {
                    format!(
                        "{}<{}>",
                        name,
                        supplied
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
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
                } else if self.aliases.active.contains(&label) {
                    Type::Named(label.clone(), self.aliases.shapes[&label].reference())
                } else if self.aliases.active.len() >= 64 {
                    self.out.push(err(
                        "SELU0523",
                        format!("type `{label}` expands without a finite shape"),
                        &t.span,
                    ));
                    Type::Error
                } else {
                    self.aliases.active.push(label.clone());
                    let shape = crate::types::NamedShape::new();
                    self.aliases.shapes.insert(label.clone(), shape.clone());
                    self.types.named_roots.push(shape.clone());
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
                    self.aliases.active.pop();
                    fn unguarded(t: &Type) -> bool {
                        match t {
                            Type::Optional(t) => unguarded(t),
                            Type::Named(_, s) => s.get().is_none(),
                            _ => false,
                        }
                    }
                    let naked_cycle = unguarded(&body);
                    if naked_cycle {
                        self.out.push(err(
                            "SELU0523",
                            format!("type `{label}` has no finite shape"),
                            &t.span,
                        ));
                    }
                    shape.set(if naked_cycle { Type::Error } else { body });
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
            K::Tuple(ts) => {
                for t in ts {
                    self.alias_rewrite(t, env, names);
                }
            }
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
            if let (ExprKind::Unary { op: unary, expr }, ExprKind::Str(tag)) =
                (&test.kind, &literal.kind)
                && unary == "typeof"
                && let ExprKind::Ident(n) = &expr.kind
            {
                let ty = match tag.as_str() {
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
                };
                return Some(if op == "==" {
                    (vec![(n.clone(), ty)], vec![])
                } else {
                    (vec![], vec![(n.clone(), ty)])
                });
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
        K::Tuple(ts) => {
            for t in ts {
                rename_types(t, names);
            }
        }
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

pub(crate) fn shape_of(t: &Type) -> Type {
    let mut t = t.clone();
    let mut seen = std::collections::HashSet::new();
    while let Type::Named(n, body) = t {
        if !seen.insert(n) {
            return Type::Error;
        }
        t = body.get().unwrap_or(Type::Error);
    }
    t
}

//! Saved declarations and load-time access checks.
use crate::{Ctx, Kind, Type, err};
use selu_diagnostics::Span;
use selu_syntax::{Stmt, StmtKind};
impl Ctx<'_> {
    pub(crate) fn declare_saved(&mut self, s: &Stmt) {
        if let StmtKind::Saved {
            name,
            name_span,
            defaults,
            ty,
            version_key,
            ..
        } = &s.kind
        {
            if version_key.is_some() {
                self.types.saved_adoptions.insert(name.clone());
            }
            let inferred = self.value(defaults);
            let ty = self.annotation(ty).unwrap_or(inferred);
            self.types.saved_schemas.insert(s.span.start, ty.clone());
            self.declare(
                name,
                name_span,
                Kind::Saved,
                adoption_type(ty, version_key.is_some()),
            );
        }
    }
    pub(crate) fn saved_access(&mut self, name: &str, span: &Span) {
        if self.fn_depth == 0
            && self.lookup(name).is_some_and(|(_, b)| {
                b.kind == Kind::Saved
                    || (b.kind == Kind::Import && self.module.saved_imports.contains(name))
            })
        {
            self.out.push(
                err(
                    "SELU0460",
                    "saved data is unavailable while addon files are loading",
                    span,
                )
                .with_help("read or write it inside a function or PLAYER_LOGIN handler"),
            );
        }
    }
    fn saved_root(&self, target: &selu_syntax::Expr) -> bool {
        use selu_syntax::ExprKind;
        match &target.kind {
            ExprKind::Ident(name) => self.lookup(name).is_some_and(|(_, b)| {
                b.kind == Kind::Saved
                    || (b.kind == Kind::Import && self.module.saved_imports.contains(name))
            }),
            ExprKind::Member { object, .. }
            | ExprKind::OptionalMember { object, .. }
            | ExprKind::Index { object, .. } => self.saved_root(object),
            _ => false,
        }
    }
    fn class_saved_error(&mut self, span: &Span) {
        self.out.push(err("SELU0469", "class instances cannot be saved because their metatables and methods cannot be persisted", span).with_help("copy the fields into a plain object, for example { x: p.x, y: p.y }"));
    }
    pub(crate) fn saved_write_value(&mut self, target: &selu_syntax::Expr, ty: &Type, span: &Span) {
        if self.saved_root(target) && contains_class(ty) {
            self.class_saved_error(span);
        }
    }
    pub(crate) fn saved_collection_write(
        &mut self,
        callee: &selu_syntax::Expr,
        args: &[selu_syntax::Expr],
    ) {
        if let selu_syntax::ExprKind::Member { object, name } = &callee.kind
            && matches!(name.as_str(), "push" | "insert" | "set" | "add")
            && self.saved_root(object)
        {
            for arg in args {
                if self.types.get(arg.id).is_some_and(contains_class) {
                    self.class_saved_error(&arg.span);
                }
            }
        }
    }
    pub(crate) fn saved_stmt(&mut self, s: &Stmt) {
        if self.scopes.len() != 1 {
            self.out.push(
                err(
                    "SELU0462",
                    "saved declarations and migrations must be at file scope",
                    &s.span,
                )
                .with_help("move it to the top level"),
            );
            return;
        }
        match &s.kind {
            StmtKind::Saved {
                name,
                name_span,
                defaults,
                version_key,
                legacy,
                adopted,
                ..
            } => {
                if name.starts_with("__selu")
                    || selu_syntax::is_lua_keyword(name)
                    || (*adopted && name == "_G")
                {
                    self.out.push(
                        err("SELU0463", "this saved name is reserved", name_span)
                            .with_help("choose another name"),
                    );
                }
                if !persistable(defaults) {
                    self.out.push(
                        err(
                            "SELU0469",
                            "saved defaults must contain only literal data or empty maps",
                            &defaults.span,
                        )
                        .with_help(
                            "use objects, arrays, strings, numbers, booleans and new Map<K, V>()",
                        ),
                    );
                }
                if let selu_syntax::ExprKind::Object(fields) = &defaults.kind {
                    for field in fields {
                        if version_key.as_ref() == Some(&field.key) {
                            self.out.push(err(
                                "SELU0900",
                                "the mapped version key is compiler-owned; omit it from defaults",
                                &field.key_span,
                            ));
                        }

                        if field.key == "__selu_version"
                            || (version_key.is_some()
                                && field.key == "__selu_saved_adoption_context")
                        {
                            self.out.push(err(
                                "SELU0470",
                                format!("{} is reserved for saved data", field.key),
                                &field.key_span,
                            ));
                        }
                    }
                }
                let inferred = self.value(defaults);
                let ty = self
                    .types
                    .saved_schemas
                    .get(&s.span.start)
                    .cloned()
                    .unwrap_or(inferred.clone());
                let inferred = self.value_expected(defaults, Some(&ty));
                if !inferred.assignable_to(&ty) {
                    self.out.push(err(
                        "SELU0313",
                        format!("defaults of type {inferred} do not fit {ty}"),
                        &defaults.span,
                    ));
                }
                if version_key.is_some() && !closed_schema(&ty, 0) {
                    self.out.push(err(
                        "SELU0901",
                        "adoption requires a finite closed record schema",
                        &defaults.span,
                    ));
                }
                if contains_class(&ty) {
                    self.class_saved_error(&defaults.span);
                } else if !serializable(&ty) {
                    self.out.push(
                        err(
                            "SELU0469",
                            "saved data contains a type that cannot be serialized",
                            &defaults.span,
                        )
                        .with_help("use primitive map keys and serializable values at every depth"),
                    );
                }
                if !matches!(crate::shape_of(&ty), Type::Object(_)) {
                    self.out.push(err(
                        "SELU0464",
                        "saved defaults must be an object",
                        &defaults.span,
                    ));
                }
                if (version_key.is_some() || legacy.is_some()) && !adopted {
                    self.out.push(err(
                        "SELU0900",
                        "adoption options require saved global",
                        name_span,
                    ));
                }
                if legacy.is_some() && version_key.is_none() {
                    self.out.push(err(
                        "SELU0900",
                        "legacy adoption requires a mapped version key",
                        name_span,
                    ));
                }
                if legacy.as_ref().is_some_and(|n| {
                    n == name
                        || n == "_G"
                        || n.starts_with("__selu")
                        || selu_syntax::is_lua_keyword(n)
                }) {
                    self.out
                        .push(err("SELU0900", "invalid legacy global", name_span));
                }
                if version_key.as_deref() == Some("__selu_saved_adoption_context") {
                    self.out.push(err(
                        "SELU0900",
                        "the version key cannot use the retry-context namespace",
                        name_span,
                    ));
                }
                self.finish_decl(name, adoption_type(ty, version_key.is_some()));
            }
            StmtKind::Migrate {
                name,
                name_span,
                step_span,
                from,
                to,
                param,
                context,
                body,
            } => {
                if self.lookup(name).is_none_or(|(_, b)| b.kind != Kind::Saved) {
                    self.out.push(
                        err(
                            "SELU0465",
                            "this migration does not name a saved declaration",
                            name_span,
                        )
                        .with_help("use a saved name declared in this file"),
                    );
                }
                if *to != from.saturating_add(1) {
                    self.out.push(
                        err(
                            "SELU0465",
                            "a migration must advance exactly one version",
                            step_span,
                        )
                        .with_help("write a separate migration for each adjacent version"),
                    );
                }
                if let Some((_, b)) = self.lookup(name).map(|(i, b)| (i, b.clone())) {
                    self.check_ready(name, &b, &s.span);
                }
                for param in std::iter::once(param).chain(context.as_deref()) {
                    if param.name.starts_with("__selu") || selu_syntax::is_lua_keyword(&param.name)
                    {
                        self.out.push(
                            err(
                                "SELU0463",
                                "this migration parameter name is reserved",
                                &param.span,
                            )
                            .with_help("choose another name"),
                        );
                    }
                }
                let adoption = self.types.saved_adoptions.contains(name);
                if adoption && param.ty.is_none() {
                    self.out.push(err(
                        "SELU0901",
                        "adoption migrations require a closed historical schema",
                        &param.span,
                    ));
                }
                if !adoption && (param.ty.is_some() || context.is_some()) {
                    self.out.push(err(
                        "SELU0901",
                        "typed historical readers require mapped-version adoption",
                        &param.span,
                    ));
                }
                let ty = self.annotation(&param.ty).unwrap_or(Type::Untyped);
                if param.ty.is_some()
                    && (!closed_schema(&ty, 0) || !matches!(crate::shape_of(&ty), Type::Object(_)))
                {
                    self.out.push(err(
                        "SELU0901",
                        "historical schemas must be closed serializable records",
                        &param.span,
                    ));
                }
                self.types.saved_readers.insert(s.span.start, ty.clone());
                let mut params = vec![param.clone()];
                let mut types = vec![ty];
                if let Some(context) = context {
                    let ty = self.annotation(&context.ty).unwrap_or(Type::Untyped);
                    if !closed_schema(&ty, 0) || !matches!(crate::shape_of(&ty), Type::Object(_)) {
                        self.out.push(err(
                            "SELU0901",
                            "migration context requires a closed historical record",
                            &context.span,
                        ));
                    }
                    self.types.saved_contexts.insert(s.span.start, ty.clone());
                    params.push(context.as_ref().clone());
                    types.push(ty);
                }
                self.function_body(&params, types, None, body);
            }
            _ => unreachable!(),
        }
    }
}

fn persistable(e: &selu_syntax::Expr) -> bool {
    use selu_syntax::ExprKind;
    match &e.kind {
        ExprKind::Number(_) | ExprKind::Bool(_) | ExprKind::Str(_) => true,
        ExprKind::Object(fs) => fs.iter().all(|f| persistable(&f.value)),
        ExprKind::Array(es) => es.iter().all(persistable),
        ExprKind::Unary { op, expr } => op == "-" && matches!(expr.kind, ExprKind::Number(_)),
        ExprKind::New { class, args, .. } => class == "Map" && args.is_empty(),
        _ => false,
    }
}

fn serializable(ty: &Type) -> bool {
    serializable_seen(ty, &mut Default::default())
}
fn serializable_seen(ty: &Type, seen: &mut std::collections::HashSet<String>) -> bool {
    if let Type::Named(_, s) = ty
        && !seen.insert(s.key.clone())
    {
        return true;
    }
    match crate::shape_of(ty) {
        Type::Number
        | Type::String
        | Type::Boolean
        | Type::Null
        | Type::StringLiteral(_)
        | Type::NumberLiteral(_)
        | Type::BooleanLiteral(_) => true,
        Type::Optional(t) | Type::Array(t) => serializable_seen(&t, seen),
        Type::Object(fs) => fs.values().all(|t| serializable_seen(t, seen)),
        Type::Union(ts) | Type::Tuple(ts) => ts.iter().all(|t| serializable_seen(t, seen)),
        Type::Map(k, v) => {
            matches!(
                crate::shape_of(&k),
                Type::Number | Type::String | Type::Boolean
            ) && serializable_seen(&v, seen)
        }
        Type::Set(k) => matches!(
            crate::shape_of(&k),
            Type::Number | Type::String | Type::Boolean
        ),
        _ => false,
    }
}
fn contains_class(ty: &Type) -> bool {
    contains_class_seen(ty, &mut Default::default())
}
fn contains_class_seen(ty: &Type, seen: &mut std::collections::HashSet<String>) -> bool {
    if let Type::Named(_, s) = ty
        && !seen.insert(s.key.clone())
    {
        return false;
    }
    match crate::shape_of(ty) {
        Type::Class(..) | Type::ClassObject(_) => true,
        Type::Optional(t) | Type::Array(t) | Type::Set(t) => contains_class_seen(&t, seen),
        Type::Object(fs) => fs.values().any(|t| contains_class_seen(t, seen)),
        Type::Union(ts) | Type::Tuple(ts) => ts.iter().any(|t| contains_class_seen(t, seen)),
        Type::Map(k, v) | Type::WeakMap(k, v) => {
            contains_class_seen(&k, seen) || contains_class_seen(&v, seen)
        }
        _ => false,
    }
}

fn adoption_type(ty: Type, adoption: bool) -> Type {
    if !adoption {
        return ty;
    }
    Type::Object(
        [
            ("status".into(), Type::String),
            ("data".into(), Type::Optional(Box::new(ty.clone()))),
            (
                "whenReady".into(),
                saved_function(vec![saved_function(vec![ty], Type::Void)], Type::Void),
            ),
        ]
        .into(),
    )
}

pub(crate) fn closed_schema(ty: &Type, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    match crate::shape_of(ty) {
        Type::Number
        | Type::String
        | Type::Boolean
        | Type::Null
        | Type::StringLiteral(_)
        | Type::NumberLiteral(_)
        | Type::BooleanLiteral(_) => true,
        Type::Optional(t) | Type::Array(t) => closed_schema(&t, depth + 1),
        Type::Union(ts) => ts.iter().all(|t| closed_schema(t, depth + 1)),
        Type::SavedRepair(t) => {
            matches!(
                crate::shape_of(&t),
                Type::Object(_) | Type::Array(_) | Type::Map(..)
            ) && closed_schema(&t, depth + 1)
        }
        Type::Object(fs) => fs.values().all(|t| closed_schema(t, depth + 1)),
        Type::Map(k, v) => {
            matches!(
                crate::shape_of(&k),
                Type::Number | Type::String | Type::Boolean
            ) && closed_schema(&v, depth + 1)
        }
        _ => false,
    }
}

fn saved_function(params: Vec<Type>, ret: Type) -> Type {
    Type::Function {
        multiple_results: false,
        unspecified_callback: false,
        api: Default::default(),
        api_semantics: Default::default(),
        required: params.len(),
        params,
        ret: Box::new(ret),
        rest: None,
    }
}

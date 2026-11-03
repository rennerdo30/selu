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
            ..
        } = &s.kind
        {
            let ty = self.value(defaults);
            self.declare(name, name_span, Kind::Saved, ty.clone());
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
                ..
            } => {
                if name.starts_with("__selu") || selu_syntax::is_lua_keyword(name) {
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
                        if field.key == "__selu_version" {
                            self.out.push(err(
                                "SELU0470",
                                "__selu_version is reserved for saved data",
                                &field.key_span,
                            ));
                        }
                    }
                }
                let ty = self.value(defaults);
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
                if !matches!(ty, Type::Object(_)) {
                    self.out.push(err(
                        "SELU0464",
                        "saved defaults must be an object",
                        &defaults.span,
                    ));
                }
                self.finish_decl(name, ty);
            }
            StmtKind::Migrate {
                name,
                name_span,
                step_span,
                from,
                to,
                param,
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
                if param.name.starts_with("__selu") || selu_syntax::is_lua_keyword(&param.name) {
                    self.out.push(
                        err(
                            "SELU0463",
                            "this migration parameter name is reserved",
                            &param.span,
                        )
                        .with_help("choose another name"),
                    );
                }
                self.function_body(std::slice::from_ref(param), vec![Type::Untyped], None, body);
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
    match ty {
        Type::Number | Type::String | Type::Boolean | Type::Null => true,
        Type::Optional(t) | Type::Array(t) => serializable(t),
        Type::Object(fs) => fs.values().all(serializable),
        Type::Tuple(ts) => ts.iter().all(serializable),
        Type::Map(k, v) => {
            matches!(**k, Type::Number | Type::String | Type::Boolean) && serializable(v)
        }
        Type::Set(k) => matches!(**k, Type::Number | Type::String | Type::Boolean),
        _ => false,
    }
}

fn contains_class(ty: &Type) -> bool {
    match ty {
        Type::Class(..) | Type::ClassObject(_) => true,
        Type::Optional(t) | Type::Array(t) | Type::Set(t) => contains_class(t),
        Type::Object(fields) => fields.values().any(contains_class),
        Type::Tuple(types) => types.iter().any(contains_class),
        Type::Map(k, v) | Type::WeakMap(k, v) => contains_class(k) || contains_class(v),
        _ => false,
    }
}

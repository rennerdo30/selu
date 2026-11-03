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
                from,
                to,
                param,
                body,
            } => {
                if self.lookup(name).is_none_or(|(_, b)| b.kind != Kind::Saved)
                    || *to != from.saturating_add(1)
                {
                    self.out.push(err(
                        "SELU0465",
                        "a migration must name saved data and advance exactly one version",
                        &s.span,
                    ));
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

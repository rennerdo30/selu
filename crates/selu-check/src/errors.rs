//! Structured error checking.
use crate::{Ctx, Type, err};
use selu_syntax::{Expr, Param, Stmt, StmtKind, TypeExpr};
pub(crate) fn error_type() -> Type {
    Type::Class(
        "Error".into(),
        vec![],
        [
            ("message".into(), Type::String),
            ("stack".into(), Type::optional(Type::String)),
            ("value".into(), Type::Unknown),
        ]
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>()
        .into(),
    )
}
impl Ctx<'_> {
    pub(crate) fn reserved_error_name(&mut self, name: &str, span: &selu_diagnostics::Span) {
        // Some bindings are visited by more than one declaration pass.
        if name == "Error"
            && !self
                .out
                .iter()
                .any(|d| d.code == "SELU0542" && d.span.as_ref() == Some(span))
        {
            self.out.push(
                err("SELU0542", "Error is a reserved built-in name", span)
                    .with_help("choose another declaration name"),
            );
        }
    }

    pub(crate) fn error_new(&mut self, e: &Expr, types: &[TypeExpr], args: &[Expr]) -> Type {
        let valid = args.len() <= 1 && types.is_empty();
        let mut strings = true;
        let mut failed = false;
        for a in args {
            let t = crate::shape_of(&self.value(a));
            failed |= t == Type::Error;
            strings &= t == Type::String;
        }
        if failed {
            return Type::Error;
        }
        if !valid || !strings {
            self.out.push(
                err(
                    "SELU0540",
                    "Error takes an optional string message",
                    &e.span,
                )
                .with_help("write `new Error(\"message\")`"),
            );
            return Type::Error;
        }
        error_type()
    }
    pub(crate) fn error_stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Throw { value, .. } => {
                let t = crate::shape_of(&self.value(value));
                if t != Type::Error && t != error_type() {
                    self.out.push(
                        err("SELU0541", "throw needs an Error value", &value.span)
                            .with_help("wrap the message in `new Error(...)`"),
                    );
                }
            }
            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                self.prepare_region(body, &[]);
                let entry = self.flow_state();
                self.block(body, &[], Default::default());
                let body_out = self.flow_state();
                let mut outgoing = Vec::new();
                if !crate::always_exits(body) {
                    outgoing.push(body_out);
                }
                if let Some((p, b)) = catch {
                    self.restore_flow(entry);
                    let params: Vec<(Param, Type)> =
                        p.iter().map(|p| (p.clone(), error_type())).collect();
                    self.block(b, &params, Default::default());
                    if !crate::always_exits(b) {
                        outgoing.push(self.flow_state());
                    }
                }
                self.restore_flow(crate::flow_state::intersect(outgoing));
                if let Some(b) = finally {
                    self.block(b, &[], Default::default());
                }
            }
            _ => unreachable!(),
        }
    }
}

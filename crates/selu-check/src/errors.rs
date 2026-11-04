//! Structured error checking.
use crate::{Ctx, Type, err};
use selu_syntax::{Expr, Param, Stmt, StmtKind, TypeExpr};
pub(crate) fn error_type() -> Type {
    Type::Class(
        "Error".into(),
        vec![],
        [("message".into(), Type::String)].into(),
    )
}
impl Ctx<'_> {
    pub(crate) fn error_new(&mut self, e: &Expr, types: &[TypeExpr], args: &[Expr]) -> Type {
        let valid = args.len() == 1 && types.is_empty();
        let mut strings = true;
        for a in args {
            strings &= crate::shape_of(&self.value(a)) == Type::String;
        }
        if !valid || !strings {
            self.out.push(
                err("SELU0540", "Error needs one string message", &e.span)
                    .with_help("write `new Error(\"message\")`"),
            );
        }
        error_type()
    }
    pub(crate) fn error_stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Throw { value, .. } => {
                if crate::shape_of(&self.value(value)) != error_type() {
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
                self.block(body, &[], Default::default());
                if let Some((p, b)) = catch {
                    let params: Vec<(Param, Type)> = vec![(p.clone(), error_type())];
                    self.block(b, &params, Default::default());
                }
                if let Some(b) = finally {
                    self.block(b, &[], Default::default());
                }
            }
            _ => unreachable!(),
        }
    }
}

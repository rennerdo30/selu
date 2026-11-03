//! Control flow (package A in docs/WORK-PACKAGES.md).
//!
//! Loops, `break`/`continue`, `cond ? a : b`, `x += v` and `x++`.

use crate::{Ctx, Type};
use selu_syntax::{Expr, Stmt};

impl Ctx<'_> {
    pub(crate) fn loop_stmt(&mut self, s: &Stmt) {
        self.unsupported("this loop statement", &s.span);
    }

    pub(crate) fn control_expr(&mut self, e: &Expr) -> Type {
        self.unsupported("this expression", &e.span)
    }
}

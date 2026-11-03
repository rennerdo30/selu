//! Functions (package C in docs/WORK-PACKAGES.md).
//!
//! Arrow and function expressions, default and rest parameters, destructuring.

use crate::{Ctx, Type};
use selu_syntax::{Expr, Param, Stmt};

impl Ctx<'_> {
    pub(crate) fn function_expr(&mut self, e: &Expr) -> Type {
        self.unsupported("a function expression", &e.span)
    }

    /// A parameter with a default value or `...rest`.
    pub(crate) fn params_with_extras(&mut self, p: &Param) {
        self.unsupported("default and rest parameters", &p.span);
    }

    pub(crate) fn destructure(&mut self, s: &Stmt) {
        self.unsupported("destructuring", &s.span);
    }
}

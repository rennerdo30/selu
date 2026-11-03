//! Data (package B in docs/WORK-PACKAGES.md).
//!
//! Arrays, objects, indexing, `?.`, `new Map/Set/WeakMap`, template strings.

use crate::{Ctx, Type};
use selu_syntax::Expr;

impl Ctx<'_> {
    pub(crate) fn data_expr(&mut self, e: &Expr) -> Type {
        self.unsupported("this expression", &e.span)
    }
}

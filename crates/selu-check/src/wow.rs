//! WoW features (package D, later in docs/WORK-PACKAGES.md).
//!
//! Tagged templates (`L`, `log`), and later `saved`, `command`, `hook`.

use crate::{Ctx, Type};
use selu_syntax::Expr;

impl Ctx<'_> {
    pub(crate) fn tagged_template(&mut self, e: &Expr) -> Type {
        self.unsupported("a tagged template", &e.span)
    }
}

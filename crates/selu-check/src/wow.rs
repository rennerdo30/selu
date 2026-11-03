//! WoW features (package D4 in docs/WORK-PACKAGES.md): the built-ins
//! `command`, `hook`, `hookScript` and `log`, and tagged templates (`L`,
//! `log.warn`).

use crate::{Ctx, Type};
use selu_syntax::Expr;

impl Ctx<'_> {
    pub(crate) fn tagged_template(&mut self, e: &Expr) -> Type {
        self.unsupported("a tagged template", &e.span)
    }

    /// A call to one of Selu's built-ins. `None` when `callee` isn't one
    /// (including when the code declares a name that shadows it); the
    /// normal call checking then runs.
    pub(crate) fn builtin_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> Option<Type> {
        let _ = (e, callee, args);
        None
    }
}

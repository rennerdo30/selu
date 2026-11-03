//! Classes (package D3 in docs/WORK-PACKAGES.md): `class`, fields,
//! `constructor`, methods, `static`, `extends`, `super` and `this`.

use crate::{Ctx, Type};
use selu_syntax::{ClassDecl, Expr, Stmt};

impl Ctx<'_> {
    /// Declares the class name while a block declares its names.
    pub(crate) fn declare_class(&mut self, c: &ClassDecl) {
        let _ = c;
    }

    pub(crate) fn class_decl(&mut self, s: &Stmt, c: &ClassDecl) {
        let _ = c;
        self.unsupported("`class`", &s.span);
    }

    /// `this` and `super`.
    pub(crate) fn class_expr(&mut self, e: &Expr) -> Type {
        self.unsupported("`this` and `super`", &e.span)
    }
}

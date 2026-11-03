//! Modules (package D1 in docs/WORK-PACKAGES.md): `import`, `export`, the
//! shared addon table and the `.toc` order from the import graph.

use crate::{Ctx, Type};
use selu_syntax::Stmt;

impl Ctx<'_> {
    /// Declares the names an `import` brings into the file's scope (called
    /// while a block declares its names, before any statement is checked).
    pub(crate) fn declare_imports(&mut self, s: &Stmt) {
        let _ = s;
    }

    pub(crate) fn import_stmt(&mut self, s: &Stmt) {
        self.unsupported("`import`", &s.span);
    }

    /// `export <declaration>`; `inner` is the declaration.
    pub(crate) fn export_stmt(&mut self, s: &Stmt, inner: &Stmt) {
        let _ = inner;
        let _: Type = self.unsupported("`export`", &s.span);
    }
}

//! Cross-file signatures and module declaration checks.
use crate::{Ctx, Kind, Type, err};
use selu_diagnostics::Span;
use selu_syntax::{Stmt, StmtKind};
use std::collections::BTreeMap;
#[derive(Debug, Clone)]
pub struct ModuleExport {
    pub ty: Type,
    pub mutable: bool,
    /// Class signatures reachable through this export, keyed by module and declaration.
    pub classes: std::collections::HashMap<String, crate::ClassInfo>,
}
impl Default for ModuleExport {
    fn default() -> Self {
        Self {
            ty: Type::Untyped,
            mutable: false,
            classes: Default::default(),
        }
    }
}
pub type ModuleExports = BTreeMap<String, ModuleExport>;
#[derive(Debug, Default, Clone)]
pub struct ModuleBindings {
    pub imports: BTreeMap<String, ModuleExports>,
    pub exports: ModuleExports,
}
impl Ctx<'_> {
    /// Imports are hoisted so every signature in the block can see their types.
    pub(crate) fn declare_block_imports(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            if matches!(s.kind, StmtKind::Import { .. }) {
                self.declare_imports(s);
            }
        }
    }

    pub(crate) fn declare_imports(&mut self, s: &Stmt) {
        if let StmtKind::Import {
            names,
            from,
            default,
            ..
        } = &s.kind
        {
            if default.is_some() {
                return;
            }
            for (remote, local, span, _) in names {
                let ty = self
                    .module
                    .imports
                    .get(from)
                    .and_then(|m| m.get(remote))
                    .map(|e| e.ty.clone())
                    .unwrap_or(Type::Untyped);
                self.declare(local, span, Kind::Import, ty.clone());
                self.finish_decl(local, ty);
            }
        }
    }
    pub(crate) fn import_stmt(&mut self, s: &Stmt) {
        if self.module_not_top_level(s) {
            return;
        }
        if let StmtKind::Import {
            names,
            from,
            from_span,
            default,
        } = &s.kind
        {
            if default.is_some() {
                self.unsupported("asset imports", &s.span);
                return;
            }
            let Some(exports) = self.module.imports.get(from) else {
                self.out.push(
                    err(
                        "SELU0400",
                        format!("module `{from}` was not resolved"),
                        from_span,
                    )
                    .with_help("build the project from src/main.selu"),
                );
                return;
            };
            for (remote, _, _, span) in names {
                if !exports.contains_key(remote) {
                    let help = "check the exported name or export it from the imported module";
                    self.out.push(
                        err(
                            "SELU0401",
                            format!("module `{from}` does not export `{remote}`"),
                            span,
                        )
                        .with_help(help),
                    );
                }
            }
        }
    }
    pub(crate) fn module_not_top_level(&mut self, s: &Stmt) -> bool {
        if self.scopes.len() != 1 {
            self.out.push(
                err(
                    "SELU0406",
                    "imports and exports are only allowed at the top level",
                    &s.span,
                )
                .with_help("move it to the top level"),
            );
            true
        } else {
            false
        }
    }
    pub(crate) fn export_stmt(&mut self, s: &Stmt, inner: &Stmt) {
        if self.module_not_top_level(s) {
            return;
        }
        self.stmt(inner);
        let (name, mutable) = match &inner.kind {
            StmtKind::VarDecl { name, is_const, .. } => (name, !is_const),
            StmtKind::FuncDecl { name, .. } => (name, false),
            StmtKind::Global { name, .. } => (name, true),
            StmtKind::Class(c) => (&c.name, false),
            _ => return,
        };
        if let Some(ty) = self.type_of(name) {
            self.module.exports.insert(
                name.clone(),
                ModuleExport {
                    ty,
                    mutable,
                    classes: self.classes.infos.clone(),
                },
            );
        }
    }
    pub(crate) fn module_write(&mut self, name: &str, span: &Span) -> bool {
        if self
            .lookup(name)
            .is_some_and(|(_, b)| b.kind == Kind::Import)
        {
            self.out.push(
                err("SELU0402", format!("import `{name}` is read-only"), span)
                    .with_help("call an exported function to change the module's state"),
            );
            true
        } else {
            false
        }
    }
}

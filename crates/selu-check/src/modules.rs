//! Cross-file signatures and module declaration checks.
use crate::{Ctx, Kind, Type, err};
use selu_diagnostics::Span;
use selu_syntax::{Stmt, StmtKind};
use std::collections::BTreeMap;
#[derive(Debug, Clone)]
pub struct ModuleExport {
    pub has_value: bool,
    pub type_contexts: std::sync::Arc<
        std::collections::HashMap<String, std::sync::Arc<std::collections::HashMap<String, Type>>>,
    >,
    pub aliases: std::sync::Arc<std::collections::HashMap<String, selu_syntax::TypeDecl>>,
    pub ty: Type,
    pub mutable: bool,
    pub saved: bool,
    /// Class signatures reachable through this export, keyed by module and declaration.
    pub classes: std::sync::Arc<std::collections::HashMap<String, crate::ClassInfo>>,
}
impl Default for ModuleExport {
    fn default() -> Self {
        Self {
            has_value: true,
            type_contexts: Default::default(),
            aliases: Default::default(),
            ty: Type::Untyped,
            mutable: false,
            classes: Default::default(),
            saved: false,
        }
    }
}
pub type ModuleExports = BTreeMap<String, ModuleExport>;
#[derive(Debug, Default, Clone)]
pub struct ModuleBindings {
    /// Saved import aliases, populated during the block import prepass.
    pub saved_imports: std::collections::HashSet<String>,
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
                if self
                    .module
                    .imports
                    .get(from)
                    .and_then(|m| m.get(remote))
                    .is_some_and(|e| !e.has_value)
                {
                    continue;
                }
                let ty = self
                    .module
                    .imports
                    .get(from)
                    .and_then(|m| m.get(remote))
                    .map(|e| e.ty.clone())
                    .unwrap_or(Type::Untyped);
                if self
                    .module
                    .imports
                    .get(from)
                    .and_then(|m| m.get(remote))
                    .is_some_and(|e| e.saved)
                {
                    self.module.saved_imports.insert(local.clone());
                }
                let external = self
                    .module
                    .imports
                    .get(from)
                    .and_then(|m| m.get(remote))
                    .is_some_and(|e| e.mutable);
                self.declare_classified(local, span, Kind::Import, ty.clone(), external);
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
            if let Some(diagnostic) = reserved_module_path(from, from_span) {
                self.out.push(diagnostic);
                return;
            }
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
        let context = std::sync::Arc::new(self.class_type_names());
        for d in self
            .aliases
            .declarations
            .values()
            .filter(|d| d.name_span.file == s.span.file)
        {
            self.aliases
                .contexts
                .insert(crate::aliases::declaration_key(d), context.clone());
        }
        self.stmt(inner);
        if let StmtKind::TypeDecl(d) = &inner.kind {
            let e = self
                .module
                .exports
                .entry(d.name.clone())
                .or_insert_with(|| ModuleExport {
                    has_value: false,
                    ..Default::default()
                });
            e.aliases = self.aliases.declarations.clone();
            return;
        }
        let (name, mutable) = match &inner.kind {
            StmtKind::VarDecl { name, is_const, .. } => (name, !is_const),
            StmtKind::Saved { name, .. } => (name, true),
            StmtKind::FuncDecl { name, .. } => (name, false),
            StmtKind::Global { name, .. } => (name, true),
            StmtKind::Class(c) => (&c.name, false),
            _ => return,
        };
        if let Some(ty) = self.type_of(name) {
            let e = self.module.exports.entry(name.clone()).or_default();
            e.has_value = true;
            e.ty = ty;
            e.mutable = mutable;
            // The final module snapshot is attached once after checking.
            e.saved = matches!(inner.kind, StmtKind::Saved { .. });
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

/// Module namespace slots share the compiler-reserved prefix with generated names.
pub fn reserved_module_path(path: &str, span: &Span) -> Option<selu_diagnostics::Diagnostic> {
    path.split(['/', '\\'])
        .any(|part| part.starts_with("__selu"))
        .then(|| {
            err(
                "SELU0318",
                "module path names starting with __selu are reserved",
                span,
            )
            .with_help("rename the file or directory so no path segment starts with __selu")
        })
}

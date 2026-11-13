//! Module exports use per-module tables; mutable exports are getter closures.
use crate::{
    Lowerer,
    lua::{self, LBlock, LExpr, LFunction, LStmt, LStmtKind, name},
};
use selu_syntax::{Expr, ExprKind, Program, Stmt, StmtKind, visit};
use std::collections::{BTreeMap, HashSet};
#[derive(Default, Clone)]
pub struct ModuleBindings {
    pub key: String,
    pub(crate) type_only: bool,
    pub addon_name: String,
    /// Some module of the addon uses try/throw (see `uses_errors`).
    pub addon_errors: bool,
    pub imports: BTreeMap<String, (String, selu_check::ModuleExports)>,
    live_reads: HashSet<u32>,
    declarations: Vec<Stmt>,
}
impl ModuleBindings {
    pub fn new(
        key: String,
        imports: BTreeMap<String, (String, selu_check::ModuleExports)>,
        program: &Program,
    ) -> Self {
        let mut live = HashSet::new();
        for s in &program.stmts {
            if let StmtKind::Import { names, from, .. } = &s.kind
                && let Some((_, exports)) = imports.get(from)
            {
                for (remote, local, _, _) in names {
                    if exports.get(remote).is_some_and(|e| e.mutable) {
                        live.insert(local.clone());
                    }
                }
            }
        }
        let mut live_reads = HashSet::new();
        walk_block(&program.stmts, &live, &mut live_reads, false);
        Self {
            addon_errors: false,
            type_only: program.stmts.iter().all(|s| match &s.kind {
                StmtKind::TypeDecl(_) => true,
                StmtKind::Export(inner) => matches!(inner.kind, StmtKind::TypeDecl(_)),
                StmtKind::Import {
                    names,
                    from,
                    default,
                    ..
                } => {
                    default.is_none()
                        && imports.get(from).is_some_and(|(_, es)| {
                            names
                                .iter()
                                .all(|(n, ..)| es.get(n).is_some_and(|e| !e.has_value))
                        })
                }
                _ => false,
            }),
            addon_name: "Addon".into(),
            key,
            imports,
            live_reads,
            declarations: program
                .stmts
                .iter()
                .filter(|s| matches!(s.kind, StmtKind::Import { .. }))
                .cloned()
                .collect(),
        }
    }
}
fn bindings(s: &Stmt, active: &mut HashSet<String>) {
    match &s.kind {
        StmtKind::VarDecl { name, .. } | StmtKind::FuncDecl { name, .. } => {
            active.remove(name);
        }
        StmtKind::Destructure { pattern, .. } => match pattern {
            selu_syntax::Pattern::Array(xs) => {
                for (n, _) in xs.iter().flatten() {
                    active.remove(n);
                }
            }
            selu_syntax::Pattern::Object(xs) => {
                for (_, n, _, _) in xs {
                    active.remove(n);
                }
            }
        },
        StmtKind::Class(c) => {
            active.remove(&c.name);
        }
        _ => {}
    }
}
fn walk_block(ss: &[Stmt], active: &HashSet<String>, out: &mut HashSet<u32>, shadow: bool) {
    let mut active = active.clone();
    if shadow {
        for s in ss {
            bindings(s, &mut active);
        }
    }
    for s in ss {
        walk_stmt(s, &active, out);
    }
}
fn walk_stmt(s: &Stmt, active: &HashSet<String>, out: &mut HashSet<u32>) {
    let mut active = active.clone();
    match &s.kind {
        StmtKind::FuncDecl { params, body, .. } | StmtKind::OnHandler { params, body, .. } => {
            for p in params {
                active.remove(&p.name);
            }
            for p in params {
                if let Some(e) = &p.default {
                    walk_expr(e, &active, out);
                }
            }
            walk_block(body, &active, out, true);
            return;
        }
        StmtKind::Class(c) => {
            for m in &c.members {
                match &m.kind {
                    selu_syntax::ClassMemberKind::Field { init: Some(e), .. } => {
                        walk_expr(e, &active, out)
                    }
                    selu_syntax::ClassMemberKind::Method { params, body, .. }
                    | selu_syntax::ClassMemberKind::Constructor { params, body } => {
                        let mut active = active.clone();
                        for p in params {
                            active.remove(&p.name);
                        }
                        for p in params {
                            if let Some(e) = &p.default {
                                walk_expr(e, &active, out);
                            }
                        }
                        walk_block(body, &active, out, true);
                    }
                    _ => {}
                }
            }
            return;
        }
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            walk_block(body, &active, out, true);
            if let Some((p, b)) = catch {
                let mut caught = active.clone();
                if let Some(p) = p {
                    caught.remove(&p.name);
                }
                walk_block(b, &caught, out, true);
            }
            if let Some(b) = finally {
                walk_block(b, &active, out, true);
            }
            return;
        }
        StmtKind::Block(ss) => {
            walk_block(ss, &active, out, true);
            return;
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            walk_expr(cond, &active, out);
            walk_block(then_branch, &active, out, true);
            if let Some(ss) = else_branch {
                walk_block(ss, &active, out, true);
            }
            return;
        }
        StmtKind::While { cond, body } | StmtKind::DoWhile { cond, body } => {
            walk_expr(cond, &active, out);
            walk_block(body, &active, out, true);
            return;
        }
        StmtKind::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(s) = init {
                bindings(s, &mut active);
                walk_stmt(s, &active, out);
            }
            if let Some(e) = cond {
                walk_expr(e, &active, out);
            }
            if let Some(e) = update {
                walk_expr(e, &active, out);
            }
            walk_block(body, &active, out, true);
            return;
        }
        StmtKind::ForOf {
            binding,
            iter,
            body,
            ..
        } => {
            walk_expr(iter, &active, out);
            match binding {
                selu_syntax::ForBinding::Name(n, _) => {
                    active.remove(n);
                }
                selu_syntax::ForBinding::Pattern(p) => {
                    let fake = Stmt {
                        kind: StmtKind::Destructure {
                            is_const: true,
                            pattern: p.clone(),
                            init: iter.clone(),
                        },
                        span: s.span.clone(),
                    };
                    bindings(&fake, &mut active);
                }
            }
            walk_block(body, &active, out, true);
            return;
        }
        _ => {}
    }
    visit::child_stmts_and_exprs(s, &mut |c| match c {
        visit::Child::Stmt(s) => walk_stmt(s, &active, out),
        visit::Child::Expr(e) => walk_expr(e, &active, out),
    });
}
pub(crate) fn walk_expr(e: &Expr, active: &HashSet<String>, out: &mut HashSet<u32>) {
    if let ExprKind::Ident(n) = &e.kind
        && active.contains(n)
    {
        out.insert(e.id);
    }
    if let ExprKind::Function { params, body, .. } = &e.kind {
        let mut active = active.clone();
        for p in params {
            active.remove(&p.name);
        }
        for p in params {
            if let Some(e) = &p.default {
                walk_expr(e, &active, out);
            }
        }
        match body {
            selu_syntax::FnBody::Block(ss) => walk_block(ss, &active, out, true),
            selu_syntax::FnBody::Expr(e) => walk_expr(e, &active, out),
        }
        return;
    }
    visit::child_exprs_and_stmts(e, &mut |c| match c {
        visit::Child::Stmt(s) => walk_stmt(s, active, out),
        visit::Child::Expr(e) => walk_expr(e, active, out),
    });
}
/// `__selu_ns["key"]`: a module's export table.
pub(crate) fn module_table(key: &str) -> LExpr {
    table(key)
}
fn table(key: &str) -> LExpr {
    LExpr::Index(
        Box::new(name("__selu_ns")),
        Box::new(LExpr::Str(key.into())),
    )
}
impl Lowerer<'_> {
    pub(crate) fn module_binding(&self, e: &Expr) -> Option<LExpr> {
        if self.module.live_reads.contains(&e.id)
            && let ExprKind::Ident(n) = &e.kind
        {
            return Some(lua::call(name(n), vec![]));
        }
        None
    }
    pub(crate) fn module_prologue(&self, out: &mut LBlock) {
        for s in &self.module.declarations {
            self.emit_import(s, out);
        }
        if self.module.type_only {
            return;
        }
        out.push(LStmt {
            kind: LStmtKind::Assign(vec![table(&self.module.key)], vec![LExpr::Table(vec![])]),
            span: None,
        });
        // Re-exports publish the imported module's slot as is: a getter for
        // a mutable export stays a getter.
        for s in &self.module.declarations {
            if let StmtKind::Import {
                names,
                from,
                reexport: true,
                ..
            } = &s.kind
                && let Some((key, exports)) = self.module.imports.get(from)
            {
                for (remote, local, _, _) in names {
                    if exports.get(remote).is_some_and(|e| !e.has_value) {
                        continue;
                    }
                    out.push(LStmt::new(
                        LStmtKind::Assign(
                            vec![LExpr::Field(
                                Box::new(table(&self.module.key)),
                                local.clone(),
                            )],
                            vec![LExpr::Field(Box::new(table(key)), remote.clone())],
                        ),
                        &s.span,
                    ));
                }
            }
        }
    }
    pub(crate) fn import_stmt(&mut self, _s: &Stmt, _out: &mut LBlock) {}
    fn emit_import(&self, s: &Stmt, out: &mut LBlock) {
        if let StmtKind::Import { names, from, .. } = &s.kind
            && let Some((key, exports)) = self.module.imports.get(from)
        {
            for (remote, local, _, _) in names {
                if exports.get(remote).is_some_and(|e| !e.has_value) {
                    continue;
                }
                out.push(LStmt::new(
                    LStmtKind::Local(
                        vec![local.clone()],
                        vec![LExpr::Field(Box::new(table(key)), remote.clone())],
                    ),
                    &s.span,
                ));
            }
        }
    }
    pub(crate) fn export_stmt(&mut self, _s: &Stmt, inner: &Stmt, forward: bool, out: &mut LBlock) {
        self.stmt(inner, forward, out);
        let (n, mutable) = match &inner.kind {
            StmtKind::VarDecl { name, is_const, .. } => (name, !is_const),
            StmtKind::Saved { name, .. } => (name, true),
            StmtKind::FuncDecl { name, .. } => (name, false),
            StmtKind::Global { name, .. } => (name, true),
            StmtKind::Class(c) => (&c.name, false),
            _ => return,
        };
        if self.module.key.is_empty() {
            return;
        }
        let value = if mutable {
            LExpr::Function(LFunction {
                params: vec![],
                vararg: false,
                body: vec![LStmt::new(LStmtKind::Return(vec![name(n)]), &inner.span)],
            })
        } else {
            name(n)
        };
        out.push(LStmt::new(
            LStmtKind::Assign(
                vec![LExpr::Field(Box::new(table(&self.module.key)), n.clone())],
                vec![value],
            ),
            &inner.span,
        ));
    }
}

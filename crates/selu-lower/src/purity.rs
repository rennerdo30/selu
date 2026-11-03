//! Conservative origin proof; structural types do not prove absence of __index.
use crate::{Lowerer, declaration_names};
use selu_syntax::{
    ClassMemberKind, Expr, ExprKind, ForBinding, Param, Pattern, Program, Stmt, StmtKind,
    visit::{self, Child},
};
use std::collections::{HashMap, HashSet};
#[derive(Default)]
pub(crate) struct Origins {
    literals: HashMap<String, Expr>,
    bindings: HashMap<String, usize>,
    writes: HashSet<String>,
}
impl Origins {
    pub(crate) fn collect(program: &Program) -> Self {
        let mut origins = Self::default();
        for stmt in &program.stmts {
            origins.stmt(stmt);
        }
        origins
            .literals
            .retain(|n, _| origins.bindings.get(n) == Some(&1) && !origins.writes.contains(n));
        origins
    }
    fn binding(&mut self, name: &str) {
        *self.bindings.entry(name.into()).or_default() += 1;
    }
    fn params(&mut self, ps: &[Param]) {
        for p in ps {
            self.binding(&p.name);
        }
    }
    fn child(&mut self, c: Child<'_>) {
        match c {
            Child::Stmt(s) => self.stmt(s),
            Child::Expr(e) => self.expr(e),
        }
    }
    fn stmt(&mut self, s: &Stmt) {
        // The visitor visits exported declarations themselves as children.
        if !matches!(s.kind, StmtKind::Export(_)) {
            for n in declaration_names(s) {
                self.binding(&n);
            }
        }
        match &s.kind {
            StmtKind::VarDecl {
                name,
                init: Some(e),
                ..
            } => {
                self.literals.insert(name.clone(), *e.clone());
            }
            StmtKind::FuncDecl { params, .. } | StmtKind::OnHandler { params, .. } => {
                self.params(params)
            }
            StmtKind::Migrate { param, .. } => self.params(std::slice::from_ref(param)),
            StmtKind::Global { name, .. } => self.binding(name),
            StmtKind::Import { names, default, .. } => {
                for (_, n, _, _) in names {
                    self.binding(n);
                }
                if let Some((n, _)) = default {
                    self.binding(n);
                }
            }
            StmtKind::ForOf { binding, .. } => match binding {
                ForBinding::Name(n, _) => self.binding(n),
                ForBinding::Pattern(Pattern::Array(items)) => {
                    for (n, _) in items.iter().flatten() {
                        self.binding(n);
                    }
                }
                ForBinding::Pattern(Pattern::Object(items)) => {
                    for (_, n, _, _) in items {
                        self.binding(n);
                    }
                }
            },
            StmtKind::Class(c) => {
                for m in &c.members {
                    match &m.kind {
                        ClassMemberKind::Method { params, .. }
                        | ClassMemberKind::Constructor { params, .. } => self.params(params),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        visit::child_stmts_and_exprs(s, &mut |c| self.child(c));
    }
    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Function { params, .. } => self.params(params),
            ExprKind::Assign { target, .. }
            | ExprKind::CompoundAssign { target, .. }
            | ExprKind::Update { target, .. } => {
                if let ExprKind::Ident(n) = &target.kind {
                    self.writes.insert(n.clone());
                }
            }
            _ => {}
        }
        visit::child_exprs_and_stmts(e, &mut |c| self.child(c));
    }
}
impl Lowerer<'_> {
    pub(crate) fn plain_table(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::This => true,
            ExprKind::Object(_) | ExprKind::Array(_) | ExprKind::New { .. } => true,
            ExprKind::Ident(n) => self.origins.literals.get(n).is_some_and(|e| {
                matches!(
                    e.kind,
                    ExprKind::Object(_) | ExprKind::Array(_) | ExprKind::New { .. }
                )
            }),
            // Types alone cannot establish origin, even for annotations
            // assigned from untyped game values. Retain snapshots on doubt.
            _ => false,
        }
    }
}

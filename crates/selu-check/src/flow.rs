//! Lexical capture writes, collected before flow checking (including forward closures).
use selu_syntax::{
    visit::{self, Child},
    *,
};
use std::collections::{HashMap, HashSet};
#[derive(Default)]
struct Scan {
    scopes: Vec<HashMap<String, (usize, usize)>>,
    depth: usize,
    writes: HashSet<usize>,
}
pub(crate) fn captured_writes(p: &Program) -> HashSet<usize> {
    let mut s = Scan::default();
    s.block(&p.stmts, &[]);
    s.writes
}
impl Scan {
    fn bind(&mut self, n: &str, start: usize) {
        self.scopes
            .last_mut()
            .unwrap()
            .insert(n.into(), (start, self.depth));
    }
    fn decl(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Export(s) => self.decl(s),
            StmtKind::VarDecl {
                name, name_span, ..
            }
            | StmtKind::Global {
                name, name_span, ..
            }
            | StmtKind::Saved {
                name, name_span, ..
            }
            | StmtKind::FuncDecl {
                name, name_span, ..
            } => self.bind(name, name_span.start),
            StmtKind::Destructure { pattern, .. } => match pattern {
                Pattern::Array(xs) => {
                    for (n, s) in xs.iter().flatten() {
                        self.bind(n, s.start);
                    }
                }
                Pattern::Object(xs) => {
                    for (_, n, s, _) in xs {
                        self.bind(n, s.start);
                    }
                }
            },
            _ => {}
        }
    }
    fn block(&mut self, ss: &[Stmt], ps: &[Param]) {
        self.scopes.push(HashMap::new());
        for p in ps {
            self.bind(&p.name, p.span.start);
        }
        for p in ps {
            if let Some(e) = &p.default {
                self.expr(e);
            }
        }
        for s in ss {
            self.decl(s);
        }
        for s in ss {
            self.stmt(s);
        }
        self.scopes.pop();
    }
    fn function(&mut self, ps: &[Param], body: &[Stmt]) {
        self.depth += 1;
        self.block(body, ps);
        self.depth -= 1;
    }
    fn child(&mut self, c: Child<'_>) {
        match c {
            Child::Stmt(s) => self.stmt(s),
            Child::Expr(e) => self.expr(e),
        }
    }
    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::FuncDecl { params, body, .. } | StmtKind::OnHandler { params, body, .. } => {
                self.function(params, body)
            }
            StmtKind::Migrate {
                param,
                context,
                body,
                ..
            } => {
                let mut ps = vec![param.clone()];
                ps.extend(context.iter().map(|p| p.as_ref().clone()));
                self.function(&ps, body);
            }
            StmtKind::Block(ss) => self.block(ss, &[]),
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.expr(cond);
                self.block(then_branch, &[]);
                if let Some(b) = else_branch {
                    self.block(b, &[]);
                }
            }
            StmtKind::While { cond, body } | StmtKind::DoWhile { cond, body } => {
                self.expr(cond);
                self.block(body, &[]);
            }
            StmtKind::For {
                init,
                cond,
                update,
                body,
            } => {
                self.scopes.push(HashMap::new());
                if let Some(s) = init {
                    self.decl(s);
                    self.stmt(s);
                }
                if let Some(e) = cond {
                    self.expr(e);
                }
                if let Some(e) = update {
                    self.expr(e);
                }
                self.block(body, &[]);
                self.scopes.pop();
            }
            StmtKind::ForOf {
                binding,
                iter,
                body,
                ..
            } => {
                self.expr(iter);
                self.scopes.push(HashMap::new());
                match binding {
                    ForBinding::Name(n, s) => self.bind(n, s.start),
                    ForBinding::Pattern(p) => match p {
                        Pattern::Array(xs) => {
                            for (n, s) in xs.iter().flatten() {
                                self.bind(n, s.start);
                            }
                        }
                        Pattern::Object(xs) => {
                            for (_, n, s, _) in xs {
                                self.bind(n, s.start);
                            }
                        }
                    },
                }
                self.block(body, &[]);
                self.scopes.pop();
            }
            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                self.block(body, &[]);
                if let Some((p, b)) = catch {
                    self.block(b, &p.iter().cloned().collect::<Vec<_>>());
                }
                if let Some(b) = finally {
                    self.block(b, &[]);
                }
            }
            StmtKind::Class(c) => {
                for m in &c.members {
                    match &m.kind {
                        ClassMemberKind::Field { init: Some(e), .. } => {
                            if !m.is_static {
                                self.depth += 1;
                            }
                            self.expr(e);
                            if !m.is_static {
                                self.depth -= 1;
                            }
                        }
                        ClassMemberKind::Method { params, body, .. }
                        | ClassMemberKind::Constructor { params, body, .. } => {
                            self.function(params, body)
                        }
                        _ => {}
                    }
                }
            }
            _ => visit::child_stmts_and_exprs(s, &mut |c| self.child(c)),
        }
    }
    fn expr(&mut self, e: &Expr) {
        if let ExprKind::Function { params, body, .. } = &e.kind {
            self.depth += 1;
            self.scopes.push(HashMap::new());
            for p in params {
                self.bind(&p.name, p.span.start);
            }
            for p in params {
                if let Some(e) = &p.default {
                    self.expr(e);
                }
            }
            match body {
                FnBody::Block(ss) => self.block(ss, &[]),
                FnBody::Expr(e) => self.expr(e),
            }
            self.scopes.pop();
            self.depth -= 1;
            return;
        }
        let target = match &e.kind {
            ExprKind::Assign { target, .. }
            | ExprKind::CompoundAssign { target, .. }
            | ExprKind::Update { target, .. } => Some(target),
            _ => None,
        };
        if let Some(target) = target
            && let ExprKind::Ident(n) = &target.kind
            && let Some((start, depth)) = self.scopes.iter().rev().find_map(|s| s.get(n))
            && *depth < self.depth
        {
            self.writes.insert(*start);
        }
        visit::child_exprs_and_stmts(e, &mut |c| self.child(c));
    }
}

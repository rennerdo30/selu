//! Control-flow checking, including lexical loop boundaries.
use crate::{Ctx, Scope, Type, err};
use selu_syntax::{Expr, ExprKind, Stmt, StmtKind, visit};

pub(crate) struct LoopCtx;

impl Ctx<'_> {
    pub(crate) fn loop_stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Break | StmtKind::Continue => {
                if self.loops.is_empty() {
                    self.out.push(
                        err(
                            "SELU0340",
                            "this statement needs an enclosing loop",
                            &s.span,
                        )
                        .with_help("move it inside a loop in this function"),
                    );
                }
            }
            StmtKind::While { cond, body } => {
                let (yes, _) = self.condition(cond);
                self.loops.push(LoopCtx);
                self.block(body, &[], yes);
                self.loops.pop();
            }
            StmtKind::DoWhile { body, cond } => {
                self.loops.push(LoopCtx);
                self.block(body, &[], Vec::new());
                self.loops.pop();
                self.condition(cond);
            }
            StmtKind::For {
                init,
                cond,
                update,
                body,
            } => {
                self.scopes.push(Scope::default());
                if let Some(init) = init {
                    self.block_for_init(init);
                }
                let yes = cond
                    .as_ref()
                    .map(|c| self.condition(c).0)
                    .unwrap_or_default();
                self.loops.push(LoopCtx);
                self.block(body, &[], yes.clone());
                if let Some(u) = update {
                    // A body's writes, calls or transfers may invalidate facts
                    // on paths reaching the update. Do not guess across them.
                    let yes = yes
                        .into_iter()
                        .filter(|(name, _)| !body.iter().any(|s| invalidates_stmt(s, name)))
                        .collect();
                    self.scopes.push(Scope::default());
                    self.narrow_here(yes);
                    self.expr(u);
                    self.scopes.pop();
                }
                self.loops.pop();
                if let Some(init) = init
                    && let StmtKind::VarDecl { name, .. } = &init.kind
                {
                    for stmt in body {
                        reject_capture(stmt, name, self.out);
                    }
                }
                self.scopes.pop();
            }
            _ => unreachable!(),
        }
    }

    fn block_for_init(&mut self, s: &Stmt) {
        if let StmtKind::VarDecl {
            name,
            name_span,
            is_const,
            ty,
            ..
        } = &s.kind
        {
            let t = ty
                .as_ref()
                .map(crate::types::resolve_quiet)
                .unwrap_or(Type::Untyped);
            self.declare(
                name,
                name_span,
                if *is_const {
                    crate::Kind::Const
                } else {
                    crate::Kind::Let
                },
                t,
            );
        }
        self.stmt(s);
    }

    pub(crate) fn control_expr(&mut self, e: &Expr) -> Type {
        match &e.kind {
            ExprKind::Conditional {
                cond,
                then,
                otherwise,
            } => {
                let (yes, no) = self.condition(cond);
                self.scopes.push(Scope::default());
                self.narrow_here(yes);
                let a = self.value(then);
                self.scopes.pop();
                self.scopes.push(Scope::default());
                self.narrow_here(no);
                let b = self.value(otherwise);
                self.scopes.pop();
                Type::join(&a, &b).unwrap_or_else(|| {
                    self.out.push(
                        err(
                            "SELU0342",
                            "both conditional branches must have a common type",
                            &e.span,
                        )
                        .with_help(
                            "return the same type on both sides, or use null for an optional value",
                        ),
                    );
                    Type::Error
                })
            }
            ExprKind::CompoundAssign { op, target, value } => {
                let result = self.binary(e, op, target, value);
                self.control_write(target, &result);
                Type::Void
            }
            ExprKind::Update { target, .. } => {
                let t = self.value(target);
                if crate::shape_of(&t) != Type::Number && !t.is_open() {
                    self.out.push(
                        err(
                            "SELU0341",
                            "increment and decrement need a number",
                            &target.span,
                        )
                        .with_help("use a number variable"),
                    );
                }
                self.control_write(target, &Type::Number);
                Type::Void
            }
            _ => unreachable!(),
        }
    }

    fn control_write(&mut self, target: &Expr, t: &Type) {
        match &target.kind {
            ExprKind::Ident(n) => self.write(n, &target.span, t),
            ExprKind::Member { .. } | ExprKind::Index { .. } => {}
            _ => self.out.push(
                err(
                    "SELU0344",
                    "this expression is not an assignment target",
                    &target.span,
                )
                .with_help("assign to a variable, field or index"),
            ),
        }
    }
}

// Until package C can cooperate in per-turn environment copying, reject
// captures rather than silently giving fallback loops Lua's shared binding.
fn reject_capture(s: &Stmt, name: &str, out: &mut Vec<selu_diagnostics::Diagnostic>) {
    if matches!(
        s.kind,
        StmtKind::FuncDecl { .. } | StmtKind::OnHandler { .. }
    ) {
        if let Some(span) = captured_stmt(s, name) {
            out.push(
                err(
                    "SELU0343",
                    "capturing a counted loop binding is not supported yet",
                    &span,
                )
                .with_help("copy the value into a body-local variable and capture that variable"),
            );
        }
        return;
    }
    visit::child_stmts_and_exprs(s, &mut |c| match c {
        visit::Child::Stmt(s) => reject_capture(s, name, out),
        visit::Child::Expr(e) => reject_expr_capture(e, name, out),
    });
}
fn reject_expr_capture(e: &Expr, name: &str, out: &mut Vec<selu_diagnostics::Diagnostic>) {
    if matches!(e.kind, ExprKind::Function { .. }) {
        if let Some(span) = captured_expr(e, name) {
            out.push(
                err(
                    "SELU0343",
                    "capturing a counted loop binding is not supported yet",
                    &span,
                )
                .with_help("copy the value into a body-local variable and capture that variable"),
            );
        }
        return;
    }
    visit::child_exprs_and_stmts(e, &mut |c| match c {
        visit::Child::Stmt(s) => reject_capture(s, name, out),
        visit::Child::Expr(e) => reject_expr_capture(e, name, out),
    });
}

fn captured_stmt(s: &Stmt, name: &str) -> Option<selu_diagnostics::Span> {
    let mut found = None;
    visit::child_stmts_and_exprs(s, &mut |c| {
        if found.is_none() {
            found = match c {
                visit::Child::Stmt(s) => captured_stmt(s, name),
                visit::Child::Expr(e) => captured_expr(e, name),
            };
        }
    });
    found
}
fn captured_expr(e: &Expr, name: &str) -> Option<selu_diagnostics::Span> {
    if matches!(&e.kind, ExprKind::Ident(n) if n == name) {
        return Some(e.span.clone());
    }
    let mut found = None;
    visit::child_exprs_and_stmts(e, &mut |c| {
        if found.is_none() {
            found = match c {
                visit::Child::Stmt(s) => captured_stmt(s, name),
                visit::Child::Expr(e) => captured_expr(e, name),
            };
        }
    });
    found
}
fn invalidates_stmt(s: &Stmt, name: &str) -> bool {
    if matches!(s.kind, StmtKind::Continue) {
        return true;
    }
    let mut invalid = false;
    visit::child_stmts_and_exprs(s, &mut |c| {
        invalid |= match c {
            visit::Child::Stmt(s) => invalidates_stmt(s, name),
            visit::Child::Expr(e) => invalidates_expr(e, name),
        }
    });
    invalid
}
fn invalidates_expr(e: &Expr, name: &str) -> bool {
    match &e.kind {
        ExprKind::Call { .. } => return true,
        ExprKind::Assign { target, .. }
        | ExprKind::CompoundAssign { target, .. }
        | ExprKind::Update { target, .. }
            if matches!(&target.kind, ExprKind::Ident(n) if n == name) =>
        {
            return true;
        }
        _ => {}
    }
    let mut invalid = false;
    visit::child_exprs_and_stmts(e, &mut |c| {
        invalid |= match c {
            visit::Child::Stmt(s) => invalidates_stmt(s, name),
            visit::Child::Expr(e) => invalidates_expr(e, name),
        }
    });
    invalid
}

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
                self.block(body, &[], yes);
                if let Some(u) = update {
                    self.expr(u);
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
                let left = self.value(target);
                let right = self.value(value);
                let valid = left.is_open()
                    || right.is_open()
                    || (left == right
                        && (left == Type::Number || (op == "+" && left == Type::String)));
                if !valid {
                    self.out.push(
                        err(
                            "SELU0341",
                            "this assignment needs two numbers, or two strings for `+=`",
                            &e.span,
                        )
                        .with_help("use operands of the same supported type"),
                    );
                }
                self.control_write(target, &left);
                Type::Void
            }
            ExprKind::Update { target, .. } => {
                let t = self.value(target);
                if t != Type::Number && !t.is_open() {
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
    if let StmtKind::FuncDecl { .. } = &s.kind {
        let mut captures = false;
        visit::idents_in_stmt(s, &mut |n| captures |= n == name);
        if captures {
            out.push(
                err(
                    "SELU0343",
                    "capturing a counted loop binding is not supported yet",
                    &s.span,
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
        let mut captures = false;
        visit::idents_in_expr(e, &mut |n| captures |= n == name);
        if captures {
            out.push(
                err(
                    "SELU0343",
                    "capturing a counted loop binding is not supported yet",
                    &e.span,
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

//! Control-flow checking, including lexical loop boundaries.
use crate::{Ctx, Scope, Type, err};
use selu_syntax::{Expr, ExprKind, Stmt, StmtKind};

#[derive(Clone)]
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
                self.prepare_region(body, &[cond]);
                let entry = self.flow_state();
                let (yes, _) = self.condition(cond);
                self.restore_flow(yes);
                self.loops.push(LoopCtx);
                self.block(body, &[], vec![]);
                self.restore_flow(entry);
                self.loops.pop();
            }
            StmtKind::DoWhile { body, cond } => {
                self.prepare_region(body, &[cond]);
                let entry = self.flow_state();
                self.loops.push(LoopCtx);
                self.block(body, &[], Vec::new());
                self.loops.pop();
                self.condition(cond);
                self.restore_flow(entry);
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
                let extra: Vec<&Expr> = cond
                    .iter()
                    .chain(update.iter())
                    .map(|e| e.as_ref())
                    .collect();
                self.prepare_region(body, &extra);
                let entry = self.flow_state();
                if let Some(c) = cond {
                    let (yes, _) = self.condition(c);
                    self.restore_flow(yes);
                }
                self.loops.push(LoopCtx);
                self.block(body, &[], vec![]);
                if let Some(u) = update {
                    self.prepare_region(body, &[]);
                    self.expr(u);
                }
                self.restore_flow(entry);
                self.loops.pop();
                self.close_scope();
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
                self.restore_flow(yes);
                let a = self.value(then);
                let then_out = self.flow_state();
                self.restore_flow(no);
                let b = self.value(otherwise);
                let else_out = self.flow_state();
                self.restore_flow(crate::flow_state::intersect(vec![then_out, else_out]));
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
                let target_info = self.prepare_write(target);
                let r = self.value(value);
                let result = self.binary_values(e, op, target, value, target_info.read.clone(), r);
                self.commit_write(target, &target_info, &result);
                Type::Void
            }
            ExprKind::Update { target, .. } => {
                let target_info = self.prepare_write(target);
                let t = &target_info.read;
                if crate::shape_of(t) != Type::Number && !t.is_open() {
                    self.out.push(
                        err(
                            "SELU0341",
                            "increment and decrement need a number",
                            &target.span,
                        )
                        .with_help("use a number variable"),
                    );
                }
                self.commit_write(target, &target_info, &Type::Number);
                Type::Void
            }
            _ => unreachable!(),
        }
    }
}

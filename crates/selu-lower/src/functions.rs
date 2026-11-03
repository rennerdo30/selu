//! Lua closures, parameter prologues and destructuring (package C).
use crate::Lowerer;
use crate::lua::{BinOp, LBlock, LExpr, LField, LFunction, LStmt, LStmtKind, bin, name};
use selu_syntax::{Expr, ExprKind, FnBody, Param, Pattern, Stmt, StmtKind};

impl Lowerer<'_> {
    pub(crate) fn function_with_extras(&mut self, params: &[Param], body: &[Stmt]) -> LFunction {
        let mut prologue = Vec::new();
        for p in params.iter().filter(|p| p.rest) {
            prologue.push(LStmt::new(
                LStmtKind::Local(vec![p.name.clone()], Vec::new()),
                &p.span,
            ));
        }
        for p in params {
            if p.rest {
                prologue.push(LStmt::new(
                    LStmtKind::Assign(
                        vec![name(&p.name)],
                        vec![LExpr::Table(vec![LField::Pos(LExpr::Vararg)])],
                    ),
                    &p.span,
                ));
            }
            if let Some(d) = &p.default {
                let mut b = Vec::new();
                let v = self.expr(d, &mut b);
                b.push(LStmt::new(
                    LStmtKind::Assign(vec![name(&p.name)], vec![v]),
                    &p.span,
                ));
                prologue.push(LStmt::new(
                    LStmtKind::If(vec![(bin(BinOp::Eq, name(&p.name), LExpr::Nil), b)], None),
                    &p.span,
                ));
            }
        }
        prologue.extend(self.block(body));
        LFunction {
            params: params
                .iter()
                .filter(|p| !p.rest)
                .map(|p| p.name.clone())
                .collect(),
            vararg: params.iter().any(|p| p.rest),
            body: prologue,
        }
    }
    pub(crate) fn function_expr(&mut self, e: &Expr, _pre: &mut LBlock) -> LExpr {
        let ExprKind::Function { params, body, .. } = &e.kind else {
            unreachable!()
        };
        let body = match body {
            FnBody::Block(b) => b.clone(),
            FnBody::Expr(v) => vec![Stmt {
                kind: StmtKind::Return(Some(v.clone())),
                span: v.span.clone(),
            }],
        };
        LExpr::Function(self.function(params, &body))
    }
    pub(crate) fn function_return(&mut self, e: &Expr, pre: &mut LBlock) -> Vec<LExpr> {
        if let ExprKind::Array(items) = &e.kind
            && matches!(self.types.get(e.id), Some(selu_check::Type::Tuple(_)))
        {
            return self
                .seq(&items.iter().collect::<Vec<_>>(), pre)
                .into_iter()
                .map(|v| {
                    if matches!(v, LExpr::Call(..) | LExpr::Method(..)) {
                        LExpr::Paren(Box::new(v))
                    } else {
                        v
                    }
                })
                .collect();
        }
        if !matches!(e.kind, ExprKind::Call { .. })
            && let Some(selu_check::Type::Tuple(items)) = self.types.get(e.id)
        {
            let count = items.len();
            let value = self.expr(e, pre);
            let temp = self.hold(value, &e.span, pre);
            return (1..=count)
                .map(|i| {
                    LExpr::Index(
                        Box::new(name(&temp)),
                        Box::new(LExpr::Number(i.to_string())),
                    )
                })
                .collect();
        }
        vec![self.expr(e, pre)]
    }
    pub(crate) fn destructure(&mut self, s: &Stmt, out: &mut LBlock) {
        let StmtKind::Destructure { pattern, init, .. } = &s.kind else {
            unreachable!()
        };
        let v = self.expr(init, out);
        if let Pattern::Array(items) = pattern
            && matches!(init.kind, ExprKind::Call { .. })
            && matches!(
                self.types.get(init.id),
                Some(selu_check::Type::Tuple(_) | selu_check::Type::Untyped)
            )
        {
            let names = items
                .iter()
                .map(|x| {
                    x.as_ref()
                        .map(|(n, _)| n.clone())
                        .unwrap_or_else(|| self.temp())
                })
                .collect();
            out.push(LStmt::new(LStmtKind::Local(names, vec![v]), &s.span));
            return;
        }
        let t = self.temp();
        out.push(LStmt::new(
            LStmtKind::Local(vec![t.clone()], vec![v]),
            &s.span,
        ));
        match pattern {
            Pattern::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    if let Some((n, _)) = item {
                        out.push(LStmt::new(
                            LStmtKind::Local(
                                vec![n.clone()],
                                vec![LExpr::Index(
                                    Box::new(name(&t)),
                                    Box::new(LExpr::Number((i + 1).to_string())),
                                )],
                            ),
                            &s.span,
                        ));
                    }
                }
            }
            Pattern::Object(items) => {
                for (field, n, _) in items {
                    out.push(LStmt::new(
                        LStmtKind::Local(
                            vec![n.clone()],
                            vec![LExpr::Field(Box::new(name(&t)), field.clone())],
                        ),
                        &s.span,
                    ));
                }
            }
        }
    }
}

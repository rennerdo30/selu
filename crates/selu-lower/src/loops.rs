//! Lua 5.1 control flow. No goto, iterator closures or runtime helpers.
use crate::Lowerer;
use crate::lua::{BinOp, LBlock, LExpr, LStmt, LStmtKind, bin, name, not};
use selu_diagnostics::Span;
use selu_syntax::{Expr, ExprKind, Stmt, StmtKind, visit};

pub(crate) struct LoopCtx {
    pub(crate) wrapped: bool,
    pub(crate) break_flag: Option<String>,
}
fn stmt(kind: LStmtKind, span: &Span) -> LStmt {
    LStmt::new(kind, span)
}

impl Lowerer<'_> {
    /// Package B calls this for its for-of body. The returned statements
    /// belong directly inside the actual Lua loop (including flag handling).
    pub(crate) fn loop_body(&mut self, body: &[Stmt], span: &Span) -> LBlock {
        let (mut continues, mut breaks) = (false, false);
        for s in body {
            transfers(s, &mut continues, &mut breaks);
        }
        let flag = (continues && breaks).then(|| self.temp());
        self.loops.push(LoopCtx {
            wrapped: continues,
            break_flag: flag.clone(),
        });
        let b = self.block(body);
        self.loops.pop();
        if !continues {
            return b;
        }
        let mut out = Vec::new();
        if let Some(f) = &flag {
            out.push(stmt(
                LStmtKind::Local(vec![f.clone()], vec![LExpr::Bool(false)]),
                span,
            ));
        }
        out.push(stmt(LStmtKind::Repeat(b, LExpr::Bool(true)), span));
        if let Some(f) = flag {
            out.push(stmt(
                LStmtKind::If(vec![(name(&f), vec![stmt(LStmtKind::Break, span)])], None),
                span,
            ));
        }
        out
    }

    pub(crate) fn loop_stmt(&mut self, s: &Stmt, out: &mut LBlock) {
        let sp = &s.span;
        match &s.kind {
            StmtKind::Break | StmtKind::Continue => {
                let ctx = self.loops.last().expect("checked loop transfer");
                let mut b = Vec::new();
                if matches!(s.kind, StmtKind::Break)
                    && ctx.wrapped
                    && let Some(f) = &ctx.break_flag
                {
                    b.push(stmt(
                        LStmtKind::Assign(vec![name(f)], vec![LExpr::Bool(true)]),
                        sp,
                    ));
                }
                b.push(stmt(LStmtKind::Break, sp));
                // The printer wraps the `break` in `do ... end` if anything
                // follows it in the same block.
                out.extend(b);
            }
            StmtKind::While { cond, body } => {
                let mut pre = Vec::new();
                let c = self.expr(cond, &mut pre);
                let b = self.loop_body(body, sp);
                if pre.is_empty() {
                    out.push(stmt(LStmtKind::While(c, b), sp));
                } else {
                    pre.push(stmt(
                        LStmtKind::If(vec![(not(c), vec![stmt(LStmtKind::Break, sp)])], None),
                        sp,
                    ));
                    pre.extend(b);
                    out.push(stmt(LStmtKind::While(LExpr::Bool(true), pre), sp));
                }
            }
            StmtKind::DoWhile { body, cond } => {
                let b = self.loop_body(body, sp);
                // Body-local declarations cannot leak into the until clause.
                let mut b = vec![stmt(LStmtKind::Do(b), sp)];
                let c = self.expr(cond, &mut b);
                out.push(stmt(LStmtKind::Repeat(b, not(c)), sp));
            }
            StmtKind::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some((var, start, stop, step)) =
                    numeric(init.as_deref(), cond.as_deref(), update.as_deref(), body)
                {
                    let b = self.loop_body(body, sp);
                    out.push(stmt(
                        LStmtKind::NumFor {
                            var,
                            start,
                            stop,
                            step: Some(step),
                            body: b,
                        },
                        sp,
                    ));
                    return;
                }
                let mut outer = Vec::new();
                if let Some(i) = init {
                    self.stmt(i, false, &mut outer);
                }
                let mut b = Vec::new();
                if let Some(c) = cond {
                    let c = self.expr(c, &mut b);
                    b.push(stmt(
                        LStmtKind::If(vec![(not(c), vec![stmt(LStmtKind::Break, sp)])], None),
                        sp,
                    ));
                }
                let body = self.loop_body(body, sp);
                b.push(stmt(LStmtKind::Do(body), sp));
                if let Some(u) = update {
                    self.expr_stmt(u, sp, &mut b);
                }
                outer.push(stmt(LStmtKind::While(LExpr::Bool(true), b), sp));
                out.push(stmt(LStmtKind::Do(outer), sp));
            }
            _ => unreachable!(),
        }
    }

    pub(crate) fn control_stmt(&mut self, e: &Expr, span: &Span, out: &mut LBlock) {
        let (target, op, value) = match &e.kind {
            ExprKind::CompoundAssign { target, op, value } => {
                (target.as_ref(), op.as_str(), Some(value.as_ref()))
            }
            ExprKind::Update { target, op } => {
                (target.as_ref(), if op == "++" { "+" } else { "-" }, None)
            }
            _ => unreachable!(),
        };
        if let ExprKind::Member { object, .. } = &target.kind
            && (matches!(object.kind, ExprKind::This) || self.types.is_stable(object.id))
            && value.is_none_or(|e| !self.may_run_code(e))
        {
            let mut pre = Vec::new();
            let t = self.target(target, &mut pre);
            let mut one = target.clone();
            one.kind = ExprKind::Number("1".into());
            let result = self.binary(e, op, target, value.unwrap_or(&one), &mut pre);
            self.finish(
                pre,
                stmt(LStmtKind::Assign(vec![t], vec![result]), span),
                out,
            );
            return;
        }
        let mut pre = Vec::new();
        let t = match &target.kind {
            ExprKind::Member {
                object,
                name: field,
            } => {
                let obj = self.expr(object, &mut pre);
                let obj = self.hold(obj, span, &mut pre);
                LExpr::Field(Box::new(name(&obj)), field.clone())
            }
            ExprKind::Index { object, index } => {
                let obj = self.expr(object, &mut pre);
                let obj = self.hold(obj, span, &mut pre);
                let ix = self.expr(index, &mut pre);
                let ix = if matches!(
                    self.types.get(object.id),
                    Some(selu_check::Type::Array(_) | selu_check::Type::Tuple(_))
                ) {
                    bin(BinOp::Add, ix, LExpr::Number("1".into()))
                } else {
                    ix
                };
                let ix = self.hold(ix, span, &mut pre);
                LExpr::Index(Box::new(name(&obj)), Box::new(name(&ix)))
            }
            _ => self.target(target, &mut pre),
        };
        // Plain locals and pure right operands need no snapshot. Otherwise
        // read the old value before the RHS can call code or run a prelude.
        let old = if matches!(t, LExpr::Name(_)) && value.is_none_or(|e| !self.may_run_code(e)) {
            t.clone()
        } else {
            let old = self.hold(t.clone(), span, &mut pre);
            name(&old)
        };
        // Reuse binary lowering's operator selection using already evaluated
        // operands. Keep their original ids so both operand types are used.
        let mut left = target.clone();
        left.kind = match old {
            LExpr::Name(n) => ExprKind::Ident(n),
            _ => unreachable!(),
        };
        let mut right = value.unwrap_or(target).clone();
        match value {
            None => right.kind = ExprKind::Number("1".into()),
            Some(v) if !self.may_run_code(v) => {}
            Some(v) => {
                let v = self.expr(v, &mut pre);
                right.kind = ExprKind::Ident(self.hold(v, span, &mut pre));
            }
        }
        let result = self.binary(e, op, &left, &right, &mut pre);
        self.finish(
            pre,
            stmt(LStmtKind::Assign(vec![t], vec![result]), span),
            out,
        );
    }

    pub(crate) fn control_expr(&mut self, e: &Expr, pre: &mut LBlock) -> LExpr {
        let ExprKind::Conditional {
            cond,
            then,
            otherwise,
        } = &e.kind
        else {
            unreachable!()
        };
        let c = self.expr(cond, pre);
        let mut a_pre = Vec::new();
        let a = self.expr(then, &mut a_pre);
        let mut b_pre = Vec::new();
        let b = self.expr(otherwise, &mut b_pre);
        let safe = matches!(
            self.types.get(then.id),
            Some(
                selu_check::Type::Number
                    | selu_check::Type::String
                    | selu_check::Type::Array(_)
                    | selu_check::Type::Object(_)
                    | selu_check::Type::Function { .. }
            )
        );
        if safe && a_pre.is_empty() && b_pre.is_empty() {
            return bin(BinOp::Or, bin(BinOp::And, c, a), b);
        }
        let t = self.temp();
        pre.push(stmt(LStmtKind::Local(vec![t.clone()], vec![]), &e.span));
        a_pre.push(stmt(LStmtKind::Assign(vec![name(&t)], vec![a]), &then.span));
        b_pre.push(stmt(
            LStmtKind::Assign(vec![name(&t)], vec![b]),
            &otherwise.span,
        ));
        pre.push(stmt(LStmtKind::If(vec![(c, a_pre)], Some(b_pre)), &e.span));
        name(&t)
    }
}

fn transfers(s: &Stmt, continues: &mut bool, breaks: &mut bool) {
    match &s.kind {
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            for s in body
                .iter()
                .chain(catch.iter().flat_map(|(_, b)| b))
                .chain(finally.iter().flatten())
            {
                transfers(s, continues, breaks);
            }
        }
        StmtKind::Continue => *continues = true,
        StmtKind::Break => *breaks = true,
        StmtKind::Block(b) => {
            for s in b {
                transfers(s, continues, breaks);
            }
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            for s in then_branch {
                transfers(s, continues, breaks);
            }
            if let Some(b) = else_branch {
                for s in b {
                    transfers(s, continues, breaks);
                }
            }
        }
        _ => {}
    }
}

// Restrict numeric lowering to small integer literals. Lua's inclusive
// limit adjustment is not equivalent for fractional bounds or large doubles.
fn integer(e: &Expr) -> Option<i64> {
    let n = match &e.kind {
        ExprKind::Number(s) => s.parse::<i64>().ok()?,
        ExprKind::Unary { op, expr } if op == "-" => -integer(expr)?,
        _ => return None,
    };
    (n.abs() <= 1_000_000_000).then_some(n)
}
fn numeric(
    init: Option<&Stmt>,
    cond: Option<&Expr>,
    update: Option<&Expr>,
    body: &[Stmt],
) -> Option<(String, LExpr, LExpr, LExpr)> {
    let StmtKind::VarDecl {
        name: var,
        is_const: false,
        init: Some(start),
        ..
    } = &init?.kind
    else {
        return None;
    };
    let start = integer(start)?;
    let ExprKind::Binary { op, left, right } = &cond?.kind else {
        return None;
    };
    if !matches!(&left.kind, ExprKind::Ident(n) if n == var) {
        return None;
    }
    let bound = integer(right)?;
    let step = match &update?.kind {
        ExprKind::Update { op, target } if matches!(&target.kind, ExprKind::Ident(n) if n == var) => {
            if op == "++" {
                1
            } else {
                -1
            }
        }
        ExprKind::CompoundAssign { op, target, value } if matches!(&target.kind, ExprKind::Ident(n) if n == var) => {
            match op.as_str() {
                "+" => integer(value)?,
                "-" => -integer(value)?,
                _ => return None,
            }
        }
        _ => return None,
    };
    let stop = match op.as_str() {
        "<" if step > 0 => bound - 1,
        "<=" if step > 0 => bound,
        ">" if step < 0 => bound + 1,
        ">=" if step < 0 => bound,
        _ => return None,
    };
    let mut writes = false;
    for s in body {
        writes_stmt(s, var, &mut writes);
    }
    if writes {
        return None;
    }
    Some((
        var.clone(),
        LExpr::Number(start.to_string()),
        LExpr::Number(stop.to_string()),
        LExpr::Number(step.to_string()),
    ))
}
fn writes_stmt(s: &Stmt, var: &str, writes: &mut bool) {
    visit::child_stmts_and_exprs(s, &mut |c| match c {
        visit::Child::Stmt(s) => writes_stmt(s, var, writes),
        visit::Child::Expr(e) => writes_expr(e, var, writes),
    });
}
fn writes_expr(e: &Expr, var: &str, writes: &mut bool) {
    match &e.kind {
        ExprKind::Assign { target, .. }
        | ExprKind::CompoundAssign { target, .. }
        | ExprKind::Update { target, .. } => {
            if matches!(&target.kind, ExprKind::Ident(n) if n == var) {
                *writes = true;
            }
        }
        _ => {}
    }
    visit::child_exprs_and_stmts(e, &mut |c| match c {
        visit::Child::Stmt(s) => writes_stmt(s, var, writes),
        visit::Child::Expr(e) => writes_expr(e, var, writes),
    });
}

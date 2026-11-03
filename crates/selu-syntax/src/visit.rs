//! Small read-only walks over the AST that several passes need.

use crate::ast::*;

/// Calls `f` with every identifier read or written anywhere inside `s`,
/// including inside nested function bodies. Declared names themselves are
/// not reported, only uses.
pub fn idents_in_stmt(s: &Stmt, f: &mut impl FnMut(&str)) {
    match &s.kind {
        StmtKind::VarDecl { init, .. } => {
            if let Some(e) = init {
                idents_in_expr(e, f);
            }
        }
        StmtKind::Global { init, .. } => idents_in_expr(init, f),
        StmtKind::FuncDecl { body, .. } | StmtKind::OnHandler { body, .. } => {
            for s in body {
                idents_in_stmt(s, f);
            }
        }
        StmtKind::Return(e) => {
            if let Some(e) = e {
                idents_in_expr(e, f);
            }
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            idents_in_expr(cond, f);
            for s in then_branch.iter().chain(else_branch.iter().flatten()) {
                idents_in_stmt(s, f);
            }
        }
        StmtKind::Block(b) => {
            for s in b {
                idents_in_stmt(s, f);
            }
        }
        StmtKind::Expr(e) => idents_in_expr(e, f),
    }
}

pub fn idents_in_expr(e: &Expr, f: &mut impl FnMut(&str)) {
    match &e.kind {
        ExprKind::Ident(n) => f(n),
        ExprKind::Binary { left, right, .. } => {
            idents_in_expr(left, f);
            idents_in_expr(right, f);
        }
        ExprKind::Unary { expr, .. } => idents_in_expr(expr, f),
        ExprKind::Assign { target, value } => {
            idents_in_expr(target, f);
            idents_in_expr(value, f);
        }
        ExprKind::Member { object, .. } => idents_in_expr(object, f),
        ExprKind::Call { callee, args } => {
            idents_in_expr(callee, f);
            for a in args {
                idents_in_expr(a, f);
            }
        }
        ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::Null => {}
    }
}

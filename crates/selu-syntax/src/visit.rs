//! Small read-only walks over the AST that several passes need.

use crate::ast::*;

/// Calls `f` with every identifier read or written anywhere inside `s`,
/// including inside nested function bodies. Declared names themselves are
/// not reported, only uses.
pub fn idents_in_stmt(s: &Stmt, f: &mut impl FnMut(&str)) {
    child_stmts_and_exprs(s, &mut |c| match c {
        Child::Stmt(s) => idents_in_stmt(s, f),
        Child::Expr(e) => idents_in_expr(e, f),
    });
}

pub fn idents_in_expr(e: &Expr, f: &mut impl FnMut(&str)) {
    if let ExprKind::Ident(n) = &e.kind {
        f(n);
    }
    child_exprs_and_stmts(e, &mut |c| match c {
        Child::Stmt(s) => idents_in_stmt(s, f),
        Child::Expr(e) => idents_in_expr(e, f),
    });
}

/// A direct child of a statement or expression.
pub enum Child<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

fn params<'a>(ps: &'a [Param], f: &mut impl FnMut(Child<'a>)) {
    for p in ps {
        if let Some(d) = &p.default {
            f(Child::Expr(d));
        }
    }
}

fn stmts<'a>(ss: &'a [Stmt], f: &mut impl FnMut(Child<'a>)) {
    for s in ss {
        f(Child::Stmt(s));
    }
}

/// Calls `f` with each direct child of `s`, in source order.
pub fn child_stmts_and_exprs<'a>(s: &'a Stmt, f: &mut impl FnMut(Child<'a>)) {
    match &s.kind {
        StmtKind::VarDecl { init, .. } => {
            if let Some(e) = init {
                f(Child::Expr(e));
            }
        }
        StmtKind::Destructure { init, .. } | StmtKind::Global { init, .. } => f(Child::Expr(init)),
        StmtKind::FuncDecl {
            params: ps, body, ..
        }
        | StmtKind::OnHandler {
            params: ps, body, ..
        } => {
            params(ps, f);
            stmts(body, f);
        }
        StmtKind::Return(e) => {
            if let Some(e) = e {
                f(Child::Expr(e));
            }
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            f(Child::Expr(cond));
            stmts(then_branch, f);
            if let Some(e) = else_branch {
                stmts(e, f);
            }
        }
        StmtKind::While { cond, body } => {
            f(Child::Expr(cond));
            stmts(body, f);
        }
        StmtKind::DoWhile { body, cond } => {
            stmts(body, f);
            f(Child::Expr(cond));
        }
        StmtKind::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(s) = init {
                f(Child::Stmt(s));
            }
            if let Some(e) = cond {
                f(Child::Expr(e));
            }
            if let Some(e) = update {
                f(Child::Expr(e));
            }
            stmts(body, f);
        }
        StmtKind::ForOf { iter, body, .. } => {
            f(Child::Expr(iter));
            stmts(body, f);
        }
        StmtKind::Block(b) => stmts(b, f),
        StmtKind::Expr(e) => f(Child::Expr(e)),
        StmtKind::Import { .. } => {}
        StmtKind::Export(inner) => f(Child::Stmt(inner)),
        StmtKind::Class(c) => {
            for m in &c.members {
                match &m.kind {
                    ClassMemberKind::Field { init, .. } => {
                        if let Some(e) = init {
                            f(Child::Expr(e));
                        }
                    }
                    ClassMemberKind::Method {
                        params: ps, body, ..
                    }
                    | ClassMemberKind::Constructor { params: ps, body } => {
                        params(ps, f);
                        stmts(body, f);
                    }
                }
            }
        }
        StmtKind::Break | StmtKind::Continue => {}
    }
}

/// Calls `f` with each direct child of `e`, in evaluation order.
pub fn child_exprs_and_stmts<'a>(e: &'a Expr, f: &mut impl FnMut(Child<'a>)) {
    let mut ex = |x: &'a Expr| f(Child::Expr(x));
    match &e.kind {
        ExprKind::Number(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::Null
        | ExprKind::Ident(_)
        | ExprKind::This
        | ExprKind::Super => {}
        ExprKind::Template { exprs, .. } => exprs.iter().for_each(ex),
        ExprKind::TaggedTemplate { tag, exprs, .. } => {
            ex(tag);
            exprs.iter().for_each(ex);
        }
        ExprKind::Array(items) => items.iter().for_each(ex),
        ExprKind::Object(fields) => fields.iter().for_each(|fl| ex(&fl.value)),
        ExprKind::Binary { left, right, .. } => {
            ex(left);
            ex(right);
        }
        ExprKind::Unary { expr, .. } => ex(expr),
        ExprKind::Conditional {
            cond,
            then,
            otherwise,
        } => {
            ex(cond);
            ex(then);
            ex(otherwise);
        }
        ExprKind::Assign { target, value } | ExprKind::CompoundAssign { target, value, .. } => {
            ex(target);
            ex(value);
        }
        ExprKind::Update { target, .. } => ex(target),
        ExprKind::Member { object, .. } | ExprKind::OptionalMember { object, .. } => ex(object),
        ExprKind::Index { object, index } => {
            ex(object);
            ex(index);
        }
        ExprKind::Call { callee, args } => {
            ex(callee);
            args.iter().for_each(ex);
        }
        ExprKind::New { args, .. } => args.iter().for_each(ex),
        ExprKind::Function {
            params: ps, body, ..
        } => {
            params(ps, f);
            match body {
                FnBody::Block(b) => stmts(b, f),
                FnBody::Expr(x) => f(Child::Expr(x)),
            }
        }
    }
}

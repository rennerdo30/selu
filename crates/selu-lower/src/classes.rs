//! Class tables, shared methods and in-place base initialization.
use crate::Lowerer;
use crate::lua::{self, LBlock, LExpr, LField, LFunction, LStmt, LStmtKind, name};
use selu_check::Type;
use selu_syntax::{ClassDecl, ClassMemberKind as M, Expr, ExprKind, Stmt};
impl Lowerer<'_> {
    pub(crate) fn class_decl(&mut self, s: &Stmt, c: &ClassDecl, forward: bool, out: &mut LBlock) {
        let saved_receiver = self.class_receiver.clone();
        let init_hygiene = c.members.iter().any(|m| match &m.kind {
            M::Constructor { params, body } => {
                params.iter().any(|p| p.name == "self") || body.iter().any(binds_self_stmt)
            }
            M::Field { init: Some(e), .. } => binds_self_expr(e),
            _ => false,
        });
        self.class_receiver = if init_hygiene { "__selu_self" } else { "self" }.into();
        let span = &s.span;
        let class = name(&c.name);
        let base_alias = format!("__selu_base_{}", c.name_span.start);
        out.push(LStmt::new(
            if forward {
                LStmtKind::Assign(vec![class.clone()], vec![LExpr::Table(vec![])])
            } else {
                LStmtKind::Local(vec![c.name.clone()], vec![LExpr::Table(vec![])])
            },
            span,
        ));
        out.push(LStmt::new(
            LStmtKind::Assign(
                vec![LExpr::Field(Box::new(class.clone()), "__index".into())],
                vec![class.clone()],
            ),
            span,
        ));
        if let Some((base, _)) = &c.extends {
            out.push(LStmt::new(
                LStmtKind::Local(vec![base_alias.clone()], vec![name(base)]),
                span,
            ));
            out.push(LStmt::new(
                LStmtKind::Call(lua::call(
                    name("setmetatable"),
                    vec![
                        class.clone(),
                        LExpr::Table(vec![LField::Named("__index".into(), name(&base_alias))]),
                    ],
                )),
                span,
            ));
        }
        let mut initializers = Vec::new();
        let mut static_initializers = Vec::new();
        for m in &c.members {
            if let M::Field {
                name: field,
                init: Some(e),
                ..
            } = &m.kind
            {
                let dest = if m.is_static {
                    &mut static_initializers
                } else {
                    &mut initializers
                };
                let value = self.expr(e, dest);
                dest.push(LStmt::new(
                    LStmtKind::Assign(
                        vec![LExpr::Field(
                            Box::new(if m.is_static {
                                class.clone()
                            } else {
                                name(&self.class_receiver)
                            }),
                            field.clone(),
                        )],
                        vec![value],
                    ),
                    &m.span,
                ));
            }
        }
        let constructor = c.members.iter().find_map(|m| {
            if let M::Constructor { params, body } = &m.kind {
                Some((params, body))
            } else {
                None
            }
        });
        let init = if let Some((params, body)) = constructor {
            let mut f = if c.extends.is_some() {
                self.function(params, &body[..1])
            } else {
                self.function(params, &[])
            };
            // A checked derived constructor begins with exactly one super call.
            if c.extends.is_some() {
                f.body.extend(initializers);
                f.body.extend(self.block(&body[1..]));
            } else {
                f.body.extend(initializers);
                f.body.extend(self.block(body));
            }
            f.params.insert(0, self.class_receiver.clone());
            f
        } else {
            let mut body = Vec::new();
            if c.extends.is_some() {
                body.push(LStmt::new(
                    LStmtKind::Call(lua::call(
                        LExpr::Field(Box::new(name(&base_alias)), "__init".into()),
                        vec![name(&self.class_receiver), LExpr::Vararg],
                    )),
                    span,
                ));
            }
            body.extend(initializers);
            LFunction {
                params: vec![self.class_receiver.clone()],
                vararg: c.extends.is_some(),
                body,
            }
        };
        out.push(LStmt::new(
            LStmtKind::Function(format!("{}.__init", c.name), init),
            span,
        ));
        let mut new_body = vec![LStmt::new(
            LStmtKind::Local(
                vec!["self".into()],
                vec![lua::call(
                    name("setmetatable"),
                    vec![LExpr::Table(vec![]), class.clone()],
                )],
            ),
            span,
        )];
        new_body.push(LStmt::new(
            LStmtKind::Call(lua::call(
                LExpr::Field(Box::new(class.clone()), "__init".into()),
                vec![name("self"), LExpr::Vararg],
            )),
            span,
        ));
        new_body.push(LStmt::new(LStmtKind::Return(vec![name("self")]), span));
        out.push(LStmt::new(
            LStmtKind::Function(
                format!("{}.new", c.name),
                LFunction {
                    params: vec![],
                    vararg: true,
                    body: new_body,
                },
            ),
            span,
        ));
        for m in &c.members {
            if let M::Method {
                name: method,
                params,
                body,
                ..
            } = &m.kind
            {
                let hygiene = !m.is_static
                    && (params.iter().any(|p| p.name == "self")
                        || body.iter().any(binds_self_stmt));
                self.class_receiver = if hygiene { "__selu_self" } else { "self" }.into();
                let mut f = self.function(params, body);
                if hygiene {
                    f.params.insert(0, self.class_receiver.clone());
                }
                out.push(LStmt::new(
                    LStmtKind::Function(
                        format!(
                            "{}{}{}",
                            c.name,
                            if m.is_static || hygiene { "." } else { ":" },
                            method
                        ),
                        f,
                    ),
                    &m.span,
                ));
            }
        }
        out.extend(static_initializers);
        self.class_receiver = saved_receiver;
    }
    pub(crate) fn class_expr(&mut self, e: &Expr, _pre: &mut LBlock) -> LExpr {
        if matches!(e.kind, ExprKind::This) {
            name(&self.class_receiver)
        } else {
            self.super_name(e)
        }
    }
    fn super_name(&self, e: &Expr) -> LExpr {
        match self.types.get(e.id) {
            Some(Type::Function { ret, .. }) => {
                if let Type::Class(n, _) = ret.as_ref() {
                    name(&format!("__selu_base_{}", n.rsplit('@').next().unwrap()))
                } else {
                    unreachable!()
                }
            }
            _ => unreachable!(),
        }
    }
    pub(crate) fn class_new(&mut self, class: &str, args: &[Expr], pre: &mut LBlock) -> LExpr {
        let vals = self.seq(&args.iter().collect::<Vec<_>>(), pre);
        lua::call(LExpr::Field(Box::new(name(class)), "new".into()), vals)
    }
    pub(crate) fn class_super_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        pre: &mut LBlock,
    ) -> Option<LExpr> {
        let (base, method) = match &callee.kind {
            ExprKind::Super => (self.super_name(callee), "__init".to_string()),
            ExprKind::Member { object, name } if matches!(object.kind, ExprKind::Super) => {
                (self.super_name(object), name.clone())
            }
            _ => return None,
        };
        let mut vals = vec![name(&self.class_receiver)];
        vals.extend(self.seq(&args.iter().collect::<Vec<_>>(), pre));
        Some(lua::call(LExpr::Field(Box::new(base), method), vals))
    }
}

/// New expressions store their class name outside the identifier tree.
pub(crate) fn class_uses(s: &Stmt, f: &mut impl FnMut(&str)) {
    use selu_syntax::visit::{self, Child};
    fn expr(e: &Expr, f: &mut impl FnMut(&str)) {
        if let ExprKind::New { class, .. } = &e.kind {
            f(class);
        }
        visit::child_exprs_and_stmts(e, &mut |c| match c {
            Child::Expr(e) => expr(e, f),
            Child::Stmt(s) => class_uses(s, f),
        });
    }
    if let selu_syntax::StmtKind::Class(c) = &s.kind
        && let Some((base, _)) = &c.extends
    {
        f(base);
    }
    visit::child_stmts_and_exprs(s, &mut |c| match c {
        Child::Expr(e) => expr(e, f),
        Child::Stmt(s) => class_uses(s, f),
    });
}

fn binds_self_stmt(s: &Stmt) -> bool {
    use selu_syntax::{ForBinding, Pattern, StmtKind as S};
    fn pat(p: &Pattern) -> bool {
        match p {
            Pattern::Array(xs) => xs.iter().flatten().any(|(n, _)| n == "self"),
            Pattern::Object(xs) => xs.iter().any(|(_, n, _, _)| n == "self"),
        }
    }
    let own = match &s.kind {
        S::VarDecl { name, .. } | S::FuncDecl { name, .. } => name == "self",
        S::Destructure { pattern, .. } => pat(pattern),
        S::ForOf { binding, .. } => match binding {
            ForBinding::Name(n, _) => n == "self",
            ForBinding::Pattern(p) => pat(p),
        },
        _ => false,
    } || match &s.kind {
        S::FuncDecl { params, .. } | S::OnHandler { params, .. } => {
            params.iter().any(|p| p.name == "self")
        }
        _ => false,
    };
    let mut found = own;
    selu_syntax::visit::child_stmts_and_exprs(s, &mut |c| {
        found |= match c {
            selu_syntax::visit::Child::Stmt(s) => binds_self_stmt(s),
            selu_syntax::visit::Child::Expr(e) => binds_self_expr(e),
        };
    });
    found
}
fn binds_self_expr(e: &Expr) -> bool {
    let mut found =
        matches!(&e.kind,ExprKind::Function {params,..} if params.iter().any(|p|p.name=="self"));
    selu_syntax::visit::child_exprs_and_stmts(e, &mut |c| {
        found |= match c {
            selu_syntax::visit::Child::Stmt(s) => binds_self_stmt(s),
            selu_syntax::visit::Child::Expr(e) => binds_self_expr(e),
        };
    });
    found
}

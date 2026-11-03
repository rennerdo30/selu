//! Class tables, shared methods and in-place base initialization.
use crate::Lowerer;
use crate::lua::{self, LBlock, LExpr, LField, LFunction, LStmt, LStmtKind, name};
use selu_check::Type;
use selu_syntax::{ClassDecl, ClassMemberKind as M, Expr, ExprKind, Stmt};
impl Lowerer<'_> {
    pub(crate) fn class_decl(&mut self, s: &Stmt, c: &ClassDecl, forward: bool, out: &mut LBlock) {
        let saved_receiver = self.class_receiver.clone();
        let span = &s.span;
        let class = name(&c.name);
        let class_alias = format!("__selu_class_{}", c.name_span.start);
        let base_alias = format!("__selu_base_{}", c.name_span.start);
        let proto = LExpr::Field(Box::new(name(&class_alias)), "__proto".into());
        out.push(LStmt::new(
            if forward {
                LStmtKind::Assign(vec![class.clone()], vec![LExpr::Table(vec![])])
            } else {
                LStmtKind::Local(vec![c.name.clone()], vec![LExpr::Table(vec![])])
            },
            span,
        ));
        out.push(LStmt::new(
            LStmtKind::Local(vec![class_alias.clone()], vec![class.clone()]),
            span,
        ));
        out.push(LStmt::new(
            LStmtKind::Assign(vec![proto.clone()], vec![LExpr::Table(vec![])]),
            span,
        ));
        out.push(LStmt::new(
            LStmtKind::Assign(
                vec![LExpr::Field(Box::new(proto.clone()), "__index".into())],
                vec![proto.clone()],
            ),
            span,
        ));
        if let Some((base, _)) = &c.extends {
            out.push(LStmt::new(
                LStmtKind::Local(vec![base_alias.clone()], vec![name(base)]),
                span,
            ));
            for (target, base) in [
                (class.clone(), name(&base_alias)),
                (
                    proto.clone(),
                    LExpr::Field(Box::new(name(&base_alias)), "__proto".into()),
                ),
            ] {
                out.push(LStmt::new(
                    LStmtKind::Call(lua::call(
                        name("__selu_setmetatable"),
                        vec![
                            target,
                            LExpr::Table(vec![LField::Named("__index".into(), base)]),
                        ],
                    )),
                    span,
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
        let mut initializers = Vec::new();
        let mut static_initializers = Vec::new();
        let field_hygiene = c
            .members
            .iter()
            .any(|m| matches!(&m.kind,M::Field{init:Some(e),..} if binds_self_expr(e)));
        self.class_receiver = if field_hygiene { "__selu_self" } else { "self" }.into();
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
                                name(&class_alias)
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
        out.push(LStmt::new(
            LStmtKind::Function(
                format!("{}.__fields", c.name),
                LFunction {
                    params: vec![self.class_receiver.clone()],
                    vararg: false,
                    body: initializers,
                },
            ),
            span,
        ));
        let ctor_hygiene = constructor.is_some_and(|(params, body)| {
            params
                .iter()
                .any(|p| p.name == "self" || p.default.as_ref().is_some_and(|e| binds_self_expr(e)))
                || body.iter().any(binds_self_stmt)
        });
        self.class_receiver = if ctor_hygiene { "__selu_self" } else { "self" }.into();
        let field_call = LStmt::new(
            LStmtKind::Call(lua::call(
                LExpr::Field(Box::new(name(&class_alias)), "__fields".into()),
                vec![name(&self.class_receiver)],
            )),
            span,
        );
        let init = if let Some((params, body)) = constructor {
            // Lower the complete body once, preserving its forward locals and defaults.
            let mut f = self.function(params, body);
            if c.extends.is_some() {
                let first = &body[0].span;
                let index = f
                    .body
                    .iter()
                    .rposition(|s| s.span.as_ref() == Some(first))
                    .expect("checked super statement")
                    + 1;
                f.body.insert(index, field_call);
            } else {
                f.body.insert(0, field_call);
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
            body.push(field_call);
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
        let new_params: Vec<String> = constructor
            .map(|(ps, _)| {
                ps.iter()
                    .filter(|p| !p.rest)
                    .map(|p| p.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let vararg = constructor.map_or(c.extends.is_some(), |(ps, _)| ps.iter().any(|p| p.rest));
        let instance = if new_params.iter().any(|n| n == "self") {
            "__selu_instance"
        } else {
            "self"
        };
        let mut args = vec![name(instance)];
        args.extend(new_params.iter().map(|n| name(n)));
        if vararg {
            args.push(LExpr::Vararg);
        }
        let new_body = vec![
            LStmt::new(
                LStmtKind::Local(
                    vec![instance.into()],
                    vec![lua::call(
                        name("__selu_setmetatable"),
                        vec![LExpr::Table(vec![]), proto],
                    )],
                ),
                span,
            ),
            LStmt::new(
                LStmtKind::Call(lua::call(
                    LExpr::Field(Box::new(name(&class_alias)), "__init".into()),
                    args,
                )),
                span,
            ),
            LStmt::new(LStmtKind::Return(vec![name(instance)]), span),
        ];
        out.push(LStmt::new(
            LStmtKind::Function(
                format!("{}.new", c.name),
                LFunction {
                    params: new_params,
                    vararg,
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
                    && (params.iter().any(|p| {
                        p.name == "self" || p.default.as_ref().is_some_and(|e| binds_self_expr(e))
                    }) || body.iter().any(binds_self_stmt));
                self.class_receiver = if hygiene { "__selu_self" } else { "self" }.into();
                let mut f = self.function(params, body);
                if hygiene {
                    f.params.insert(0, self.class_receiver.clone());
                }
                out.push(LStmt::new(
                    LStmtKind::Function(
                        format!(
                            "{}{}{}{}",
                            c.name,
                            if m.is_static { "" } else { ".__proto" },
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
                if let Type::Class(n, _, _) = ret.as_ref() {
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
            ExprKind::Member { object, name } if matches!(object.kind, ExprKind::Super) => (
                LExpr::Field(Box::new(self.super_name(object)), "__proto".into()),
                name.clone(),
            ),
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
        S::Class(c) => c.name == "self",
        _ => false,
    } || match &s.kind {
        S::FuncDecl { params, .. } | S::OnHandler { params, .. } => {
            params.iter().any(|p| p.name == "self")
        }
        _ => false,
    };
    let mut found = own;
    selu_syntax::visit::idents_in_stmt(s, &mut |n| {
        found |= n == "self";
    });
    selu_syntax::visit::child_stmts_and_exprs(s, &mut |c| {
        found |= match c {
            selu_syntax::visit::Child::Stmt(s) => binds_self_stmt(s),
            selu_syntax::visit::Child::Expr(e) => binds_self_expr(e),
        };
    });
    found
}
fn binds_self_expr(e: &Expr) -> bool {
    let mut found = matches!(&e.kind,ExprKind::Ident(n) if n=="self")
        || matches!(&e.kind,ExprKind::Function {params,..} if params.iter().any(|p|p.name=="self"));
    selu_syntax::visit::child_exprs_and_stmts(e, &mut |c| {
        found |= match c {
            selu_syntax::visit::Child::Stmt(s) => binds_self_stmt(s),
            selu_syntax::visit::Child::Expr(e) => binds_self_expr(e),
        };
    });
    found
}

/// Capture compiler dependencies before user declarations can shadow them.
pub(crate) fn runtime_prelude(stmts: &[Stmt], out: &mut LBlock) {
    fn expression(e: &Expr) -> bool {
        let mut found = false;
        selu_syntax::visit::child_exprs_and_stmts(e, &mut |c| match c {
            selu_syntax::visit::Child::Stmt(s) => found |= contains(s),
            selu_syntax::visit::Child::Expr(e) => found |= expression(e),
        });
        found
    }
    fn contains(s: &Stmt) -> bool {
        let mut found = matches!(s.kind, selu_syntax::StmtKind::Class(_));
        selu_syntax::visit::child_stmts_and_exprs(s, &mut |c| match c {
            selu_syntax::visit::Child::Stmt(s) => found |= contains(s),
            selu_syntax::visit::Child::Expr(e) => found |= expression(e),
        });
        found
    }
    if stmts.iter().any(contains) {
        out.push(LStmt {
            kind: LStmtKind::Local(
                vec!["__selu_setmetatable".into()],
                vec![name("setmetatable")],
            ),
            span: None,
        });
    }
}

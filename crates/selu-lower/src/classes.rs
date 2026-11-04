//! Class tables, shared methods and in-place base initialization.
use crate::Lowerer;
use crate::lua::{self, LBlock, LExpr, LField, LFunction, LStmt, LStmtKind, name};
use selu_check::Type;
use selu_syntax::{ClassDecl, ClassMemberKind as M, Expr, ExprKind, Stmt};
#[derive(Default)]
pub(crate) struct ClassLoweringState {
    plans: std::collections::HashMap<usize, ClassPlan>,
}
#[derive(Clone)]
struct ClassPlan {
    class_name: String,
    base_name: Option<String>,
    split: bool,
    fields_helper: bool,
    base_live: bool,
}
#[derive(Clone, Copy, Default)]
struct ClassReference {
    local: Option<usize>,
    mutable: bool,
    live: bool,
}
struct ClassAnalysis<'a> {
    types: &'a selu_check::TypeTable,
    imports: &'a crate::ModuleBindings,
    keys: std::collections::HashMap<String, usize>,
    bases: std::collections::HashSet<usize>,
    references: std::collections::HashMap<usize, ClassReference>,
}
type ClassScope = std::collections::HashMap<String, ClassReference>;
impl ClassAnalysis<'_> {
    fn value(&self, e: &Expr, mutable: bool) -> ClassReference {
        ClassReference {
            local: match self.types.get(e.id) {
                Some(Type::ClassObject(key)) => self.keys.get(key).copied(),
                _ => None,
            },
            mutable,
            live: false,
        }
    }
    fn declaration(&self, s: &Stmt, scope: &mut ClassScope) {
        use selu_syntax::{Pattern, StmtKind as S};
        match &s.kind {
            S::Export(inner) => self.declaration(inner, scope),
            S::Class(c) => {
                scope.insert(
                    c.name.clone(),
                    ClassReference {
                        local: Some(c.name_span.start),
                        ..Default::default()
                    },
                );
            }
            S::VarDecl {
                name,
                is_const,
                init,
                ..
            } => {
                scope.insert(
                    name.clone(),
                    init.as_ref()
                        .map(|e| self.value(e, !is_const))
                        .unwrap_or_default(),
                );
            }
            S::FuncDecl { name, .. } => {
                scope.insert(name.clone(), ClassReference::default());
            }
            S::Destructure { pattern, .. } => match pattern {
                Pattern::Array(xs) => {
                    for (n, _) in xs.iter().flatten() {
                        scope.insert(n.clone(), ClassReference::default());
                    }
                }
                Pattern::Object(xs) => {
                    for (_, n, _, _) in xs {
                        scope.insert(n.clone(), ClassReference::default());
                    }
                }
            },
            S::Import { names, from, .. } => {
                for (remote, local, _, _) in names {
                    let export = self
                        .imports
                        .imports
                        .get(from)
                        .and_then(|(_, exports)| exports.get(remote));
                    scope.insert(
                        local.clone(),
                        ClassReference {
                            local: export.and_then(|e| {
                                if let Type::ClassObject(key) = &e.ty {
                                    self.keys.get(key).copied()
                                } else {
                                    None
                                }
                            }),
                            mutable: export.is_some_and(|e| e.mutable),
                            live: export.is_some_and(|e| e.mutable),
                        },
                    );
                }
            }
            _ => {}
        }
    }
    fn block(&mut self, ss: &[Stmt], outer: &ClassScope) {
        let mut scope = outer.clone();
        for s in ss {
            self.declaration(s, &mut scope);
        }
        for s in ss {
            self.statement(s, &scope);
        }
    }
    fn function(&mut self, params: &[selu_syntax::Param], body: &[Stmt], outer: &ClassScope) {
        let scope = self.parameters(params, outer);
        self.block(body, &scope);
    }
    fn parameters(&mut self, params: &[selu_syntax::Param], outer: &ClassScope) -> ClassScope {
        let mut scope = outer.clone();
        for p in params {
            scope.insert(
                p.name.clone(),
                p.default
                    .as_ref()
                    .map(|e| self.value(e, true))
                    .unwrap_or_default(),
            );
        }
        for p in params {
            if let Some(e) = &p.default {
                self.expression(e, &scope);
            }
        }
        scope
    }
    fn statement(&mut self, s: &Stmt, scope: &ClassScope) {
        use selu_syntax::{
            StmtKind as S,
            visit::{self, Child},
        };
        match &s.kind {
            S::Export(inner) => self.statement(inner, scope),
            S::Class(c) => {
                if let Some((base, _)) = &c.extends {
                    let reference = scope.get(base).copied().unwrap_or_default();
                    if let Some(id) = reference.local {
                        self.bases.insert(id);
                    }
                    self.references.insert(c.name_span.start, reference);
                }
                for m in &c.members {
                    match &m.kind {
                        M::Field { init: Some(e), .. } => self.expression(e, scope),
                        M::Method { params, body, .. } | M::Constructor { params, body } => {
                            self.function(params, body, scope)
                        }
                        _ => {}
                    }
                }
            }
            S::FuncDecl { params, body, .. } | S::OnHandler { params, body, .. } => {
                self.function(params, body, scope)
            }
            S::Try {
                body,
                catch,
                finally,
            } => {
                self.block(body, scope);
                if let Some((p, b)) = catch {
                    let params: Vec<_> = p.iter().cloned().collect();
                    self.function(&params, b, scope);
                }
                if let Some(b) = finally {
                    self.block(b, scope);
                }
            }
            S::Block(ss) => self.block(ss, scope),
            S::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.expression(cond, scope);
                self.block(then_branch, scope);
                if let Some(ss) = else_branch {
                    self.block(ss, scope);
                }
            }
            S::While { cond, body } | S::DoWhile { cond, body } => {
                self.expression(cond, scope);
                self.block(body, scope);
            }
            S::For {
                init,
                cond,
                update,
                body,
            } => {
                let mut scope = scope.clone();
                if let Some(s) = init {
                    self.declaration(s, &mut scope);
                    self.statement(s, &scope);
                }
                if let Some(e) = cond {
                    self.expression(e, &scope);
                }
                if let Some(e) = update {
                    self.expression(e, &scope);
                }
                self.block(body, &scope);
            }
            S::ForOf {
                binding,
                iter,
                body,
                ..
            } => {
                self.expression(iter, scope);
                let mut scope = scope.clone();
                match binding {
                    selu_syntax::ForBinding::Name(n, _) => {
                        scope.insert(n.clone(), ClassReference::default());
                    }
                    selu_syntax::ForBinding::Pattern(p) => match p {
                        selu_syntax::Pattern::Array(xs) => {
                            for (n, _) in xs.iter().flatten() {
                                scope.insert(n.clone(), ClassReference::default());
                            }
                        }
                        selu_syntax::Pattern::Object(xs) => {
                            for (_, n, _, _) in xs {
                                scope.insert(n.clone(), ClassReference::default());
                            }
                        }
                    },
                }
                self.block(body, &scope);
            }
            _ => visit::child_stmts_and_exprs(s, &mut |c| match c {
                Child::Stmt(s) => self.statement(s, scope),
                Child::Expr(e) => self.expression(e, scope),
            }),
        }
    }
    fn expression(&mut self, e: &Expr, scope: &ClassScope) {
        use selu_syntax::visit::{self, Child};
        if let ExprKind::Function { params, body, .. } = &e.kind {
            let scope = self.parameters(params, scope);
            match body {
                selu_syntax::FnBody::Block(ss) => self.block(ss, &scope),
                selu_syntax::FnBody::Expr(e) => self.expression(e, &scope),
            }
            return;
        }
        visit::child_exprs_and_stmts(e, &mut |c| match c {
            Child::Stmt(s) => self.statement(s, scope),
            Child::Expr(e) => self.expression(e, scope),
        });
    }
}
fn class_bindings(c: &ClassDecl) -> std::collections::HashSet<String> {
    use selu_syntax::{
        ForBinding, Pattern, StmtKind as S,
        visit::{self, Child},
    };
    fn pattern(p: &Pattern, names: &mut std::collections::HashSet<String>) {
        match p {
            Pattern::Array(xs) => names.extend(xs.iter().flatten().map(|(n, _)| n.clone())),
            Pattern::Object(xs) => names.extend(xs.iter().map(|(_, n, _, _)| n.clone())),
        }
    }
    fn expr(e: &Expr, names: &mut std::collections::HashSet<String>) {
        if let ExprKind::Function { params, .. } = &e.kind {
            names.extend(params.iter().map(|p| p.name.clone()));
        }
        visit::child_exprs_and_stmts(e, &mut |c| match c {
            Child::Stmt(s) => stmt(s, names),
            Child::Expr(e) => expr(e, names),
        });
    }
    fn stmt(s: &Stmt, names: &mut std::collections::HashSet<String>) {
        match &s.kind {
            S::VarDecl { name, .. } | S::FuncDecl { name, .. } => {
                names.insert(name.clone());
            }
            S::Class(c) => {
                names.insert(c.name.clone());
            }
            S::Destructure { pattern: p, .. } => pattern(p, names),
            S::ForOf { binding, .. } => match binding {
                ForBinding::Name(n, _) => {
                    names.insert(n.clone());
                }
                ForBinding::Pattern(p) => pattern(p, names),
            },
            _ => {}
        }
        visit::child_stmts_and_exprs(s, &mut |c| match c {
            Child::Stmt(s) => stmt(s, names),
            Child::Expr(e) => expr(e, names),
        });
    }
    let mut names = std::collections::HashSet::new();
    for m in &c.members {
        match &m.kind {
            M::Constructor { params, body } | M::Method { params, body, .. } => {
                names.extend(params.iter().map(|p| p.name.clone()));
                for p in params {
                    if let Some(e) = &p.default {
                        expr(e, &mut names);
                    }
                }
                for s in body {
                    stmt(s, &mut names);
                }
            }
            M::Field { init: Some(e), .. } => expr(e, &mut names),
            _ => {}
        }
    }
    names
}
impl Lowerer<'_> {
    pub(crate) fn class_decl(&mut self, s: &Stmt, c: &ClassDecl, forward: bool, out: &mut LBlock) {
        let saved_receiver = self.class_receiver.clone();
        let span = &s.span;
        let class = name(&c.name);
        let plan = self.classes.plans[&c.name_span.start].clone();
        let class_alias = plan.class_name;
        let base_alias = plan.base_name.clone().unwrap_or_default();
        let proto = LExpr::Field(Box::new(name(&class_alias)), "methods".into());
        out.push(LStmt::new(
            if forward {
                LStmtKind::Assign(vec![class.clone()], vec![LExpr::Table(vec![])])
            } else {
                LStmtKind::Local(vec![c.name.clone()], vec![LExpr::Table(vec![])])
            },
            span,
        ));
        if class_alias != c.name {
            out.push(LStmt::new(
                LStmtKind::Local(vec![class_alias.clone()], vec![class.clone()]),
                span,
            ));
        }
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
            if base_alias != *base {
                out.push(LStmt::new(
                    LStmtKind::Local(
                        vec![base_alias.clone()],
                        vec![if plan.base_live {
                            lua::call(name(base), vec![])
                        } else {
                            name(base)
                        }],
                    ),
                    span,
                ));
            }
            for (target, base) in [
                (class.clone(), name(&base_alias)),
                (
                    proto.clone(),
                    LExpr::Field(Box::new(name(&base_alias)), "methods".into()),
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
        let ctor_hygiene = constructor.is_some_and(|(params, body)| {
            params
                .iter()
                .any(|p| p.name == "self" || p.default.as_ref().is_some_and(|e| binds_self_expr(e)))
                || body.iter().any(binds_self_stmt)
        });
        let mut initializers = Vec::new();
        let mut static_initializers = Vec::new();
        let field_hygiene = c
            .members
            .iter()
            .any(|m| matches!(&m.kind,M::Field{init:Some(e),..} if binds_self_expr(e)));
        self.class_receiver = if field_hygiene || ctor_hygiene {
            "__selu_self"
        } else {
            "self"
        }
        .into();
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
        let initialization = if plan.fields_helper {
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
            vec![LStmt::new(
                LStmtKind::Call(lua::call(
                    LExpr::Field(Box::new(name(&class_alias)), "__fields".into()),
                    vec![name(&self.class_receiver)],
                )),
                span,
            )]
        } else {
            initializers
        };
        let mut init = if let Some((params, body)) = constructor {
            let mut f = self.function(params, body);
            if c.extends.is_some() {
                let first = &body[0].span;
                let index = f
                    .body
                    .iter()
                    .rposition(|s| s.span.as_ref() == Some(first))
                    .expect("checked super statement")
                    + 1;
                f.body.splice(index..index, initialization);
            } else {
                f.body.splice(0..0, initialization);
            }
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
            body.extend(initialization);
            LFunction {
                params: vec![],
                vararg: c.extends.is_some(),
                body,
            }
        };
        let allocation = LStmt::new(
            LStmtKind::Local(
                vec![self.class_receiver.clone()],
                vec![lua::call(
                    name("__selu_setmetatable"),
                    vec![LExpr::Table(vec![]), proto],
                )],
            ),
            span,
        );
        let new_function = if plan.split {
            let params = init.params.clone();
            let vararg = init.vararg;
            init.params.insert(0, self.class_receiver.clone());
            out.push(LStmt::new(
                LStmtKind::Function(format!("{}.__init", c.name), init),
                span,
            ));
            let mut args = vec![name(&self.class_receiver)];
            args.extend(params.iter().map(|n| name(n)));
            if vararg {
                args.push(LExpr::Vararg);
            }
            LFunction {
                params,
                vararg,
                body: vec![
                    allocation,
                    LStmt::new(
                        LStmtKind::Call(lua::call(
                            LExpr::Field(Box::new(name(&class_alias)), "__init".into()),
                            args,
                        )),
                        span,
                    ),
                    LStmt::new(LStmtKind::Return(vec![name(&self.class_receiver)]), span),
                ],
            }
        } else {
            constructor_returns(&mut init.body, &self.class_receiver);
            init.body.insert(0, allocation);
            if !matches!(
                init.body.last().map(|s| &s.kind),
                Some(LStmtKind::Return(_))
            ) {
                init.body.push(LStmt::new(
                    LStmtKind::Return(vec![name(&self.class_receiver)]),
                    span,
                ));
            }
            init
        };
        out.push(LStmt::new(
            LStmtKind::Function(format!("{}.new", c.name), new_function),
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
                            if m.is_static { "" } else { ".methods" },
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
                    let id = n.rsplit('@').next().unwrap().parse::<usize>().unwrap();
                    name(self.classes.plans[&id].base_name.as_ref().unwrap())
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
                LExpr::Field(Box::new(self.super_name(object)), "methods".into()),
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
        S::Try {
            catch: Some((Some(p), _)),
            ..
        } => p.name == "self",
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

/// Plan class helpers and capture runtime dependencies before source statements.
impl Lowerer<'_> {
    pub(crate) fn class_prelude(&mut self, stmts: &[Stmt], out: &mut LBlock) {
        use selu_syntax::{
            StmtKind as S,
            visit::{self, Child},
        };
        fn expression(e: &Expr, classes: &mut Vec<(ClassDecl, bool)>) {
            visit::child_exprs_and_stmts(e, &mut |c| match c {
                Child::Stmt(s) => statement(s, false, classes),
                Child::Expr(e) => expression(e, classes),
            });
        }
        fn statement(s: &Stmt, exported: bool, classes: &mut Vec<(ClassDecl, bool)>) {
            if let S::Export(inner) = &s.kind {
                statement(inner, true, classes);
                return;
            }
            if let S::Class(c) = &s.kind {
                classes.push((c.clone(), exported));
            }
            visit::child_stmts_and_exprs(s, &mut |c| match c {
                Child::Stmt(s) => statement(s, false, classes),
                Child::Expr(e) => expression(e, classes),
            });
        }
        let mut classes = Vec::new();
        for s in stmts {
            statement(s, false, &mut classes);
        }
        let keys = classes
            .iter()
            .map(|(c, _)| {
                (
                    format!("{}@{}@{}", c.name, c.name_span.file, c.name_span.start),
                    c.name_span.start,
                )
            })
            .collect();
        let mut analysis = ClassAnalysis {
            types: self.types,
            imports: &self.module,
            keys,
            bases: Default::default(),
            references: Default::default(),
        };
        analysis.block(stmts, &ClassScope::new());
        for (c, exported) in &classes {
            let bindings = class_bindings(c);
            let class_name = if bindings.contains(&c.name) {
                format!("__selu_class_{}", c.name_span.start)
            } else {
                c.name.clone()
            };
            let base_name = c.extends.as_ref().map(|(base, _)| {
                if bindings.contains(base)
                    || analysis
                        .references
                        .get(&c.name_span.start)
                        .is_some_and(|r| r.mutable)
                {
                    format!("__selu_base_{}", c.name_span.start)
                } else {
                    base.clone()
                }
            });
            let ctor = c
                .members
                .iter()
                .find(|m| matches!(m.kind, M::Constructor { .. }));
            let fields_helper = ctor.is_some_and(|ctor| {
                let M::Constructor { params, body } = &ctor.kind else {
                    unreachable!()
                };
                let mut bound: std::collections::HashSet<String> =
                    params.iter().map(|p| p.name.clone()).collect();
                if c.extends.is_some() {
                    bound.extend(crate::used_before_declaration(body));
                }
                c.members.iter().filter(|m| !m.is_static).any(|m| {
                    if let M::Field { init: Some(e), .. } = &m.kind {
                        let mut reads = std::collections::HashSet::new();
                        crate::modules::walk_expr(e, &bound, &mut reads);
                        !reads.is_empty()
                    } else {
                        false
                    }
                })
            });
            self.classes.plans.insert(
                c.name_span.start,
                ClassPlan {
                    class_name,
                    base_name,
                    split: *exported || analysis.bases.contains(&c.name_span.start),
                    base_live: analysis
                        .references
                        .get(&c.name_span.start)
                        .is_some_and(|r| r.live),
                    fields_helper,
                },
            );
        }
        if !classes.is_empty() {
            out.push(LStmt {
                kind: LStmtKind::Local(
                    vec!["__selu_setmetatable".into()],
                    vec![name("setmetatable")],
                ),
                span: None,
            });
        }
    }
}

fn constructor_returns(body: &mut LBlock, receiver: &str) {
    for s in body {
        match &mut s.kind {
            LStmtKind::Return(values) if values.is_empty() => values.push(name(receiver)),
            LStmtKind::If(arms, other) => {
                for (_, body) in arms {
                    constructor_returns(body, receiver);
                }
                if let Some(body) = other {
                    constructor_returns(body, receiver);
                }
            }
            LStmtKind::While(_, body)
            | LStmtKind::Repeat(body, _)
            | LStmtKind::Do(body)
            | LStmtKind::NumFor { body, .. }
            | LStmtKind::GenFor { body, .. } => constructor_returns(body, receiver),
            _ => {}
        }
    }
}

//! Class signatures and lexical receiver context.
use crate::{Ctx, Kind, Type, err, types};
use selu_syntax::{ClassDecl, ClassMemberKind as M, Expr, ExprKind, Stmt, StmtKind, TypeExpr};
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
pub(crate) struct ClassContext {
    infos: HashMap<String, Info>,
    pub(crate) receiver: Option<(String, bool)>,
}
#[derive(Clone)]
struct Info {
    instance: Type,
    fields: BTreeMap<String, Type>,
    statics: BTreeMap<String, Type>,
    constructor: Type,
    base: Option<String>,
}
impl Ctx<'_> {
    fn validate_class(&mut self, c: &ClassDecl) {
        use selu_syntax::{
            is_lua_keyword,
            visit::{self, Child},
        };
        fn super_expr(
            e: &Expr,
            allowed: Option<usize>,
            out: &mut Vec<selu_diagnostics::Diagnostic>,
        ) {
            if matches!(&e.kind,ExprKind::Call {callee,..} if matches!(callee.kind,ExprKind::Super))
                && allowed != Some(e.span.start)
            {
                out.push(err("SELU0432","Super construction is only allowed once at the start of a derived constructor.",&e.span));
            }
            visit::child_exprs_and_stmts(e, &mut |c| match c {
                Child::Expr(e) => super_expr(e, allowed, out),
                Child::Stmt(s) => super_stmt(s, allowed, out),
            });
        }
        fn super_stmt(
            s: &Stmt,
            allowed: Option<usize>,
            out: &mut Vec<selu_diagnostics::Diagnostic>,
        ) {
            visit::child_stmts_and_exprs(s, &mut |c| match c {
                Child::Expr(e) => super_expr(e, allowed, out),
                Child::Stmt(s) => super_stmt(s, allowed, out),
            });
        }
        if is_lua_keyword(&c.name) || c.name.starts_with("__selu") {
            self.out.push(err(
                "SELU0431",
                "This class name is reserved by Lua or the compiler.",
                &c.name_span,
            ));
        }
        let mut names = std::collections::HashSet::new();
        let mut constructor = false;
        for m in &c.members {
            let (params, body) = match &m.kind {
                M::Field {
                    name,
                    name_span,
                    init,
                    ..
                } => {
                    if matches!(name.as_str(), "new" | "__init" | "__index")
                        || !names.insert((m.is_static, name.clone()))
                    {
                        self.out.push(err(
                            "SELU0431",
                            "This member name is reserved or already declared.",
                            name_span,
                        ));
                    }
                    if let Some(e) = init {
                        super_expr(e, None, self.out);
                    }
                    continue;
                }
                M::Method {
                    name,
                    name_span,
                    params,
                    body,
                    ..
                } => {
                    if matches!(name.as_str(), "new" | "__init" | "__index")
                        || is_lua_keyword(name)
                        || !names.insert((m.is_static, name.clone()))
                    {
                        self.out.push(err(
                            "SELU0431",
                            "This member name is reserved or already declared.",
                            name_span,
                        ));
                    }
                    (params, body)
                }
                M::Constructor { params, body } => {
                    if constructor || m.is_static {
                        self.out.push(err(
                            "SELU0431",
                            "A class has one non-static constructor.",
                            &m.span,
                        ));
                    }
                    constructor = true;
                    (params, body)
                }
            };
            let allowed = if matches!(m.kind, M::Constructor { .. }) && c.extends.is_some() {
                body.iter()
                    .find(|s| is_super_statement(s))
                    .map(|s| match &s.kind {
                        StmtKind::Expr(e) => e.span.start,
                        _ => unreachable!(),
                    })
            } else {
                None
            };
            for p in params {
                if is_lua_keyword(&p.name) || p.name.starts_with("__selu") {
                    self.out.push(err(
                        "SELU0431",
                        "This parameter name is reserved by Lua or the compiler.",
                        &p.span,
                    ));
                }
                if let Some(e) = &p.default {
                    super_expr(e, None, self.out);
                }
            }
            for s in body {
                super_stmt(s, allowed, self.out);
            }
        }
    }

    pub(crate) fn predeclare_class_names(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            let s = match &s.kind {
                StmtKind::Export(inner) => inner.as_ref(),
                _ => s,
            };
            if let StmtKind::Class(c) = &s.kind {
                let key = format!("{}@{}", c.name, c.name_span.start);
                let instance = Type::Class(key.clone(), Vec::new());
                self.classes.infos.insert(
                    key.clone(),
                    Info {
                        instance: instance.clone(),
                        fields: BTreeMap::new(),
                        statics: BTreeMap::new(),
                        base: None,
                        constructor: Type::Function {
                            params: Vec::new(),
                            ret: Box::new(instance),
                            required: 0,
                            rest: None,
                        },
                    },
                );
                self.declare(&c.name, &c.name_span, Kind::Const, Type::ClassObject(key));
            }
        }
    }

    pub(crate) fn class_resolve(&mut self, t: &TypeExpr) -> Type {
        let mut names = HashMap::new();
        for scope in &self.scopes {
            for (n, b) in &scope.names {
                if let Type::ClassObject(key) = &b.ty {
                    if let Some(i) = self.classes.infos.get(key) {
                        names.insert(n.clone(), i.instance.clone());
                    }
                } else {
                    names.remove(n);
                }
            }
        }
        types::resolve_classes(t, self.out, &names)
    }
    pub(crate) fn declare_class(&mut self, c: &ClassDecl) {
        let key = format!("{}@{}", c.name, c.name_span.start);
        let base = c.extends.as_ref().and_then(|(n, _)| match self.type_of(n) {
            Some(Type::ClassObject(k)) => Some(k),
            _ => None,
        });
        let inherited = base
            .as_ref()
            .and_then(|n| self.classes.infos.get(n))
            .cloned();
        let mut bases = Vec::new();
        if let Some(b) = &inherited
            && let Type::Class(n, parents) = &b.instance
        {
            bases.push(n.clone());
            bases.extend(parents.clone());
        }
        let instance = Type::Class(key.clone(), bases);
        let empty = Type::Function {
            params: vec![],
            ret: Box::new(instance.clone()),
            required: 0,
            rest: None,
        };
        self.classes.infos.insert(
            key.clone(),
            Info {
                instance: instance.clone(),
                fields: inherited
                    .as_ref()
                    .map(|b| b.fields.clone())
                    .unwrap_or_default(),
                statics: inherited
                    .as_ref()
                    .map(|b| b.statics.clone())
                    .unwrap_or_default(),
                constructor: inherited
                    .as_ref()
                    .map(|b| b.constructor.clone())
                    .unwrap_or(empty),
                base,
            },
        );
        for m in &c.members {
            let entry = match &m.kind {
                M::Field { name, ty, init, .. } => {
                    let t = self
                        .annotation(ty)
                        .or_else(|| {
                            init.as_ref().map(|e| match &e.kind {
                                ExprKind::Number(_) => Type::Number,
                                ExprKind::Str(_) => Type::String,
                                ExprKind::Bool(_) => Type::Boolean,
                                _ => Type::Untyped,
                            })
                        })
                        .unwrap_or(Type::Error);
                    Some((name.clone(), t))
                }
                M::Method {
                    name, params, ret, ..
                } => {
                    let r = self.annotation(ret).unwrap_or(Type::Untyped);
                    Some((name.clone(), self.function_signature(params, r)))
                }
                M::Constructor { params, .. } => {
                    let t = self.function_signature(params, instance.clone());
                    self.classes.infos.get_mut(&key).unwrap().constructor = t;
                    None
                }
            };
            if let Some((n, t)) = entry {
                let i = self.classes.infos.get_mut(&key).unwrap();
                if m.is_static {
                    i.statics.insert(n, t);
                } else {
                    i.fields.insert(n, t);
                }
            }
        }
    }
    pub(crate) fn class_decl(&mut self, _s: &Stmt, c: &ClassDecl) {
        let key = format!("{}@{}", c.name, c.name_span.start);
        if c.extends.is_some() {
            self.declare_class(c);
        }
        let info = self.classes.infos[&key].clone();
        self.validate_class(c);
        if let Some((base, span)) = &c.extends
            && let Some((_, b)) = self.lookup(base).map(|(i, b)| (i, b.clone()))
        {
            self.check_ready(base, &b, span);
        }

        if let Some((base, span)) = &c.extends
            && !matches!(self.type_of(base), Some(Type::ClassObject(_)))
        {
            self.out
                .push(err("SELU0420", "The base must be a declared class.", span));
        }
        self.finish_decl(&c.name, Type::ClassObject(key.clone()));
        for m in &c.members {
            let saved = self.classes.receiver.replace((key.clone(), m.is_static));
            match &m.kind {
                M::Field {
                    name,
                    name_span,
                    ty,
                    init,
                } => {
                    if ty.is_none() && init.is_none() {
                        self.out.push(
                            err(
                                "SELU0421",
                                "A field needs a type or an initializer.",
                                name_span,
                            )
                            .with_help("write a field type or starting value"),
                        );
                    }
                    if let Some(e) = init {
                        let want = if m.is_static {
                            info.statics.get(name)
                        } else {
                            info.fields.get(name)
                        };
                        let t = self.value_expected(e, want);
                        if ty.is_none() {
                            let i = self.classes.infos.get_mut(&key).unwrap();
                            if m.is_static {
                                i.statics.insert(name.clone(), t.clone());
                            } else {
                                i.fields.insert(name.clone(), t.clone());
                            }
                        }
                        if let Some(w) = want
                            && !t.assignable_to(w)
                        {
                            self.out.push(err(
                                "SELU0422",
                                "The initializer does not match the field type.",
                                &e.span,
                            ));
                        }
                    }
                }
                M::Method {
                    name,
                    name_span,
                    params,
                    ret,
                    body,
                } => {
                    let ts = self.param_types(params);
                    let written = self.annotation(ret);
                    let result = self.function_body(params, ts, written, body);
                    let mut sig = if m.is_static {
                        info.statics[name].clone()
                    } else {
                        info.fields[name].clone()
                    };
                    if let Type::Function { ret, .. } = &mut sig {
                        **ret = result;
                    }
                    if !m.is_static
                        && let Some(base) = info
                            .base
                            .as_ref()
                            .and_then(|n| self.classes.infos.get(n))
                            .and_then(|b| b.fields.get(name))
                        && !sig.assignable_to(base)
                    {
                        self.out.push(err(
                            "SELU0423",
                            "The override must match the base method signature.",
                            name_span,
                        ));
                    }
                    let i = self.classes.infos.get_mut(&key).unwrap();
                    if m.is_static {
                        i.statics.insert(name.clone(), sig);
                    } else {
                        i.fields.insert(name.clone(), sig);
                    }
                }
                M::Constructor { params, body } => {
                    if info.base.is_some() && !body.first().is_some_and(is_super_statement) {
                        self.out.push(
                            err(
                                "SELU0424",
                                "A derived constructor must start with super(...).",
                                &m.span,
                            )
                            .with_help("move super(...) before other statements"),
                        );
                    }
                    let ts = self.param_types(params);
                    self.function_body(params, ts, Some(Type::Void), body);
                }
            }
            self.classes.receiver = saved;
        }
    }
    pub(crate) fn class_expr(&mut self, e: &Expr) -> Type {
        let Some((n, static_)) = self.classes.receiver.clone() else {
            self.out.push(err(
                "SELU0425",
                "This receiver is only available inside a class method or constructor.",
                &e.span,
            ));
            return Type::Error;
        };
        if static_ {
            self.out.push(err(
                "SELU0425",
                "Static members cannot use this or super.",
                &e.span,
            ));
            return Type::Error;
        }
        let info = &self.classes.infos[&n];
        if matches!(e.kind, ExprKind::This) {
            return info.instance.clone();
        }
        if let Some(b) = info.base.as_ref().and_then(|b| self.classes.infos.get(b)) {
            let mut sig = b.constructor.clone();
            if let Type::Function { ret, .. } = &mut sig {
                **ret = info.instance.clone();
            }
            sig
        } else {
            self.out
                .push(err("SELU0426", "Super requires a base class.", &e.span));
            Type::Error
        }
    }
    pub(crate) fn class_member(&mut self, t: &Type, name: &str, e: &Expr) -> Option<Type> {
        let (n, static_) = match t {
            Type::Class(n, _) => (n, false),
            Type::ClassObject(n) => (n, true),
            Type::Function { ret, .. } if matches!(&e.kind, ExprKind::Member {object,..} if matches!(object.kind,ExprKind::Super)) => {
                match ret.as_ref() {
                    Type::Class(n, _) => (n, false),
                    _ => return None,
                }
            }
            _ => return None,
        };
        let mut i = self.classes.infos.get(n)?;
        if matches!(t, Type::Function { .. }) {
            i = self.classes.infos.get(i.base.as_ref()?)?;
        }
        let found = if static_ {
            i.statics.get(name)
        } else {
            i.fields.get(name)
        };
        Some(found.cloned().unwrap_or_else(|| {
            self.out.push(
                err(
                    "SELU0427",
                    format!("This class has no member `{name}`."),
                    &e.span,
                )
                .with_help("check the member name"),
            );
            Type::Error
        }))
    }
    pub(crate) fn class_new(
        &mut self,
        e: &Expr,
        name: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> Type {
        let Some(Type::ClassObject(key)) = self.type_of(name) else {
            return self.unsupported("construction of this value", &e.span);
        };
        if let Some((_, b)) = self.lookup(name).map(|(s, b)| (s, b.clone())) {
            self.check_ready(name, &b, &e.span);
        }
        let Some(i) = self.classes.infos.get(&key).cloned() else {
            return self.unsupported("construction of this class", &e.span);
        };
        if !type_args.is_empty() {
            self.out.push(err(
                "SELU0428",
                "Classes do not take type arguments.",
                &e.span,
            ));
        }
        if let Type::Function {
            params,
            required,
            rest,
            ..
        } = &i.constructor
        {
            if args.len() < *required || (rest.is_none() && args.len() > params.len()) {
                self.out.push(err(
                    "SELU0429",
                    "The constructor argument count does not match.",
                    &e.span,
                ));
            }
            let ts = self.function_arguments(args, &i.constructor);
            for (index, (a, t)) in args.iter().zip(ts).enumerate() {
                if let Some(w) = params.get(index).or(rest.as_deref())
                    && !t.assignable_to(w)
                {
                    self.out.push(err(
                        "SELU0430",
                        "The constructor argument has the wrong type.",
                        &a.span,
                    ));
                }
            }
        }
        i.instance
    }
}
fn is_super_statement(s: &Stmt) -> bool {
    matches!(&s.kind, StmtKind::Expr(e) if matches!(&e.kind, ExprKind::Call {callee,..} if matches!(callee.kind, ExprKind::Super)))
}

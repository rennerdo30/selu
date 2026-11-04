//! Class signatures and lexical receiver context.
use crate::{Ctx, Kind, Type, err, types};
use selu_syntax::{ClassDecl, ClassMemberKind as M, Expr, ExprKind, Stmt, StmtKind, TypeExpr};
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
pub(crate) struct ClassContext {
    pub(crate) infos: HashMap<String, ClassInfo>,
    initialized: bool,
    pub(crate) receiver: Option<(String, bool)>,
}
#[derive(Debug, Clone)]
pub struct ClassInfo {
    instance: Type,
    fields: BTreeMap<String, Type>,
    statics: BTreeMap<String, Type>,
    constructor: Type,
    base: Option<String>,
    methods: std::collections::HashSet<String>,
}
impl ClassContext {
    pub(crate) fn from_imports(module: &crate::ModuleBindings) -> Self {
        let mut ctx = Self::default();
        for exports in module.imports.values() {
            for e in exports.values() {
                ctx.infos.extend(e.classes.clone());
            }
        }
        ctx
    }
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
            if matches!(s.kind, StmtKind::Class(_)) {
                return;
            }
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
                    if matches!(
                        name.as_str(),
                        "new" | "__init" | "__index" | "__fields" | "__proto"
                    ) || (m.is_static && name == "methods")
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
                    if matches!(
                        name.as_str(),
                        "new" | "__init" | "__index" | "__fields" | "__proto"
                    ) || (m.is_static && name == "methods")
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
                    self.out.push(
                        err(
                            "SELU0431",
                            "This parameter name is reserved by Lua or the compiler.",
                            &selu_diagnostics::Span::new(
                                &p.span.file,
                                p.span.start,
                                p.span.start + p.name.len(),
                            ),
                        )
                        .with_help("choose a parameter name that is not reserved"),
                    );
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
                let key = format!("{}@{}@{}", c.name, c.name_span.file, c.name_span.start);
                let instance = Type::Class(key.clone(), Vec::new(), BTreeMap::new());
                self.classes.infos.insert(
                    key.clone(),
                    ClassInfo {
                        instance: instance.clone(),
                        fields: BTreeMap::new(),
                        statics: BTreeMap::new(),
                        base: None,
                        methods: Default::default(),
                        constructor: Type::Function {
                            multiple_results: false,
                            unspecified_callback: false,
                            api: Default::default(),
                            api_semantics: Default::default(),
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

    fn check_before_super(&mut self, e: &Expr) {
        if !self.classes.initialized
            && (matches!(e.kind, ExprKind::This)
                || matches!(&e.kind,ExprKind::Member {object,..} if matches!(object.kind,ExprKind::Super)))
        {
            self.out.push(
                err(
                    "SELU0433",
                    "This cannot be read before super returns.",
                    &e.span,
                )
                .with_help("use this after the super(...) statement"),
            );
        }
        if let ExprKind::Call { callee, .. } = &e.kind
            && let ExprKind::Function { body, .. } = &callee.kind
        {
            match body {
                selu_syntax::FnBody::Expr(e) => self.check_before_super(e),
                selu_syntax::FnBody::Block(stmts) => {
                    for s in stmts {
                        self.check_before_super_stmt(s);
                    }
                }
            }
        }
        if matches!(e.kind, ExprKind::Function { .. }) {
            return;
        }
        selu_syntax::visit::child_exprs_and_stmts(e, &mut |c| {
            if let selu_syntax::visit::Child::Expr(e) = c {
                self.check_before_super(e);
            }
        });
    }
    fn check_before_super_stmt(&mut self, s: &Stmt) {
        if matches!(s.kind, StmtKind::Class(_) | StmtKind::FuncDecl { .. }) {
            return;
        }
        selu_syntax::visit::child_stmts_and_exprs(s, &mut |c| match c {
            selu_syntax::visit::Child::Expr(e) => self.check_before_super(e),
            selu_syntax::visit::Child::Stmt(s) => self.check_before_super_stmt(s),
        });
    }
    pub(crate) fn refresh_class_type(&self, t: Type) -> Type {
        match t {
            Type::Class(ref key, _, _) if self.classes.infos.contains_key(key) => {
                self.instance_type(key)
            }
            _ => t,
        }
    }
    fn instance_type(&self, key: &str) -> Type {
        let i = &self.classes.infos[key];
        let Type::Class(n, b, _) = &i.instance else {
            unreachable!()
        };
        Type::Class(n.clone(), b.clone(), i.fields.clone())
    }
    pub(crate) fn class_resolve(&mut self, t: &TypeExpr) -> Type {
        self.class_resolve_with_aliases(t, &HashMap::new())
    }
    pub(crate) fn class_resolve_with_aliases(
        &mut self,
        t: &TypeExpr,
        aliases: &HashMap<String, Type>,
    ) -> Type {
        let mut names = self.frame_type_names();
        names.extend(self.class_type_names());
        names.extend(aliases.clone());
        types::resolve_classes(t, self.out, &names)
    }
    pub(crate) fn class_type_names(&self) -> HashMap<String, Type> {
        let mut names = HashMap::new();
        for scope in &self.scopes {
            for (n, b) in &scope.names {
                if let Type::ClassObject(key) = &b.ty {
                    if self.classes.infos.contains_key(key) {
                        names.insert(n.clone(), self.instance_type(key));
                    }
                } else {
                    names.remove(n);
                }
            }
        }
        names
    }
    pub(crate) fn declare_class(&mut self, c: &ClassDecl) {
        let key = format!("{}@{}@{}", c.name, c.name_span.file, c.name_span.start);
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
            && let Type::Class(n, parents, _) = &b.instance
        {
            bases.push(n.clone());
            bases.extend(parents.clone());
        }
        let instance = Type::Class(key.clone(), bases, BTreeMap::new());
        let empty = Type::Function {
            multiple_results: false,
            unspecified_callback: false,
            api: Default::default(),
            api_semantics: Default::default(),
            params: vec![],
            ret: Box::new(instance.clone()),
            required: 0,
            rest: None,
        };
        self.classes.infos.insert(
            key.clone(),
            ClassInfo {
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
                methods: inherited
                    .as_ref()
                    .map(|i| i.methods.clone())
                    .unwrap_or_default(),
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
                if !m.is_static {
                    if matches!(m.kind, M::Method { .. }) {
                        i.methods.insert(n.clone());
                    } else {
                        i.methods.remove(&n);
                    }
                }
                if m.is_static {
                    i.statics.insert(n, t);
                } else {
                    i.fields.insert(n, t);
                }
            }
        }
    }
    pub(crate) fn class_decl(&mut self, _s: &Stmt, c: &ClassDecl) {
        let key = format!("{}@{}@{}", c.name, c.name_span.file, c.name_span.start);
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
            let saved_initialized = self.classes.initialized;
            self.classes.initialized = true;
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
                        if !m.is_static {
                            self.fn_depth += 1;
                        }
                        let t = self.value_expected(e, want);
                        if !m.is_static {
                            self.fn_depth -= 1;
                        }
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
                    params,
                    ret,
                    body,
                    ..
                } => {
                    let ts = self.param_types(params);
                    let written = self.annotation(ret);
                    let result = self.function_body(params, ts, written, body);
                    let mut sig = if m.is_static {
                        info.statics[name].clone()
                    } else {
                        info.fields[name].clone()
                    };
                    if let Type::Function {
                        params: fixed, ret, ..
                    } = &mut sig
                    {
                        *fixed = self.checked_parameter_types(params, fixed.clone());
                        **ret = result;
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
                    if info.base.is_some() {
                        self.classes.initialized = false;
                        for p in params {
                            if let Some(e) = &p.default {
                                self.check_before_super(e);
                            }
                        }
                        if let Some(Stmt {
                            kind: StmtKind::Expr(e),
                            ..
                        }) = body.first()
                        {
                            self.check_before_super(e);
                        }
                        self.classes.initialized = true;
                    }
                    let ts = self.param_types(params);
                    self.function_body(params, ts, Some(Type::Void), body);
                    let mut sig = self.classes.infos[&key].constructor.clone();
                    if let Type::Function { params: fixed, .. } = &mut sig {
                        *fixed = self.checked_parameter_types(params, fixed.clone());
                    }
                    self.classes.infos.get_mut(&key).unwrap().constructor = sig;
                }
            }
            if let M::Field {
                name, name_span, ..
            }
            | M::Method {
                name, name_span, ..
            } = &m.kind
                && let Some(base) = info.base.as_ref().and_then(|k| self.classes.infos.get(k))
            {
                let have = &self.classes.infos[&key];
                let (actual, expected) = if m.is_static {
                    (have.statics.get(name), base.statics.get(name))
                } else {
                    (have.fields.get(name), base.fields.get(name))
                };
                if let (Some(a), Some(w)) = (actual, expected)
                    && (!a.assignable_to(w)
                        || (!m.is_static
                            && have.methods.contains(name) != base.methods.contains(name)))
                {
                    self.out.push(
                        err(
                            "SELU0423",
                            "The override must preserve the base member type.",
                            name_span,
                        )
                        .with_help(format!("expected `{name}: {w}`")),
                    );
                }
            }
            self.classes.receiver = saved;
            self.classes.initialized = saved_initialized;
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
            return self.instance_type(&n);
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
            Type::Class(n, _, _) => (n, false),
            Type::ClassObject(n) => (n, true),
            Type::Function { ret, .. } if matches!(&e.kind, ExprKind::Member {object,..} if matches!(object.kind,ExprKind::Super)) => {
                match ret.as_ref() {
                    Type::Class(n, _, _) => (n, false),
                    _ => return None,
                }
            }
            _ => return None,
        };
        let mut i = self.classes.infos.get(n)?;
        if matches!(t, Type::Function { .. }) {
            i = self.classes.infos.get(i.base.as_ref()?)?;
        }
        if !static_ && i.methods.contains(name) {
            self.types.mark_method_call(e.id);
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
                self.out.push(
                    err(
                        "SELU0429",
                        "The constructor argument count does not match.",
                        &e.span,
                    )
                    .with_help(format!("expected constructor {}", i.constructor)),
                );
            }
            let ts = self.function_arguments(args, &i.constructor);
            for (index, (a, t)) in args.iter().zip(ts).enumerate() {
                if let Some(w) = params.get(index).or(rest.as_deref())
                    && !t.assignable_to(w)
                {
                    self.out.push(
                        err(
                            "SELU0430",
                            "The constructor argument has the wrong type.",
                            &a.span,
                        )
                        .with_help(format!("expected constructor {}", i.constructor)),
                    );
                }
            }
        }
        self.instance_type(&key)
    }
}
fn is_super_statement(s: &Stmt) -> bool {
    matches!(&s.kind, StmtKind::Expr(e) if matches!(&e.kind, ExprKind::Call {callee,..} if matches!(callee.kind, ExprKind::Super)))
}

/// Explain the intentional structural boundary at every checker mismatch site.
pub(crate) fn mismatch_help(
    d: selu_diagnostics::Diagnostic,
    have: &Type,
    want: &Type,
) -> selu_diagnostics::Diagnostic {
    let shaped_have = crate::shape_of(have);
    let shaped_want = crate::shape_of(&want.non_null());
    let have = &shaped_have;
    let want = &shaped_want;
    if matches!(have, Type::Object(_))
        && let Type::Class(name, _, _) = want
    {
        d.with_help(format!("an object literal has no class metatable or methods; construct it with new {}(...) instead",name.split('@').next().unwrap()))
    } else {
        d
    }
}

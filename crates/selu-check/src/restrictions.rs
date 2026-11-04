//! Restriction policy from manifest data; broad access flags are not combat flags.
use selu_manifest::{Declaration, Manifest};

pub(crate) fn secret_results(m: &Manifest, d: &Declaration) -> Vec<bool> {
    let full = d
        .namespace
        .as_ref()
        .map(|n| format!("{n}.{}", d.name))
        .unwrap_or_else(|| d.name.clone());
    raw_secret_results(d, m.function(&full).is_some_and(|f| f.returns_secret))
}
fn raw_secret_results(d: &Declaration, returns_secret: bool) -> Vec<bool> {
    let per_result = d.returns.iter().any(|r| {
        ["SecretValue", "ConditionalSecret"]
            .iter()
            .any(|k| r.attributes.get(*k).and_then(|v| v.as_bool()) == Some(true))
    });
    let all = !per_result
        && (d.attributes.get("SecretReturns").and_then(|v| v.as_bool()) == Some(true)
            || d.attributes
                .get("SecretReturnsForAspect")
                .and_then(|v| v.as_array())
                .is_some_and(|aspects| !aspects.is_empty())
            || returns_secret
            || d.attributes.iter().any(|(r, v)| {
                (r.starts_with("SecretWhen") || r.starts_with("SecretIn"))
                    && v.as_bool() == Some(true)
            }));
    d.returns
        .iter()
        .enumerate()
        .map(|(index, r)| {
            d.attributes
                .get(&format!("SeluSecretResult{index}"))
                .and_then(|v| v.as_bool())
                == Some(true)
                || r.attributes.get("NeverSecret").and_then(|v| v.as_bool()) != Some(true)
                    && (all
                        || ["SecretValue", "ConditionalSecret"]
                            .iter()
                            .any(|k| r.attributes.get(*k).and_then(|v| v.as_bool()) == Some(true)))
        })
        .collect()
}

/// Every context may execute during combat, including file load on demand.
pub(crate) fn check_uses(
    program: &selu_syntax::Program,
    types: &crate::TypeTable,
    out: &mut Vec<selu_diagnostics::Diagnostic>,
) {
    use selu_syntax::{ExprKind, StmtKind, visit};
    fn secret_use(
        e: &selu_syntax::Expr,
        types: &crate::TypeTable,
        out: &mut Vec<selu_diagnostics::Diagnostic>,
    ) {
        if let Some(ty) = types.get(e.id)
            && secret_inner(ty).is_some()
        {
            out.push(
                crate::err(
                    "SELU0601",
                    format!("{ty} requires an explicit readable value"),
                    &e.span,
                )
                .with_help("use `value ?? fallback` or `readable(value)` before reading it"),
            );
        }
    }
    fn walk(
        c: visit::Child<'_>,
        allowed: Option<selu_syntax::ExprId>,
        types: &crate::TypeTable,
        out: &mut Vec<selu_diagnostics::Diagnostic>,
    ) {
        match c {
            visit::Child::Stmt(s) => {
                let cond = match &s.kind {
                    StmtKind::If { cond, .. }
                    | StmtKind::While { cond, .. }
                    | StmtKind::DoWhile { cond, .. } => Some(cond.as_ref()),
                    StmtKind::For { cond, .. } => cond.as_deref(),
                    _ => None,
                };
                if let Some(e) = cond {
                    secret_use(e, types, out);
                }
                let allowed = if let StmtKind::Expr(e) = &s.kind {
                    Some(e.id)
                } else {
                    None
                };
                visit::child_stmts_and_exprs(s, &mut |c| walk(c, allowed, types, out));
            }
            visit::Child::Expr(e) => {
                match &e.kind {
                    ExprKind::Binary { op, left, right } if op != "??" => {
                        secret_use(left, types, out);
                        secret_use(right, types, out);
                    }
                    ExprKind::Unary { expr, .. } => secret_use(expr, types, out),
                    ExprKind::Conditional { cond, .. } => secret_use(cond, types, out),
                    ExprKind::Member { object, .. } => secret_use(object, types, out),
                    ExprKind::Index { object, index } => {
                        secret_use(object, types, out);
                        secret_use(index, types, out);
                    }
                    ExprKind::Template { exprs, .. } => {
                        for expr in exprs {
                            secret_use(expr, types, out);
                        }
                    }
                    ExprKind::CompoundAssign { target, value, .. } => {
                        secret_use(target, types, out);
                        secret_use(value, types, out);
                    }
                    ExprKind::Call { callee, args } => {
                        secret_use(callee, types, out);
                        let readable = matches!(&callee.kind, ExprKind::Ident(n) if n == "readable")
                            && types.get(callee.id).is_none();
                        if !readable {
                            for (i, arg) in args.iter().enumerate() {
                                let accepts = matches!(types.get(callee.id), Some(crate::Type::Function {api_semantics,params,rest,..}) if api_semantics.accepts_secrets || params.get(i).or(rest.as_deref()).is_some_and(|p| secret_inner(p).is_some()));
                                if !accepts {
                                    secret_use(arg, types, out);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                if allowed != Some(e.id)
                    && let ExprKind::Call { callee, .. } = &e.kind
                    && matches!(types.get(callee.id), Some(crate::Type::Function { api_semantics, .. }) if api_semantics.protected || api_semantics.dynamic_protected)
                {
                    out.push(
                        crate::err(
                            "SELU0600",
                            "a protected call whose result is used cannot be deferred",
                            &e.span,
                        )
                        .with_help("call it as a statement, then read state after combat"),
                    );
                }
                visit::child_exprs_and_stmts(e, &mut |c| walk(c, None, types, out));
            }
        }
    }
    for s in &program.stmts {
        walk(visit::Child::Stmt(s), None, types, out);
    }
}

/// Merge restriction policies without changing any target's API signatures.
pub(crate) fn strictest_policies(manifest: &mut Manifest, targets: &[Manifest]) {
    if targets.len() < 2 {
        return;
    }
    use std::collections::HashMap;
    #[derive(Default)]
    struct Policy {
        results: Vec<bool>,
        protected: bool,
        accepts: Option<bool>,
    }
    let mut policies: HashMap<String, Policy> = HashMap::new();
    fn record(
        policies: &mut HashMap<String, Policy>,
        key: String,
        d: &Declaration,
        secret: bool,
        supported: bool,
    ) {
        let mask = raw_secret_results(d, secret);
        let entry = policies.entry(key).or_default();
        entry
            .results
            .resize(entry.results.len().max(mask.len()), false);
        for (into, value) in entry.results.iter_mut().zip(mask) {
            *into |= value;
        }
        if supported {
            entry.accepts = Some(entry.accepts.unwrap_or(true) && accepts_secrets(d));
        }
        entry.protected |= d
            .attributes
            .get("IsProtectedFunction")
            .and_then(|v| v.as_bool())
            == Some(true);
    }
    for target in targets {
        let supported = has_secrets(target);
        for f in &target.functions {
            if f.available
                && let Some(d) = &f.signature
            {
                record(
                    &mut policies,
                    f.name.clone(),
                    d,
                    f.returns_secret,
                    supported,
                );
            }
        }
        for w in &target.widgets {
            for d in &w.methods {
                record(
                    &mut policies,
                    format!("@{}.{}", w.name, d.name),
                    d,
                    false,
                    supported,
                );
            }
        }
    }
    fn apply(d: &mut Declaration, policy: Option<&Policy>) {
        if let Some(policy) = policy {
            if let Some(accepts) = policy.accepts {
                d.attributes
                    .insert("SeluAcceptsSecrets".into(), accepts.into());
            }
            for (i, secret) in policy.results.iter().enumerate() {
                if *secret {
                    d.attributes
                        .insert(format!("SeluSecretResult{i}"), true.into());
                }
            }
            if policy.protected {
                d.attributes
                    .insert("IsProtectedFunction".into(), true.into());
            }
        }
    }
    for f in &mut manifest.functions {
        if let Some(d) = &mut f.signature {
            apply(d, policies.get(&f.name));
        }
    }
    for w in &mut manifest.widgets {
        for d in &mut w.methods {
            apply(d, policies.get(&format!("@{}.{}", w.name, d.name)));
        }
    }
}

pub(crate) fn accepts_secrets(d: &Declaration) -> bool {
    if let Some(value) = d
        .attributes
        .get("SeluAcceptsSecrets")
        .and_then(|v| v.as_bool())
    {
        return value;
    }
    matches!(
        d.attributes.get("SecretArguments").and_then(|v| v.as_str()),
        Some("AllowedWhenTainted" | "AllowedWhenUntainted")
    )
}
pub fn secret_inner(ty: &crate::Type) -> Option<crate::Type> {
    match crate::shape_of(ty) {
        crate::Type::Secret(t) => Some(*t),
        crate::Type::Optional(t) => secret_inner(&t).map(crate::Type::optional),
        _ => None,
    }
}
pub(crate) fn has_secrets(m: &Manifest) -> bool {
    fn declared(d: &Declaration) -> bool {
        d.attributes.iter().any(|(k, v)| {
            (k == "SecretReturns"
                || k.starts_with("SecretWhen")
                || k.starts_with("SecretIn")
                || k.starts_with("SeluSecretResult"))
                && v.as_bool() == Some(true)
                || k == "SecretReturnsForAspect" && v.as_array().is_some_and(|a| !a.is_empty())
        }) || d.returns.iter().any(|r| {
            ["ConditionalSecret", "SecretValue"]
                .iter()
                .any(|k| r.attributes.get(*k).and_then(|v| v.as_bool()) == Some(true))
        })
    }
    m.functions
        .iter()
        .any(|f| f.returns_secret || f.signature.as_ref().is_some_and(declared))
        || m.widgets.iter().any(|w| w.methods.iter().any(declared))
}
pub(crate) fn collapse_secrets(ty: crate::Type, supported: bool) -> crate::Type {
    use crate::Type;
    if supported {
        return ty;
    }
    let collapse = |t| collapse_secrets(t, false);
    match ty {
        Type::Secret(t) => collapse(*t),
        Type::Optional(t) => Type::optional(collapse(*t)),
        Type::Array(t) => Type::Array(Box::new(collapse(*t))),
        Type::Map(k, v) => Type::Map(Box::new(collapse(*k)), Box::new(collapse(*v))),
        Type::Set(t) => Type::Set(Box::new(collapse(*t))),
        Type::WeakMap(k, v) => Type::WeakMap(Box::new(collapse(*k)), Box::new(collapse(*v))),
        Type::Tuple(ts) => Type::Tuple(ts.into_iter().map(collapse).collect()),
        Type::Object(fs) => Type::Object(fs.into_iter().map(|(k, t)| (k, collapse(t))).collect()),
        Type::Function {
            multiple_results,
            unspecified_callback,
            api,
            api_semantics,
            params,
            ret,
            required,
            rest,
        } => Type::Function {
            multiple_results,
            unspecified_callback,
            api,
            api_semantics,
            params: params.into_iter().map(collapse).collect(),
            ret: Box::new(collapse(*ret)),
            required,
            rest: rest.map(|t| Box::new(collapse(*t))),
        },
        t => t,
    }
}
impl crate::Ctx<'_> {
    pub(crate) fn restriction_data_call(
        &mut self,
        callee: &selu_syntax::Expr,
        params: &[crate::Type],
        ret: &crate::Type,
    ) {
        self.types.set(
            callee.id,
            crate::Type::Function {
                multiple_results: false,
                unspecified_callback: false,
                api: Default::default(),
                api_semantics: Default::default(),
                params: params.to_vec(),
                ret: Box::new(ret.clone()),
                required: params.len(),
                rest: None,
            },
        );
    }
    pub(crate) fn readable_call(
        &mut self,
        e: &selu_syntax::Expr,
        callee: &selu_syntax::Expr,
        args: &[selu_syntax::Expr],
    ) -> Option<crate::Type> {
        if !matches!(&callee.kind, selu_syntax::ExprKind::Ident(n) if n == "readable" && self.lookup(n).is_none())
        {
            return None;
        }
        if args.len() != 1 {
            self.out.push(
                crate::err("SELU0603", "readable needs one value", &e.span)
                    .with_help("write `readable(value)`"),
            );
            return Some(crate::Type::Error);
        }
        let ty = self.value(&args[0]);
        Some(crate::Type::optional(secret_inner(&ty).unwrap_or(ty)))
    }
    pub(crate) fn restriction_call_policy(
        &self,
        e: &selu_syntax::Expr,
        ty: &crate::Type,
    ) -> crate::Type {
        use selu_syntax::ExprKind;
        let ExprKind::Call { callee, .. } = &e.kind else {
            return ty.clone();
        };
        let can_defer = self
            .protected_functions
            .iter()
            .any(|candidate| candidate.assignable_to(ty))
            || self.protected_methods
                && matches!(
                    ty,
                    crate::Type::Function {
                        unspecified_callback: true,
                        ..
                    }
                );
        let mut ty = ty.clone();
        if let crate::Type::Function {
            api,
            api_semantics,
            unspecified_callback,
            ..
        } = &mut ty
        {
            let dynamic = match &callee.kind {
                ExprKind::Ident(n) => self.lookup(n).is_some_and(|(_, b)| {
                    !matches!(b.kind, crate::Kind::Const | crate::Kind::Function)
                }),
                ExprKind::Member { object, .. } => {
                    !matches!(self.types.get(object.id), Some(crate::Type::Widget(..)))
                        && callee
                            .dotted_name()
                            .is_none_or(|n| self.manifest.function(&n).is_none())
                }
                _ => true,
            };
            api_semantics.dynamic_protected = (api_semantics.dynamic_protected
                || dynamic && api.0.is_some()
                || *unspecified_callback)
                && can_defer
                && !api_semantics.widget_protected;
            if dynamic || api_semantics.dynamic_protected {
                api_semantics.accepts_secrets = false;
            }
        }
        ty
    }
}

pub(crate) fn plain_results(ret: crate::Type) -> crate::Type {
    use crate::Type;
    match ret {
        Type::Widget(n, mut bases) => {
            bases.push("@selu_plain".into());
            Type::Widget(n, bases)
        }
        Type::Tuple(ts) => Type::Tuple(ts.into_iter().map(plain_results).collect()),
        Type::Optional(t) => Type::optional(plain_results(*t)),
        t => t,
    }
}

fn forget_plain(ty: crate::Type) -> crate::Type {
    use crate::Type;
    match ty {
        Type::Widget(n, mut bases) => {
            bases.retain(|b| b != "@selu_plain");
            Type::Widget(n, bases)
        }
        Type::Tuple(ts) => Type::Tuple(ts.into_iter().map(forget_plain).collect()),
        Type::Object(fs) => {
            Type::Object(fs.into_iter().map(|(n, t)| (n, forget_plain(t))).collect())
        }
        Type::Optional(t) => Type::optional(forget_plain(*t)),
        Type::Secret(t) => Type::secret(forget_plain(*t)),
        Type::Array(t) => Type::Array(Box::new(forget_plain(*t))),
        Type::Map(k, v) => Type::Map(k, Box::new(forget_plain(*v))),
        Type::WeakMap(k, v) => Type::WeakMap(k, Box::new(forget_plain(*v))),
        Type::Function {
            multiple_results,
            unspecified_callback,
            api,
            mut api_semantics,
            params,
            ret,
            required,
            rest,
        } => {
            api_semantics.frame_factory = false;
            api_semantics.plain_result = false;
            api_semantics.dynamic_protected |=
                (api.0.is_some() || unspecified_callback) && !api_semantics.widget_protected;
            api_semantics.accepts_secrets = false;
            Type::Function {
                multiple_results,
                unspecified_callback,
                api,
                api_semantics,
                params,
                ret: Box::new(forget_plain(*ret)),
                required,
                rest,
            }
        }
        t => t,
    }
}
impl crate::Ctx<'_> {
    pub(crate) fn restriction_value_type(
        &self,
        e: &selu_syntax::Expr,
        ty: crate::Type,
    ) -> crate::Type {
        use selu_syntax::ExprKind;
        let mutable = match &e.kind {
            ExprKind::Ident(n) => self.lookup(n).is_some_and(|(_, b)| {
                !matches!(b.kind, crate::Kind::Const | crate::Kind::Function)
            }),
            ExprKind::Index { .. } => true,
            ExprKind::Member { object, .. } => {
                !matches!(self.types.get(object.id), Some(crate::Type::Widget(..)))
                    && e.dotted_name()
                        .is_none_or(|n| self.manifest.function(&n).is_none())
            }
            _ => false,
        };
        if mutable { forget_plain(ty) } else { ty }
    }
}

pub(crate) fn protected_functions(m: &Manifest) -> Vec<crate::Type> {
    m.functions
        .iter()
        .filter_map(|f| {
            f.signature
                .as_ref()
                .filter(|d| {
                    d.attributes
                        .get("IsProtectedFunction")
                        .and_then(|v| v.as_bool())
                        == Some(true)
                })
                .map(|d| crate::api::signature_for(m, &f.name, d))
        })
        .collect()
}

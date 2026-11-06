//! Function checking and contextual typing (package C).
use crate::{Ctx, Kind, Type, err};
use selu_syntax::{Expr, ExprKind, FnBody, Param, Pattern, Stmt, StmtKind};

impl Ctx<'_> {
    pub(crate) fn function_signature(&mut self, params: &[Param], ret: Type) -> Type {
        let ts = self.param_types(params);
        let required = params
            .iter()
            .rposition(|p| !p.rest && p.default.is_none())
            .map_or(0, |i| i + 1);
        let rest = params.iter().zip(&ts).find(|(p, _)| p.rest).map(|(_, t)| {
            Box::new(match self.alias_shape(t.clone()) {
                Type::Array(t) => *t,
                _ => Type::Error,
            })
        });
        Type::Function {
            multiple_results: false,
            unspecified_callback: false,
            api: Default::default(),
            api_semantics: Default::default(),
            params: params
                .iter()
                .zip(ts)
                .filter(|(p, _)| !p.rest)
                .map(|(_, t)| t)
                .collect(),
            required,
            rest,
            ret: Box::new(ret),
        }
    }
    pub(crate) fn params_with_extras(&mut self, p: &Param) {
        let rest_type = if p.rest {
            p.ty.as_ref()
                .map(|t| self.alias_annotation(t))
                .map(|t| self.alias_shape(t))
        } else {
            None
        };
        if p.rest
            && (!matches!(rest_type, Some(Type::Array(t)) if !t.is_optional())
                || p.default.is_some())
        {
            self.out.push(
                err(
                    "SELU0370",
                    "A rest parameter needs an array type with non-null elements and no default.",
                    &p.span,
                )
                .with_help("write `...rest: T[]` as the last parameter"),
            );
        }
    }
    pub(crate) fn function_block(
        &mut self,
        body: &[Stmt],
        params: &[(Param, Type)],
        expected: Option<&Type>,
    ) {
        self.function_expected
            .push(expected.map(|t| self.alias_shape(t.clone())));
        let mut params = params.to_vec();
        // Defaults execute in parameter scope, before the user's body.
        self.scopes.push(Default::default());
        for (p, t) in &params {
            self.declare(&p.name, &p.span, Kind::Param, t.clone());
            if let Some(binding) = self
                .scopes
                .last_mut()
                .and_then(|s| s.names.get_mut(&p.name))
            {
                binding.ready = false;
            }
        }
        let count = params.len();
        for (i, (p, t)) in params.iter_mut().enumerate() {
            if p.rest && i + 1 != count {
                self.out.push(
                    err("SELU0371", "A rest parameter must be last.", &p.span)
                        .with_help("move the rest parameter to the end"),
                );
            }
            if let Some(d) = &p.default {
                let dt = self.value_expected(d, Some(t));
                if *t == Type::Untyped {
                    *t = dt.clone();
                }
                if !dt.assignable_to(t) {
                    self.out.push(
                        err(
                            "SELU0372",
                            "The default value does not match the parameter type.",
                            &d.span,
                        )
                        .with_help(format!("use a value of type `{t}`")),
                    );
                }
            }
            self.finish_decl(&p.name, t.clone());
        }
        self.block(body, &params, Vec::new());
        self.close_scope();
        self.function_expected.pop();
    }
    pub(crate) fn checked_parameter_types(&self, params: &[Param], fixed: Vec<Type>) -> Vec<Type> {
        params
            .iter()
            .filter(|p| !p.rest)
            .zip(fixed)
            .map(|(p, t)| {
                if t == Type::Untyped {
                    p.default
                        .as_ref()
                        .and_then(|d| self.types.get(d.id))
                        .cloned()
                        .unwrap_or(t)
                } else {
                    t
                }
            })
            .collect()
    }
    /// Package B can use this for callbacks: records the expression's type.
    pub(crate) fn value_expected(&mut self, e: &Expr, expected: Option<&Type>) -> Type {
        let shaped = expected.map(|t| self.alias_shape(t.clone()));
        let shaped = shaped.map(|t| {
            if !matches!(e.kind, ExprKind::Null) {
                self.alias_shape(t.non_null())
            } else {
                t
            }
        });
        let expected = shaped.as_ref();
        if let Some(Type::SavedRepair(table)) = expected
            && matches!(e.kind, ExprKind::Object(_) | ExprKind::Array(_))
        {
            return self.value_expected(e, Some(table));
        }
        if let Some(Type::Union(_)) = expected {
            // References and calls already have checked types; a source union
            // need not fit one target arm when it fits the complete union.
            if !matches!(
                e.kind,
                ExprKind::Object(_)
                    | ExprKind::Array(_)
                    | ExprKind::Function { .. }
                    | ExprKind::Str(_)
                    | ExprKind::Number(_)
                    | ExprKind::Bool(_)
                    | ExprKind::Null
                    | ExprKind::Unary { .. }
            ) {
                return self.value(e);
            }
            for member in expected.unwrap().members() {
                if let Some(t) = self.try_value_context(e, &member) {
                    return t;
                }
            }
            // An uncontextualized callback could infer open parameters and
            // appear assignable after all checked alternatives failed.
            self.out.push(err(
                "SELU0313",
                format!(
                    "expression does not fit any member of `{}`",
                    expected.unwrap()
                ),
                &e.span,
            ));
            return self.value(e);
        }
        let literal = match (&e.kind, expected) {
            (ExprKind::Str(s), Some(Type::StringLiteral(_))) => {
                Some(Type::StringLiteral(s.clone()))
            }
            (ExprKind::Number(s), Some(Type::NumberLiteral(_))) => {
                Some(Type::NumberLiteral(s.clone()))
            }
            (ExprKind::Bool(b), Some(Type::BooleanLiteral(_))) => Some(Type::BooleanLiteral(*b)),
            (ExprKind::Unary { op, expr }, Some(Type::NumberLiteral(_))) if op == "-" => {
                if let ExprKind::Number(s) = &expr.kind {
                    Some(Type::NumberLiteral(format!("-{s}")))
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(t) = literal {
            self.types.set(e.id, t.clone());
            return t;
        }
        if let (ExprKind::Object(values), Some(Type::Object(fields))) = (&e.kind, expected) {
            let mut checked: std::collections::BTreeMap<_, _> = values
                .iter()
                .map(|v| {
                    (v.key.clone(), {
                        let actual = self.value_expected(&v.value, fields.get(&v.key));
                        if let Some(want) = fields.get(&v.key)
                            && actual.assignable_to(want)
                            && !crate::saved::contains_class(&actual)
                        {
                            want.clone()
                        } else {
                            actual
                        }
                    })
                })
                .collect();
            // Only a fresh literal can introduce absent writable optional
            // fields. An existing alias may hide an incompatible field.
            for (name, ty) in fields {
                if ty.is_optional() {
                    checked.entry(name.clone()).or_insert_with(|| ty.clone());
                }
            }
            let t = Type::Object(checked);
            self.types.set(e.id, t.clone());
            return t;
        }
        if let (ExprKind::Array(items), Some(Type::Array(element))) = (&e.kind, expected) {
            for item in items {
                let got = self.value_expected(item, Some(element));
                if !got.assignable_to(element) {
                    self.out.push(err(
                        "SELU0351",
                        format!("array element {got} does not fit {element}"),
                        &item.span,
                    ));
                }
            }
            let t = Type::Array(element.clone());
            self.types.set(e.id, t.clone());
            return t;
        }
        if let ExprKind::Array(items) = &e.kind
            && items.is_empty()
            && let Some(Type::Array(_)) = expected
        {
            let t = expected.unwrap().clone();
            self.types.set(e.id, t.clone());
            return t;
        }
        if matches!(e.kind, ExprKind::Function { .. }) {
            let t = self.contextual_function(e, expected);
            self.types.set(e.id, t.clone());
            t
        } else {
            self.value(e)
        }
    }
    fn try_value_context(&mut self, e: &Expr, expected: &Type) -> Option<Type> {
        let mut diagnostics = Vec::new();
        let mut trial = Ctx {
            manifest: self.manifest,
            flow_info: self.flow_info.clone(),
            next_binding: self.next_binding,
            out: &mut diagnostics,
            binding_statements: self.binding_statements.clone(),
            secret_capable: self.secret_capable,
            protected_functions: self.protected_functions.clone(),
            protected_methods: self.protected_methods,
            options: self.options,
            module: self.module.clone(),
            aliases: self.aliases.clone(),
            classes: self.classes.clone(),
            loops: self.loops.clone(),
            availability: self.availability.clone(),
            scopes: self.scopes.clone(),
            fn_depth: self.fn_depth,
            returns: self.returns.clone(),
            function_expected: self.function_expected.clone(),
            types: self.types.clone(),
        };
        let t = trial.value_expected(e, Some(expected));
        if !t.assignable_to(expected)
            || trial
                .out
                .iter()
                .any(|d| d.severity == selu_diagnostics::Severity::Error)
        {
            return None;
        }
        self.flow_info = trial.flow_info;
        self.next_binding = trial.next_binding;
        self.binding_statements = trial.binding_statements;
        self.secret_capable = trial.secret_capable;
        self.protected_functions = trial.protected_functions;
        self.protected_methods = trial.protected_methods;
        self.options = trial.options;
        self.module = trial.module;
        self.aliases = trial.aliases;
        self.classes = trial.classes;
        self.loops = trial.loops;
        self.availability = trial.availability;
        self.scopes = trial.scopes;
        self.fn_depth = trial.fn_depth;
        self.returns = trial.returns;
        self.function_expected = trial.function_expected;
        self.types = trial.types;
        self.out.extend(diagnostics);
        Some(t)
    }
    pub(crate) fn function_arguments(&mut self, args: &[Expr], callee: &Type) -> Vec<Type> {
        let first = if matches!(callee, Type::Function { api_semantics, .. } if api_semantics.table_operation.is_some())
        {
            args.first().map(|e| self.value(e))
        } else {
            None
        };
        let specialized = self.script_signature(args, callee);
        let specialized = self.table_signature(&specialized, first.as_ref(), None, args.len());
        let callee = &specialized;
        args.iter()
            .enumerate()
            .map(|(i, e)| {
                if i == 0
                    && let Some(first) = &first
                {
                    return first.clone();
                }
                let expected = match callee {
                    Type::Function { params, rest, .. } => params.get(i).or(rest.as_deref()),
                    _ => None,
                };
                let actual = self.value_expected(e, expected);
                if matches!(callee, Type::Untyped) && actual.contains_opaque() {
                    self.out.push(
                        err(
                            "SELU0335",
                            "an untyped call cannot consume an opaque native payload",
                            &e.span,
                        )
                        .with_help(
                            "use a matching sourced native slot or narrow the payload with typeof",
                        ),
                    );
                }
                actual
            })
            .collect()
    }
    pub(crate) fn function_expr(&mut self, e: &Expr) -> Type {
        self.contextual_function(e, None)
    }
    fn contextual_function(&mut self, e: &Expr, expected: Option<&Type>) -> Type {
        let ExprKind::Function {
            params, ret, body, ..
        } = &e.kind
        else {
            unreachable!()
        };
        let mut ps = params.clone();
        let mut ts = self.param_types(&ps);
        if let Some(Type::Function {
            params: context,
            rest,
            api_semantics,
            ..
        }) = expected
        {
            for (i, (p, t)) in ps.iter_mut().zip(&mut ts).enumerate() {
                if p.ty.is_none() {
                    *t = context
                        .get(i)
                        .or(rest.as_deref())
                        .cloned()
                        .unwrap_or_else(|| {
                            if api_semantics.native_callback && p.default.is_none() && !p.rest {
                                // The finite native signature supplies no value
                                // here. Do not manufacture an unchecked payload.
                                Type::Null
                            } else {
                                Type::Untyped
                            }
                        });
                }
            }
        }
        let written = self.annotation(ret).or_else(|| match expected {
            Some(Type::Function { ret, .. }) if **ret != Type::Void => Some((**ret).clone()),
            _ => None,
        });
        let body = match body {
            FnBody::Block(b) => b.clone(),
            FnBody::Expr(v) => vec![Stmt {
                kind: StmtKind::Return(Some(v.clone())),
                span: v.span.clone(),
            }],
        };
        let saved_this = if matches!(
            e.kind,
            ExprKind::Function {
                is_arrow: false,
                ..
            }
        ) {
            self.classes.receiver.take()
        } else {
            None
        };
        let result = self.function_body(&ps, ts.clone(), written, &body);
        if matches!(
            e.kind,
            ExprKind::Function {
                is_arrow: false,
                ..
            }
        ) {
            self.classes.receiver = saved_this;
        }
        let required = ps
            .iter()
            .rposition(|p| !p.rest && p.default.is_none())
            .map_or(0, |i| i + 1);
        let rest = ps.iter().zip(&ts).find(|(p, _)| p.rest).map(|(_, t)| {
            Box::new(match self.alias_shape(t.clone()) {
                Type::Array(t) => *t,
                _ => Type::Error,
            })
        });
        Type::Function {
            multiple_results: false,
            unspecified_callback: false,
            api: Default::default(),
            api_semantics: Default::default(),
            params: self.checked_parameter_types(
                &ps,
                ps.iter()
                    .zip(ts)
                    .filter(|(p, _)| !p.rest)
                    .map(|(_, t)| t)
                    .collect(),
            ),
            required,
            rest,
            ret: Box::new(result),
        }
    }
    pub(crate) fn function_return(&mut self, e: &Expr) -> Type {
        if matches!(e.kind, ExprKind::Call { .. })
            && matches!(self.function_expected.last(), Some(Some(Type::Tuple(_))))
        {
            return self.expr(e);
        }
        if let ExprKind::Array(items) = &e.kind
            && let Some(Some(Type::Tuple(want))) = self.function_expected.last().cloned()
        {
            let have = items
                .iter()
                .enumerate()
                .map(|(i, e)| self.value_expected(e, want.get(i)))
                .collect();
            let t = Type::Tuple(have);
            self.types.set(e.id, t.clone());
            return t;
        }
        let expected = self.function_expected.last().cloned().flatten();
        self.value_expected(e, expected.as_ref())
    }
    pub(crate) fn destructure(&mut self, s: &Stmt) {
        let StmtKind::Destructure { pattern, init, .. } = &s.kind else {
            unreachable!()
        };
        let t = if matches!(init.kind, ExprKind::Call { .. }) {
            self.expr(init)
        } else {
            self.value(init)
        };
        if crate::game_iterator(&t).is_some()
            || matches!(&t, Type::Tuple(ts) if ts.iter().any(|t| crate::game_iterator(t).is_some()))
        {
            self.check_tuple_value(init, t);
            return;
        }
        let open_tail = if let ExprKind::Call { callee, .. } = &init.kind {
            matches!(self.types.get(callee.id), Some(Type::Function { api_semantics, .. }) if api_semantics.untyped_return_tail)
        } else {
            false
        };
        let typed_tail = if let ExprKind::Call { callee, .. } = &init.kind {
            match self.types.get(callee.id) {
                Some(Type::Function { api_semantics, .. }) => {
                    api_semantics.typed_return_tail.as_deref().cloned()
                }
                _ => None,
            }
        } else {
            None
        };
        let t = crate::shape_of(&t);
        match pattern {
            Pattern::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    if let Some((n, span)) = item {
                        let ty = match &t {
                            Type::Tuple(ts) => ts.get(i).cloned().unwrap_or_else(|| {
                                if let Some(tail) = &typed_tail {
                                    return tail.clone();
                                }
                                if open_tail {
                                    return Type::Untyped;
                                }
                                self.out.push(
                                    err(
                                        "SELU0373",
                                        "This tuple has no element at this position.",
                                        span,
                                    )
                                    .with_help("remove the extra binding"),
                                );
                                Type::Error
                            }),
                            Type::Array(t) => (**t).clone(),
                            Type::Untyped | Type::Error => Type::Untyped,
                            _ => {
                                self.out.push(
                                    err(
                                        "SELU0374",
                                        "Array destructuring needs an array, tuple or game call.",
                                        &init.span,
                                    )
                                    .with_help("use an array or tuple value"),
                                );
                                Type::Error
                            }
                        };
                        self.finish_decl(n, ty);
                    }
                }
            }
            Pattern::Object(items) => {
                for (field, n, _, field_span) in items {
                    let ty = match &t {
                        Type::Object(fs) => fs.get(field).cloned().unwrap_or_else(|| {
                            self.out.push(
                                err(
                                    "SELU0375",
                                    format!("This object has no field `{field}`."),
                                    field_span,
                                )
                                .with_help("use an existing field name"),
                            );
                            Type::Error
                        }),
                        Type::Untyped | Type::Error => Type::Untyped,
                        _ => {
                            self.out.push(
                                err(
                                    "SELU0376",
                                    "Object destructuring needs an object.",
                                    &init.span,
                                )
                                .with_help("use an object value"),
                            );
                            Type::Error
                        }
                    };
                    self.finish_decl(n, ty);
                }
            }
        }
    }
}

impl Ctx<'_> {
    pub(crate) fn check_tuple_value(&mut self, e: &Expr, t: Type) -> Type {
        let original = t.clone();
        let t = crate::shape_of(&t);
        if crate::game_iterator(&t).is_some()
            || matches!(&t, Type::Tuple(ts) if ts.iter().any(|t| crate::game_iterator(t).is_some()))
        {
            self.out.push(err(
                "SELU0357",
                "an API iterator triple must be called directly in a for ... of loop",
                &e.span,
            ));
            return Type::Error;
        }
        if matches!(e.kind, ExprKind::Call { .. }) {
            self.types.mark_single_value(e.id);
        }
        if let ExprKind::Call { callee, .. } = &e.kind
            && matches!(
                self.types.get(callee.id),
                Some(Type::Function {
                    multiple_results: true,
                    ..
                })
            )
            && let Type::Tuple(ts) = &t
        {
            let first = ts.first().cloned().unwrap_or(Type::Null);
            self.types.set(e.id, first.clone());
            return first;
        }

        if matches!(e.kind, ExprKind::Call { .. }) && matches!(t, Type::Tuple(_)) {
            self.out.push(
                err(
                    "SELU0377",
                    "A tuple-returning call must be destructured before using its values.",
                    &e.span,
                )
                .with_fix("const [a, b] = f();"),
            );
            Type::Error
        } else {
            original
        }
    }
}

pub(crate) fn body_param_types(params: &[Param], fixed: &[Type], rest: Option<&Type>) -> Vec<Type> {
    params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if p.rest {
                Type::Array(Box::new(rest.cloned().unwrap_or(Type::Error)))
            } else {
                fixed.get(i).cloned().unwrap_or(Type::Untyped)
            }
        })
        .collect()
}

impl Ctx<'_> {
    pub(crate) fn function_member(&mut self, t: &Type, name: &str, object: &Expr) -> Type {
        let shaped = crate::shape_of(t);
        let t = &shaped;
        if matches!(t, Type::Widget(..)) {
            self.types.mark_method_call(object.id);
        }

        if let Type::Object(fs) = t
            && let Some(ty) = fs.get(name)
        {
            return ty.clone();
        }
        self.member(t, name, object)
    }
}

impl Ctx<'_> {
    pub(crate) fn tuple_condition(&mut self, e: &Expr) {
        self.out.push(
            err(
                "SELU0377",
                "A tuple cannot be used as a condition.",
                &e.span,
            )
            .with_fix("const [a, b] = f();"),
        );
    }
}

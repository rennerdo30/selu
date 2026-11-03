//! Function checking and contextual typing (package C).
use crate::{Ctx, Kind, Type, err, types};
use selu_syntax::{Expr, ExprKind, FnBody, Param, Pattern, Stmt, StmtKind};

impl Ctx<'_> {
    pub(crate) fn function_signature(&mut self, params: &[Param], ret: Type) -> Type {
        let ts = self.param_types(params);
        let required = params
            .iter()
            .rposition(|p| !p.rest && p.default.is_none())
            .map_or(0, |i| i + 1);
        let rest = params.iter().zip(&ts).find(|(p, _)| p.rest).map(|(_, t)| {
            Box::new(match t {
                Type::Array(t) => (**t).clone(),
                _ => Type::Error,
            })
        });
        Type::Function {
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
        if p.rest
            && (!matches!(
                p.ty.as_ref().map(types::resolve_quiet),
                Some(Type::Array(t)) if !t.is_optional()
            ) || p.default.is_some())
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
        self.function_expected.push(expected.cloned());
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
        self.scopes.pop();
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
        if matches!(e.kind, ExprKind::Function { .. }) {
            let t = self.contextual_function(e, expected);
            self.types.set(e.id, t.clone());
            t
        } else {
            self.value(e)
        }
    }
    pub(crate) fn function_arguments(&mut self, args: &[Expr], callee: &Type) -> Vec<Type> {
        args.iter()
            .enumerate()
            .map(|(i, e)| {
                let expected = match callee {
                    Type::Function { params, rest, .. } => params.get(i).or(rest.as_deref()),
                    _ => None,
                };
                self.value_expected(e, expected)
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
            ..
        }) = expected
        {
            for (i, (p, t)) in ps.iter_mut().zip(&mut ts).enumerate() {
                if p.ty.is_none() {
                    *t = context
                        .get(i)
                        .or(rest.as_deref())
                        .cloned()
                        .unwrap_or(Type::Untyped);
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
        let result = self.function_body(&ps, ts.clone(), written, &body);
        let required = ps
            .iter()
            .rposition(|p| !p.rest && p.default.is_none())
            .map_or(0, |i| i + 1);
        let rest = ps.iter().zip(&ts).find(|(p, _)| p.rest).map(|(_, t)| {
            Box::new(match t {
                Type::Array(t) => (**t).clone(),
                _ => Type::Error,
            })
        });
        Type::Function {
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
        self.value(e)
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
        match pattern {
            Pattern::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    if let Some((n, span)) = item {
                        let ty = match &t {
                            Type::Tuple(ts) => ts.get(i).cloned().unwrap_or_else(|| {
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
            t
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

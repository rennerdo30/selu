//! Data expressions and their type rules.
use crate::{Ctx, Type, err, types};
use selu_syntax::{Expr, ExprKind};

impl Ctx<'_> {
    pub(crate) fn data_expr(&mut self, e: &Expr) -> Type {
        match &e.kind {
            ExprKind::Array(items) => {
                if items.is_empty() {
                    self.out.push(
                        err("SELU0351", "an empty array needs a written type", &e.span)
                            .with_help("write `const xs: number[] = [];`"),
                    );
                }
                let mut ty = Type::Untyped;
                for (i, item) in items.iter().enumerate() {
                    let t = self.value(item);
                    if t.is_optional() {
                        self.out.push(
                            err("SELU0350", "array elements cannot be null", &item.span)
                                .with_help("use a Map for gaps"),
                        );
                    }
                    if i == 0 {
                        ty = t;
                    } else if !t.same(&ty) {
                        self.out.push(
                            err(
                                "SELU0351",
                                "array elements must have the same type",
                                &item.span,
                            )
                            .with_help("use an object for different kinds of values"),
                        );
                    }
                }
                Type::Array(Box::new(ty))
            }
            ExprKind::Object(fields) => Type::Object(
                fields
                    .iter()
                    .map(|f| (f.key.clone(), self.value(&f.value)))
                    .collect(),
            ),
            ExprKind::Index { object, index } => {
                let t = crate::shape_of(&self.value(object));
                let i = self.value(index).operand_type();
                if i != Type::Number && !i.is_open() {
                    self.out.push(
                        err("SELU0352", "an array index must be a number", &index.span)
                            .with_help("use a 0-based number index"),
                    );
                }
                match t {
                    Type::Array(t) => *t,
                    Type::Untyped | Type::Error => Type::Untyped,
                    _ => {
                        self.out.push(
                            err("SELU0352", "only arrays can be indexed", &object.span)
                                .with_help("use get(key) for a Map"),
                        );
                        Type::Error
                    }
                }
            }
            ExprKind::OptionalMember { object, name } => {
                let t = self.value(object).non_null();
                if t == Type::Null {
                    Type::Null
                } else {
                    Type::optional(self.member(&t, name, e))
                }
            }
            ExprKind::Template { exprs, .. } => {
                for x in exprs {
                    let t = crate::shape_of(&self.value(x));
                    if !matches!(
                        t,
                        Type::Number | Type::String | Type::Boolean | Type::Error | Type::Untyped
                    ) {
                        self.out.push(
                            err(
                                "SELU0353",
                                "template interpolation needs a non-null string, number or boolean",
                                &x.span,
                            )
                            .with_help("use `?? \"\"` or compare the value first"),
                        );
                    }
                }
                Type::String
            }
            ExprKind::New {
                class,
                type_args,
                args,
                ..
            } => {
                if class == "Error" {
                    return self.error_new(e, type_args, args);
                }
                if !matches!(class.as_str(), "Map" | "Set" | "WeakMap") {
                    return self.class_new(e, class, type_args, args);
                }
                for a in args {
                    self.value(a);
                }
                let ts: Vec<_> = type_args.iter().map(|t| self.data_annotation(t)).collect();
                if !args.is_empty()
                    || !matches!(
                        (class.as_str(), ts.len()),
                        ("Set", 1) | ("Map" | "WeakMap", 2)
                    )
                {
                    self.out.push(
                        err(
                            "SELU0354",
                            "collection construction needs type arguments and no value arguments",
                            &e.span,
                        )
                        .with_help(
                            "write `new Map<K, V>()`, `new Set<T>()` or `new WeakMap<K, V>()`",
                        ),
                    );
                    return Type::Error;
                }
                if ts.iter().any(Type::is_optional) {
                    self.out.push(
                        err(
                            "SELU0350",
                            "collection keys and values cannot be null",
                            &e.span,
                        )
                        .with_help("delete entries instead of storing null"),
                    );
                }
                match class.as_str() {
                    "Set" => Type::Set(Box::new(ts[0].clone())),
                    "Map" => Type::Map(Box::new(ts[0].clone()), Box::new(ts[1].clone())),
                    _ => Type::WeakMap(Box::new(ts[0].clone()), Box::new(ts[1].clone())),
                }
            }
            _ => unreachable!(),
        }
    }
}

impl Ctx<'_> {
    pub(crate) fn data_initializer(&mut self, e: &Expr, expected: Option<&Type>) -> Type {
        let shaped = expected.map(|t| self.alias_shape(t.clone()));
        let expected = shaped.as_ref();
        if matches!(&e.kind, ExprKind::Array(v) if v.is_empty()) {
            if let Some(Type::Array(t)) = expected {
                let ty = Type::Array(t.clone());
                self.types.set(e.id, ty.clone());
                return ty;
            }
            self.out.push(
                err(
                    "SELU0351",
                    "an empty array needs a written array type",
                    &e.span,
                )
                .with_help("write `const xs: number[] = [];`"),
            );
            return Type::Error;
        }
        self.value_expected(e, expected)
    }

    pub(crate) fn write_contexts(&mut self, target: &Expr) -> Vec<Type> {
        match &target.kind {
            ExprKind::Ident(n) => self
                .lookup(n)
                .map(|(_, b)| vec![b.ty.clone()])
                .unwrap_or_default(),
            ExprKind::Member { object, name } => {
                let t = self.value(object);
                t.members()
                    .iter()
                    .map(|t| self.member(t, name, object))
                    .collect()
            }
            _ => vec![self.expr(target)],
        }
    }
    pub(crate) fn data_write(&mut self, target: &Expr, value: &Type) {
        if let ExprKind::Member { object, name } = &target.kind {
            let t = self.value(object);
            if name == "length"
                && t.members()
                    .iter()
                    .any(|t| matches!(t.operand_type(), Type::Array(_) | Type::String))
            {
                self.out
                    .push(err("SELU0355", "length is read-only", &target.span));
                return;
            }
        }
        for ty in self.write_contexts(target) {
            self.types.set(target.id, ty.clone());
            if !value.assignable_to(&ty) {
                self.out.push(
                    err(
                        "SELU0355",
                        format!("cannot store `{value}` in `{ty}`"),
                        &target.span,
                    )
                    .with_help("the value must fit every possible receiver arm"),
                );
            }
        }
    }

    pub(crate) fn data_member(&mut self, t: &Type, field: &str, e: &Expr) -> Option<Type> {
        match t {
            Type::Object(fields) => Some(fields.get(field).cloned().unwrap_or_else(|| {
                self.out.push(
                    err(
                        "SELU0330",
                        format!("this object has no field `{field}`"),
                        &e.span,
                    )
                    .with_help("check the field spelling or add it to the object type"),
                );
                Type::Error
            })),
            Type::Array(_) | Type::String | Type::StringLiteral(_) if field == "length" => {
                Some(Type::Number)
            }
            _ => None,
        }
    }

    pub(crate) fn data_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> Option<Type> {
        let ExprKind::Member { object, name } = &callee.kind else {
            return None;
        };
        let math =
            matches!(&object.kind, ExprKind::Ident(n) if n == "Math" && self.lookup(n).is_none());
        let t = if math {
            Type::Number
        } else {
            self.data_receiver(object)
        };
        if t == Type::String
            && let Some(t) = self.pattern_call(e, callee, name, args)
        {
            return Some(t);
        }
        if let Some(t) = self.pattern_receiver(&t, callee, name, args) {
            return Some(t);
        }
        let (params, ret) = if math {
            match name.as_str() {
                "floor" | "ceil" | "round" | "abs" => (vec![Type::Number], Type::Number),
                "clamp" => (vec![Type::Number; 3], Type::Number),
                "min" | "max" if !args.is_empty() => (vec![Type::Number; args.len()], Type::Number),
                "random" => (vec![], Type::Number),
                _ => {
                    self.unsupported("this Math operation", &e.span);
                    return Some(Type::Error);
                }
            }
        } else {
            match &t {
                Type::Array(item) => match name.as_str() {
                    "clear" => (vec![], Type::Void),
                    "push" => (vec![(**item).clone()], Type::Void),
                    "pop" => (vec![], Type::optional((**item).clone())),
                    "insert" => (vec![Type::Number, (**item).clone()], Type::Void),
                    "removeAt" => (vec![Type::Number], Type::optional((**item).clone())),
                    "indexOf" => (vec![(**item).clone()], Type::Number),
                    "includes" => (vec![(**item).clone()], Type::Boolean),
                    "slice" => (vec![Type::Number; args.len()], t.clone()),
                    "join" => (
                        if args.is_empty() {
                            vec![]
                        } else {
                            vec![Type::String]
                        },
                        Type::String,
                    ),
                    "sort" if args.is_empty() => (vec![], Type::Void),
                    "forEach" | "map" | "filter" | "find" | "some" | "every" | "reduce"
                    | "sort" => {
                        return Some(self.data_callback(e, name, item, args));
                    }
                    _ => return Some(self.unsupported("this array method", &e.span)),
                },
                Type::String => match name.as_str() {
                    "upper" | "lower" | "trim" => (vec![], Type::String),
                    "slice" => (vec![Type::Number; args.len()], Type::String),
                    "includes" | "startsWith" | "endsWith" => (vec![Type::String], Type::Boolean),
                    "split" => (vec![Type::String], Type::Array(Box::new(Type::String))),
                    _ => return Some(self.unsupported("this string method", &e.span)),
                },
                Type::Map(k, v) | Type::WeakMap(k, v) => match name.as_str() {
                    "get" => (vec![(**k).clone()], Type::optional((**v).clone())),
                    "set" => (vec![(**k).clone(), (**v).clone()], Type::Void),
                    "has" | "delete" => (vec![(**k).clone()], Type::Boolean),
                    "count" => (vec![], Type::Number),
                    "clear" => (vec![], Type::Void),
                    _ => return Some(self.unsupported("this map method", &e.span)),
                },
                Type::Set(item) => match name.as_str() {
                    "add" => (vec![(**item).clone()], Type::Void),
                    "has" | "delete" => (vec![(**item).clone()], Type::Boolean),
                    "count" => (vec![], Type::Number),
                    "clear" => (vec![], Type::Void),
                    _ => return Some(self.unsupported("this set method", &e.span)),
                },
                _ => return None,
            }
        };
        if let Type::Array(item) = &t {
            let allowed = match name.as_str() {
                "sort" => {
                    !args.is_empty() || matches!(crate::shape_of(item), Type::Number | Type::String)
                }
                "join" => matches!(
                    crate::shape_of(item),
                    Type::Number | Type::String | Type::Boolean
                ),
                _ => true,
            };
            if !allowed {
                self.out.push(
                    err(
                        "SELU0355",
                        "this operation needs scalar array elements",
                        &e.span,
                    )
                    .with_help(
                        "use numbers or strings for sort, or numbers, strings or booleans for join",
                    ),
                );
            }
        }
        if matches!(name.as_str(), "slice") && (args.is_empty() || args.len() > 2) {
            self.out.push(
                err("SELU0356", "slice needs one or two indices", &e.span)
                    .with_help("write slice(from, to)"),
            );
        }
        if params.len() != args.len() {
            self.out.push(
                err(
                    "SELU0356",
                    format!("this method needs {} arguments", params.len()),
                    &e.span,
                )
                .with_help("check the method arguments"),
            );
        }
        self.restriction_data_call(callee, &params, &ret);
        for (arg, want) in args.iter().zip(&params) {
            let got = self.value(arg);
            if !got.assignable_to(want) {
                self.out.push(
                    err(
                        "SELU0355",
                        format!("expected `{want}`, got `{got}`"),
                        &arg.span,
                    )
                    .with_help("pass a value of the expected type"),
                );
            }
        }
        Some(ret)
    }

    pub(crate) fn for_of(&mut self, s: &selu_syntax::Stmt) {
        use selu_syntax::{ForBinding, Pattern, StmtKind};
        let StmtKind::ForOf {
            is_const,
            binding,
            iter,
            body,
        } = &s.kind
        else {
            unreachable!()
        };
        let iter_type = self.expr(iter);
        let mut t = crate::shape_of(&iter_type);
        if let Type::Tuple(items) = &t
            && items.len() == 1
            && crate::game_iterator(&items[0]).is_some()
        {
            t = items[0].clone();
            self.types.set(iter.id, t.clone());
        } else if crate::game_iterator(&t).is_none() {
            t = crate::shape_of(&self.check_tuple_value(iter, iter_type));
        }
        if self.pattern_for_of(s, &t) {
            return;
        }
        let iterator = crate::game_iterator(&t);
        if iterator.is_some() && !matches!(iter.kind, ExprKind::Call { .. }) {
            self.out.push(err(
                "SELU0357",
                "an API iterator triple must be called directly in the loop",
                &iter.span,
            ));
            return;
        }
        let bindings = match (&t, binding) {
            (_, ForBinding::Name(n, sp)) if iterator.is_some() => vec![(
                n.clone(),
                sp.clone(),
                iterator.unwrap().first().cloned().unwrap_or(Type::Untyped),
            )],
            (_, ForBinding::Pattern(Pattern::Array(items))) if iterator.is_some() => items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    item.as_ref().map(|(n, sp)| {
                        (
                            n.clone(),
                            sp.clone(),
                            iterator.unwrap().get(i).cloned().unwrap_or(Type::Untyped),
                        )
                    })
                })
                .collect(),
            (Type::Array(t) | Type::Set(t), ForBinding::Name(n, sp)) => {
                vec![(n.clone(), sp.clone(), (**t).clone())]
            }
            (Type::Map(k, v) | Type::WeakMap(k, v), ForBinding::Pattern(Pattern::Array(items)))
                if items.len() == 2 =>
            {
                items
                    .iter()
                    .zip([&**k, &**v])
                    .filter_map(|(i, t)| i.as_ref().map(|(n, s)| (n.clone(), s.clone(), t.clone())))
                    .collect()
            }
            _ => {
                self.out.push(
                    err(
                        "SELU0357",
                        "iteration needs an array or set value, or a map key/value pair",
                        &iter.span,
                    )
                    .with_help("use `x of xs` or `[key, value] of map`"),
                );
                return;
            }
        };
        self.scopes.push(crate::Scope::default());
        for (n, sp, t) in bindings {
            self.declare(
                &n,
                &sp,
                if *is_const {
                    crate::Kind::Const
                } else {
                    crate::Kind::Let
                },
                t.clone(),
            );
            self.finish_decl(&n, t);
        }
        self.loops.push(crate::loops::LoopCtx);
        self.block(body, &[], vec![]);
        self.loops.pop();
        self.scopes.pop();
    }
}

impl Ctx<'_> {
    pub(crate) fn data_receiver(&mut self, e: &Expr) -> Type {
        let t = self
            .types
            .get(e.id)
            .cloned()
            .unwrap_or_else(|| self.value(e));
        t.operand_type()
    }

    pub(crate) fn data_annotation(&mut self, t: &selu_syntax::TypeExpr) -> Type {
        fn validate(t: &selu_syntax::TypeExpr, out: &mut Vec<selu_diagnostics::Diagnostic>) {
            use selu_syntax::TypeExprKind as K;
            match &t.kind {
                K::Array(inner) => {
                    if types::resolve_quiet(inner).is_optional() {
                        out.push(
                            err("SELU0350", "array elements cannot be null", &inner.span)
                                .with_help("use a Map for gaps"),
                        );
                    }
                    validate(inner, out);
                }
                K::Optional(inner) => validate(inner, out),
                K::Union(items) | K::Tuple(items) | K::Named { args: items, .. } => {
                    for t in items {
                        validate(t, out);
                    }
                }
                K::StringLiteral(_) | K::NumberLiteral(_) | K::BooleanLiteral(_) => {}
                K::Object(fields) => {
                    for f in fields {
                        validate(&f.ty, out);
                    }
                }
                K::Function { params, ret, .. } => {
                    for t in params {
                        validate(t, out);
                    }
                    validate(ret, out);
                }
            }
        }
        validate(t, self.out);
        self.alias_annotation(t)
    }
}

impl Ctx<'_> {
    fn data_callback(&mut self, e: &Expr, method: &str, item: &Type, args: &[Expr]) -> Type {
        let reduce = method == "reduce";
        if args.is_empty() || args.len() > if reduce { 2 } else { 1 } {
            self.out.push(
                err(
                    "SELU0356",
                    "this method needs a callback and reduce allows one initial value",
                    &e.span,
                )
                .with_help("pass a function value"),
            );
        }
        let acc = args
            .get(1)
            .map(|a| self.value(a))
            .unwrap_or_else(|| item.clone());
        let input = if reduce {
            vec![
                acc.clone(),
                item.clone(),
                Type::Number,
                Type::Array(Box::new(item.clone())),
            ]
        } else if method == "sort" {
            vec![item.clone(), item.clone()]
        } else {
            vec![
                item.clone(),
                Type::Number,
                Type::Array(Box::new(item.clone())),
            ]
        };
        let want_ret = match method {
            "filter" | "find" | "some" | "every" => Some(Type::Boolean),
            "sort" => Some(Type::Number),
            "reduce" => Some(acc.clone()),
            _ => None,
        };
        let expected = Type::Function {
            multiple_results: false,
            unspecified_callback: false,
            api: Default::default(),
            api_semantics: Default::default(),
            params: input.clone(),
            required: input.len(),
            rest: None,
            ret: Box::new(want_ret.clone().unwrap_or(Type::Void)),
        };
        let callback = args
            .first()
            .map(|a| self.value_expected(a, Some(&expected)))
            .map(|t| crate::shape_of(&t));
        for arg in args.iter().skip(2) {
            self.value(arg);
        }
        if let Some(callback) = callback.as_ref()
            && !callback.result_convention_matches(&expected)
        {
            self.out.push(crate::api::mismatch_help(
                err(
                    "SELU0485",
                    "Callback result convention does not match.",
                    &args[0].span,
                ),
                callback,
                &expected,
            ));
            return Type::Error;
        }
        let ret = match callback.as_ref() {
            Some(Type::Function {
                params,
                ret,
                required,
                rest,
                ..
            }) => {
                // TypeScript permits callbacks to ignore trailing arguments.
                let compatible = *required <= input.len()
                    && input.iter().enumerate().all(|(i, given)| {
                        params
                            .get(i)
                            .or(rest.as_deref())
                            .is_none_or(|accepts| given.assignable_to(accepts))
                    })
                    && want_ret.as_ref().is_none_or(|want| ret.assignable_to(want));
                if !compatible {
                    self.out.push(
                        err(
                            "SELU0355",
                            "callback parameters or result do not match this array method",
                            &args[0].span,
                        )
                        .with_help(format!(
                            "the callback receives {}",
                            input
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                    );
                }
                (**ret).clone()
            }
            Some(Type::Error) => Type::Error,
            Some(Type::Untyped) => Type::Untyped,
            Some(_) => {
                self.out.push(
                    err(
                        "SELU0355",
                        "this array method needs a function value",
                        &args[0].span,
                    )
                    .with_help("pass a declared function or a function-typed value"),
                );
                Type::Error
            }
            None => Type::Error,
        };
        match method {
            "map" => {
                if ret.is_optional() || ret == Type::Void {
                    self.out.push(
                        err("SELU0350", "array elements cannot be null", &args[0].span)
                            .with_help("use a Map for gaps"),
                    );
                }
                Type::Array(Box::new(ret))
            }
            "filter" => Type::Array(Box::new(item.clone())),
            "find" => Type::optional(item.clone()),
            "some" | "every" => Type::Boolean,
            "reduce" => acc,
            _ => Type::Void,
        }
    }
}

use crate::{Ctx, Type, err};
use selu_syntax::{Expr, ExprKind};
impl Ctx<'_> {
    fn wow_name(&self, tag: &Expr) -> Option<String> {
        let n = tag.dotted_name()?;
        if self.lookup(n.split('.').next()?).is_some() {
            None
        } else {
            Some(n)
        }
    }
    pub(crate) fn wow_join(&mut self, e: &Expr, left: &Expr, right: &Expr) {
        let is_l = |v: &Expr| matches!(&v.kind, ExprKind::TaggedTemplate { tag, .. } if self.wow_name(tag).as_deref() == Some("L"));
        if is_l(left) && is_l(right) {
            self.out.push(
                selu_diagnostics::Diagnostic::warning(
                    "SELU0444",
                    "Localise the whole sentence together.",
                )
                .with_span(e.span.clone())
                .with_help("use one L template"),
            );
        }
    }
    pub(crate) fn tagged_template(&mut self, e: &Expr) -> Type {
        let ExprKind::TaggedTemplate { tag, exprs, .. } = &e.kind else {
            unreachable!()
        };
        let n = self.wow_name(tag);
        if n.as_deref() == Some("luaPattern") {
            return self.pattern_tag(e);
        }
        let valid = matches!(
            n.as_deref(),
            Some("L" | "log.debug" | "log.info" | "log.warn" | "log.error" | "log.warnOnce")
        );
        if !valid {
            return self.unsupported("this template tag", &tag.span);
        }
        if exprs.len() > 64 {
            self.out.push(
                err(
                    "SELU0445",
                    "A template supports at most 64 interpolations.",
                    &e.span,
                )
                .with_help("split the work into smaller labels or log entries"),
            );
        }
        for x in exprs {
            let t = self.value(x);
            if !matches!(
                crate::shape_of(&t),
                Type::String | Type::Number | Type::Boolean | Type::Error
            ) {
                self.out.push(
                    err(
                        "SELU0443",
                        "Template arguments need a non-null string, number or boolean.",
                        &x.span,
                    )
                    .with_help("use ?? or narrow the value first"),
                );
            }
        }
        if n.as_deref() == Some("L") {
            Type::String
        } else {
            Type::Void
        }
    }
    pub(crate) fn builtin_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> Option<Type> {
        if let Some(t) = self.readable_call(e, callee, args) {
            return Some(t);
        }
        if let Some(t) = self.has_call(e, callee, args) {
            return Some(t);
        }
        let n = self.wow_name(callee)?;
        if n == "onError" {
            let expected = Type::Function {
                multiple_results: false,
                unspecified_callback: false,
                api: Default::default(),
                api_semantics: Default::default(),
                params: vec![crate::errors::error_type()],
                required: 1,
                rest: None,
                ret: Box::new(Type::Void),
            };
            if args.len() != 1 {
                self.out.push(err(
                    "SELU0543",
                    "onError needs one (error: Error) => void callback.",
                    &e.span,
                ));
            }
            for arg in args {
                let actual = self.value_expected(arg, Some(&expected));
                if !actual.assignable_to(&expected) {
                    self.out.push(err(
                        "SELU0543",
                        "onError needs an Error callback.",
                        &arg.span,
                    ));
                }
            }
            return Some(Type::Void);
        }
        if n == "keybinding" {
            return Some(self.binding_call(e, args));
        }
        if !matches!(n.as_str(), "command" | "hook" | "hookScript") {
            return None;
        }
        let count_ok = if n == "command" {
            args.len() >= 2
        } else {
            args.len() == 3 || n == "hook" && args.len() == 2
        };
        if !count_ok {
            self.out.push(
                err(
                    "SELU0440",
                    "This built-in has the wrong number of arguments.",
                    &e.span,
                )
                .with_help(if n == "command" { r#"use command("/addon", (args) => { ... })"# } else { r#"use hook("GetTime", () => { ... }) or hookScript(frame, "OnShow", (self) => { ... })"# }),
            );
            return Some(Type::Error);
        }
        let last = args.len() - 1;
        for (i, x) in args[..last].iter().enumerate() {
            if n == "command" {
                if !matches!(&x.kind, ExprKind::Str(s) if s.starts_with('/') && s.len()>1 && !s.chars().any(char::is_whitespace))
                {
                    self.out.push(err(
                        "SELU0441",
                        "A slash alias must be a literal beginning with / and contain no spaces.",
                        &x.span,
                    ).with_help(r#"use a literal alias such as "/addon""#));
                }
                self.value(x);
            } else if i + 1 == last {
                if crate::shape_of(&self.value(x)) != Type::String {
                    self.out.push(
                        err("SELU0441", "A hook name must be a string.", &x.span)
                            .with_help(r#"use "GetTime" for hook or "OnShow" for hookScript"#),
                    );
                }
            } else {
                let ty = crate::shape_of(&self.value(x));
                if !matches!(
                    ty,
                    Type::Untyped | Type::Error | Type::Object(_) | Type::Widget(..)
                ) {
                    self.out.push(
                        err(
                            "SELU0446",
                            "A hook target must be a game object or an object table.",
                            &x.span,
                        )
                        .with_help(r#"use hook(GameTooltip, "SetUnit", (self) => { ... })"#),
                    );
                }
            }
        }
        let expected = Type::Function {
            multiple_results: false,
            unspecified_callback: false,
            api: Default::default(),
            api_semantics: Default::default(),
            params: if n == "command" {
                vec![Type::String]
            } else {
                vec![]
            },
            required: 0,
            rest: if n == "command" {
                None
            } else {
                Some(Box::new(Type::Untyped))
            },
            ret: Box::new(Type::Void),
        };
        let t = crate::shape_of(&self.value_expected(&args[last], Some(&expected)));
        if !matches!(t, Type::Function { .. } | Type::Error) {
            self.out.push(
                err(
                    "SELU0442",
                    "The callback must be a function.",
                    &args[last].span,
                )
                .with_help("use (args: string) => { ... } for command, or () => { ... } for hook"),
            );
        }
        if n == "command"
            && let Type::Function {
                params,
                required,
                rest,
                ..
            } = &t
            && (*required > 1
                || params
                    .first()
                    .or(rest.as_deref())
                    .is_some_and(|p| !Type::String.assignable_to(p)))
        {
            self.out.push(
                err(
                    "SELU0442",
                    "A command callback must accept one string argument.",
                    &args[last].span,
                )
                .with_help("use (args: string) => { ... }"),
            );
        }
        Some(Type::Void)
    }
}

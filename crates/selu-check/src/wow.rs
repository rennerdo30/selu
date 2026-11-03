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
            if !matches!(t, Type::String | Type::Number | Type::Boolean | Type::Error) {
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
        let n = self.wow_name(callee)?;
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
                if self.value(x) != Type::String {
                    self.out.push(
                        err("SELU0441", "A hook name must be a string.", &x.span)
                            .with_help(r#"use "GetTime" for hook or "OnShow" for hookScript"#),
                    );
                }
            } else {
                let ty = self.value(x);
                if !matches!(ty, Type::Untyped | Type::Error | Type::Object(_)) {
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
        let t = self.value_expected(&args[last], Some(&expected));
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

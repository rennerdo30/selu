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
        if n == "allowTaint" {
            self.types.mark_builtin_call(callee.id);
            if args.len() != 2 {
                self.out.push(err(
                    "SELU0607",
                    "allowTaint needs a reason and an expression",
                    &e.span,
                ));
                return Some(Type::Error);
            }
            self.value(&args[0]);
            // A wrapped call may return nothing (`hookScript`, a setter):
            // the wrapper is then a statement. The caller's own value check
            // still rejects it where a value is needed.
            if matches!(args[1].kind, ExprKind::Call { .. }) {
                return Some(self.expr(&args[1]));
            }
            return Some(self.value(&args[1]));
        }
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
            return Some(Type::Object(
                [
                    ("ensureInstalled".into(), handle_method(Type::Void)),
                    ("isInstalled".into(), handle_method(Type::Boolean)),
                    ("downstreamFailureCount".into(), handle_method(Type::Number)),
                    (
                        "setForwarding".into(),
                        Type::Function {
                            multiple_results: false,
                            unspecified_callback: false,
                            api: Default::default(),
                            api_semantics: Default::default(),
                            params: vec![Type::Boolean],
                            required: 1,
                            rest: None,
                            ret: Box::new(Type::Void),
                        },
                    ),
                ]
                .into(),
            ));
        }
        if n == "keybinding" {
            return Some(self.binding_call(e, args));
        }
        if !matches!(n.as_str(), "command" | "hook" | "hookScript") {
            return None;
        }
        self.types.mark_builtin_call(callee.id);
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
        let mut target = None;
        // `hook(SomeMixin, "Method", ...)`: a Blizzard mixin's method, for
        // every frame mixed after the hook.
        let mixin_self = if n == "hook" && args.len() == 3 {
            self.hook_mixin(&args[0])
        } else {
            None
        };
        if mixin_self.is_some() {
            self.types.mark_mixin_hook(callee.id);
        }
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
                } else if n == "hook" && !matches!(x.kind, ExprKind::Str(_)) {
                    // A name built at runtime would be a dynamic game lookup.
                    self.out.push(
                        err("SELU0441", "A hook name must be a literal string.", &x.span)
                            .with_help(r#"write the function or method name, for example hook("GetTime", ...)"#),
                    );
                }
            } else {
                let ty = match (&mixin_self, i) {
                    (Some(t), 0) => t.clone(),
                    _ => crate::shape_of(&self.value(x)),
                };
                target = Some(ty.clone());
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
        // The hooked function or method must be part of the game data, as a
        // call to it would have to be.
        let mut method = None;
        if n == "hook"
            && let ExprKind::Str(name) = &args[last - 1].kind
        {
            match &target {
                None if self.manifest.function(name).is_none() => {
                    self.out.push(
                        err(
                            "SELU0447",
                            format!("`{name}` is not a function in the game data"),
                            &args[0].span,
                        )
                        .with_help("hook a global function the game declares"),
                    );
                }
                Some(ty @ Type::Widget(..)) => {
                    let ty = ty.clone();
                    self.hook_target = true;
                    let member = self.api_member(&ty, name, &args[0]);
                    self.hook_target = false;
                    match member {
                        Some(f @ Type::Function { .. }) => method = Some((ty, f)),
                        Some(Type::Error) => {}
                        Some(_) => self.out.push(
                            err(
                                "SELU0447",
                                format!("`{name}` is a field of this widget, not a method"),
                                &args[1].span,
                            )
                            .with_help("hook a method of the widget or of its templates"),
                        ),
                        None => self.out.push(
                            err(
                                "SELU0447",
                                format!("this widget has no method `{name}` in the game data"),
                                &args[1].span,
                            )
                            .with_help("hook a method of the widget or of its templates"),
                        ),
                    }
                }
                _ => {}
            }
        }
        let mut expected = Type::Function {
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
        // A post-hook observes the native argument slots, including opaque
        // round-trip payloads; it does not inherit permissive native inputs.
        if n == "hook"
            && args.len() == 2
            && let ExprKind::Str(name) = &args[0].kind
            && let Some(d) = self
                .manifest
                .function(name)
                .and_then(|f| f.signature.as_ref())
            && let Type::Function {
                mut params,
                api_semantics,
                ..
            } = crate::api::signature(self.manifest, d, 0)
        {
            for (index, opaque) in api_semantics.opaque_inputs {
                if let Some(parameter) = params.get_mut(index) {
                    *parameter = opaque;
                }
            }
            if let Type::Function {
                params: expected_params,
                ..
            } = &mut expected
            {
                *expected_params = params;
            }
        }
        // A method post-hook receives the object, then the method's own
        // arguments.
        if let Some((
            receiver,
            Type::Function {
                params,
                api_semantics,
                ..
            },
        )) = method
            && let Type::Function {
                params: expected_params,
                ..
            } = &mut expected
        {
            let mut params = params;
            for (index, opaque) in api_semantics.opaque_inputs {
                if let Some(parameter) = params.get_mut(index) {
                    *parameter = opaque;
                }
            }
            *expected_params = std::iter::once(receiver).chain(params).collect();
        }
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

fn handle_method(ret: Type) -> Type {
    Type::Function {
        multiple_results: false,
        unspecified_callback: false,
        api: Default::default(),
        api_semantics: Default::default(),
        params: vec![],
        required: 0,
        rest: None,
        ret: Box::new(ret),
    }
}

impl Ctx<'_> {
    /// The receiver type of `hook(SomeMixin, ...)` when `x` names a mixin a
    /// game template uses: the frames carrying it (the one template that uses
    /// it, or the mixin itself when several do).
    fn hook_mixin(&mut self, x: &Expr) -> Option<Type> {
        let ExprKind::Ident(name) = &x.kind else {
            return None;
        };
        if self.lookup(name).is_some() {
            return None;
        }
        let m = self.manifest;
        let users: Vec<_> = m
            .templates
            .iter()
            .filter(|t| t.mixins.iter().any(|x| x == name))
            .collect();
        let first = users.first()?;
        let kind = first.ty.as_deref().unwrap_or("Frame");
        let kind = if users.iter().all(|t| t.ty.as_deref() == Some(kind)) {
            kind
        } else {
            "Frame"
        };
        let Type::Widget(widget, mut bases) = crate::api::widget(m, kind) else {
            return None;
        };
        let tag = if users.len() == 1 { &first.name } else { name };
        bases.push(format!("@template:{tag}"));
        crate::frames::tag_template_closure(m, &mut bases);
        Some(Type::Widget(widget, bases))
    }
}

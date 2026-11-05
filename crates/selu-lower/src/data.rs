//! Data expressions lowered through the Lua syntax tree.
use crate::Lowerer;
use crate::lua::{self, BinOp, LBlock, LExpr, LField, LStmt, LStmtKind, bin, name};
use selu_check::Type;
use selu_syntax::{Expr, ExprKind};

impl Lowerer<'_> {
    pub(crate) fn data_expr(&mut self, e: &Expr, pre: &mut LBlock) -> LExpr {
        match &e.kind {
            ExprKind::New {
                class, args, line, ..
            } if class == "Error" => self.error_new(args, &e.span.file, *line, pre),
            ExprKind::Array(items) => {
                let refs: Vec<_> = items.iter().collect();
                LExpr::Table(
                    self.seq(&refs, pre)
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| {
                            LField::Pos(
                                if i + 1 == items.len()
                                    && matches!(
                                        v,
                                        LExpr::Call(..) | LExpr::Method(..) | LExpr::Vararg
                                    )
                                {
                                    LExpr::Paren(Box::new(v))
                                } else {
                                    v
                                },
                            )
                        })
                        .collect(),
                )
            }
            ExprKind::Object(fields) => {
                let refs: Vec<_> = fields.iter().map(|f| &f.value).collect();
                LExpr::Table(
                    fields
                        .iter()
                        .zip(self.seq(&refs, pre))
                        .map(|(f, v)| LField::Named(f.key.clone(), v))
                        .collect(),
                )
            }
            ExprKind::Index { .. } => self.data_target(e, pre),
            ExprKind::OptionalMember {
                object,
                name: field,
            } => {
                let value = self.expr(object, pre);
                let t = self.hold(value, &e.span, pre);
                pre.push(LStmt::new(
                    LStmtKind::If(
                        vec![(
                            bin(BinOp::Ne, name(&t), LExpr::Nil),
                            vec![LStmt::new(
                                LStmtKind::Assign(
                                    vec![name(&t)],
                                    vec![typed_field(name(&t), field, self.types.get(object.id))],
                                ),
                                &e.span,
                            )],
                        )],
                        None,
                    ),
                    &e.span,
                ));
                name(&t)
            }
            ExprKind::Template { quasis, exprs } => {
                let mut format = quasis[0].replace('%', "%%");
                for q in &quasis[1..] {
                    format.push_str("%s");
                    format.push_str(&q.replace('%', "%%"));
                }
                let refs: Vec<_> = exprs.iter().collect();
                let vals = self.seq(&refs, pre);
                let mut args = vec![LExpr::Str(format)];
                args.extend(exprs.iter().zip(vals).map(|(e, v)| {
                    if self.types.get(e.id) == Some(&Type::Boolean) {
                        lua::call(name("tostring"), vec![v])
                    } else {
                        v
                    }
                }));
                lua::call(
                    LExpr::Field(Box::new(name("string")), "format".into()),
                    args,
                )
            }
            ExprKind::New { class, args, .. }
                if !matches!(class.as_str(), "Map" | "Set" | "WeakMap") =>
            {
                self.class_new(class, args, pre)
            }
            ExprKind::New { class, .. } if class == "WeakMap" => lua::call(
                name("setmetatable"),
                vec![
                    LExpr::Table(vec![]),
                    LExpr::Table(vec![LField::Named("__mode".into(), LExpr::Str("k".into()))]),
                ],
            ),
            ExprKind::New { .. } => LExpr::Table(vec![]),
            _ => unreachable!(),
        }
    }

    pub(crate) fn data_target(&mut self, target: &Expr, pre: &mut LBlock) -> LExpr {
        let ExprKind::Index { object, index } = &target.kind else {
            unreachable!()
        };
        let mut vals = self.seq(&[object, index], pre);
        self.capture_store_receiver(&mut vals, &target.span, pre);
        let i = vals.pop().unwrap();
        let i = match &index.kind {
            ExprKind::Number(n) => n
                .parse::<u64>()
                .ok()
                .filter(|n| *n < (1u64 << 53))
                .and_then(|n| n.checked_add(1))
                .map(|n| LExpr::Number(n.to_string()))
                .unwrap_or_else(|| bin(BinOp::Add, i.clone(), LExpr::Number("1".into()))),
            _ => bin(BinOp::Add, i, LExpr::Number("1".into())),
        };
        LExpr::Index(Box::new(vals.remove(0)), Box::new(i))
    }
}

fn num(n: usize) -> LExpr {
    LExpr::Number(n.to_string())
}
fn index(a: LExpr, i: LExpr) -> LExpr {
    LExpr::Index(Box::new(a), Box::new(i))
}
fn field(a: LExpr, f: &str) -> LExpr {
    LExpr::Field(Box::new(a), f.into())
}
fn len(a: LExpr) -> LExpr {
    LExpr::Unary(lua::UnOp::Len, Box::new(a))
}
fn lib(lib: &str, f: &str, args: Vec<LExpr>) -> LExpr {
    lua::call(field(name(lib), f), args)
}
impl Lowerer<'_> {
    pub(crate) fn data_field(&mut self, object: &Expr, f: &str, pre: &mut LBlock) -> LExpr {
        let v = self.expr(object, pre);
        typed_field(v, f, self.types.get(object.id))
    }
    pub(crate) fn data_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        pre: &mut LBlock,
    ) -> Option<LExpr> {
        let ExprKind::Member {
            object,
            name: method,
        } = &callee.kind
        else {
            return None;
        };
        let math = matches!(&object.kind,ExprKind::Ident(n) if n == "Math")
            && self.types.get(object.id).is_none();
        let ty = self.types.get(object.id).cloned().unwrap_or(Type::Untyped);
        if !math
            && !matches!(
                ty,
                Type::String | Type::Array(_) | Type::Map(..) | Type::WeakMap(..) | Type::Set(_)
            )
        {
            return None;
        }
        if ty == Type::String
            && let Some(v) = self.pattern_method(callee, args, pre)
        {
            return Some(v);
        }
        let span = &callee.span;
        if math {
            let refs: Vec<_> = args.iter().collect();
            let mut vs = self.seq(&refs, pre);
            return Some(match method.as_str() {
                "round" => lib(
                    "math",
                    "floor",
                    vec![bin(BinOp::Add, vs.remove(0), LExpr::Number("0.5".into()))],
                ),
                "clamp" => {
                    let x = vs.remove(0);
                    let lo = vs.remove(0);
                    let hi = vs.remove(0);
                    lib("math", "min", vec![lib("math", "max", vec![x, lo]), hi])
                }
                m => lib("math", m, vs),
            });
        }
        // A map read and arithmetic on plain fields cannot call user code.
        // Stable receivers/keys may be read again after the nullish prelude.
        if matches!(ty, Type::Map(..) | Type::WeakMap(..)) {
            if method == "get"
                && self.plain_table(object)
                && args.iter().all(|e| !self.may_run_code(e))
                && self.types.is_stable(object.id)
            {
                let obj = self.expr(object, pre);
                let key = self.expr(&args[0], pre);
                return Some(index(obj, key));
            }
            if method == "set"
                && self.plain_table(object)
                && self.types.is_stable(object.id)
                && args.iter().all(|e| !self.may_run_code(e))
            {
                let obj = self.expr(object, pre);
                let key = self.expr(&args[0], pre);
                let value = self.expr(&args[1], pre);
                pre.push(LStmt::new(
                    LStmtKind::Assign(vec![index(obj, key)], vec![value]),
                    span,
                ));
                return Some(LExpr::Nil);
            }
        }
        let mut refs = vec![&**object];
        refs.extend(args);
        let mut vals = self.seq(&refs, pre);
        self.capture_store_receiver(&mut vals, span, pre);
        let mut stable = Vec::new();
        for v in vals {
            stable.push(if v.is_pure() || matches!(v, LExpr::Function(_)) {
                v
            } else {
                let t = self.hold(v, span, pre);
                name(&t)
            });
        }
        let obj = stable.remove(0);
        let mut a = stable;
        if matches!(ty, Type::Array(_))
            && (matches!(
                method.as_str(),
                "forEach" | "map" | "filter" | "find" | "some" | "every" | "reduce"
            ) || method == "sort" && !a.is_empty())
        {
            return Some(self.data_callback(
                method,
                obj,
                a,
                span,
                pre,
                self.types.is_stable(object.id),
            ));
        }
        if method == "slice" {
            for bound in &mut a {
                let value = lib("math", "floor", vec![bound.clone()]);
                let t = self.temp();
                pre.push(LStmt::new(
                    LStmtKind::Local(vec![t.clone()], vec![value.clone()]),
                    span,
                ));
                pre.push(LStmt::new(
                    LStmtKind::If(
                        vec![(
                            bin(BinOp::Lt, value.clone(), num(0)),
                            vec![LStmt::new(
                                LStmtKind::Assign(
                                    vec![name(&t)],
                                    vec![lib(
                                        "math",
                                        "max",
                                        vec![
                                            bin(BinOp::Add, len(obj.clone()), value.clone()),
                                            num(0),
                                        ],
                                    )],
                                ),
                                span,
                            )],
                        )],
                        Some(vec![LStmt::new(
                            LStmtKind::Assign(
                                vec![name(&t)],
                                vec![lib("math", "min", vec![value, len(obj.clone())])],
                            ),
                            span,
                        )]),
                    ),
                    span,
                ));
                *bound = name(&t);
            }
        }
        let mut emit = |kind| pre.push(LStmt::new(kind, span));
        let result = match &ty {
            Type::Map(..) | Type::WeakMap(..) | Type::Set(_) => match method.as_str() {
                "get" => index(obj, a[0].clone()),
                "has" => bin(BinOp::Ne, index(obj, a[0].clone()), LExpr::Nil),
                "set" | "add" => {
                    emit(LStmtKind::Assign(
                        vec![index(obj, a[0].clone())],
                        vec![if method == "add" {
                            LExpr::Bool(true)
                        } else {
                            a[1].clone()
                        }],
                    ));
                    LExpr::Nil
                }
                "delete" => {
                    let t = self.temp();
                    emit(LStmtKind::Local(
                        vec![t.clone()],
                        vec![bin(BinOp::Ne, index(obj.clone(), a[0].clone()), LExpr::Nil)],
                    ));
                    emit(LStmtKind::Assign(
                        vec![index(obj, a[0].clone())],
                        vec![LExpr::Nil],
                    ));
                    name(&t)
                }
                "count" | "clear" => {
                    let t = self.temp();
                    let k = self.temp();
                    emit(LStmtKind::Local(vec![t.clone()], vec![num(0)]));
                    let action = if method == "clear" {
                        LStmtKind::Assign(vec![index(obj.clone(), name(&k))], vec![LExpr::Nil])
                    } else {
                        LStmtKind::Assign(vec![name(&t)], vec![bin(BinOp::Add, name(&t), num(1))])
                    };
                    emit(LStmtKind::GenFor {
                        names: vec![k],
                        exprs: vec![lua::call(name("pairs"), vec![obj])],
                        body: vec![LStmt::new(action, span)],
                    });
                    if method == "count" {
                        name(&t)
                    } else {
                        LExpr::Nil
                    }
                }
                _ => unreachable!(),
            },
            Type::Array(_) => match method.as_str() {
                "clear" => {
                    let i = self.temp();
                    emit(LStmtKind::NumFor {
                        var: i.clone(),
                        start: len(obj.clone()),
                        stop: num(1),
                        step: Some(LExpr::Number("-1".into())),
                        body: vec![LStmt::new(
                            LStmtKind::Assign(vec![index(obj, name(&i))], vec![LExpr::Nil]),
                            span,
                        )],
                    });
                    LExpr::Nil
                }
                "push" => {
                    emit(LStmtKind::Assign(
                        vec![index(obj.clone(), bin(BinOp::Add, len(obj), num(1)))],
                        vec![a[0].clone()],
                    ));
                    LExpr::Nil
                }
                "pop" => LExpr::Paren(Box::new(lib("table", "remove", vec![obj]))),
                "insert" => lib(
                    "table",
                    "insert",
                    vec![obj, bin(BinOp::Add, a[0].clone(), num(1)), a[1].clone()],
                ),
                "removeAt" => LExpr::Paren(Box::new(lib(
                    "table",
                    "remove",
                    vec![obj, bin(BinOp::Add, a[0].clone(), num(1))],
                ))),
                "sort" => lib("table", "sort", vec![obj]),
                "join" => {
                    let separator = a.first().cloned().unwrap_or(LExpr::Str(",".into()));
                    if matches!(&ty,Type::Array(t) if selu_check::shape_of(t) == Type::Boolean) {
                        let t = self.temp();
                        let i = self.temp();
                        emit(LStmtKind::Local(
                            vec![t.clone()],
                            vec![LExpr::Table(vec![])],
                        ));
                        emit(LStmtKind::NumFor {
                            var: i.clone(),
                            start: num(1),
                            stop: len(obj.clone()),
                            step: None,
                            body: vec![LStmt::new(
                                LStmtKind::Assign(
                                    vec![index(name(&t), name(&i))],
                                    vec![lua::call(name("tostring"), vec![index(obj, name(&i))])],
                                ),
                                span,
                            )],
                        });
                        lib("table", "concat", vec![name(&t), separator])
                    } else {
                        lib("table", "concat", vec![obj, separator])
                    }
                }
                "slice" => {
                    let t = self.temp();
                    let i = self.temp();
                    emit(LStmtKind::Local(
                        vec![t.clone()],
                        vec![LExpr::Table(vec![])],
                    ));
                    emit(LStmtKind::NumFor {
                        var: i.clone(),
                        start: bin(BinOp::Add, a[0].clone(), num(1)),
                        stop: lib(
                            "math",
                            "min",
                            vec![
                                a.get(1).cloned().unwrap_or_else(|| len(obj.clone())),
                                len(obj.clone()),
                            ],
                        ),
                        step: None,
                        body: vec![LStmt::new(
                            LStmtKind::Assign(
                                vec![index(name(&t), bin(BinOp::Add, len(name(&t)), num(1)))],
                                vec![index(obj, name(&i))],
                            ),
                            span,
                        )],
                    });
                    name(&t)
                }
                "indexOf" | "includes" => {
                    let t = self.temp();
                    let i = self.temp();
                    emit(LStmtKind::Local(
                        vec![t.clone()],
                        vec![if method == "includes" {
                            LExpr::Bool(false)
                        } else {
                            LExpr::Number("-1".into())
                        }],
                    ));
                    emit(LStmtKind::NumFor {
                        var: i.clone(),
                        start: num(1),
                        stop: len(obj.clone()),
                        step: None,
                        body: vec![LStmt::new(
                            LStmtKind::If(
                                vec![(
                                    bin(BinOp::Eq, index(obj, name(&i)), a[0].clone()),
                                    vec![
                                        LStmt::new(
                                            LStmtKind::Assign(
                                                vec![name(&t)],
                                                vec![if method == "includes" {
                                                    LExpr::Bool(true)
                                                } else {
                                                    bin(BinOp::Sub, name(&i), num(1))
                                                }],
                                            ),
                                            span,
                                        ),
                                        LStmt::new(LStmtKind::Break, span),
                                    ],
                                )],
                                None,
                            ),
                            span,
                        )],
                    });
                    name(&t)
                }
                _ => unreachable!(),
            },
            Type::String => match method.as_str() {
                "upper" | "lower" => lib("string", method, vec![obj]),
                "slice" => lib(
                    "string",
                    "sub",
                    vec![
                        obj,
                        bin(BinOp::Add, a[0].clone(), num(1)),
                        a.get(1).cloned().unwrap_or(LExpr::Number("-1".into())),
                    ],
                ),
                "trim" => LExpr::Paren(Box::new(lib(
                    "string",
                    "match",
                    vec![obj, LExpr::Str("^%s*(.-)%s*$".into())],
                ))),
                "includes" => bin(
                    BinOp::Ne,
                    lib(
                        "string",
                        "find",
                        vec![obj, a[0].clone(), num(1), LExpr::Bool(true)],
                    ),
                    LExpr::Nil,
                ),
                "startsWith" => bin(
                    BinOp::Eq,
                    lib("string", "sub", vec![obj, num(1), len(a[0].clone())]),
                    a[0].clone(),
                ),
                "endsWith" => bin(
                    BinOp::Eq,
                    lib(
                        "string",
                        "sub",
                        vec![
                            obj.clone(),
                            bin(
                                BinOp::Add,
                                bin(BinOp::Sub, len(obj), len(a[0].clone())),
                                num(1),
                            ),
                        ],
                    ),
                    a[0].clone(),
                ),
                "split" => {
                    let result = self.temp();
                    let pos = self.temp();
                    let found = self.temp();
                    emit(LStmtKind::Local(
                        vec![result.clone()],
                        vec![LExpr::Table(vec![])],
                    ));
                    emit(LStmtKind::Local(vec![pos.clone()], vec![num(1)]));
                    let append = |value| {
                        LStmt::new(
                            LStmtKind::Assign(
                                vec![index(
                                    name(&result),
                                    bin(BinOp::Add, len(name(&result)), num(1)),
                                )],
                                vec![value],
                            ),
                            span,
                        )
                    };
                    let chars = vec![append(lib(
                        "string",
                        "sub",
                        vec![obj.clone(), name(&pos), name(&pos)],
                    ))];
                    let scan = vec![
                        LStmt::new(
                            LStmtKind::Local(
                                vec![found.clone()],
                                vec![lib(
                                    "string",
                                    "find",
                                    vec![obj.clone(), a[0].clone(), name(&pos), LExpr::Bool(true)],
                                )],
                            ),
                            span,
                        ),
                        LStmt::new(
                            LStmtKind::If(
                                vec![(
                                    bin(BinOp::Eq, name(&found), LExpr::Nil),
                                    vec![
                                        append(lib("string", "sub", vec![obj.clone(), name(&pos)])),
                                        LStmt::new(LStmtKind::Break, span),
                                    ],
                                )],
                                None,
                            ),
                            span,
                        ),
                        append(lib(
                            "string",
                            "sub",
                            vec![
                                obj.clone(),
                                name(&pos),
                                bin(BinOp::Sub, name(&found), num(1)),
                            ],
                        )),
                        LStmt::new(
                            LStmtKind::Assign(
                                vec![name(&pos)],
                                vec![bin(BinOp::Add, name(&found), len(a[0].clone()))],
                            ),
                            span,
                        ),
                    ];
                    emit(LStmtKind::If(
                        vec![(
                            bin(BinOp::Eq, a[0].clone(), LExpr::Str("".into())),
                            vec![LStmt::new(
                                LStmtKind::NumFor {
                                    var: pos.clone(),
                                    start: num(1),
                                    stop: len(obj),
                                    step: None,
                                    body: chars,
                                },
                                span,
                            )],
                        )],
                        Some(vec![LStmt::new(
                            LStmtKind::While(LExpr::Bool(true), scan),
                            span,
                        )]),
                    ));
                    name(&result)
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        };
        // Store-only methods return nil to the statement dispatcher; the
        // checker rejects their use as values.
        Some(result)
    }

    pub(crate) fn for_of(&mut self, s: &selu_syntax::Stmt, out: &mut LBlock) {
        if self.pattern_for_of(s, out) {
            return;
        }
        use selu_syntax::{ForBinding, Pattern, StmtKind};
        let StmtKind::ForOf {
            binding,
            iter,
            body,
            ..
        } = &s.kind
        else {
            unreachable!()
        };
        let mut pre = vec![];
        let v = self.expr(iter, &mut pre);
        if self
            .types
            .get(iter.id)
            .and_then(selu_check::game_iterator)
            .is_some()
        {
            // Preserve Lua's iterator/state/control results. A scalar temporary
            // or parentheses here would discard the state and break pairs/ipairs.
            let iterator = match v {
                LExpr::Paren(v) => *v,
                v => v,
            };
            let mut names = match binding {
                ForBinding::Name(n, _) => vec![n.clone()],
                ForBinding::Pattern(Pattern::Array(items)) => items
                    .iter()
                    .map(|v| {
                        v.as_ref()
                            .map(|(n, _)| n.clone())
                            .unwrap_or_else(|| self.temp())
                    })
                    .collect(),
                _ => unreachable!(),
            };
            if names.is_empty() {
                names.push(self.temp());
            }
            pre.push(LStmt::new(
                LStmtKind::GenFor {
                    names,
                    exprs: vec![iterator],
                    body: self.loop_body(body, &s.span),
                },
                &s.span,
            ));
            out.extend(pre);
            return;
        }
        let t = self.hold(v, &iter.span, &mut pre);
        let obj = name(&t);
        let mut body = self.loop_body(body, &s.span);
        let kind = match self.types.get(iter.id) {
            Some(Type::Array(_)) => {
                let ForBinding::Name(n, _) = binding else {
                    unreachable!()
                };
                let i = self.temp();
                body.insert(
                    0,
                    LStmt::new(
                        LStmtKind::Local(vec![n.clone()], vec![index(obj.clone(), name(&i))]),
                        &s.span,
                    ),
                );
                body.insert(
                    1,
                    LStmt::new(
                        LStmtKind::If(
                            vec![(
                                bin(BinOp::Eq, name(n), LExpr::Nil),
                                vec![LStmt::new(LStmtKind::Break, &s.span)],
                            )],
                            None,
                        ),
                        &s.span,
                    ),
                );
                LStmtKind::NumFor {
                    var: i,
                    start: num(1),
                    stop: len(obj),
                    step: None,
                    body,
                }
            }
            _ => {
                let names = match binding {
                    ForBinding::Name(n, _) => vec![n.clone()],
                    ForBinding::Pattern(Pattern::Array(items)) => items
                        .iter()
                        .map(|i| {
                            i.as_ref()
                                .map(|(n, _)| n.clone())
                                .unwrap_or_else(|| self.temp())
                        })
                        .collect(),
                    _ => unreachable!(),
                };
                LStmtKind::GenFor {
                    names,
                    exprs: vec![lua::call(name("pairs"), vec![obj])],
                    body,
                }
            }
        };
        pre.push(LStmt::new(kind, &s.span));
        out.push(LStmt::new(LStmtKind::Do(pre), &s.span));
    }
}

fn typed_field(value: LExpr, f: &str, ty: Option<&Type>) -> LExpr {
    if f == "length" && ty.is_some_and(|t| matches!(t.non_null(), Type::Array(_) | Type::String)) {
        len(value)
    } else {
        field(value, f)
    }
}

impl Lowerer<'_> {
    fn data_callback(
        &mut self,
        method: &str,
        obj: LExpr,
        args: Vec<LExpr>,
        span: &selu_diagnostics::Span,
        pre: &mut LBlock,
        stable_receiver: bool,
    ) -> LExpr {
        // Both the receiver and function value stay fixed if a callback
        // reassigns their source names while the operation is running.
        let obj =
            if stable_receiver || matches!(obj, LExpr::Name(ref n) if n.starts_with("__selu_t")) {
                obj
            } else {
                let receiver = self.hold(obj, span, pre);
                name(&receiver)
            };
        let cb = match &args[0] {
            LExpr::Function(f)
                if method != "sort"
                    && !f.vararg
                    && f.body
                        .iter()
                        .take(f.body.len().saturating_sub(1))
                        .all(no_transfer)
                    && f.body.last().is_some_and(|s| {
                        matches!(&s.kind, LStmtKind::Return(values) if values.len() <= 1)
                            || method == "forEach" && no_transfer(s)
                    }) =>
            {
                args[0].clone()
            }
            value => {
                let t = self.hold(value.clone(), span, pre);
                name(&t)
            }
        };
        if method == "sort" {
            let left = self.temp();
            let right = self.temp();
            let compare = LExpr::Function(lua::LFunction {
                params: vec![left.clone(), right.clone()],
                vararg: false,
                body: vec![LStmt::new(
                    LStmtKind::Return(vec![bin(
                        BinOp::Lt,
                        lua::call(cb, vec![name(&left), name(&right)]),
                        num(0),
                    )]),
                    span,
                )],
            });
            return lib("table", "sort", vec![obj, compare]);
        }
        let result = self.temp();
        let i = self.temp();
        let value = self.temp();
        let initial = match method {
            "map" | "filter" => LExpr::Table(vec![]),
            "some" => LExpr::Bool(false),
            "every" => LExpr::Bool(true),
            "reduce" => args
                .get(1)
                .cloned()
                .unwrap_or_else(|| index(obj.clone(), num(1))),
            _ => LExpr::Nil,
        };
        if method != "forEach" {
            pre.push(LStmt::new(
                LStmtKind::Local(vec![result.clone()], vec![initial]),
                span,
            ));
        }
        let mut body = vec![
            LStmt::new(
                LStmtKind::Local(vec![value.clone()], vec![index(obj.clone(), name(&i))]),
                span,
            ),
            LStmt::new(
                LStmtKind::If(
                    vec![(
                        bin(BinOp::Eq, name(&value), LExpr::Nil),
                        vec![LStmt::new(LStmtKind::Break, span)],
                    )],
                    None,
                ),
                span,
            ),
        ];
        let mut cb_args = vec![name(&value), bin(BinOp::Sub, name(&i), num(1)), obj.clone()];
        if method == "reduce" {
            cb_args.insert(0, name(&result));
        }
        let call = if let LExpr::Function(f) = &cb {
            let mut inline = f.body.clone();
            let terminal = inline.pop();
            let safe = !f.vararg && inline.iter().all(no_transfer);
            if safe
                && matches!(
                    terminal.as_ref().map(|s| &s.kind),
                    Some(LStmtKind::Return(_))
                )
            {
                if !f.params.is_empty() {
                    body.push(LStmt::new(
                        LStmtKind::Local(
                            f.params.clone(),
                            cb_args.iter().take(f.params.len()).cloned().collect(),
                        ),
                        span,
                    ));
                }
                body.extend(inline);
                let LStmtKind::Return(values) = terminal.unwrap().kind else {
                    unreachable!()
                };
                values.into_iter().next().unwrap_or(LExpr::Nil)
            } else if method == "forEach" && f.body.iter().all(no_transfer) && !f.vararg {
                if !f.params.is_empty() {
                    body.push(LStmt::new(
                        LStmtKind::Local(
                            f.params.clone(),
                            cb_args.iter().take(f.params.len()).cloned().collect(),
                        ),
                        span,
                    ));
                }
                body.extend(f.body.clone());
                LExpr::Nil
            } else {
                lua::call(cb, cb_args)
            }
        } else {
            lua::call(cb, cb_args)
        };
        let store = |v| LStmt::new(LStmtKind::Assign(vec![name(&result)], vec![v]), span);
        let append = |v| {
            LStmt::new(
                LStmtKind::Assign(
                    vec![index(
                        name(&result),
                        bin(BinOp::Add, len(name(&result)), num(1)),
                    )],
                    vec![v],
                ),
                span,
            )
        };
        let action = match method {
            "forEach" if call == LExpr::Nil => LStmt::new(LStmtKind::Do(vec![]), span),
            "forEach" if matches!(call, LExpr::Call(..) | LExpr::Method(..)) => {
                LStmt::new(LStmtKind::Call(call), span)
            }
            // Discarding a value does not make it a legal Lua call statement.
            // Keep possible metamethods/effects by evaluating into an ignored local.
            "forEach" if call.is_pure() => LStmt::new(LStmtKind::Do(vec![]), span),
            "forEach" => LStmt::new(LStmtKind::Local(vec!["_".into()], vec![call]), span),
            "map" => append(
                if matches!(call, LExpr::Call(..) | LExpr::Method(..) | LExpr::Vararg) {
                    LExpr::Paren(Box::new(call))
                } else {
                    call
                },
            ),
            "reduce" => store(call),
            "filter" => LStmt::new(
                LStmtKind::If(vec![(call, vec![append(name(&value))])], None),
                span,
            ),
            _ => {
                let condition = if method == "every" {
                    lua::not(call)
                } else {
                    call
                };
                let v = if method == "find" {
                    name(&value)
                } else {
                    LExpr::Bool(method == "some")
                };
                LStmt::new(
                    LStmtKind::If(
                        vec![(
                            condition,
                            vec![store(v), LStmt::new(LStmtKind::Break, span)],
                        )],
                        None,
                    ),
                    span,
                )
            }
        };
        if !matches!(&action.kind, LStmtKind::Do(b) if b.is_empty()) {
            body.push(action);
        }
        if method == "reduce" && args.len() == 1 {
            pre.push(LStmt::new(
                LStmtKind::If(
                    vec![(
                        bin(BinOp::Eq, index(obj.clone(), num(1)), LExpr::Nil),
                        vec![LStmt::new(
                            LStmtKind::Call(lua::call(
                                name("error"),
                                vec![LExpr::Str(
                                    "reduce of an empty array needs an initial value".into(),
                                )],
                            )),
                            span,
                        )],
                    )],
                    None,
                ),
                span,
            ));
        }
        pre.push(LStmt::new(
            LStmtKind::NumFor {
                var: i,
                start: num(if method == "reduce" && args.len() == 1 {
                    2
                } else {
                    1
                }),
                stop: len(obj),
                step: None,
                body,
            },
            span,
        ));
        if method == "forEach" {
            LExpr::Nil
        } else {
            name(&result)
        }
    }
}

impl Lowerer<'_> {
    fn capture_store_receiver(
        &mut self,
        values: &mut [LExpr],
        span: &selu_diagnostics::Span,
        pre: &mut LBlock,
    ) {
        // Stabilizing a later impure operand emits a statement. Snapshot
        // earlier name operands first, including map keys and the receiver.
        for i in 0..values.len() {
            if matches!(values[i], LExpr::Name(_))
                && values[i + 1..]
                    .iter()
                    .any(|v| !v.is_pure() && !matches!(v, LExpr::Function(_)))
            {
                let t = self.hold(values[i].clone(), span, pre);
                values[i] = name(&t);
            }
        }
    }
}

// Function bodies with early returns retain their call boundary: a return must
// never escape the enclosing source function or skip an array-method action.
fn no_transfer(s: &LStmt) -> bool {
    match &s.kind {
        LStmtKind::Return(_) | LStmtKind::Break => false,
        LStmtKind::If(arms, other) => {
            arms.iter().all(|(_, b)| b.iter().all(no_transfer))
                && other.as_ref().is_none_or(|b| b.iter().all(no_transfer))
        }
        LStmtKind::While(_, b)
        | LStmtKind::Repeat(b, _)
        | LStmtKind::Do(b)
        | LStmtKind::NumFor { body: b, .. }
        | LStmtKind::GenFor { body: b, .. } => b.iter().all(no_transfer),
        _ => true,
    }
}

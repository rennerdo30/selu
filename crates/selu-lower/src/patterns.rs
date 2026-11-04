use crate::{
    Lowerer,
    lua::{self, BinOp, LBlock, LExpr, LStmt, LStmtKind, bin, name},
};
use selu_syntax::{Expr, ExprKind, ForBinding, Pattern, Stmt, StmtKind};
fn lib(method: &str, args: Vec<LExpr>) -> LExpr {
    lua::call(LExpr::Field(Box::new(name("string")), method.into()), args)
}
fn one() -> LExpr {
    LExpr::Number("1".into())
}
fn pattern_call<'a>(
    e: &'a Expr,
    types: &selu_check::TypeTable,
) -> Option<(&'a Expr, &'a str, &'a [Expr])> {
    let ExprKind::Call { callee, args } = &e.kind else {
        return None;
    };
    let ExprKind::Member { object, name } = &callee.kind else {
        return None;
    };
    if !matches!(
        types.get(object.id).map(selu_check::shape_of),
        Some(selu_check::Type::String)
    ) {
        return None;
    }
    if !matches!(
        name.as_str(),
        "match" | "search" | "matchAll" | "replace" | "replaceAll"
    ) || args.first().and_then(selu_check::patterns::text).is_none()
    {
        return None;
    }
    Some((object, name, args))
}
impl Lowerer<'_> {
    pub(crate) fn pattern_template(&mut self, e: &Expr) -> Option<LExpr> {
        selu_check::patterns::text(e).map(|(s, _)| LExpr::Str(s))
    }
    pub(crate) fn pattern_method(
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
        if !matches!(
            method.as_str(),
            "match" | "search" | "matchAll" | "replace" | "replaceAll"
        ) {
            return None;
        }
        let (s, _) = selu_check::patterns::text(args.first()?)?;
        let refs: Vec<_> = std::iter::once(object.as_ref())
            .chain(args.iter())
            .collect();
        let mut vs = self.seq(&refs, pre);
        let v = match method.as_str() {
            "search" => {
                let v = lib("find", vs);
                let t = self.hold(v, &callee.span, pre);
                return Some(bin(BinOp::And, name(&t), bin(BinOp::Sub, name(&t), one())));
            }
            "replace" | "replaceAll" => {
                let caps = selu_check::patterns::captures(&s).unwrap();
                let ty = self.types.get(args[1].id).map(selu_check::shape_of);
                vs[2] = match (&vs[2], ty) {
                    (LExpr::Str(s), _) => LExpr::Str(s.replace('%', "%%")),
                    (_, Some(selu_check::Type::String)) => escape_replacement(vs[2].clone()),
                    (_, Some(selu_check::Type::Untyped)) => {
                        let r = self.hold(vs[2].clone(), &callee.span, pre);
                        let mut arms = vec![(
                            bin(
                                BinOp::Eq,
                                lua::call(name("__selu_type"), vec![name(&r)]),
                                LExpr::Str("string".into()),
                            ),
                            vec![LStmt::new(
                                LStmtKind::Assign(
                                    vec![name(&r)],
                                    vec![escape_replacement(name(&r))],
                                ),
                                &callee.span,
                            )],
                        )];
                        if caps.contains(&true) {
                            let mut body = vec![];
                            let cb =
                                self.position_callback(name(&r), &caps, &callee.span, &mut body);
                            body.push(LStmt::new(
                                LStmtKind::Assign(vec![name(&r)], vec![cb]),
                                &callee.span,
                            ));
                            arms.push((
                                bin(
                                    BinOp::Eq,
                                    lua::call(name("__selu_type"), vec![name(&r)]),
                                    LExpr::Str("function".into()),
                                ),
                                body,
                            ));
                        }
                        pre.push(LStmt::new(LStmtKind::If(arms, None), &callee.span));
                        name(&r)
                    }
                    (LExpr::Function(f), _) if !f.vararg => {
                        let mut f = f.clone();
                        let mut body = vec![];
                        self.pattern_positions(&f.params, &caps, false, &callee.span, &mut body);
                        body.append(&mut f.body);
                        f.body = body;
                        LExpr::Function(f)
                    }
                    _ if caps.contains(&true) => {
                        self.position_callback(vs[2].clone(), &caps, &callee.span, pre)
                    }
                    _ => vs[2].clone(),
                };
                if method == "replace" {
                    vs.push(one());
                }
                lib("gsub", vs)
            }
            "matchAll" => lib("gmatch", vs),
            _ => {
                let v = lib("match", vs);
                let caps = selu_check::patterns::captures(&s).unwrap();
                if caps.first() == Some(&true) {
                    let names = vec![self.temp()];
                    pre.push(LStmt::new(
                        LStmtKind::Local(names.clone(), vec![v]),
                        &callee.span,
                    ));
                    self.pattern_positions(&names, &caps, true, &callee.span, pre);
                    return Some(name(&names[0]));
                }
                v
            }
        };
        Some(v)
    }
    fn position_callback(
        &mut self,
        value: LExpr,
        caps: &[bool],
        span: &selu_diagnostics::Span,
        pre: &mut LBlock,
    ) -> LExpr {
        let cb = self.hold(value, span, pre);
        let params: Vec<_> = caps.iter().map(|_| self.temp()).collect();
        let mut body = vec![];
        self.pattern_positions(&params, caps, false, span, &mut body);
        body.push(LStmt::new(
            LStmtKind::Return(vec![lua::call(
                name(&cb),
                params.iter().map(|n| name(n)).collect(),
            )]),
            span,
        ));
        LExpr::Function(lua::LFunction {
            params,
            vararg: false,
            body,
        })
    }
    fn pattern_positions(
        &mut self,
        names: &[String],
        caps: &[bool],
        optional: bool,
        span: &selu_diagnostics::Span,
        out: &mut LBlock,
    ) {
        for (n, p) in names.iter().zip(caps) {
            if *p {
                let v = bin(BinOp::Sub, name(n), one());
                let v = if optional {
                    bin(BinOp::And, name(n), v)
                } else {
                    v
                };
                out.push(LStmt::new(LStmtKind::Assign(vec![name(n)], vec![v]), span));
            }
        }
    }
    pub(crate) fn pattern_values(&mut self, e: &Expr, pre: &mut LBlock) -> Option<Vec<LExpr>> {
        let (object, method, args) = pattern_call(e, self.types)?;
        if method != "match" {
            return None;
        }
        let (s, _) = selu_check::patterns::text(&args[0])?;
        let caps = selu_check::patterns::captures(&s).ok()?;
        let caps = if self.types.is_single_value(e.id) {
            caps.into_iter().take(1).collect::<Vec<_>>()
        } else {
            caps
        };
        if !caps.contains(&true) {
            return None;
        }
        let vs = self.seq(&[object, &args[0]], pre);
        let names: Vec<_> = caps.iter().map(|_| self.temp()).collect();
        pre.push(LStmt::new(
            LStmtKind::Local(names.clone(), vec![lib("match", vs)]),
            &e.span,
        ));
        self.pattern_positions(&names, &caps, true, &e.span, pre);
        Some(
            names
                .iter()
                .take(if self.types.is_single_value(e.id) {
                    1
                } else {
                    names.len()
                })
                .map(|n| name(n))
                .collect(),
        )
    }
    pub(crate) fn pattern_destructure(
        &mut self,
        s: &Stmt,
        forward: bool,
        out: &mut LBlock,
    ) -> bool {
        let StmtKind::Destructure {
            pattern: Pattern::Array(items),
            init,
            ..
        } = &s.kind
        else {
            return false;
        };
        if items.is_empty() && pattern_call(init, self.types).is_some() {
            let ExprKind::Call { callee, args } = &init.kind else {
                unreachable!()
            };
            let v = self.pattern_method(callee, args, out).unwrap();
            // A position projection is already evaluated by its temporary prelude.
            if matches!(v, LExpr::Call(..)) {
                out.push(LStmt::new(LStmtKind::Call(v), &s.span));
            }
            return true;
        }
        let Some(values) = self.pattern_values(init, out) else {
            return false;
        };
        let names: Vec<_> = items
            .iter()
            .map(|i| {
                i.as_ref()
                    .map(|(n, _)| n.clone())
                    .unwrap_or_else(|| self.temp())
            })
            .collect();
        out.push(LStmt::new(
            if forward {
                LStmtKind::Assign(names.iter().map(|n| name(n)).collect(), values)
            } else {
                LStmtKind::Local(names, values)
            },
            &s.span,
        ));
        true
    }
    pub(crate) fn pattern_for_of(&mut self, s: &Stmt, out: &mut LBlock) -> bool {
        let StmtKind::ForOf {
            binding,
            iter,
            body,
            ..
        } = &s.kind
        else {
            return false;
        };
        let Some(caps) = self
            .types
            .get(iter.id)
            .and_then(selu_check::patterns::iterator_captures)
        else {
            return false;
        };
        let mut names = match binding {
            ForBinding::Name(n, _) => vec![n.clone()],
            ForBinding::Pattern(Pattern::Array(v)) => v
                .iter()
                .map(|i| {
                    i.as_ref()
                        .map(|(n, _)| n.clone())
                        .unwrap_or_else(|| self.temp())
                })
                .collect(),
            _ => unreachable!(),
        };
        if names.is_empty() {
            names.push(self.temp());
        }
        let mut pre = vec![];
        let iterator = if let Some((object, "matchAll", args)) = pattern_call(iter, self.types) {
            let vs = self.seq(&[object, &args[0]], &mut pre);
            lib("gmatch", vs)
        } else {
            self.expr(iter, &mut pre)
        };
        let mut lowered = vec![];
        self.pattern_positions(&names, &caps, false, &s.span, &mut lowered);
        lowered.extend(self.loop_body(body, &s.span));
        pre.push(LStmt::new(
            LStmtKind::GenFor {
                names,
                exprs: vec![iterator],
                body: lowered,
            },
            &s.span,
        ));
        out.extend(pre);
        true
    }
}

fn escape_replacement(value: LExpr) -> LExpr {
    LExpr::Paren(Box::new(lib(
        "gsub",
        vec![value, LExpr::Str("%%".into()), LExpr::Str("%%%%".into())],
    )))
}
pub(crate) fn needs_type(e: &Expr, types: &selu_check::TypeTable) -> bool {
    pattern_call(e, types).is_some_and(|(_, m, args)| {
        matches!(m, "replace" | "replaceAll")
            && args.get(1).is_some_and(|a| {
                matches!(
                    types.get(a.id).map(selu_check::shape_of),
                    Some(selu_check::Type::Untyped)
                )
            })
    })
}

//! Protected completions retain scalar/tuple values without packing tables.
use crate::{
    Lowerer,
    lua::{self, *},
};
use selu_diagnostics::Span;
use selu_syntax::{Expr, Stmt, StmtKind};

fn st(k: LStmtKind, s: &Span) -> LStmt {
    LStmt::new(k, s)
}
fn field(e: LExpr, n: &str) -> LExpr {
    LExpr::Field(Box::new(e), n.into())
}
fn eq(n: &str, v: &str) -> LExpr {
    bin(BinOp::Eq, name(n), LExpr::Str(v.into()))
}
// Do not enter function expressions/declarations; their returns belong to them.
// Loop breaks are only rewritten at depth zero, including A's repeat wrapper.
fn completions(
    b: &mut LBlock,
    depth: usize,
    count: &mut usize,
    breaks: &mut bool,
    returns: &mut bool,
) {
    for s in b {
        match &mut s.kind {
            LStmtKind::Return(v) => {
                *returns = true;
                *count = (*count).max(v.len());
                v.insert(0, LExpr::Str("return".into()));
            }
            LStmtKind::Break if depth == 0 => {
                *breaks = true;
                s.kind = LStmtKind::Return(vec![LExpr::Str("break".into())]);
            }
            LStmtKind::Do(b) => completions(b, depth, count, breaks, returns),
            LStmtKind::If(arms, other) => {
                for (_, b) in arms {
                    completions(b, depth, count, breaks, returns);
                }
                if let Some(b) = other {
                    completions(b, depth, count, breaks, returns);
                }
            }
            LStmtKind::While(_, b)
            | LStmtKind::Repeat(b, _)
            | LStmtKind::NumFor { body: b, .. }
            | LStmtKind::GenFor { body: b, .. } => {
                completions(b, depth + 1, count, breaks, returns)
            }
            _ => {}
        }
    }
}
impl Lowerer<'_> {
    pub(crate) fn error_new(&mut self, args: &[Expr], pre: &mut LBlock) -> LExpr {
        LExpr::Table(vec![LField::Named(
            "message".into(),
            self.expr(&args[0], pre),
        )])
    }
    pub(crate) fn error_stmt(&mut self, s: &Stmt, out: &mut LBlock) {
        let sp = &s.span;
        match &s.kind {
            StmtKind::Throw { value, line } => {
                let mut pre = vec![];
                let v = self.expr(value, &mut pre);
                let message = bin(
                    BinOp::Concat,
                    LExpr::Str(format!("{}:{line}: ", sp.file)),
                    field(v, "message"),
                );
                pre.push(st(
                    LStmtKind::Call(lua::call(
                        name("error"),
                        vec![message, LExpr::Number("0".into())],
                    )),
                    sp,
                ));
                out.extend(pre);
            }
            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                let mut body = self.block(body);
                let mut catch_body = catch.as_ref().map(|(_, b)| self.block(b));
                let mut final_body = finally.as_ref().map(|b| self.block(b));
                let (mut count, mut breaks, mut returns) =
                    (return_width(s, self.types), false, false);
                completions(&mut body, 0, &mut count, &mut breaks, &mut returns);
                if let Some(b) = &mut catch_body {
                    completions(b, 0, &mut count, &mut breaks, &mut returns);
                }
                if let Some(b) = &mut final_body {
                    completions(b, 0, &mut count, &mut breaks, &mut returns);
                }
                let ok = self.temp();
                let status = self.temp();
                let values: Vec<_> = (0..count).map(|_| self.temp()).collect();
                let mut names = vec![ok.clone(), status.clone()];
                names.extend(values.clone());
                let protect = |body| {
                    lua::call(
                        name("pcall"),
                        vec![LExpr::Function(LFunction {
                            params: vec![],
                            vararg: false,
                            body,
                        })],
                    )
                };
                let mut b = vec![st(LStmtKind::Local(names.clone(), vec![protect(body)]), sp)];
                if let (Some((p, _)), Some(mut cb)) = (catch, catch_body) {
                    cb.insert(
                        0,
                        st(
                            LStmtKind::Local(
                                vec![p.name.clone()],
                                vec![LExpr::Table(vec![LField::Named(
                                    "message".into(),
                                    lua::call(name("tostring"), vec![name(&status)]),
                                )])],
                            ),
                            sp,
                        ),
                    );
                    b.push(st(
                        LStmtKind::If(
                            vec![(
                                not(name(&ok)),
                                vec![st(
                                    LStmtKind::Assign(
                                        names.iter().map(|n| name(n)).collect(),
                                        vec![protect(cb)],
                                    ),
                                    sp,
                                )],
                            )],
                            None,
                        ),
                        sp,
                    ));
                }
                if let Some(fb) = final_body {
                    let flag = self.loops.last().and_then(|c| c.break_flag.clone());
                    let saved_flag = flag.as_ref().map(|f| {
                        let saved = self.temp();
                        b.push(st(LStmtKind::Local(vec![saved.clone()], vec![name(f)]), sp));
                        b.push(st(
                            LStmtKind::Assign(vec![name(f)], vec![LExpr::Bool(false)]),
                            sp,
                        ));
                        saved
                    });
                    let fresh: Vec<_> = names.iter().map(|_| self.temp()).collect();
                    b.push(st(LStmtKind::Local(fresh.clone(), vec![protect(fb)]), sp));
                    let override_c = bin(
                        BinOp::Or,
                        not(name(&fresh[0])),
                        bin(BinOp::Ne, name(&fresh[1]), LExpr::Nil),
                    );
                    if let (Some(flag), Some(saved)) = (flag, saved_flag) {
                        b.push(st(
                            LStmtKind::If(
                                vec![(
                                    not(override_c.clone()),
                                    vec![st(
                                        LStmtKind::Assign(vec![name(&flag)], vec![name(&saved)]),
                                        sp,
                                    )],
                                )],
                                None,
                            ),
                            sp,
                        ));
                    }
                    b.push(st(
                        LStmtKind::If(
                            vec![(
                                override_c,
                                vec![st(
                                    LStmtKind::Assign(
                                        names.iter().map(|n| name(n)).collect(),
                                        fresh.iter().map(|n| name(n)).collect(),
                                    ),
                                    sp,
                                )],
                            )],
                            None,
                        ),
                        sp,
                    ));
                }
                b.push(st(
                    LStmtKind::If(
                        vec![(
                            not(name(&ok)),
                            vec![st(
                                LStmtKind::Call(lua::call(
                                    name("error"),
                                    vec![name(&status), LExpr::Number("0".into())],
                                )),
                                sp,
                            )],
                        )],
                        None,
                    ),
                    sp,
                ));
                if returns {
                    b.push(st(
                        LStmtKind::If(
                            vec![(
                                eq(&status, "return"),
                                vec![st(
                                    LStmtKind::Return(values.iter().map(|n| name(n)).collect()),
                                    sp,
                                )],
                            )],
                            None,
                        ),
                        sp,
                    ));
                }
                if breaks {
                    b.push(st(
                        LStmtKind::If(
                            vec![(eq(&status, "break"), vec![st(LStmtKind::Break, sp)])],
                            None,
                        ),
                        sp,
                    ));
                }
                out.push(st(LStmtKind::Do(b), sp));
            }
            _ => unreachable!(),
        }
    }
}
fn return_width(s: &Stmt, types: &selu_check::TypeTable) -> usize {
    if matches!(
        s.kind,
        StmtKind::FuncDecl { .. } | StmtKind::OnHandler { .. } | StmtKind::Class(_)
    ) {
        return 0;
    }
    if let StmtKind::Return(Some(e)) = &s.kind {
        if types.is_single_value(e.id) {
            return 1;
        }
        return match types.get(e.id) {
            Some(selu_check::Type::Tuple(ts)) => ts.len(),
            _ => 1,
        };
    }
    let mut width = 0;
    selu_syntax::visit::child_stmts_and_exprs(s, &mut |c| {
        if let selu_syntax::visit::Child::Stmt(s) = c {
            width = width.max(return_width(s, types));
        }
    });
    width
}

impl Lowerer<'_> {
    // Literal completions have no lexical captures. Move their protected
    // function to the loop's enclosing block, preserving all other closures.
    pub(crate) fn hoist_error_closures(&mut self, block: &mut LBlock, out: &mut LBlock) {
        for s in block {
            let protected = match &s.kind {
                LStmtKind::Local(names, _) => {
                    names.len() >= 2 && names.iter().all(|n| n.starts_with("__selu_"))
                }
                LStmtKind::Assign(targets, _) => {
                    targets.len() >= 2
                        && targets
                            .iter()
                            .all(|e| matches!(e, LExpr::Name(n) if n.starts_with("__selu_")))
                }
                _ => false,
            };
            match &mut s.kind {
                LStmtKind::Local(_, values) | LStmtKind::Assign(_, values) => {
                    for v in values {
                        if !protected {
                            continue;
                        }
                        if let LExpr::Call(callee, args) = v
                            && matches!(callee.as_ref(), LExpr::Name(n) if n == "pcall")
                            && let [LExpr::Function(f)] = args.as_mut_slice()
                            && f.body.iter().all(|s| matches!(&s.kind,LStmtKind::Return(v) if v.iter().all(|e| matches!(e,LExpr::Nil|LExpr::Bool(_)|LExpr::Number(_)|LExpr::Str(_)))))
                        {
                            let n = self.temp();
                            out.push(LStmt {kind:LStmtKind::LocalFunction(n.clone(),f.clone()),span:s.span.clone()});
                            args[0] = name(&n);
                        }
                    }
                }
                LStmtKind::If(arms, other) => {
                    for (_, b) in arms {
                        self.hoist_error_closures(b, out);
                    }
                    if let Some(b) = other {
                        self.hoist_error_closures(b, out);
                    }
                }
                LStmtKind::Do(b)
                | LStmtKind::While(_, b)
                | LStmtKind::Repeat(b, _)
                | LStmtKind::NumFor { body: b, .. }
                | LStmtKind::GenFor { body: b, .. } => self.hoist_error_closures(b, out),
                _ => {}
            }
        }
    }
}

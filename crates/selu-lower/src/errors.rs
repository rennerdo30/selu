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
        let message = args
            .first()
            .map(|e| self.expr(e, pre))
            .unwrap_or_else(|| LExpr::Str(String::new()));
        lua::call(name("__selu_new_error"), vec![message])
    }

    pub(crate) fn error_stmt(&mut self, s: &Stmt, out: &mut LBlock) {
        let sp = &s.span;
        match &s.kind {
            StmtKind::Throw { value, line } => {
                let mut pre = vec![];
                let v = self.expr(value, &mut pre);
                pre.push(st(
                    LStmtKind::Call(lua::call(
                        name("__selu_throw"),
                        vec![v, LExpr::Str(format!("{}:{line}: ", sp.file))],
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
                        name("__selu_protect"),
                        vec![LExpr::Function(LFunction {
                            params: vec![],
                            vararg: false,
                            body,
                        })],
                    )
                };
                let mut b = vec![st(LStmtKind::Local(names.clone(), vec![protect(body)]), sp)];
                if let (Some((p, _)), Some(mut cb)) = (catch, catch_body) {
                    if let Some(p) = p {
                        cb.insert(
                            0,
                            st(
                                LStmtKind::Local(
                                    vec![p.name.clone()],
                                    vec![lua::call(name("__selu_wrap_error"), vec![name(&status)])],
                                ),
                                sp,
                            ),
                        );
                    }
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
                                    name("__selu_raise"),
                                    vec![name(&status)],
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
    // A helper may share references to cells outside this loop. It must stay
    // inside the loop if it captures any binding created by an iteration.
    pub(crate) fn hoist_error_closures(&mut self, block: &mut LBlock, out: &mut LBlock) {
        let mut varying = std::collections::HashSet::new();
        iteration_bindings(block, &mut varying);
        self.hoist_protected(block, out, &varying);
    }
    fn hoist_protected(
        &mut self,
        block: &mut LBlock,
        out: &mut LBlock,
        varying: &std::collections::HashSet<String>,
    ) {
        for s in block {
            match &mut s.kind {
                LStmtKind::Local(_, values) | LStmtKind::Assign(_, values) => {
                    for v in values {
                        if let LExpr::Call(callee, args) = v
                            && matches!(callee.as_ref(), LExpr::Name(n) if n == "__selu_protect")
                            && let [LExpr::Function(f)] = args.as_mut_slice()
                            && !free_names(f)
                                .iter()
                                .any(|n| varying.contains(n) || n == "<raw>")
                        {
                            let n = self.temp();
                            out.push(LStmt {
                                kind: LStmtKind::LocalFunction(n.clone(), f.clone()),
                                span: s.span.clone(),
                            });
                            args[0] = name(&n);
                        }
                    }
                }
                LStmtKind::If(arms, other) => {
                    for (_, b) in arms {
                        self.hoist_protected(b, out, varying);
                    }
                    if let Some(b) = other {
                        self.hoist_protected(b, out, varying);
                    }
                }
                LStmtKind::Do(b)
                | LStmtKind::While(_, b)
                | LStmtKind::Repeat(b, _)
                | LStmtKind::NumFor { body: b, .. }
                | LStmtKind::GenFor { body: b, .. } => self.hoist_protected(b, out, varying),
                _ => {}
            }
        }
    }
}
type Names = std::collections::HashSet<String>;
fn iteration_bindings(block: &LBlock, names: &mut Names) {
    for s in block {
        match &s.kind {
            LStmtKind::Local(ns, _) => names.extend(ns.iter().cloned()),
            LStmtKind::LocalFunction(n, _) => {
                names.insert(n.clone());
            }
            LStmtKind::NumFor { var, body, .. } => {
                names.insert(var.clone());
                iteration_bindings(body, names);
            }
            LStmtKind::GenFor {
                names: ns, body, ..
            } => {
                names.extend(ns.iter().cloned());
                iteration_bindings(body, names);
            }
            LStmtKind::If(arms, other) => {
                for (_, b) in arms {
                    iteration_bindings(b, names);
                }
                if let Some(b) = other {
                    iteration_bindings(b, names);
                }
            }
            LStmtKind::Do(b) | LStmtKind::While(_, b) | LStmtKind::Repeat(b, _) => {
                iteration_bindings(b, names)
            }
            _ => {}
        }
    }
}
fn free_names(f: &LFunction) -> Names {
    fn expr(e: &LExpr, bound: &Names, free: &mut Names) {
        match e {
            LExpr::Name(n) => {
                if !bound.contains(n) {
                    free.insert(n.clone());
                }
            }
            LExpr::Field(e, _) | LExpr::Paren(e) | LExpr::Unary(_, e) => expr(e, bound, free),
            LExpr::Index(a, b) | LExpr::Binary(_, a, b) => {
                expr(a, bound, free);
                expr(b, bound, free);
            }
            LExpr::Call(e, args) | LExpr::Method(e, _, args) => {
                expr(e, bound, free);
                for a in args {
                    expr(a, bound, free);
                }
            }
            LExpr::Function(f) => {
                let mut inner = bound.clone();
                inner.extend(f.params.iter().cloned());
                block(&f.body, &inner, free);
            }
            LExpr::Table(fields) => {
                for f in fields {
                    match f {
                        LField::Pos(v) | LField::Named(_, v) => expr(v, bound, free),
                        LField::Keyed(k, v) => {
                            expr(k, bound, free);
                            expr(v, bound, free);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn block(b: &LBlock, outer: &Names, free: &mut Names) {
        let mut bound = outer.clone();
        for s in b {
            match &s.kind {
                LStmtKind::Local(ns, vs) => {
                    for v in vs {
                        expr(v, &bound, free);
                    }
                    bound.extend(ns.iter().cloned());
                }
                LStmtKind::Assign(ts, vs) => {
                    for v in ts.iter().chain(vs) {
                        expr(v, &bound, free);
                    }
                }
                LStmtKind::Call(e) => expr(e, &bound, free),
                LStmtKind::Return(vs) => {
                    for v in vs {
                        expr(v, &bound, free);
                    }
                }
                LStmtKind::If(arms, other) => {
                    for (c, b) in arms {
                        expr(c, &bound, free);
                        block(b, &bound, free);
                    }
                    if let Some(b) = other {
                        block(b, &bound, free);
                    }
                }
                LStmtKind::While(c, b) | LStmtKind::Repeat(b, c) => {
                    expr(c, &bound, free);
                    block(b, &bound, free);
                }
                LStmtKind::Do(b) => block(b, &bound, free),
                LStmtKind::NumFor {
                    var,
                    start,
                    stop,
                    step,
                    body,
                } => {
                    expr(start, &bound, free);
                    expr(stop, &bound, free);
                    if let Some(s) = step {
                        expr(s, &bound, free);
                    }
                    let mut inner = bound.clone();
                    inner.insert(var.clone());
                    block(body, &inner, free);
                }
                LStmtKind::GenFor { names, exprs, body } => {
                    for e in exprs {
                        expr(e, &bound, free);
                    }
                    let mut inner = bound.clone();
                    inner.extend(names.iter().cloned());
                    block(body, &inner, free);
                }
                LStmtKind::LocalFunction(n, f) => {
                    bound.insert(n.clone());
                    expr(&LExpr::Function(f.clone()), &bound, free);
                }
                LStmtKind::Function(n, f) => {
                    expr(&name(n.split(['.', ':']).next().unwrap()), &bound, free);
                    expr(&LExpr::Function(f.clone()), &bound, free);
                }
                LStmtKind::Raw(_) => {
                    free.insert("<raw>".into());
                }
                _ => {}
            }
        }
    }
    let mut free = Names::new();
    block(&f.body, &f.params.iter().cloned().collect(), &mut free);
    free
}

pub(crate) fn uses_errors(program: &selu_syntax::Program) -> bool {
    fn inspect(c: selu_syntax::visit::Child<'_>, found: &mut bool) {
        use selu_syntax::visit::{self, Child};
        match c {
            Child::Stmt(s) => {
                *found |= matches!(s.kind, StmtKind::Try { .. } | StmtKind::Throw { .. });
                visit::child_stmts_and_exprs(s, &mut |c| inspect(c, found));
            }
            Child::Expr(e) => {
                *found |=
                    matches!(&e.kind, selu_syntax::ExprKind::New {class,..} if class=="Error");
                visit::child_exprs_and_stmts(e, &mut |c| inspect(c, found));
            }
        }
    }
    let mut found = false;
    for s in &program.stmts {
        inspect(selu_syntax::visit::Child::Stmt(s), &mut found);
    }
    found
}
impl Lowerer<'_> {
    pub(crate) fn error_captures(&mut self, program: &selu_syntax::Program, out: &mut LBlock) {
        if uses_errors(program) {
            out.push(crate::raw("local __selu_pcall, __selu_error, __selu_tostring = pcall, error, tostring\nlocal __selu_error_setmetatable, __selu_error_getmetatable = setmetatable, getmetatable"));
        }
    }
    pub(crate) fn error_setup(&mut self, program: &selu_syntax::Program) -> LBlock {
        if !uses_errors(program) {
            return vec![];
        }
        let mut width = 0;
        fn inspect(
            c: selu_syntax::visit::Child<'_>,
            types: &selu_check::TypeTable,
            width: &mut usize,
        ) {
            use selu_syntax::visit::{self, Child};
            match c {
                Child::Stmt(s) => {
                    if matches!(s.kind, StmtKind::Return(_)) {
                        *width = (*width).max(return_width(s, types));
                    }
                    visit::child_stmts_and_exprs(s, &mut |c| inspect(c, types, width));
                }
                Child::Expr(e) => {
                    visit::child_exprs_and_stmts(e, &mut |c| inspect(c, types, width))
                }
            }
        }
        for s in &program.stmts {
            inspect(selu_syntax::visit::Child::Stmt(s), self.types, &mut width);
        }
        let mut results = vec!["ok".to_string(), "status".to_string()];
        results.extend((1..=width).map(|i| format!("value{i}")));
        let results = results.join(", ");
        vec![
            crate::raw(ERROR_RUNTIME),
            crate::raw(&format!(
                "local function __selu_protect(fn, ...)\n  __selu_errors.depth = __selu_errors.depth + 1\n  local {results} = __selu_pcall(fn, ...)\n  __selu_errors.depth = __selu_errors.depth - 1\n  return {results}\nend"
            )),
        ]
    }
}
const ERROR_RUNTIME: &str = r#"if not __selu_ns.__selu_errors then
  __selu_ns.__selu_errors = { depth = 0, meta = {} }
  __selu_ns.__selu_errors.meta.__tostring = function(value)
    return (value.__selu_position or "") .. value.message
  end
end
local __selu_errors = __selu_ns.__selu_errors
local function __selu_error_text(value)
  return __selu_tostring(value) .. ""
end
local function __selu_safe_error_text(value)
  local ok, message = __selu_pcall(__selu_error_text, value)
  if ok then return message end
  return "<error object>"
end
local function __selu_new_error(message, value)
  return __selu_error_setmetatable({ message = message, value = value }, __selu_errors.meta)
end
local function __selu_is_error(value)
  if __selu_error_getmetatable(value) == __selu_errors.meta then return true end
  return false
end
local function __selu_wrap_error(value)
  local ok, isError = __selu_pcall(__selu_is_error, value)
  if ok and isError then return value end
  return __selu_new_error(__selu_safe_error_text(value), value)
end
local function __selu_raise(value)
  if __selu_errors.depth == 0 then
    __selu_error(__selu_safe_error_text(value), 0)
  else
    __selu_error(value, 0)
  end
end
local function __selu_throw(value, position)
  if value.__selu_position == nil then value.__selu_position = position end
  __selu_raise(value)
end"#;

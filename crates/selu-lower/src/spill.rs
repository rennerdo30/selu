//! Lua 5.1 allows at most 200 active local variables in one function, and a
//! file's top level is one function. A large module's top-level bindings
//! (functions, constants, classes, imports, temporaries) can pass that
//! limit, and the client then refuses to load the file ("main function has
//! more than 200 local variables").
//!
//! When a file's top level declares more than `LIMIT` locals, the
//! declarations past the limit become fields of one module table
//! (`__selu_spill`), and every later reference to them is rewritten to that
//! field. Inner scopes that declare the same name (parameters, locals, loop
//! variables) keep their own binding. A name written in a Raw statement
//! (hand-written runtime Lua the pass cannot rewrite) is never spilled.

use crate::lua::{LBlock, LExpr, LField, LFunction, LStmt, LStmtKind};
use std::collections::HashSet;

/// Top-level locals kept as locals. The rest of Lua's 200 is headroom for
/// locals inside top-level `do` blocks and the spill table itself.
pub(crate) const LIMIT: usize = 180;
pub(crate) const TABLE: &str = "__selu_spill";

fn raw_locals(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("local function ") {
            if let Some(name) = rest.split('(').next() {
                names.push(name.trim().to_owned());
            }
        } else if let Some(rest) = line.strip_prefix("local ") {
            let head = rest.split('=').next().unwrap_or("");
            for name in head.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    names.push(name.to_owned());
                }
            }
        }
    }
    names
}

fn declared(stmt: &LStmt) -> Vec<String> {
    match &stmt.kind {
        LStmtKind::Local(names, _) => names.clone(),
        LStmtKind::LocalFunction(name, _) => vec![name.clone()],
        LStmtKind::Raw(text) => raw_locals(text),
        _ => Vec::new(),
    }
}

fn words(text: &str) -> HashSet<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Moves top-level declarations past `LIMIT` into the spill table.
pub(crate) fn spill(block: &mut LBlock) {
    let total: usize = block.iter().map(|s| declared(s).len()).sum();
    if total <= LIMIT {
        return;
    }
    let mut in_raw: HashSet<String> = HashSet::new();
    for stmt in block.iter() {
        if let LStmtKind::Raw(text) = &stmt.kind {
            in_raw.extend(words(text));
        }
    }
    // The table itself is one local.
    let mut running = 1usize;
    let mut spilled: HashSet<String> = HashSet::new();
    let old = std::mem::take(block);
    let mut out: LBlock = Vec::with_capacity(old.len() + 1);
    out.push(LStmt {
        kind: LStmtKind::Local(vec![TABLE.into()], vec![LExpr::Table(Vec::new())]),
        span: None,
    });
    for mut stmt in old {
        let names = declared(&stmt);
        let movable = !matches!(stmt.kind, LStmtKind::Raw(_))
            && !names.is_empty()
            && names.iter().all(|n| !in_raw.contains(n) && n != TABLE);
        let spill_this = movable && running + names.len() > LIMIT;
        match &mut stmt.kind {
            LStmtKind::Local(names, values) => {
                // Values see the bindings before this statement.
                for v in values.iter_mut() {
                    expr(v, &spilled, &mut Scope::default());
                }
                if spill_this {
                    for n in names.iter() {
                        spilled.insert(n.clone());
                    }
                    let targets: Vec<LExpr> = names.iter().map(|n| field(n)).collect();
                    let values = if values.is_empty() {
                        vec![LExpr::Nil; targets.len()]
                    } else {
                        std::mem::take(values)
                    };
                    stmt.kind = LStmtKind::Assign(targets, values);
                } else {
                    for n in names.iter() {
                        spilled.remove(n);
                    }
                    running += names.len();
                }
            }
            LStmtKind::LocalFunction(name, f) => {
                if spill_this {
                    spilled.insert(name.clone());
                    function(f, &spilled);
                    let target = field(name);
                    let value = LExpr::Function(std::mem::replace(
                        f,
                        LFunction {
                            params: Vec::new(),
                            vararg: false,
                            body: Vec::new(),
                        },
                    ));
                    stmt.kind = LStmtKind::Assign(vec![target], vec![value]);
                } else {
                    spilled.remove(name);
                    running += 1;
                    function(f, &spilled);
                }
            }
            LStmtKind::Raw(_) => {
                running += names.len();
            }
            _ => {
                let mut scope = Scope::default();
                statement(&mut stmt, &spilled, &mut scope);
            }
        }
        out.push(stmt);
    }
    *block = out;
}

fn field(name: &str) -> LExpr {
    LExpr::Field(Box::new(LExpr::Name(TABLE.into())), name.to_owned())
}

/// Names bound by enclosing inner scopes (they shadow spilled names).
#[derive(Default, Clone)]
struct Scope {
    bound: HashSet<String>,
}

fn function(f: &mut LFunction, spilled: &HashSet<String>) {
    let mut scope = Scope::default();
    for p in &f.params {
        scope.bound.insert(p.clone());
    }
    block(&mut f.body, spilled, &mut scope);
}

fn block(b: &mut LBlock, spilled: &HashSet<String>, scope: &mut Scope) {
    // Locals declared in this block are visible to the statements after
    // them only, and not outside the block.
    let mut inner = scope.clone();
    for stmt in b.iter_mut() {
        statement(stmt, spilled, &mut inner);
    }
}

fn statement(stmt: &mut LStmt, spilled: &HashSet<String>, scope: &mut Scope) {
    match &mut stmt.kind {
        LStmtKind::Local(names, values) => {
            for v in values.iter_mut() {
                expr(v, spilled, scope);
            }
            for n in names.iter() {
                scope.bound.insert(n.clone());
            }
        }
        LStmtKind::LocalFunction(name, f) => {
            scope.bound.insert(name.clone());
            let mut inner = scope.clone();
            for p in &f.params {
                inner.bound.insert(p.clone());
            }
            block(&mut f.body, spilled, &mut inner);
        }
        LStmtKind::Function(name, f) => {
            let mut inner = scope.clone();
            for p in &f.params {
                inner.bound.insert(p.clone());
            }
            let method = name.contains(':');
            if method {
                inner.bound.insert("self".into());
            }
            block(&mut f.body, spilled, &mut inner);
            let root = name.split(['.', ':']).next().unwrap_or("").to_owned();
            if spilled.contains(&root) && !scope.bound.contains(&root) {
                // `function a.b:c(...)` becomes `__selu_spill.a.b.c = function(self, ...)`.
                let mut target = field(&root);
                for part in name[root.len()..].split(['.', ':']).filter(|p| !p.is_empty()) {
                    target = LExpr::Field(Box::new(target), part.to_owned());
                }
                let mut value = std::mem::replace(
                    f,
                    LFunction {
                        params: Vec::new(),
                        vararg: false,
                        body: Vec::new(),
                    },
                );
                if method {
                    value.params.insert(0, "self".into());
                }
                stmt.kind = LStmtKind::Assign(vec![target], vec![LExpr::Function(value)]);
            }
        }
        LStmtKind::Assign(targets, values) => {
            for t in targets.iter_mut() {
                expr(t, spilled, scope);
            }
            for v in values.iter_mut() {
                expr(v, spilled, scope);
            }
        }
        LStmtKind::Call(e) => expr(e, spilled, scope),
        LStmtKind::If(arms, otherwise) => {
            for (cond, body) in arms.iter_mut() {
                expr(cond, spilled, scope);
                block(body, spilled, scope);
            }
            if let Some(body) = otherwise {
                block(body, spilled, scope);
            }
        }
        LStmtKind::While(cond, body) => {
            expr(cond, spilled, scope);
            block(body, spilled, scope);
        }
        LStmtKind::Repeat(body, cond) => {
            // `until` sees the body's locals.
            let mut inner = scope.clone();
            for stmt in body.iter_mut() {
                statement(stmt, spilled, &mut inner);
            }
            expr(cond, spilled, &mut inner);
        }
        LStmtKind::NumFor {
            var,
            start,
            stop,
            step,
            body,
        } => {
            expr(start, spilled, scope);
            expr(stop, spilled, scope);
            if let Some(step) = step {
                expr(step, spilled, scope);
            }
            let mut inner = scope.clone();
            inner.bound.insert(var.clone());
            block(body, spilled, &mut inner);
        }
        LStmtKind::GenFor { names, exprs, body } => {
            for e in exprs.iter_mut() {
                expr(e, spilled, scope);
            }
            let mut inner = scope.clone();
            for n in names.iter() {
                inner.bound.insert(n.clone());
            }
            block(body, spilled, &mut inner);
        }
        LStmtKind::Do(body) => block(body, spilled, scope),
        LStmtKind::Return(values) => {
            for v in values.iter_mut() {
                expr(v, spilled, scope);
            }
        }
        LStmtKind::Break | LStmtKind::Raw(_) | LStmtKind::Comment(_) => {}
    }
}

fn expr(e: &mut LExpr, spilled: &HashSet<String>, scope: &mut Scope) {
    match e {
        LExpr::Name(n) => {
            if spilled.contains(n.as_str()) && !scope.bound.contains(n.as_str()) {
                *e = field(n);
            }
        }
        LExpr::Field(object, _) => expr(object, spilled, scope),
        LExpr::Index(object, key) => {
            expr(object, spilled, scope);
            expr(key, spilled, scope);
        }
        LExpr::Call(f, args) => {
            expr(f, spilled, scope);
            for a in args.iter_mut() {
                expr(a, spilled, scope);
            }
        }
        LExpr::Method(object, _, args) => {
            expr(object, spilled, scope);
            for a in args.iter_mut() {
                expr(a, spilled, scope);
            }
        }
        LExpr::Binary(_, l, r) => {
            expr(l, spilled, scope);
            expr(r, spilled, scope);
        }
        LExpr::Unary(_, v) | LExpr::Paren(v) => expr(v, spilled, scope),
        LExpr::Function(f) => {
            let mut inner = scope.clone();
            for p in &f.params {
                inner.bound.insert(p.clone());
            }
            block(&mut f.body, spilled, &mut inner);
        }
        LExpr::Table(fields) => {
            for f in fields.iter_mut() {
                match f {
                    LField::Pos(v) | LField::Named(_, v) => expr(v, spilled, scope),
                    LField::Keyed(k, v) => {
                        expr(k, spilled, scope);
                        expr(v, spilled, scope);
                    }
                }
            }
        }
        LExpr::Nil | LExpr::Bool(_) | LExpr::Number(_) | LExpr::Str(_) | LExpr::Vararg => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::print_block;

    fn local(n: &str, v: LExpr) -> LStmt {
        LStmt {
            kind: LStmtKind::Local(vec![n.into()], vec![v]),
            span: None,
        }
    }

    #[test]
    fn a_small_file_is_unchanged() {
        let mut b = vec![local("a", LExpr::Number("1".into()))];
        let before = b.clone();
        spill(&mut b);
        assert_eq!(b, before);
    }

    #[test]
    fn locals_past_the_limit_become_fields_and_references_follow() {
        let mut b: LBlock = (0..LIMIT + 5)
            .map(|i| local(&format!("v{i}"), LExpr::Number(i.to_string())))
            .collect();
        let last = format!("v{}", LIMIT + 4);
        // A function that reads the spilled name and shadows it in a loop.
        b.push(LStmt {
            kind: LStmtKind::LocalFunction(
                "f".into(),
                LFunction {
                    params: vec!["x".into()],
                    vararg: false,
                    body: vec![
                        LStmt {
                            kind: LStmtKind::Call(LExpr::Call(
                                Box::new(LExpr::Name("print".into())),
                                vec![LExpr::Name(last.clone()), LExpr::Name("v0".into())],
                            )),
                            span: None,
                        },
                        LStmt {
                            kind: LStmtKind::NumFor {
                                var: last.clone(),
                                start: LExpr::Number("1".into()),
                                stop: LExpr::Number("2".into()),
                                step: None,
                                body: vec![LStmt {
                                    kind: LStmtKind::Call(LExpr::Call(
                                        Box::new(LExpr::Name("print".into())),
                                        vec![LExpr::Name(last.clone())],
                                    )),
                                    span: None,
                                }],
                            },
                            span: None,
                        },
                    ],
                },
            ),
            span: None,
        });
        spill(&mut b);
        let text = print_block(&b).text;
        let locals = text.lines().filter(|l| l.starts_with("local ")).count();
        assert!(locals <= LIMIT, "{locals} top-level locals");
        assert!(text.contains(&format!("__selu_spill.{last} = ")), "{text}");
        assert!(text.contains(&format!("print(__selu_spill.{last}, v0)")), "{text}");
        // The loop variable keeps its own binding.
        assert!(text.contains(&format!("print({last})")), "{text}");
    }
}

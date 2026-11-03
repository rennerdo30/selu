//! Language rules that don't depend on the target: names Lua can't use,
//! where assignment may appear, and which `??` forms the lowering can emit
//! correctly. Each rule prevents the compiler from writing invalid or wrong
//! Lua, so the user gets an error here instead of a load error in the game.

use selu_diagnostics::{Diagnostic, Span};
use selu_syntax::{Expr, ExprKind, Program, Stmt, StmtKind, is_lua_keyword};

/// Whether `a ?? b` can be lowered inline as `(a == nil and b or a)`: `a`
/// must be free of side effects (it's evaluated twice) and `b` must be a
/// value that is never `false` or `nil` (otherwise `and/or` picks the wrong
/// side). Every other `??` must sit where the lowering can use a statement.
pub fn inline_coalesce_ok(left: &Expr, right: &Expr) -> bool {
    left.dotted_name().is_some()
        && matches!(
            right.kind,
            ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Bool(true)
        )
}

pub fn check_rules(program: &Program) -> Vec<Diagnostic> {
    let mut r = Rules { out: Vec::new() };
    r.stmts(&program.stmts);
    r.out
}

struct Rules {
    out: Vec<Diagnostic>,
}

impl Rules {
    fn name(&mut self, name: &str, span: &Span) {
        if is_lua_keyword(name) {
            self.out.push(
                Diagnostic::error(
                    "SELU0304",
                    format!("`{name}` can't be used as a name: it is a reserved word in Lua"),
                )
                .with_span(span.clone())
                .with_help(format!(
                    "pick another name, for example `{name}Value` or `my{}`",
                    capitalize(name)
                )),
            );
        }
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::VarDecl {
                name,
                name_span,
                init,
                ..
            } => {
                self.name(name, name_span);
                if let Some(e) = init {
                    self.top_value(e);
                }
            }
            StmtKind::Global {
                name,
                name_span,
                init,
            } => {
                self.name(name, name_span);
                self.top_value(init);
            }
            StmtKind::FuncDecl {
                name,
                name_span,
                params,
                body,
                ..
            } => {
                self.name(name, name_span);
                for p in params {
                    self.name(&p.name, &p.span);
                }
                self.stmts(body);
            }
            StmtKind::OnHandler { params, body, .. } => {
                for p in params {
                    self.name(&p.name, &p.span);
                }
                self.stmts(body);
            }
            StmtKind::Return(Some(e)) => self.top_value(e),
            StmtKind::Return(None) => {}
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.expr(cond);
                self.stmts(then_branch);
                if let Some(e) = else_branch {
                    self.stmts(e);
                }
            }
            StmtKind::Block(b) => self.stmts(b),
            StmtKind::Expr(e) => match &e.kind {
                // The only place an assignment may appear.
                ExprKind::Assign { target, value } => {
                    if !matches!(target.kind, ExprKind::Ident(_) | ExprKind::Member { .. }) {
                        self.out.push(
                            Diagnostic::error(
                                "SELU0306",
                                "the left side of `=` must be a variable or a field",
                            )
                            .with_span(target.span.clone())
                            .with_help("did you mean `==` to compare?"),
                        );
                    }
                    self.expr(target);
                    self.top_value(value);
                }
                _ => self.expr(e),
            },
        }
    }

    /// A value in a position where the lowering can emit statements around
    /// it (declaration, assignment, return): a top-level `??` chain is fine
    /// in any form there.
    fn top_value(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Binary { op, left, right } if op == "??" => {
                self.top_value(left);
                self.expr(right);
            }
            _ => self.expr(e),
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Ident(name) => self.name(name, &e.span),
            ExprKind::Assign { .. } => self.out.push(
                Diagnostic::error(
                    "SELU0305",
                    "an assignment can't be used as a value; put it on its own line",
                )
                .with_span(e.span.clone())
                .with_help("did you mean `==` to compare?"),
            ),
            ExprKind::Binary { op, left, right } => {
                if op == "??" && !inline_coalesce_ok(left, right) {
                    self.out.push(
                        Diagnostic::error("SELU0308", "this `??` isn't supported here yet")
                            .with_span(e.span.clone())
                            .with_help("store it first: `const value = a ?? b;`, then use `value`"),
                    );
                }
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Unary { expr, .. } => self.expr(expr),
            ExprKind::Member { object, .. } => self.expr(object),
            ExprKind::Call { callee, args } => {
                if let ExprKind::Member { name, .. } = &callee.kind
                    && is_lua_keyword(name)
                {
                    self.out.push(
                        Diagnostic::error(
                            "SELU0309",
                            format!("a method named `{name}` can't be called: it is a reserved word in Lua"),
                        )
                        .with_span(callee.span.clone()),
                    );
                }
                self.expr(callee);
                for a in args {
                    self.expr(a);
                }
            }
            ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::Null => {}
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(src: &str) -> Vec<String> {
        let p = selu_syntax::parse("t.selu", src).unwrap();
        check_rules(&p).iter().map(|d| d.code.to_string()).collect()
    }

    #[test]
    fn lua_keywords_are_rejected_as_names() {
        assert_eq!(codes("const end = 1;\n"), ["SELU0304"]);
        assert_eq!(
            codes("function f(local) { return local; }\n"),
            ["SELU0304", "SELU0304"]
        );
        assert!(codes("const endTime = 1;\n").is_empty());
    }

    #[test]
    fn keyword_field_is_fine_but_keyword_method_is_not() {
        assert!(codes("const x = t.end;\n").is_empty());
        assert_eq!(codes("t.end(1);\n"), ["SELU0309"]);
    }

    #[test]
    fn assignment_only_as_a_statement() {
        assert!(codes("x = 1;\n").is_empty());
        assert!(codes("t.x = 1;\n").is_empty());
        assert_eq!(codes("print(x = 1);\n"), ["SELU0305"]);
        assert_eq!(codes("a = b = 1;\n"), ["SELU0305"]);
        assert_eq!(codes("f() = 1;\n"), ["SELU0306"]);
    }

    #[test]
    fn coalesce_forms() {
        // Statement positions: any form.
        assert!(codes("const x = f() ?? g();\n").is_empty());
        assert!(codes("const x = a ?? b ?? false;\n").is_empty());
        assert!(codes("function h() { return f() ?? 1; }\n").is_empty());
        assert!(codes("t.x = f() ?? null;\n").is_empty());
        // Inline: only a side-effect-free left side and a never-false right side.
        assert!(codes("print(a.b ?? 0);\n").is_empty());
        assert_eq!(codes("print(f() ?? 0);\n"), ["SELU0308"]);
        assert_eq!(codes("print(a ?? false);\n"), ["SELU0308"]);
        assert_eq!(codes("print(a ?? b);\n"), ["SELU0308"]);
    }
}

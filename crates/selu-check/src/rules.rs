//! Language rules that don't depend on the target or on types: names Lua
//! can't use, names reserved for the compiler, and where assignments may
//! appear. Each rule keeps the compiler from writing invalid or wrong Lua,
//! so the user gets an error here instead of a load error in the game.

use selu_diagnostics::{Diagnostic, Span};
use selu_syntax::visit::{Child, child_exprs_and_stmts, child_stmts_and_exprs};
use selu_syntax::{
    Expr, ExprKind, ForBinding, Param, Pattern, Program, Stmt, StmtKind, is_lua_keyword,
};

/// Generated Lua uses names with this prefix for its own locals.
pub const RESERVED_PREFIX: &str = "__selu";

pub fn check_rules(program: &Program) -> Vec<Diagnostic> {
    let mut r = Rules { out: Vec::new() };
    for s in &program.stmts {
        r.stmt(s);
    }
    r.out
}

struct Rules {
    out: Vec<Diagnostic>,
}

impl Rules {
    fn name(&mut self, name: &str, span: &Span) {
        if name == "_G" {
            self.out.push(
                Diagnostic::error(
                    "SELU0648",
                    "`_G` is reserved for the game environment and cannot be bound in Selu",
                )
                .with_span(span.clone()),
            );
        } else if is_lua_keyword(name) {
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
        } else if name.starts_with(RESERVED_PREFIX) {
            self.out.push(
                Diagnostic::error(
                    "SELU0318",
                    format!("names starting with `{RESERVED_PREFIX}` are reserved for the code Selu generates"),
                )
                .with_span(span.clone())
                .with_help("pick another name"),
            );
        }
    }

    fn params(&mut self, params: &[Param]) {
        for p in params {
            self.name(&p.name, &p.span);
        }
    }

    fn pattern(&mut self, p: &Pattern) {
        match p {
            Pattern::Array(items) => {
                for (name, span) in items.iter().flatten() {
                    self.name(name, span);
                }
            }
            Pattern::Object(items) => {
                for (_, name, span, _) in items {
                    self.name(name, span);
                }
            }
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::VarDecl {
                name, name_span, ..
            }
            | StmtKind::Global {
                name, name_span, ..
            } => self.name(name, name_span),
            StmtKind::FuncDecl {
                name,
                name_span,
                params,
                ..
            } => {
                self.name(name, name_span);
                self.params(params);
            }
            StmtKind::Import { names, default, .. } => {
                for (_, local, span, _) in names {
                    self.name(local, span);
                }
                if let Some((local, span)) = default {
                    self.name(local, span);
                }
            }
            StmtKind::Try {
                catch: Some((Some(p), _)),
                ..
            } => self.params(std::slice::from_ref(p)),
            StmtKind::OnHandler { params, .. } => self.params(params),
            StmtKind::Destructure { pattern, .. } => self.pattern(pattern),
            StmtKind::ForOf { binding, .. } => match binding {
                ForBinding::Name(n, span) => self.name(n, span),
                ForBinding::Pattern(p) => self.pattern(p),
            },
            _ => {}
        }
        match &s.kind {
            // The places an assignment may appear: a statement of its own,
            // and the update part of a counted `for`.
            StmtKind::Expr(e) => self.statement_expr(e),
            StmtKind::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    self.stmt(i);
                }
                if let Some(c) = cond {
                    self.expr(c);
                }
                if let Some(u) = update {
                    self.statement_expr(u);
                }
                for b in body {
                    self.stmt(b);
                }
            }
            _ => child_stmts_and_exprs(s, &mut |c| self.child(c)),
        }
    }

    fn child(&mut self, c: Child) {
        match c {
            Child::Stmt(s) => self.stmt(s),
            Child::Expr(e) => self.expr(e),
        }
    }

    /// An expression in statement position: assignments are allowed here.
    fn statement_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Assign { target, value } | ExprKind::CompoundAssign { target, value, .. } => {
                self.target(target);
                self.expr(value);
            }
            ExprKind::Update { target, .. } => self.target(target),
            _ => self.expr(e),
        }
    }

    fn target(&mut self, target: &Expr) {
        if !matches!(
            target.kind,
            ExprKind::Ident(_) | ExprKind::Member { .. } | ExprKind::Index { .. }
        ) {
            self.out.push(
                Diagnostic::error(
                    "SELU0306",
                    "the left side of an assignment must be a variable, a field or an element",
                )
                .with_span(target.span.clone())
                .with_help("did you mean `==` to compare?"),
            );
        }
        self.expr(target);
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Ident(name) => self.name(name, &e.span),
            ExprKind::Assign { .. } | ExprKind::CompoundAssign { .. } | ExprKind::Update { .. } => {
                self.out.push(
                    Diagnostic::error(
                        "SELU0305",
                        "an assignment can't be used as a value; put it on its own line",
                    )
                    .with_span(e.span.clone())
                    .with_help("did you mean `==` to compare?"),
                );
                return;
            }
            ExprKind::Function { params, .. } => self.params(params),
            ExprKind::Call { callee, .. } => {
                if let ExprKind::Member { name, .. } | ExprKind::OptionalMember { name, .. } =
                    &callee.kind
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
            }
            _ => {}
        }
        child_exprs_and_stmts(e, &mut |c| self.child(c));
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
        assert_eq!(codes("const [a, end] = f();\n"), ["SELU0304"]);
        assert_eq!(codes("for (const repeat of xs) {}\n"), ["SELU0304"]);
        assert_eq!(codes("const g = (until) => 1;\n"), ["SELU0304"]);
    }

    #[test]
    fn compiler_names_are_reserved() {
        assert_eq!(codes("const __selu_v = 1;\n"), ["SELU0318"]);
        assert!(codes("const __other = 1;\n").is_empty());
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
        assert!(codes("xs[0] = 1;\nx += 2;\nx++;\n").is_empty());
        assert!(codes("for (let i = 0; i < 3; i++) {}\n").is_empty());
        assert_eq!(codes("print(x = 1);\n"), ["SELU0305"]);
        assert_eq!(codes("print(x++);\n"), ["SELU0305"]);
        assert_eq!(codes("const y = (x += 1);\n"), ["SELU0305"]);
        assert_eq!(codes("a = b = 1;\n"), ["SELU0305"]);
        assert_eq!(codes("f() = 1;\n"), ["SELU0306"]);
        assert_eq!(codes("f()++;\n"), ["SELU0306"]);
    }
}

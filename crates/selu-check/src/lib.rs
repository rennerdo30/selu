//! Minimal semantic pass: every global call and every `on EVENT` handler
//! must exist on the chosen target (DESIGN.md sections 2-3). Calling a
//! global the target lacks is a diagnostic naming the target.

use selu_diagnostics::Diagnostic;
use selu_manifest::Manifest;
use selu_syntax::{Expr, ExprKind, Program, Stmt, StmtKind};
use std::collections::HashSet;

mod rules;

pub fn check(program: &Program, manifest: &Manifest) -> Vec<Diagnostic> {
    let mut locals = HashSet::new();
    collect_locals(&program.stmts, &mut locals);
    let mut out = rules::check_rules(program);
    let mut ctx = Ctx {
        manifest,
        locals: &locals,
        out: &mut out,
    };
    ctx.stmts(&program.stmts);
    out
}

fn collect_locals(stmts: &[Stmt], out: &mut HashSet<String>) {
    for s in stmts {
        match &s.kind {
            StmtKind::VarDecl { name, init, .. } => {
                out.insert(name.clone());
                let _ = init;
            }
            StmtKind::FuncDecl { name, params, body } => {
                out.insert(name.clone());
                for p in params {
                    out.insert(p.name.clone());
                }
                collect_locals(body, out);
            }
            StmtKind::OnHandler { params, body, .. } => {
                for p in params {
                    out.insert(p.name.clone());
                }
                collect_locals(body, out);
            }
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_locals(then_branch, out);
                if let Some(e) = else_branch {
                    collect_locals(e, out);
                }
            }
            StmtKind::Block(b) => collect_locals(b, out),
            _ => {}
        }
    }
}

struct Ctx<'a> {
    manifest: &'a Manifest,
    locals: &'a HashSet<String>,
    out: &'a mut Vec<Diagnostic>,
}

impl Ctx<'_> {
    fn missing(&mut self, name: &str, span: selu_diagnostics::Span) {
        self.out.push(
            Diagnostic::error(
                "SELU0301",
                format!(
                    "`{name}` does not exist on target {}; this call would fail when the game loads it",
                    self.manifest.target_string()
                ),
            )
            .with_span(span)
            .with_label(format!("no `{name}` on {}", self.manifest.target_string()))
            .with_help("check the spelling, pick another target, or guard the call for a target that has it"),
        );
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            match &s.kind {
                StmtKind::VarDecl { init, .. } => {
                    if let Some(e) = init {
                        self.expr(e)
                    }
                }
                StmtKind::FuncDecl { body, .. } => self.stmts(body),
                StmtKind::OnHandler { event, body, .. } => {
                    if self.manifest.event(event).is_none() {
                        self.out.push(
                            Diagnostic::error(
                                "SELU0302",
                                format!(
                                    "event `{event}` does not exist on target {}; the handler would never run",
                                    self.manifest.target_string()
                                ),
                            )
                            .with_span(s.span.clone())
                            .with_help("check the event spelling, or pick a target that has it"),
                        );
                    }
                    self.stmts(body);
                }
                StmtKind::Return(Some(e)) => self.expr(e),
                StmtKind::Return(None) => {}
                StmtKind::If {
                    cond,
                    then_branch,
                    else_branch,
                } => {
                    self.expr(cond);
                    self.stmts(then_branch);
                    if let Some(e) = else_branch {
                        self.stmts(e)
                    }
                }
                StmtKind::Block(b) => self.stmts(b),
                StmtKind::Expr(e) => self.expr(e),
            }
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Call { callee, args } => {
                self.check_callee(callee);
                // Still walk a non-identifier callee (a method's object can
                // itself contain calls), but not the name we just checked.
                if callee.dotted_name().is_none() {
                    self.expr(callee);
                } else if let ExprKind::Member { object, .. } = &callee.kind
                    && object.dotted_name().is_none()
                {
                    self.expr(object);
                }
                for a in args {
                    self.expr(a);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Unary { expr, .. } => self.expr(expr),
            ExprKind::Assign { target, value } => {
                self.expr(target);
                self.expr(value);
            }
            ExprKind::Member { object, .. } => self.expr(object),
            _ => {}
        }
    }

    fn check_callee(&mut self, callee: &Expr) {
        match &callee.kind {
            // `foo(...)`: a local, or a global the manifest must have.
            ExprKind::Ident(name) => {
                if !self.locals.contains(name) && self.manifest.function(name).is_none() {
                    self.missing(name, callee.span.clone());
                }
            }
            // `C_Foo.Bar(...)`: a manifest namespace call. Plain method
            // calls on objects (`frame.SetPoint(...)`) are not globals and
            // are left to later, typed passes.
            ExprKind::Member { .. } => {
                if let Some(full) = callee.dotted_name() {
                    let root = full.split('.').next().unwrap_or("");
                    if root.starts_with("C_")
                        && !self.locals.contains(root)
                        && self.manifest.function(&full).is_none()
                    {
                        self.missing(&full, callee.span.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn manifest() -> Manifest {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json");
        selu_manifest::load_manifest(&p).unwrap()
    }

    #[test]
    fn missing_global_is_named_with_target() {
        let p = selu_syntax::parse("t.selu", "NoSuchGlobal(1);\n").unwrap();
        let diags = check(&p, &manifest());
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "SELU0301");
        assert!(
            diags[0].message.contains("forever@1.0.0.1000"),
            "{}",
            diags[0].message
        );
    }

    #[test]
    fn known_global_local_and_method_calls_pass() {
        let src = "function f() { return 1; }\non PLAYER_LOGIN() { print(f()); frame.SetPoint(\"TOP\"); }\n";
        let p = selu_syntax::parse("t.selu", src).unwrap();
        assert!(check(&p, &manifest()).is_empty());
    }

    #[test]
    fn unknown_event_is_an_error() {
        let p = selu_syntax::parse("t.selu", "on NOT_AN_EVENT() { print(1); }\n").unwrap();
        let diags = check(&p, &manifest());
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "SELU0302");
    }
}

//! Semantic pass: names are resolved through real block scopes (SPEC-V1
//! 2.2), and every global call and `on EVENT` handler must exist on the
//! chosen target (DESIGN.md sections 2-3).
//!
//! Scoping follows TypeScript's `let`/`const`: a name belongs to the block
//! that declares it, inner blocks may shadow it, and it can't be used before
//! its declaration in the same function. A function body may refer to a
//! name declared later in an enclosing block; the lowering forward-declares
//! such names so the Lua sees the same variable.

use selu_diagnostics::{Diagnostic, Span};
use selu_manifest::Manifest;
use selu_syntax::{Expr, ExprKind, Param, Program, Stmt, StmtKind};
use std::collections::HashMap;

mod rules;

pub fn check(program: &Program, manifest: &Manifest) -> Vec<Diagnostic> {
    let mut out = rules::check_rules(program);
    let mut ctx = Ctx {
        manifest,
        out: &mut out,
        scopes: Vec::new(),
        fn_depth: 0,
    };
    ctx.block(&program.stmts, &[]);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Const,
    Let,
    Function,
    Param,
    Global,
}

#[derive(Debug, Clone)]
struct Binding {
    kind: Kind,
    /// Function nesting depth of the declaring scope.
    fn_depth: usize,
    /// False until the declaration statement has run.
    ready: bool,
}

struct Ctx<'a> {
    manifest: &'a Manifest,
    out: &'a mut Vec<Diagnostic>,
    scopes: Vec<HashMap<String, Binding>>,
    fn_depth: usize,
}

impl Ctx<'_> {
    fn target(&self) -> String {
        self.manifest.target_string()
    }

    fn missing(&mut self, name: &str, span: Span) {
        let target = self.target();
        self.out.push(
            Diagnostic::error(
                "SELU0301",
                format!(
                    "`{name}` does not exist on target {target}; this call would fail when the game loads it"
                ),
            )
            .with_span(span)
            .with_label(format!("no `{name}` on {target}"))
            .with_help("check the spelling, declare it, pick another target, or guard the call for a target that has it"),
        );
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    /// Declares `name` in the innermost scope; only parameters start ready.
    fn declare(&mut self, name: &str, span: &Span, kind: Kind) {
        let fn_depth = self.fn_depth;
        let scope = self.scopes.last_mut().expect("a scope is open");
        if scope.contains_key(name) {
            self.out.push(
                Diagnostic::error(
                    "SELU0312",
                    format!("`{name}` is already declared in this block"),
                )
                .with_span(span.clone())
                .with_help(format!(
                    "rename one of them, or assign with `{name} = ...;` instead of declaring it again"
                )),
            );
            return;
        }
        scope.insert(
            name.to_string(),
            Binding {
                kind,
                fn_depth,
                ready: kind == Kind::Param,
            },
        );
    }

    fn mark_ready(&mut self, name: &str) {
        if let Some(b) = self.scopes.last_mut().and_then(|s| s.get_mut(name)) {
            b.ready = true;
        }
    }

    /// A block: opens a scope, declares everything the block declares (so a
    /// use before the declaration finds the right binding, not an outer
    /// one), then checks the statements in order.
    fn block(&mut self, stmts: &[Stmt], params: &[Param]) {
        self.scopes.push(HashMap::new());
        for p in params {
            self.declare(&p.name, &p.span, Kind::Param);
        }
        let top = self.scopes.len() == 1;
        for s in stmts {
            match &s.kind {
                StmtKind::VarDecl {
                    is_const,
                    name,
                    name_span,
                    ..
                } => {
                    let kind = if *is_const { Kind::Const } else { Kind::Let };
                    self.declare(name, name_span, kind);
                }
                StmtKind::FuncDecl {
                    name, name_span, ..
                } => self.declare(name, name_span, Kind::Function),
                StmtKind::Global {
                    name, name_span, ..
                } => self.declare_global(name, name_span, &s.span, top),
                _ => {}
            }
        }
        for s in stmts {
            self.stmt(s);
        }
        self.scopes.pop();
    }

    fn declare_global(&mut self, name: &str, name_span: &Span, stmt_span: &Span, top: bool) {
        if !top {
            self.out.push(
                Diagnostic::error(
                    "SELU0316",
                    "`global` can only be used at the top level of a file",
                )
                .with_span(stmt_span.clone())
                .with_help(format!(
                    "move `global {name} = ...;` to the top level, and assign it here with `{name} = ...;`"
                )),
            );
        } else if self.manifest.has_global(name) {
            self.out.push(
                Diagnostic::error(
                    "SELU0312",
                    format!(
                        "`{name}` is already a global of the game on target {}",
                        self.target()
                    ),
                )
                .with_span(name_span.clone())
                .with_help("pick a name with your addon's prefix, for example `MyAddon_Settings`"),
            );
        } else {
            self.declare(name, name_span, Kind::Global);
        }
    }

    fn function_body(&mut self, params: &[Param], body: &[Stmt]) {
        self.fn_depth += 1;
        self.block(body, params);
        self.fn_depth -= 1;
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::VarDecl { name, init, .. } => {
                if let Some(e) = init {
                    self.expr(e);
                }
                self.mark_ready(name);
            }
            StmtKind::Global { name, init, .. } => {
                self.expr(init);
                self.mark_ready(name);
            }
            StmtKind::FuncDecl {
                name, params, body, ..
            } => {
                // Ready before the body, so a function can call itself.
                self.mark_ready(name);
                self.function_body(params, body);
            }
            StmtKind::OnHandler {
                event,
                params,
                body,
            } => {
                if self.manifest.event(event).is_none() {
                    self.out.push(
                        Diagnostic::error(
                            "SELU0302",
                            format!(
                                "event `{event}` does not exist on target {}; the handler would never run",
                                self.target()
                            ),
                        )
                        .with_span(s.span.clone())
                        .with_help("check the event spelling, or pick a target that has it"),
                    );
                }
                self.function_body(params, body);
            }
            StmtKind::Return(Some(e)) => self.expr(e),
            StmtKind::Return(None) => {}
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.expr(cond);
                self.block(then_branch, &[]);
                if let Some(e) = else_branch {
                    self.block(e, &[]);
                }
            }
            StmtKind::Block(b) => self.block(b, &[]),
            StmtKind::Expr(e) => self.expr(e),
        }
    }

    /// A read of `name`.
    fn read(&mut self, name: &str, span: &Span) {
        match self.lookup(name).cloned() {
            Some(b) => self.check_ready(name, &b, span),
            None if self.manifest.has_global(name) => {}
            None => self.out.push(
                Diagnostic::error("SELU0310", format!("`{name}` is not declared"))
                    .with_span(span.clone())
                    .with_label(format!(
                        "no variable, function or game global `{name}` on {}",
                        self.target()
                    ))
                    .with_help(format!(
                        "declare it first, for example `const {name} = ...;`, or check the spelling"
                    )),
            ),
        }
    }

    fn check_ready(&mut self, name: &str, b: &Binding, span: &Span) {
        // Inside a nested function the name is read later, when the function
        // runs, so only a use in the declaring function itself is too early.
        if b.ready || b.fn_depth != self.fn_depth {
            return;
        }
        let (msg, help) = if b.kind == Kind::Function {
            (
                format!("`{name}` is called before its declaration"),
                format!("move `function {name}` above this line"),
            )
        } else {
            (
                format!("`{name}` is used before its declaration"),
                format!("move the declaration of `{name}` above this line"),
            )
        };
        self.out.push(
            Diagnostic::error("SELU0313", msg)
                .with_span(span.clone())
                .with_help(help),
        );
    }

    /// The target of `name = value;`.
    fn write(&mut self, name: &str, span: &Span) {
        match self.lookup(name).cloned() {
            Some(b) => {
                self.check_ready(name, &b, span);
                match b.kind {
                    Kind::Const => self.out.push(
                        Diagnostic::error(
                            "SELU0311",
                            format!("`{name}` is a `const` and can't be assigned again"),
                        )
                        .with_span(span.clone())
                        .with_help(format!(
                            "declare `{name}` with `let` if its value needs to change"
                        ))
                        .with_fix(format!("change `const {name}` to `let {name}`")),
                    ),
                    Kind::Function => self.out.push(
                        Diagnostic::error(
                            "SELU0311",
                            format!("`{name}` is a function and can't be assigned"),
                        )
                        .with_span(span.clone())
                        .with_help(format!("use a `let` variable if `{name}` needs to change")),
                    ),
                    Kind::Let | Kind::Param | Kind::Global => {}
                }
            }
            None if self.manifest.has_global(name) => self.out.push(
                Diagnostic::error(
                    "SELU0315",
                    format!("`{name}` belongs to the game and can't be replaced"),
                )
                .with_span(span.clone())
                .with_help("replacing a game global breaks other addons and Blizzard's UI; to run code when it is called, use `hook(...)`"),
            ),
            None => self.out.push(
                Diagnostic::error(
                    "SELU0314",
                    format!("`{name}` is not declared; writing a global needs `global`"),
                )
                .with_span(span.clone())
                .with_help(format!(
                    "declare a local with `let {name} = ...;`, or a global at the top of the file with `global {name} = ...;`"
                )),
            ),
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Ident(name) => self.read(name, &e.span),
            ExprKind::Call { callee, args } => {
                self.callee(callee);
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
                // The value runs first, then the store.
                self.expr(value);
                match &target.kind {
                    ExprKind::Ident(name) => self.write(name, &target.span),
                    _ => self.expr(target),
                }
            }
            ExprKind::Member { object, .. } => self.expr(object),
            ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::Null => {}
        }
    }

    fn callee(&mut self, callee: &Expr) {
        match &callee.kind {
            // `foo(...)`: a declared name, or a function the target has.
            ExprKind::Ident(name) => match self.lookup(name).cloned() {
                Some(b) => self.check_ready(name, &b, &callee.span),
                None => {
                    if self.manifest.function(name).is_none() {
                        self.missing(name, callee.span.clone());
                    }
                }
            },
            // `C_Foo.Bar(...)`: a namespace function the target must have,
            // unless `C_Foo` is declared in the code. Other method calls
            // (`frame.SetPoint(...)`) check their object; the methods
            // themselves are left to the typed pass.
            ExprKind::Member { object, .. } => {
                if let Some(full) = callee.dotted_name() {
                    let root = full.split('.').next().unwrap_or("");
                    if root.starts_with("C_") && self.lookup(root).is_none() {
                        if self.manifest.function(&full).is_none() {
                            self.missing(&full, callee.span.clone());
                        }
                        return;
                    }
                }
                self.expr(object);
            }
            _ => self.expr(callee),
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

    fn codes(src: &str) -> Vec<String> {
        let p = selu_syntax::parse("t.selu", src).unwrap_or_else(|d| panic!("{}", d.render(src)));
        check(&p, &manifest())
            .iter()
            .map(|d| d.code.to_string())
            .collect()
    }

    /// The source text a diagnostic underlines.
    fn spans(src: &str) -> Vec<String> {
        let p = selu_syntax::parse("t.selu", src).unwrap();
        check(&p, &manifest())
            .iter()
            .map(|d| src[d.span.as_ref().unwrap().range()].to_string())
            .collect()
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
        let src = "function f() { return 1; }\nconst frame = CreateFrame(\"Frame\");\non PLAYER_LOGIN() { print(f()); frame.SetPoint(\"TOP\", UIParent); }\n";
        assert!(codes(src).is_empty(), "{:?}", codes(src));
    }

    #[test]
    fn unknown_event_is_an_error() {
        assert_eq!(codes("on NOT_AN_EVENT() { print(1); }\n"), ["SELU0302"]);
    }

    #[test]
    fn undeclared_reads_are_errors_with_exact_spans() {
        let src = "print(count + 1);\nframe.Show();\n";
        assert_eq!(codes(src), ["SELU0310", "SELU0310"]);
        assert_eq!(spans(src), ["count", "frame"]);
        // Game globals and namespaces from the manifest are declared.
        assert!(codes("const p = UIParent;\nconst t = GameTooltip;\n").is_empty());
    }

    #[test]
    fn block_scoping_and_shadowing() {
        // An inner block may shadow; its name is gone after the block.
        let ok = "const x = 1;\nif (x > 0) { const x = 2; print(x); }\nprint(x);\n";
        assert!(codes(ok).is_empty(), "{:?}", codes(ok));
        let gone = "if (true) { const inner = 1; }\nprint(inner);\n";
        assert_eq!(codes(gone), ["SELU0310"]);
        // A parameter is shadowed by nothing in its own body: redeclaring it
        // is an error, like in TypeScript.
        assert_eq!(
            codes("function f(a) { let a = 2; return a; }\n"),
            ["SELU0312"]
        );
        assert_eq!(codes("let y = 1;\nlet y = 2;\n"), ["SELU0312"]);
        assert_eq!(spans("let y = 1;\nlet y = 2;\n"), ["y"]);
    }

    #[test]
    fn const_and_function_reassignment() {
        assert_eq!(codes("const x = 1;\nx = 2;\n"), ["SELU0311"]);
        assert_eq!(codes("function f() {}\nf = 2;\n"), ["SELU0311"]);
        assert!(codes("let x = 1;\nx = 2;\nfunction g(p) { p = 3; }\n").is_empty());
        // The const in an inner block shadows a let: assigning there fails,
        // assigning the outer one after the block is fine.
        assert_eq!(
            codes("let x = 1;\n{ const x = 2; x = 3; }\nx = 4;\n"),
            ["SELU0311"]
        );
    }

    #[test]
    fn use_before_declaration() {
        assert_eq!(codes("print(x);\nconst x = 1;\n"), ["SELU0313"]);
        assert_eq!(codes("const x = x;\n"), ["SELU0313"]);
        assert_eq!(codes("f();\nfunction f() {}\n"), ["SELU0313"]);
        // In a block, the later inner declaration wins over the outer name.
        assert_eq!(
            codes("const x = 1;\n{ print(x); const x = 2; }\n"),
            ["SELU0313"]
        );
        // Inside a function body the use happens later: fine.
        assert!(
            codes(
                "on PLAYER_LOGIN() { print(later(), limit); }\nfunction later() { return limit; }\nconst limit = 3;\n"
            )
            .is_empty()
        );
        // Recursion.
        assert!(codes("function f(n) { return f(n); }\n").is_empty());
    }

    #[test]
    fn globals_need_global() {
        assert_eq!(codes("Counter = 1;\n"), ["SELU0314"]);
        assert!(codes("global MyAddon_Count = 1;\nMyAddon_Count = 2;\n").is_empty());
        assert!(
            codes("on PLAYER_LOGIN() { MyAddon_Count = MyAddon_Count + 1; }\nglobal MyAddon_Count = 0;\n")
                .is_empty()
        );
        assert_eq!(codes("function f() { global Inner = 1; }\n"), ["SELU0316"]);
        assert_eq!(codes("print = 1;\n"), ["SELU0315"]);
        assert_eq!(codes("global UIParent = 1;\n"), ["SELU0312"]);
    }

    #[test]
    fn global_is_still_a_plain_name() {
        assert!(codes("const global = 1;\nprint(global);\n").is_empty());
    }
}

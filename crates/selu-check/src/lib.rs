//! Semantic pass: names are resolved through real block scopes, every
//! expression gets a type, and the rules of SPEC-V1 2.2 are enforced. Every
//! global call and `on EVENT` handler must exist on the chosen target
//! (DESIGN.md sections 2-3).
//!
//! Scoping follows TypeScript's `let`/`const`: a name belongs to the block
//! that declares it, inner blocks may shadow it, and it can't be used before
//! its declaration in the same function. A function body may refer to a
//! name declared later in an enclosing block; the lowering forward-declares
//! such names so the Lua sees the same variable.
//!
//! Types are inferred from values; only parameters are written. `T?` values
//! are narrowed by `x != null`, `x == null`, a bare `x` in a condition, `!`,
//! `&&`, `||` and early returns.

use selu_diagnostics::{Diagnostic, Span};
use selu_manifest::Manifest;
use selu_syntax::{Expr, ExprKind, Param, Program, Stmt, StmtKind, TypeExpr};
use std::collections::HashMap;

mod rules;
// One file per language area. The central `stmt`/`expr_inner` matches only
// dispatch; each area's checks live in its own file (docs/WORK-PACKAGES.md).
mod data;
mod functions;
mod loops;
pub mod types;
mod wow;

pub use types::{Type, TypeTable};

/// Diagnostics plus the type of every expression, for the lowering.
#[derive(Debug, Default)]
pub struct Checked {
    pub diagnostics: Vec<Diagnostic>,
    pub types: TypeTable,
}

impl Checked {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == selu_diagnostics::Severity::Error)
    }
}

/// Only the diagnostics; see [`check_program`] for the types too.
pub fn check(program: &Program, manifest: &Manifest) -> Vec<Diagnostic> {
    check_program(program, manifest).diagnostics
}

pub fn check_program(program: &Program, manifest: &Manifest) -> Checked {
    let mut out = rules::check_rules(program);
    let mut ctx = Ctx {
        manifest,
        out: &mut out,
        scopes: Vec::new(),
        fn_depth: 0,
        returns: Vec::new(),
        loops: Vec::new(),
        function_expected: Vec::new(),
        types: TypeTable::default(),
    };
    ctx.block(&program.stmts, &[], Vec::new());
    let types = ctx.types;
    Checked {
        diagnostics: out,
        types,
    }
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
    ty: Type,
    /// Function nesting depth of the declaring scope.
    fn_depth: usize,
    /// False until the declaration statement has run.
    ready: bool,
}

/// A narrower type for a name inside part of a scope (`x` is a `number`, not
/// a `number?`, after `if (x == null) return;`).
#[derive(Debug, Clone)]
struct Narrow {
    ty: Type,
    fn_depth: usize,
}

#[derive(Default)]
struct Scope {
    names: HashMap<String, Binding>,
    narrowed: HashMap<String, Narrow>,
}

type Narrowings = Vec<(String, Type)>;

struct Ctx<'a> {
    loops: Vec<loops::LoopCtx>,
    manifest: &'a Manifest,
    out: &'a mut Vec<Diagnostic>,
    scopes: Vec<Scope>,
    fn_depth: usize,
    /// Per function being checked: the type and span of every `return`.
    returns: Vec<Vec<(Type, Span)>>,
    function_expected: Vec<Option<Type>>,
    types: TypeTable,
}

fn err(code: &str, msg: impl Into<String>, span: &Span) -> Diagnostic {
    Diagnostic::error(code, msg).with_span(span.clone())
}

/// "a number", "a string?" for messages.
fn a(t: &Type) -> String {
    match t {
        Type::Null => "`null`".into(),
        Type::Void => "nothing".into(),
        Type::Function { .. } => "a function".into(),
        t => format!("a `{t}`"),
    }
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

    // ---- scopes -------------------------------------------------------

    /// The binding for `name` and the index of its scope.
    fn lookup(&self, name: &str) -> Option<(usize, &Binding)> {
        self.scopes
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, s)| s.names.get(name).map(|b| (i, b)))
    }

    /// The type of `name` at this point: a narrowing that applies here, or
    /// the declared type. A narrowing made in another function only applies
    /// to a `const`, since a `let` may change before that function runs.
    fn type_of(&self, name: &str) -> Option<Type> {
        let (at, b) = self.lookup(name)?;
        for scope in self.scopes[at..].iter().rev() {
            if let Some(n) = scope.narrowed.get(name)
                && (n.fn_depth == self.fn_depth || b.kind == Kind::Const)
            {
                return Some(n.ty.clone());
            }
        }
        Some(b.ty.clone())
    }

    fn narrow_here(&mut self, narrowings: Narrowings) {
        let fn_depth = self.fn_depth;
        let scope = self.scopes.last_mut().expect("a scope is open");
        for (name, ty) in narrowings {
            scope.narrowed.insert(name, Narrow { ty, fn_depth });
        }
    }

    /// Declares `name` in the innermost scope; only parameters start ready.
    fn declare(&mut self, name: &str, span: &Span, kind: Kind, ty: Type) {
        let fn_depth = self.fn_depth;
        let scope = self.scopes.last_mut().expect("a scope is open");
        if scope.names.contains_key(name) {
            self.out.push(
                err("SELU0312", format!("`{name}` is already declared in this block"), span)
                    .with_help(format!(
                        "rename one of them, or assign with `{name} = ...;` instead of declaring it again"
                    )),
            );
            return;
        }
        scope.names.insert(
            name.to_string(),
            Binding {
                kind,
                ty,
                fn_depth,
                ready: kind == Kind::Param,
            },
        );
    }

    /// Marks the declaration in the innermost scope as run, with its type.
    fn finish_decl(&mut self, name: &str, ty: Type) {
        if let Some(b) = self.scopes.last_mut().and_then(|s| s.names.get_mut(name)) {
            b.ready = true;
            b.ty = ty;
        }
    }

    fn annotation(&mut self, t: &Option<TypeExpr>) -> Option<Type> {
        let t = t.as_ref()?;
        Some(if types::is_void(t) {
            Type::Void
        } else {
            types::resolve(t, self.out)
        })
    }

    fn param_types(&mut self, params: &[Param]) -> Vec<Type> {
        params
            .iter()
            .map(|p| {
                if p.rest || p.default.is_some() {
                    self.params_with_extras(p);
                }
                self.annotation(&p.ty).unwrap_or(Type::Untyped)
            })
            .collect()
    }

    /// A block: opens a scope, declares everything the block declares (so a
    /// use before the declaration finds the right binding, not an outer
    /// one), then checks the statements in order.
    fn block(&mut self, stmts: &[Stmt], params: &[(Param, Type)], narrowings: Narrowings) {
        self.scopes.push(Scope::default());
        self.narrow_here(narrowings);
        for (p, ty) in params {
            self.declare(&p.name, &p.span, Kind::Param, ty.clone());
        }
        let top = self.scopes.len() == 1;
        for s in stmts {
            match &s.kind {
                StmtKind::VarDecl {
                    is_const,
                    name,
                    name_span,
                    ty,
                    ..
                } => {
                    // Until the declaration runs, a function body that uses
                    // the name sees its written type, or an unchecked one.
                    let kind = if *is_const { Kind::Const } else { Kind::Let };
                    let t = ty.as_ref().map(types::resolve_quiet);
                    self.declare(name, name_span, kind, t.unwrap_or(Type::Untyped));
                }
                StmtKind::FuncDecl {
                    name,
                    name_span,
                    params,
                    ret,
                    ..
                } => {
                    // The return type is known only after the body; until then
                    // calls see an unchecked result.
                    let ret = self.annotation(ret).unwrap_or(Type::Untyped);
                    let ty = self.function_signature(params, ret);
                    self.declare(name, name_span, Kind::Function, ty);
                }
                StmtKind::Global {
                    name, name_span, ..
                } => self.declare_global(name, name_span, &s.span, top),
                StmtKind::Destructure {
                    is_const, pattern, ..
                } => {
                    let kind = if *is_const { Kind::Const } else { Kind::Let };
                    for (name, span) in pattern_names(pattern) {
                        self.declare(name, span, kind, Type::Untyped);
                    }
                }
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
                err(
                    "SELU0316",
                    "`global` can only be used at the top level of a file",
                    stmt_span,
                )
                .with_help(format!(
                    "move `global {name} = ...;` to the top level, and assign it here with `{name} = ...;`"
                )),
            );
        } else if self.manifest.has_global(name) {
            self.out.push(
                err(
                    "SELU0312",
                    format!(
                        "`{name}` is already a global of the game on target {}",
                        self.target()
                    ),
                    name_span,
                )
                .with_help("pick a name with your addon's prefix, for example `MyAddon_Settings`"),
            );
        } else {
            self.declare(name, name_span, Kind::Global, Type::Untyped);
        }
    }

    /// Checks a function body and returns its return type: the written one,
    /// or the one inferred from its `return` statements.
    fn function_body(
        &mut self,
        params: &[Param],
        param_types: Vec<Type>,
        written_ret: Option<Type>,
        body: &[Stmt],
    ) -> Type {
        let loops = std::mem::take(&mut self.loops);
        self.fn_depth += 1;
        self.returns.push(Vec::new());
        let params: Vec<(Param, Type)> = params.iter().cloned().zip(param_types).collect();
        self.function_block(body, &params, written_ret.as_ref());
        let returns = self.returns.pop().unwrap_or_default();
        self.fn_depth -= 1;
        self.loops = loops;
        if let Some(written) = written_ret {
            for (t, span) in &returns {
                let ok = if *t == Type::Void {
                    written == Type::Void || written.is_optional() || written.is_open()
                } else {
                    t.assignable_to(&written)
                };
                if !ok {
                    self.out.push(err(
                        "SELU0337",
                        format!(
                            "this returns {}, but the function says it returns {}",
                            a(t),
                            a(&written)
                        ),
                        span,
                    ));
                }
            }
            return written;
        }
        infer_return(&returns, self.out)
    }

    // ---- statements ---------------------------------------------------

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::VarDecl {
                is_const,
                name,
                name_span,
                ty,
                init,
            } => {
                let written = self.annotation(ty);
                let ty = self.declared_type(*is_const, name, name_span, written, init.as_deref());
                self.finish_decl(name, ty);
            }
            StmtKind::Global {
                name,
                name_span,
                init,
            } => {
                let t = self.value(init);
                let t = self.inferred(name, name_span, t);
                self.finish_decl(name, t);
            }
            StmtKind::FuncDecl {
                name, params, body, ..
            } => {
                // Ready before the body, so a function can call itself.
                let declared = self.type_of(name);
                self.finish_decl(name, declared.clone().unwrap_or(Type::Error));
                if let Some(Type::Function {
                    params: param_types,
                    ret,
                    required,
                    rest,
                }) = declared
                {
                    let written = (*ret != Type::Untyped).then_some(*ret);
                    let ret = self.function_body(
                        params,
                        functions::body_param_types(params, &param_types, rest.as_deref()),
                        written,
                        body,
                    );
                    self.finish_decl(
                        name,
                        Type::Function {
                            params: self.checked_parameter_types(params, param_types),
                            ret: Box::new(ret),
                            required,
                            rest,
                        },
                    );
                }
            }
            StmtKind::OnHandler {
                event,
                params,
                body,
            } => {
                let entry = self.manifest.event(event).cloned();
                if entry.is_none() {
                    self.out.push(
                        err(
                            "SELU0302",
                            format!(
                                "event `{event}` does not exist on target {}; the handler would never run",
                                self.target()
                            ),
                            &s.span,
                        )
                        .with_help("check the event spelling, or pick a target that has it"),
                    );
                }
                // Parameter types come from the event in the manifest.
                let mut param_types = self.param_types(params);
                if let Some(entry) = entry {
                    for (t, p) in param_types.iter_mut().zip(&entry.params) {
                        if *t == Type::Untyped {
                            *t = types::manifest_type(&p.ty);
                        }
                    }
                }
                self.function_body(params, param_types, Some(Type::Untyped), body);
            }
            StmtKind::Return(e) => {
                let t = match e {
                    Some(e) => self.function_return(e),
                    None => Type::Void,
                };
                if let Some(r) = self.returns.last_mut() {
                    r.push((t, s.span.clone()));
                } else {
                    self.out.push(err(
                        "SELU0317",
                        "`return` can only be used inside a function or handler",
                        &s.span,
                    ));
                }
            }
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let (then_n, else_n) = self.condition(cond);
                self.block(then_branch, &[], then_n.clone());
                if let Some(e) = else_branch {
                    self.block(e, &[], else_n.clone());
                }
                // `if (x == null) return;`: after the `if`, x is not null.
                if always_exits(then_branch) {
                    self.narrow_here(else_n);
                } else if else_branch.as_deref().is_some_and(always_exits) {
                    self.narrow_here(then_n);
                }
            }
            StmtKind::Block(b) => self.block(b, &[], Vec::new()),
            StmtKind::While { .. }
            | StmtKind::DoWhile { .. }
            | StmtKind::For { .. }
            | StmtKind::ForOf { .. }
            | StmtKind::Break
            | StmtKind::Continue => self.loop_stmt(s),
            StmtKind::Destructure { .. } => self.destructure(s),
            StmtKind::Expr(e) => {
                // A statement may be a call returning nothing.
                self.expr(e);
            }
        }
    }

    /// The type of a declared variable, from its written type and value.
    fn declared_type(
        &mut self,
        is_const: bool,
        name: &str,
        name_span: &Span,
        written: Option<Type>,
        init: Option<&Expr>,
    ) -> Type {
        let Some(init) = init else {
            return match written {
                Some(t) if t.is_optional() && !is_const => t,
                Some(t) if !is_const && !t.is_open() => {
                    self.out.push(
                        err("SELU0322", format!("`{name}` has no starting value"), name_span)
                            .with_help(format!(
                                "give it one (`let {name}: {t} = ...;`), or allow null: `let {name}: {t}? = null;`"
                            )),
                    );
                    t
                }
                Some(t) if !is_const => t,
                _ => {
                    let keyword = if is_const { "const" } else { "let" };
                    self.out.push(
                        err(
                            "SELU0321",
                            format!("`{name}` needs a starting value"),
                            name_span,
                        )
                        .with_help(format!("write `{keyword} {name} = ...;`")),
                    );
                    written.unwrap_or(Type::Error)
                }
            };
        };
        let t = self.value_expected(init, written.as_ref());
        match written {
            Some(w) => {
                if !t.assignable_to(&w) {
                    self.out.push(err(
                        "SELU0336",
                        format!("`{name}` is {}, but this value is {}", a(&w), a(&t)),
                        &init.span,
                    ));
                }
                w
            }
            None => self.inferred(name, name_span, t),
        }
    }

    /// The type a declaration without a written type gets from its value.
    fn inferred(&mut self, name: &str, name_span: &Span, t: Type) -> Type {
        match t {
            Type::Null => {
                self.out.push(
                    err(
                        "SELU0321",
                        format!(
                            "`{name}` needs a type: `null` alone doesn't say what it will hold"
                        ),
                        name_span,
                    )
                    .with_help(format!(
                        "write the type, for example `let {name}: number? = null;`"
                    )),
                );
                Type::Error
            }
            Type::Void => Type::Error,
            t => t,
        }
    }

    // ---- names --------------------------------------------------------

    fn read(&mut self, name: &str, span: &Span) -> Type {
        match self.lookup(name).map(|(_, b)| b.clone()) {
            Some(b) => {
                self.check_ready(name, &b, span);
                self.type_of(name).unwrap_or(Type::Error)
            }
            None if self.manifest.has_global(name) => Type::Untyped,
            None => {
                self.out.push(
                    err("SELU0310", format!("`{name}` is not declared"), span)
                        .with_label(format!(
                            "no variable, function or game global `{name}` on {}",
                            self.target()
                        ))
                        .with_help(format!(
                            "declare it first, for example `const {name} = ...;`, or check the spelling"
                        )),
                );
                Type::Error
            }
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
        self.out.push(err("SELU0313", msg, span).with_help(help));
    }

    /// `name = value;` where the value has type `value`.
    fn write(&mut self, name: &str, span: &Span, value: &Type) {
        let Some((at, b)) = self.lookup(name).map(|(i, b)| (i, b.clone())) else {
            if self.manifest.has_global(name) {
                self.out.push(
                    err("SELU0315", format!("`{name}` belongs to the game and can't be replaced"), span)
                        .with_help("replacing a game global breaks other addons and Blizzard's UI; to run code when it is called, use `hook(...)`"),
                );
            } else {
                self.out.push(
                    err("SELU0314", format!("`{name}` is not declared; writing a global needs `global`"), span)
                        .with_help(format!(
                            "declare a local with `let {name} = ...;`, or a global at the top of the file with `global {name} = ...;`"
                        )),
                );
            }
            return;
        };
        self.check_ready(name, &b, span);
        match b.kind {
            Kind::Const => {
                self.out.push(
                    err(
                        "SELU0311",
                        format!("`{name}` is a `const` and can't be assigned again"),
                        span,
                    )
                    .with_help(format!(
                        "declare `{name}` with `let` if its value needs to change"
                    ))
                    .with_fix(format!("change `const {name}` to `let {name}`")),
                );
                return;
            }
            Kind::Function => {
                self.out.push(
                    err(
                        "SELU0311",
                        format!("`{name}` is a function and can't be assigned"),
                        span,
                    )
                    .with_help(format!("use a `let` variable if `{name}` needs to change")),
                );
                return;
            }
            Kind::Let | Kind::Param | Kind::Global => {}
        }
        if !value.assignable_to(&b.ty) {
            self.out.push(err(
                "SELU0336",
                format!("`{name}` is {}, but this value is {}", a(&b.ty), a(value)),
                span,
            ));
            return;
        }
        // Narrowing after the assignment: a non-null value narrows here; a
        // value that may be null undoes narrowings up to the declaration,
        // since this branch may or may not have run.
        let fn_depth = self.fn_depth;
        if b.ty.is_optional() && !value.is_optional() && !value.is_open() {
            self.narrow_here(vec![(name.to_string(), value.clone())]);
        } else {
            for scope in &mut self.scopes[at..] {
                scope.narrowed.insert(
                    name.to_string(),
                    Narrow {
                        ty: b.ty.clone(),
                        fn_depth,
                    },
                );
            }
        }
    }

    // ---- expressions --------------------------------------------------

    /// An expression whose value is used (stored, passed, returned, an
    /// operand): it must be a real value.
    fn value(&mut self, e: &Expr) -> Type {
        let t = self.expr(e);
        let t = self.check_tuple_value(e, t);
        match t {
            Type::Cond => {
                self.out.push(
                    err("SELU0327", "this `&&`/`||` mixes in a value that may be null, so its result isn't a boolean", &e.span)
                        .with_help("use it directly in an `if`, or compare first: `x != null && ...`"),
                );
                Type::Error
            }
            Type::Void => {
                self.out.push(err(
                    "SELU0332",
                    "this call returns nothing, so it has no value to use",
                    &e.span,
                ));
                Type::Error
            }
            t => t,
        }
    }

    /// An expression used as a condition (SPEC-V1 2.2: a `boolean` or a
    /// `T?`), and the narrowings that hold when it is true and when false.
    fn condition(&mut self, e: &Expr) -> (Narrowings, Narrowings) {
        let t = self.expr(e);
        self.check_condition_type(e, &t);
        self.narrowings(e)
    }

    fn check_condition_type(&mut self, e: &Expr, t: &Type) {
        let fix = match t {
            Type::Number => "compare it: `count != 0` or `count > 0`",
            Type::String => "compare it: `text != \"\"`",
            Type::Void => "this call returns nothing; call it on its own line",
            Type::Function { .. } => "did you mean to call it?",
            _ => return,
        };
        self.out.push(
            err(
                "SELU0328",
                format!("a condition must be a `boolean` or a value that may be null, not {}", a(t)),
                &e.span,
            )
            .with_help("Lua treats 0 and \"\" as true and JavaScript as false, so Selu asks you to say which you mean")
            .with_fix(fix),
        );
    }

    /// What a true or false condition says about optional names.
    fn narrowings(&self, e: &Expr) -> (Narrowings, Narrowings) {
        let optional_name = |x: &Expr| -> Option<(String, Type)> {
            let ExprKind::Ident(n) = &x.kind else {
                return None;
            };
            match self.type_of(n)? {
                Type::Optional(inner) => Some((n.clone(), *inner)),
                _ => None,
            }
        };
        match &e.kind {
            ExprKind::Ident(_) => match optional_name(e) {
                Some(n) => (vec![n], vec![]),
                None => (vec![], vec![]),
            },
            ExprKind::Unary { op, expr } if op == "!" => {
                let (t, f) = self.narrowings(expr);
                (f, t)
            }
            ExprKind::Binary { op, left, right } if op == "==" || op == "!=" => {
                let name = match (&left.kind, &right.kind) {
                    (_, ExprKind::Null) => optional_name(left),
                    (ExprKind::Null, _) => optional_name(right),
                    _ => None,
                };
                let narrowed: Narrowings = name.into_iter().collect();
                if op == "!=" {
                    (narrowed, vec![])
                } else {
                    (vec![], narrowed)
                }
            }
            ExprKind::Binary { op, left, right } if op == "&&" => {
                let (mut t, _) = self.narrowings(left);
                t.extend(self.narrowings(right).0);
                (t, vec![])
            }
            ExprKind::Binary { op, left, right } if op == "||" => {
                let (_, mut f) = self.narrowings(left);
                f.extend(self.narrowings(right).1);
                (vec![], f)
            }
            _ => (vec![], vec![]),
        }
    }

    fn expr(&mut self, e: &Expr) -> Type {
        let t = self.expr_inner(e);
        self.types.set(e.id, t.clone());
        t
    }

    fn expr_inner(&mut self, e: &Expr) -> Type {
        match &e.kind {
            ExprKind::Number(_) => Type::Number,
            ExprKind::Str(_) => Type::String,
            ExprKind::Bool(_) => Type::Boolean,
            ExprKind::Null => Type::Null,
            ExprKind::Ident(name) => self.read(name, &e.span),
            ExprKind::Call { callee, args } => self.call(e, callee, args),
            ExprKind::Unary { op, expr } => {
                if op == "!" {
                    let t = self.expr(expr);
                    self.check_condition_type(expr, &t);
                    Type::Boolean
                } else {
                    let t = self.value(expr);
                    if !t.is_open() && t != Type::Number {
                        self.out.push(err(
                            "SELU0325",
                            format!("`-` needs a number, not {}", a(&t)),
                            &expr.span,
                        ));
                    }
                    Type::Number
                }
            }
            ExprKind::Binary { op, left, right } => self.binary(e, op, left, right),
            ExprKind::Assign { target, value } => {
                // The value runs first, then the store.
                let t = self.value(value);
                match &target.kind {
                    ExprKind::Ident(name) => self.write(name, &target.span, &t),
                    _ => {
                        self.expr(target);
                    }
                }
                Type::Void
            }
            ExprKind::Member { object, name } => {
                let t = self.value(object);
                self.member(&t, name, object)
            }
            ExprKind::Conditional { .. }
            | ExprKind::CompoundAssign { .. }
            | ExprKind::Update { .. } => self.control_expr(e),
            ExprKind::Template { .. }
            | ExprKind::Array(_)
            | ExprKind::Object(_)
            | ExprKind::Index { .. }
            | ExprKind::OptionalMember { .. }
            | ExprKind::New { .. } => self.data_expr(e),
            ExprKind::Function { .. } => self.function_expr(e),
            ExprKind::TaggedTemplate { .. } => self.tagged_template(e),
        }
    }

    /// A construct whose package hasn't landed yet (docs/WORK-PACKAGES.md).
    fn unsupported(&mut self, what: &str, span: &Span) -> Type {
        self.out.push(
            err("SELU0390", format!("{what} isn't supported yet"), span).with_help(
                "this part of Selu is still being built; SPEC-V1.md describes how it will work",
            ),
        );
        Type::Error
    }

    fn member(&mut self, t: &Type, name: &str, object: &Expr) -> Type {
        match t {
            Type::Untyped | Type::Error => Type::Untyped,
            Type::Optional(_) | Type::Null => {
                let what = object.dotted_name().unwrap_or_else(|| "this value".into());
                self.out.push(
                    err(
                        "SELU0331",
                        format!("`{what}` may be null here, so `.{name}` could fail"),
                        &object.span,
                    )
                    .with_help(format!("check it first: `if ({what} != null) {{ ... }}`")),
                );
                Type::Error
            }
            t => {
                self.out.push(err(
                    "SELU0330",
                    format!("{} has no field `{name}`", capital(&a(t))),
                    &object.span,
                ));
                Type::Error
            }
        }
    }

    fn binary(&mut self, e: &Expr, op: &str, left: &Expr, right: &Expr) -> Type {
        match op {
            "&&" | "||" => {
                let l = self.expr(left);
                self.check_condition_type(left, &l);
                // The right side runs only when the left one is true (`&&`)
                // or false (`||`), so it sees that narrowing.
                let (t, f) = self.narrowings(left);
                self.scopes.push(Scope::default());
                self.narrow_here(if op == "&&" { t } else { f });
                let r = self.expr(right);
                self.scopes.pop();
                self.check_condition_type(right, &r);
                match (&l, &r) {
                    (Type::Boolean, Type::Boolean) => Type::Boolean,
                    _ if l.is_open() || r.is_open() => Type::Untyped,
                    _ => Type::Cond,
                }
            }
            "??" => {
                let l = self.value(left);
                let r = self.value(right);
                if !l.is_optional() && !l.is_open() {
                    self.out.push(
                        Diagnostic::warning("SELU0329", format!("the left side is {}, which is never null, so `??` never uses the right side", a(&l)))
                            .with_span(left.span.clone()),
                    );
                    return l;
                }
                if l == Type::Null {
                    return r;
                }
                let l = l.non_null();
                match Type::join(&l, &r) {
                    Some(t) => t,
                    None => {
                        self.out.push(err(
                            "SELU0339",
                            format!(
                                "both sides of `??` must have the same type; this is {} and {}",
                                a(&l),
                                a(&r)
                            ),
                            &e.span,
                        ));
                        Type::Error
                    }
                }
            }
            "==" | "!=" => {
                let l = self.value(left);
                let r = self.value(right);
                if Type::join(&l, &r).is_none() {
                    self.out.push(
                        err(
                            "SELU0326",
                            format!("{} and {} are never equal", capital(&a(&l)), a(&r)),
                            &e.span,
                        )
                        .with_help(
                            "Selu compares without converting types; convert one side first",
                        ),
                    );
                }
                Type::Boolean
            }
            "<" | "<=" | ">" | ">=" => {
                let l = self.value(left);
                let r = self.value(right);
                let ok = l.is_open()
                    || r.is_open()
                    || (l == r && matches!(l, Type::Number | Type::String));
                if !ok {
                    self.out.push(err(
                        "SELU0326",
                        format!(
                            "`{op}` compares two numbers or two strings, not {} and {}",
                            a(&l),
                            a(&r)
                        ),
                        &e.span,
                    ));
                }
                Type::Boolean
            }
            "+" => {
                let l = self.value(left);
                let r = self.value(right);
                match (&l, &r) {
                    (Type::Number, Type::Number) => Type::Number,
                    (Type::String, Type::String) => Type::String,
                    (Type::Error, _) | (_, Type::Error) => Type::Error,
                    (Type::Untyped, Type::Untyped) => {
                        self.out.push(
                            err(
                                "SELU0324",
                                "can't tell whether this `+` adds numbers or joins strings",
                                &e.span,
                            )
                            .with_help(
                                "give the values a type, for example a parameter `(count: number)`",
                            ),
                        );
                        Type::Error
                    }
                    (Type::Untyped, t @ (Type::Number | Type::String))
                    | (t @ (Type::Number | Type::String), Type::Untyped) => t.clone(),
                    _ => {
                        let help = if matches!(
                            (&l, &r),
                            (Type::Number, Type::String) | (Type::String, Type::Number)
                        ) {
                            "use a template string: `${a}${b}`"
                        } else {
                            "`+` adds two numbers or joins two strings"
                        };
                        self.out.push(
                            err(
                                "SELU0323",
                                format!("`+` can't combine {} and {}", a(&l), a(&r)),
                                &e.span,
                            )
                            .with_help(help),
                        );
                        Type::Error
                    }
                }
            }
            _ => {
                // - * / %
                let l = self.value(left);
                let r = self.value(right);
                for (t, side) in [(&l, left), (&r, right)] {
                    if !t.is_open() && *t != Type::Number {
                        self.out.push(err(
                            "SELU0325",
                            format!("`{op}` needs numbers, not {}", a(t)),
                            &side.span,
                        ));
                    }
                }
                Type::Number
            }
        }
    }

    fn call(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> Type {
        let callee_type = match &callee.kind {
            // `foo(...)`: a declared name, or a function the target has.
            ExprKind::Ident(name) => match self.lookup(name).map(|(_, b)| b.clone()) {
                Some(b) => {
                    self.check_ready(name, &b, &callee.span);
                    self.type_of(name).unwrap_or(Type::Error)
                }
                None => {
                    if self.manifest.function(name).is_none() {
                        self.missing(name, callee.span.clone());
                        Type::Error
                    } else {
                        Type::Untyped
                    }
                }
            },
            // `C_Foo.Bar(...)`: a namespace function the target must have,
            // unless `C_Foo` is declared in the code. Other method calls
            // (`frame.SetPoint(...)`) check their object; the methods
            // themselves are left for typed manifests.
            ExprKind::Member { object, name } => {
                let full = callee.dotted_name();
                let root = full
                    .as_deref()
                    .and_then(|f| f.split('.').next())
                    .unwrap_or("");
                if root.starts_with("C_") && self.lookup(root).is_none() {
                    let full = full.unwrap_or_default();
                    if self.manifest.function(&full).is_none() {
                        self.missing(&full, callee.span.clone());
                    }
                    Type::Untyped
                } else {
                    let t = self.value(object);
                    self.function_member(&t, name, object)
                }
            }
            _ => self.value(callee),
        };
        self.types.set(callee.id, callee_type.clone());
        let arg_types = self.function_arguments(args, &callee_type);
        match callee_type {
            Type::Function {
                params,
                ret,
                required,
                rest,
            } => {
                let name = callee
                    .dotted_name()
                    .unwrap_or_else(|| "this function".into());
                if args.len() < required || (rest.is_none() && args.len() > params.len()) {
                    self.out.push(err(
                        "SELU0334",
                        format!(
                            "`{name}` takes {} argument{}, but this call passes {}",
                            params.len(),
                            if params.len() == 1 { "" } else { "s" },
                            args.len()
                        ),
                        &e.span,
                    ));
                }
                for (i, (t, arg)) in arg_types.iter().zip(args).enumerate() {
                    let Some(p) = params.get(i).or(rest.as_deref()) else {
                        continue;
                    };
                    if !t.assignable_to(p) {
                        self.out.push(err(
                            "SELU0335",
                            format!(
                                "argument {} of `{name}` must be {}, but this is {}",
                                i + 1,
                                a(p),
                                a(t)
                            ),
                            &arg.span,
                        ));
                    }
                }
                *ret
            }
            Type::Untyped | Type::Error => Type::Untyped,
            t => {
                self.out.push(err(
                    "SELU0333",
                    format!("this is {}, not a function", a(&t)),
                    &callee.span,
                ));
                Type::Error
            }
        }
    }
}

/// Whether running `stmts` never reaches its end (it returns).
/// The names a destructuring pattern declares.
fn pattern_names(p: &selu_syntax::Pattern) -> Vec<(&String, &Span)> {
    match p {
        selu_syntax::Pattern::Array(items) => items.iter().flatten().map(|(n, s)| (n, s)).collect(),
        selu_syntax::Pattern::Object(items) => items.iter().map(|(_, n, s)| (n, s)).collect(),
    }
}

fn always_exits(stmts: &[Stmt]) -> bool {
    match stmts.last().map(|s| &s.kind) {
        Some(StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue) => true,
        Some(StmtKind::Block(b)) => always_exits(b),
        Some(StmtKind::If {
            then_branch,
            else_branch: Some(e),
            ..
        }) => always_exits(then_branch) && always_exits(e),
        _ => false,
    }
}

/// The return type of a function without a written one: `void` without a
/// value, the values' common type, and `T?` when some paths return nothing.
fn infer_return(returns: &[(Type, Span)], out: &mut Vec<Diagnostic>) -> Type {
    let mut result: Option<Type> = None;
    let mut bare = false;
    for (t, span) in returns {
        if *t == Type::Void {
            bare = true;
            continue;
        }
        result = match result {
            None => Some(t.clone()),
            Some(r) => match Type::join(&r, t) {
                Some(j) => Some(j),
                None => {
                    out.push(
                        err(
                            "SELU0338",
                            format!(
                                "this returns {}, but an earlier `return` gives {}",
                                a(t),
                                a(&r)
                            ),
                            span,
                        )
                        .with_help("a function returns one type; write `: T?` or split it in two"),
                    );
                    Some(Type::Error)
                }
            },
        };
    }
    match result {
        None => Type::Void,
        Some(t) if bare => Type::optional(t),
        Some(t) => t,
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
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

    #[test]
    fn truthiness_rule() {
        assert_eq!(codes("const n = 1;\nif (n) {}\n"), ["SELU0328"]);
        assert_eq!(codes("const s = \"a\";\nif (!s) {}\n"), ["SELU0328"]);
        assert!(codes("const n = 1;\nif (n != 0) {}\n").is_empty());
        assert!(codes("let x: number? = null;\nif (x) {}\nif (!x) {}\n").is_empty());
        assert!(codes("const b = true;\nif (b && !b) {}\n").is_empty());
        // Untyped game values are not checked yet.
        assert!(codes("if (UnitHealth(\"player\")) {}\n").is_empty());
    }

    #[test]
    fn plus_minus_and_comparisons() {
        assert!(codes("const a = 1 + 2;\nconst b = \"x\" + \"y\";\n").is_empty());
        assert_eq!(codes("const a = 1 + \"x\";\n"), ["SELU0323"]);
        assert_eq!(codes("const a = true + true;\n"), ["SELU0323"]);
        assert_eq!(codes("const a = \"x\" - 1;\n"), ["SELU0325"]);
        assert_eq!(codes("const a = -\"x\";\n"), ["SELU0325"]);
        assert_eq!(codes("const a = 1 == \"1\";\n"), ["SELU0326"]);
        assert_eq!(codes("const a = 1 < \"2\";\n"), ["SELU0326"]);
        // Untyped plus a known type takes that type; two untyped can't decide.
        assert!(codes("const t = GetTime() + 1;\n").is_empty());
        assert_eq!(codes("function f(a, b) { return a + b; }\n"), ["SELU0324"]);
        assert!(codes("function f(a: string, b) { return a + b; }\n").is_empty());
    }

    #[test]
    fn triple_equals_is_a_syntax_error_with_fix() {
        let e = selu_syntax::parse("t.selu", "const a = 1 === 1;\n").unwrap_err();
        assert_eq!(e.code, "SELU0109");
        assert_eq!(e.fix.as_deref(), Some("use `==`"));
        let e = selu_syntax::parse("t.selu", "const a = 1 !== 1;\n").unwrap_err();
        assert_eq!(e.fix.as_deref(), Some("use `!=`"));
    }

    #[test]
    fn declarations_and_assignments_are_typed() {
        assert_eq!(codes("let x = 1;\nx = \"a\";\n"), ["SELU0336"]);
        assert_eq!(codes("let x: string = 1;\n"), ["SELU0336"]);
        assert_eq!(codes("let x = null;\n"), ["SELU0321"]);
        assert_eq!(codes("let x;\n"), ["SELU0321"]);
        assert_eq!(codes("let x: number;\n"), ["SELU0322"]);
        assert!(codes("let x: number? = null;\nx = 3;\nx = null;\nlet y: string?;\n").is_empty());
        assert_eq!(codes("const x: Frame = 1;\n"), ["SELU0320"]);
    }

    #[test]
    fn optional_values_need_a_check() {
        // Using a `T?` where a `T` is needed fails until it is narrowed.
        let src = "function f(n: number?) { const m: number = n; return m; }\n";
        assert_eq!(codes(src), ["SELU0336"]);
        let narrowed = [
            "function f(n: number?) { if (n != null) { const m: number = n; } }\n",
            "function f(n: number?) { if (n == null) { return; } const m: number = n; }\n",
            "function f(n: number?) { if (!n) return; const m: number = n; }\n",
            "function f(n: number?) { if (n == null) { } else { const m: number = n; } }\n",
            "function f(n: number?) { if (n != null && n > 2) { } }\n",
            "function f(n: number?) { if (n == null || n > 2) { } }\n",
            "function f(n: number?) { const m: number = n ?? 0; }\n",
        ];
        for src in narrowed {
            assert!(codes(src).is_empty(), "{src}: {:?}", codes(src));
        }
        // An assignment that may be null undoes the narrowing.
        assert_eq!(
            codes("function f(n: number?) { if (n != null) { n = null; const m: number = n; } }\n"),
            ["SELU0336"]
        );
        assert_eq!(
            codes("function f(n: number?) { if (n == null || n > 2) { } return n > 1; }\n"),
            ["SELU0326"]
        );
        // `&&` with a `T?` is a condition, not a boolean value.
        assert_eq!(
            codes("function f(n: number?, b: boolean) { const c = n && b; }\n"),
            ["SELU0327"]
        );
        assert!(codes("function f(n: number?, b: boolean) { if (n && b) {} }\n").is_empty());
        assert_eq!(
            codes("const s: string? = null;\nprint(s.x);\n"),
            ["SELU0331"]
        );
        assert_eq!(codes("const n = 1;\nprint(n.x);\n"), ["SELU0330"]);
        assert_eq!(
            codes("const n: number? = 1;\nconst m = n ?? \"x\";\n"),
            ["SELU0339"]
        );
        let warn = codes("const n = 1;\nconst m = n ?? 2;\n");
        assert_eq!(warn, ["SELU0329"]);
    }

    #[test]
    fn functions_are_typed() {
        // Inferred return types.
        assert!(codes("function f() { return 1; }\nconst x: number = f();\n").is_empty());
        assert_eq!(
            codes(
                "function f(b: boolean) { if (b) return 1; return; }\nconst x: number = f(true);\n"
            ),
            ["SELU0336"]
        );
        assert_eq!(
            codes("function f(b: boolean) { if (b) return 1; return \"x\"; }\n"),
            ["SELU0338"]
        );
        assert_eq!(codes("function f(): string { return 1; }\n"), ["SELU0337"]);
        // Arity and argument types.
        assert_eq!(codes("function f(a: number) {}\nf();\n"), ["SELU0334"]);
        assert_eq!(codes("function f(a: number) {}\nf(\"x\");\n"), ["SELU0335"]);
        assert_eq!(codes("const n = 1;\nn();\n"), ["SELU0333"]);
        // Void has no value.
        assert_eq!(codes("function f() {}\nconst x = f();\n"), ["SELU0332"]);
        assert_eq!(codes("return 1;\n"), ["SELU0317"]);
        // Event parameters get their types from the manifest.
        assert_eq!(
            codes("on UNIT_HEALTH(unit) { const n: number = unit; }\n"),
            ["SELU0336"]
        );
    }

    #[test]
    fn constructs_still_being_built_are_reported_not_lowered() {
        for src in [
            "const xs = [1, 2];
",
            "const [a, b] = [1, 2];
",
            "const t = `a${1}`;
",
        ] {
            let c = codes(src);
            assert!(c.contains(&"SELU0390".to_string()), "{src}: {c:?}");
        }
    }
}

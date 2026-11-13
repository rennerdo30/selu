//! Lower checked Selu to readable Lua 5.1 (DESIGN.md section 4).
//!
//! The lowering builds a small Lua syntax tree (`lua.rs`) and prints it:
//! 2-space indent, local names preserved, only the brackets Lua needs, and
//! Blizzard method calls with `:` (`frame.SetPoint(...)` in Selu is
//! `frame:SetPoint(...)` in Lua). `on EVENT` handlers share one event frame.
//!
//! Lowering an expression may put statements in front of it (`pre`): a
//! temporary for `a ?? b` in the middle of a call, for example. `seq` keeps
//! the left-to-right evaluation order when that happens.

use selu_check::TypeTable;
use selu_diagnostics::Span;
use selu_manifest::Manifest;
use selu_syntax::{Expr, ExprKind, Param, Program, Stmt, StmtKind, visit};
use std::collections::HashSet;

pub mod lua;
// One file per language area; see docs/WORK-PACKAGES.md.
mod classes;
mod data;
mod errors;
mod functions;
mod loops;
mod modules;
mod patterns;
mod purity;
mod restrictions;
pub mod runtime;
pub mod saved;
mod spill;
mod foreign;
pub use errors::uses_errors;
pub use modules::ModuleBindings;
pub mod bindings;
pub mod targets;
mod wow;

use lua::{BinOp, LBlock, LExpr, LFunction, LStmt, LStmtKind, UnOp, bin, name, not};

#[derive(Debug, Clone)]
pub struct Lowered {
    pub(crate) block: LBlock,
    pub(crate) pieces: Vec<runtime::Piece>,
    pub bindings: Vec<bindings::Binding>,
    pub locale_keys: Vec<String>,
    pub lua: String,
    pub statement_lines: Vec<(u32, Span)>,
}

impl Lowered {
    /// Bind printed statement positions to the exact source text used by the build.
    pub fn source_map(&self, chunk: &str, path: &str, text: &str) -> selu_sourcemap::SourceMap {
        let mut map = selu_sourcemap::SourceMap::new(chunk);
        let source = map.add_source(path, text);
        for (generated, span) in &self.statement_lines {
            let prefix = &text[..span.start.min(text.len())];
            map.push(selu_sourcemap::Range {
                gen_start: *generated,
                gen_end: *generated,
                source,
                line: prefix.bytes().filter(|b| *b == b'\n').count() as u32 + 1,
                column: prefix.rsplit('\n').next().unwrap_or("").chars().count() as u32 + 1,
            });
        }
        map.finish();
        map
    }
}

const EVENT_PROLOGUE: &str = r#"-- Selu: shared event frame and dispatch table
local __selu_handlers = {}
local __selu_frame = CreateFrame("Frame")
__selu_frame:SetScript("OnEvent", function(self, event, ...)
  local handlers = __selu_handlers[event]
  if not handlers then return end
  for i = 1, #handlers do handlers[i](...) end
end)
local function __selu_on(event, fn)
  local handlers = __selu_handlers[event]
  if not handlers then
    handlers = {}
    __selu_handlers[event] = handlers
    __selu_frame:RegisterEvent(event)
  end
  handlers[#handlers + 1] = fn
end"#;

// With try/throw in the addon, an event is a callback entry: handlers run
// with no active Selu protection, even when fired inside a try.
fn event_prologue(errors: bool) -> String {
    if !errors {
        return EVENT_PROLOGUE.into();
    }
    EVENT_PROLOGUE.replace(
        "  for i = 1, #handlers do handlers[i](...) end\n",
        "  local depth = __selu_errors.depth\n  __selu_errors.depth = 0\n  for i = 1, #handlers do handlers[i](...) end\n  __selu_errors.depth = depth\n",
    )
}

// The shared function closes over its handler table and frame, created once.
fn module_event_prologue(errors: bool) -> String {
    format!(
        "if not __selu_ns.__selu_events then\n{}\n  __selu_ns.__selu_events = __selu_on\nend\nlocal __selu_on = __selu_ns.__selu_events",
        event_prologue(errors)
            .lines()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Lowers a program the checker accepted; `types` is the checker's table.
pub fn lower(program: &Program, manifest: &Manifest, types: &TypeTable) -> Lowered {
    lower_with_addon(program, manifest, types, "Addon")
}
pub fn lower_with_addon(
    program: &Program,
    manifest: &Manifest,
    types: &TypeTable,
    addon_name: &str,
) -> Lowered {
    let errors = errors::uses_errors(program);
    lower_addon_layout(
        program, manifest, types, addon_name, false, manifest, errors,
    )
}
fn lower_addon_layout(
    program: &Program,
    manifest: &Manifest,
    types: &TypeTable,
    addon_name: &str,
    shared: bool,
    availability: &Manifest,
    addon_errors: bool,
) -> Lowered {
    let known: HashSet<String> = manifest.functions.iter().map(|f| f.name.clone()).collect();
    let mut module = ModuleBindings::default();
    module.addon_name = addon_name.into();
    let mut l = Lowerer {
        module,
        saved_globals: saved::globals(program, addon_name),
        target: targets::TargetLowering {
            manifest: availability,
            universe: manifest,
            shared,
        },
        known: &known,
        types,
        temps: 0,
        origins: purity::Origins::collect(program),
        wow: wow::WowState::default(),
        class_receiver: "self".into(),
        classes: Default::default(),
        loops: Vec::new(),
        addon_errors,
        error_entry: false,
    };
    let mut out = l.assemble_file(program, false);
    let pieces = l.runtime_pieces(program);
    // Shared layout prints the file again after adding its prelude.
    if !shared {
        spill::spill(&mut out);
    }
    let printed = lua::print_block(&out);
    Lowered {
        block: out,
        pieces,
        bindings: l.wow.bindings,
        locale_keys: l.wow.keys,
        lua: printed.text,
        statement_lines: printed.lines,
    }
}

pub fn lower_module(
    program: &Program,
    manifest: &Manifest,
    types: &TypeTable,
    module: ModuleBindings,
) -> Lowered {
    lower_module_layout(program, manifest, types, module, false, manifest)
}
pub fn lower_module_layout(
    program: &Program,
    manifest: &Manifest,
    types: &TypeTable,
    module: ModuleBindings,
    shared: bool,
    availability: &Manifest,
) -> Lowered {
    if !program.stmts.iter().any(|s| {
        targets::contains_event(s)
            || matches!(
                s.kind,
                StmtKind::Saved { .. }
                    | StmtKind::Import { .. }
                    | StmtKind::Export(_)
                    | StmtKind::OnHandler { .. }
                    | StmtKind::AddonDecl { .. }
            )
    }) {
        return lower_addon_layout(
            program,
            manifest,
            types,
            &module.addon_name,
            shared,
            availability,
            module.addon_errors || errors::uses_errors(program),
        );
    }
    let addon_errors = module.addon_errors || errors::uses_errors(program);
    let known: HashSet<String> = manifest.functions.iter().map(|f| f.name.clone()).collect();
    let mut l = Lowerer {
        saved_globals: saved::globals(program, &module.addon_name),
        module,
        target: targets::TargetLowering {
            manifest: availability,
            universe: manifest,
            shared,
        },
        known: &known,
        types,
        temps: 0,
        origins: purity::Origins::collect(program),
        class_receiver: "self".into(),
        classes: Default::default(),
        wow: wow::WowState::default(),
        loops: Vec::new(),
        addon_errors,
        error_entry: false,
    };
    let mut out = l.assemble_file(program, true);
    let pieces = l.runtime_pieces(program);
    if !shared {
        spill::spill(&mut out);
    }
    let printed = lua::print_block(&out);
    Lowered {
        block: out,
        pieces,
        bindings: l.wow.bindings,
        locale_keys: l.wow.keys,
        lua: printed.text,
        statement_lines: printed.lines,
    }
}

fn raw(text: &str) -> LStmt {
    LStmt {
        kind: LStmtKind::Raw(text.into()),
        span: None,
    }
}

pub(crate) struct Lowerer<'a> {
    origins: purity::Origins,
    module: ModuleBindings,
    saved_globals: std::collections::HashMap<String, String>,
    wow: wow::WowState,
    class_receiver: String,
    classes: classes::ClassLoweringState,
    loops: Vec<loops::LoopCtx>,
    target: targets::TargetLowering<'a>,
    known: &'a HashSet<String>,
    types: &'a TypeTable,
    temps: u32,
    /// Some file of the addon uses try/throw: callback entries reset depth.
    addon_errors: bool,
    /// This file wrapped a callback in `__selu_entry`.
    error_entry: bool,
}

impl Lowerer<'_> {
    /// A fresh local name for a temporary value.
    pub(crate) fn temp(&mut self) -> String {
        self.temps += 1;
        format!("__selu_t{}", self.temps)
    }

    /// `stmt`, preceded by `pre`. A statement that declares nothing gets
    /// its temporaries in a `do ... end` of their own.
    pub(crate) fn finish(&mut self, pre: LBlock, stmt: LStmt, out: &mut LBlock) {
        if pre.is_empty() {
            out.push(stmt);
        } else {
            let span = stmt.span.clone();
            let mut body = pre;
            body.push(stmt);
            out.push(LStmt {
                kind: LStmtKind::Do(body),
                span,
            });
        }
    }

    // ---- blocks and statements ------------------------------------------

    /// A block. Names the block declares but uses earlier (only possible
    /// inside a function body that runs later) are declared up front with
    /// one `local a, b`, so the earlier code sees the same variable.
    pub(crate) fn block(&mut self, stmts: &[Stmt]) -> LBlock {
        let mut out = Vec::new();
        let early = used_before_declaration(stmts);
        if let Some(first) = stmts.first()
            && !early.is_empty()
        {
            out.push(LStmt::new(
                LStmtKind::Local(early.clone(), Vec::new()),
                &first.span,
            ));
        }
        for s in stmts {
            let forward = declaration_names(s).iter().any(|n| early.contains(n));
            self.stmt(s, forward, &mut out);
        }
        out
    }

    pub(crate) fn function(&mut self, params: &[Param], body: &[Stmt]) -> LFunction {
        let loops = std::mem::take(&mut self.loops);
        let result = self.function_with_extras(params, body);
        self.loops = loops;
        result
    }

    /// `forward`: the statement declares a name that was already declared
    /// with `local` at the top of the block, so it assigns instead.
    fn stmt(&mut self, s: &Stmt, forward: bool, out: &mut LBlock) {
        let span = &s.span;
        match &s.kind {
            StmtKind::Try { .. } | StmtKind::Throw { .. } => self.error_stmt(s, out),
            StmtKind::Saved { .. } | StmtKind::Migrate { .. } => {
                let start = out.len();
                self.saved_stmt(s, forward, out);
                for statement in &mut out[start..] {
                    statement.span = Some(span.clone());
                }
            }
            StmtKind::VarDecl { init: None, .. } if forward => {}
            StmtKind::VarDecl {
                name: n,
                init: None,
                ..
            } => out.push(LStmt::new(
                LStmtKind::Local(vec![n.clone()], Vec::new()),
                span,
            )),
            StmtKind::VarDecl {
                name: n,
                init: Some(e),
                ..
            } => self.assign_into(&name(n), !forward, e, span, out),
            StmtKind::Global { name: n, init, .. } => {
                self.assign_into(&name(n), false, init, span, out)
            }
            StmtKind::FuncDecl {
                name: n,
                params,
                body,
                ..
            } => {
                let f = self.function(params, body);
                let kind = if forward {
                    LStmtKind::Function(n.clone(), f)
                } else {
                    LStmtKind::LocalFunction(n.clone(), f)
                };
                out.push(LStmt::new(kind, span));
            }
            StmtKind::OnHandler {
                event,
                params,
                body,
            } => {
                let f = self.function(params, body);
                let call = lua::call(
                    name("__selu_on"),
                    vec![LExpr::Str(event.clone()), LExpr::Function(f)],
                );
                out.push(LStmt::new(LStmtKind::Call(call), span));
            }
            StmtKind::Return(None) => out.push(LStmt::new(LStmtKind::Return(Vec::new()), span)),
            StmtKind::Return(Some(e)) => {
                let mut pre = Vec::new();
                let values = self.function_return(e, &mut pre);
                self.finish(pre, LStmt::new(LStmtKind::Return(values), span), out);
            }
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => self.if_stmt(cond, then_branch, else_branch.as_deref(), span, out),
            StmtKind::Block(b) => {
                let body = self.block(b);
                out.push(LStmt::new(LStmtKind::Do(body), span));
            }
            StmtKind::Expr(e) => self.expr_stmt(e, span, out),
            StmtKind::While { .. }
            | StmtKind::DoWhile { .. }
            | StmtKind::For { .. }
            | StmtKind::Break
            | StmtKind::Continue => {
                let mut lowered = Vec::new();
                self.loop_stmt(s, &mut lowered);
                self.hoist_error_closures(&mut lowered, out);
                out.extend(lowered);
            }
            StmtKind::ForOf { .. } => {
                let mut lowered = Vec::new();
                self.for_of(s, &mut lowered);
                self.hoist_error_closures(&mut lowered, out);
                out.extend(lowered);
            }
            StmtKind::Destructure { .. } => self.destructure(s, forward, out),
            StmtKind::TypeDecl(_) => {}
            StmtKind::Import { .. } => self.import_stmt(s, out),
            StmtKind::AddonDecl { .. } => self.addon_decl(s, out),
            StmtKind::ExportAddon { .. } => self.export_addon(s, out),
            StmtKind::Export(inner) => self.export_stmt(s, inner, forward, out),
            StmtKind::Class(c) => self.class_decl(s, c, forward, out),
        }
    }

    fn if_stmt(
        &mut self,
        cond: &Expr,
        then_branch: &[Stmt],
        else_branch: Option<&[Stmt]>,
        span: &Span,
        out: &mut LBlock,
    ) {
        if self.target_if(cond, then_branch, else_branch, span, out) {
            return;
        }
        let mut pre = Vec::new();
        let c = targets::simplify(self.expr(cond, &mut pre));
        let mut arms = vec![(c, self.block(then_branch))];
        let mut otherwise = None;
        let mut rest = else_branch;
        // `else if` becomes `elseif`, unless its condition needs statements
        // first; then it stays an `if` inside the `else`.
        while let Some(stmts) = rest {
            if let [only] = stmts
                && let StmtKind::If {
                    cond,
                    then_branch,
                    else_branch,
                } = &only.kind
            {
                let mut cond_pre = Vec::new();
                let c = targets::simplify(self.expr(cond, &mut cond_pre));
                if let LExpr::Bool(b) = c
                    && cond_pre.is_empty()
                {
                    if b {
                        otherwise = Some(self.block(then_branch));
                        break;
                    }
                    rest = else_branch.as_deref();
                    continue;
                }
                if self.folded_has(cond).is_some() {
                    otherwise = Some(self.block(stmts));
                    break;
                }
                if cond_pre.is_empty() {
                    arms.push((c, self.block(then_branch)));
                    rest = else_branch.as_deref();
                    continue;
                }
            }
            otherwise = Some(self.block(stmts));
            break;
        }
        self.finish(pre, LStmt::new(LStmtKind::If(arms, otherwise), span), out);
    }

    fn expr_stmt(&mut self, e: &Expr, span: &Span, out: &mut LBlock) {
        match &e.kind {
            ExprKind::Assign { target, value } => match &target.kind {
                ExprKind::Ident(n) => self.assign_into(&name(n), false, value, span, out),
                // `t.x = v`: as in TypeScript, the object is evaluated before
                // the value. Lua reads a local `t` again at store time, so it
                // is held first when the value could run code that changes it.
                ExprKind::Member {
                    object,
                    name: field,
                } => {
                    let mut pre = Vec::new();
                    let vals = self.seq(&[object, value], &mut pre);
                    let [mut obj, v]: [LExpr; 2] = vals.try_into().expect("two operands");
                    // Only a plain name is read again at store time; Lua
                    // evaluates a longer object (`a.b`) before the value.
                    let reread = matches!(&obj, LExpr::Name(n) if !n.starts_with("__selu_t"));
                    if reread && self.may_run_code(value) && !self.is_stable(&obj, object) {
                        obj = name(&self.hold(obj, &object.span, &mut pre));
                    }
                    let t = LExpr::Field(Box::new(obj), field.clone());
                    let stmt = LStmt::new(LStmtKind::Assign(vec![t], vec![v]), span);
                    self.finish(pre, stmt, out);
                }
                _ => {
                    let mut pre = Vec::new();
                    let v = self.expr(value, &mut pre);
                    let t = self.target(target, &mut pre);
                    let stmt = LStmt::new(LStmtKind::Assign(vec![t], vec![v]), span);
                    self.finish(pre, stmt, out);
                }
            },
            ExprKind::CompoundAssign { .. } | ExprKind::Update { .. } => {
                self.control_stmt(e, span, out)
            }
            _ => {
                let mut pre = Vec::new();
                let v = self.expr(e, &mut pre);
                if v.is_pure() {
                    out.extend(pre);
                    return;
                }
                // Lua only allows calls as statements.
                let kind = match v {
                    LExpr::Call(..) | LExpr::Method(..) => LStmtKind::Call(v),
                    v => LStmtKind::Local(vec!["_".into()], vec![v]),
                };
                self.finish(pre, LStmt::new(kind, span), out);
            }
        }
    }

    /// The place an assignment stores into (`t.x`, `xs[i]`).
    pub(crate) fn target(&mut self, target: &Expr, pre: &mut LBlock) -> LExpr {
        match &target.kind {
            ExprKind::Ident(n) => name(n),
            ExprKind::Member {
                object,
                name: field,
            } => self.data_field(object, field, pre),
            _ => self.data_target(target, pre),
        }
    }

    /// `var = e` (or `local var = e` when `declare`). A `??` chain stored in
    /// a plain variable uses the variable itself, which reads best:
    ///
    /// ```lua
    /// local x = a
    /// if x == nil then x = b end
    /// ```
    pub(crate) fn assign_into(
        &mut self,
        var: &LExpr,
        declare: bool,
        e: &Expr,
        span: &Span,
        out: &mut LBlock,
    ) {
        let scalar_api_call = matches!(&e.kind, ExprKind::Call { callee, .. }
            if self.types.is_single_value(e.id) && matches!(self.types.get(callee.id),
                Some(selu_check::Type::Function { multiple_results: true, .. } | selu_check::Type::Untyped)));
        let store = |value: LExpr| {
            // A single assignment target already selects the first result.
            let value = match value {
                LExpr::Paren(inner) if scalar_api_call => *inner,
                value => value,
            };
            let kind = match (declare, var) {
                (true, LExpr::Name(n)) => LStmtKind::Local(vec![n.clone()], vec![value]),
                _ => LStmtKind::Assign(vec![var.clone()], vec![value]),
            };
            LStmt::new(kind, span)
        };
        let mut chain = Vec::new();
        flatten_coalesce(e, &mut chain);
        // The direct form must not read `var` after it was overwritten.
        let reads_var = |parts: &[&Expr]| match var {
            LExpr::Name(n) => parts.iter().any(|p| {
                let mut hit = false;
                visit::idents_in_expr(p, &mut |i| hit |= i == n);
                hit
            }),
            _ => true,
        };
        if chain.len() > 1 && (declare || !reads_var(&chain[1..])) {
            let mut pre = Vec::new();
            let first = self.expr(chain[0], &mut pre);
            let first = self.read_if_secret(chain[0], first);
            out.extend(pre);
            out.push(store(first));
            for (index, part) in chain[1..].iter().enumerate() {
                let mut part_pre = Vec::new();
                let v = self.expr(part, &mut part_pre);
                let v = if index + 2 < chain.len() {
                    self.read_if_secret(part, v)
                } else {
                    v
                };
                part_pre.push(LStmt::new(
                    LStmtKind::Assign(vec![var.clone()], vec![v]),
                    span,
                ));
                let test = bin(BinOp::Eq, var.clone(), LExpr::Nil);
                out.push(LStmt::new(
                    LStmtKind::If(vec![(test, part_pre)], None),
                    span,
                ));
            }
            return;
        }
        let mut pre = Vec::new();
        let v = self.expr(e, &mut pre);
        if declare {
            // The temporaries stay in the enclosing block: wrapping them would
            // hide the declared name.
            out.extend(pre);
            out.push(store(v));
        } else {
            self.finish(pre, store(v), out);
        }
    }

    // ---- expressions ----------------------------------------------------

    /// Lowers `exprs` in order. When a later one needs statements first, the
    /// earlier values that could change meanwhile are stored in temporaries,
    /// so everything still runs left to right.
    pub(crate) fn seq(&mut self, exprs: &[&Expr], pre: &mut LBlock) -> Vec<LExpr> {
        let mut vals: Vec<LExpr> = Vec::new();
        for e in exprs {
            let mut own = Vec::new();
            let v = self.expr(e, &mut own);
            if !own.is_empty() {
                for (prev, source) in vals.iter_mut().zip(exprs) {
                    if !self.is_stable(prev, source) {
                        let t = self.hold(prev.clone(), &e.span, pre);
                        *prev = name(&t);
                    }
                }
                pre.extend(own);
            }
            vals.push(v);
        }
        vals
    }

    /// A lowered value that later code can't change: a literal, or a name
    /// the checker marked stable (`const`, function, game global).
    pub(crate) fn is_stable(&self, v: &LExpr, source: &Expr) -> bool {
        is_constant(v) || matches!(v, LExpr::Name(_)) && self.types.is_stable(source.id)
    }

    /// A temporary holding `value`, declared in `pre`.
    /// `object?.name(args)`: the receiver is evaluated once; the call (and
    /// its arguments) run only when it is not nil. Several results come back
    /// through a function so that none is lost.
    fn optional_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr], pre: &mut LBlock) -> LExpr {
        let ExprKind::OptionalMember {
            object,
            name: method,
        } = &callee.kind
        else {
            unreachable!("an optional call has an optional member callee")
        };
        let value = self.expr(object, pre);
        let receiver = self.hold(value, &object.span, pre);
        let call = Expr {
            string_offsets: None,
            id: e.id,
            kind: ExprKind::Call {
                callee: Box::new(Expr {
                    string_offsets: None,
                    id: callee.id,
                    kind: ExprKind::Member {
                        object: Box::new(Expr {
                            string_offsets: None,
                            id: object.id,
                            kind: ExprKind::Ident(receiver.clone()),
                            span: object.span.clone(),
                        }),
                        name: method.clone(),
                    },
                    span: callee.span.clone(),
                }),
                args: args.to_vec(),
            },
            span: e.span.clone(),
        };
        let present = bin(BinOp::Ne, name(&receiver), LExpr::Nil);
        let mut body = Vec::new();
        let v = self.expr(&call, &mut body);
        let result = self.types.get(e.id).map(selu_check::shape_of);
        match result {
            // No result: a method of Selu's own or an API method.
            Some(selu_check::Type::Void) | Some(selu_check::Type::Tuple(_)) if !matches!(&result, Some(selu_check::Type::Tuple(ts)) if !ts.is_empty()) =>
            {
                // A collection method may lower to statements alone
                // (`map.set` stores into the table).
                if matches!(v, LExpr::Call(..) | LExpr::Method(..)) {
                    body.push(LStmt::new(LStmtKind::Call(v), &e.span));
                }
                pre.push(LStmt::new(
                    LStmtKind::If(vec![(present, body)], None),
                    &e.span,
                ));
                LExpr::Nil
            }
            Some(selu_check::Type::Optional(inner))
                if matches!(selu_check::shape_of(&inner), selu_check::Type::Tuple(_)) =>
            {
                body.push(LStmt::new(LStmtKind::Return(vec![v]), &e.span));
                let guarded = vec![LStmt::new(
                    LStmtKind::If(vec![(present, body)], None),
                    &e.span,
                )];
                LExpr::Call(
                    Box::new(LExpr::Paren(Box::new(LExpr::Function(lua::LFunction {
                        params: Vec::new(),
                        vararg: false,
                        body: guarded,
                    })))),
                    Vec::new(),
                )
            }
            _ => {
                let out = self.temp();
                pre.push(LStmt::new(
                    LStmtKind::Local(vec![out.clone()], Vec::new()),
                    &e.span,
                ));
                body.push(LStmt::new(
                    LStmtKind::Assign(vec![name(&out)], vec![v]),
                    &e.span,
                ));
                pre.push(LStmt::new(
                    LStmtKind::If(vec![(present, body)], None),
                    &e.span,
                ));
                name(&out)
            }
        }
    }

    pub(crate) fn hold(&mut self, value: LExpr, span: &Span, pre: &mut LBlock) -> String {
        let t = self.temp();
        pre.push(LStmt::new(
            LStmtKind::Local(vec![t.clone()], vec![value]),
            span,
        ));
        t
    }

    pub(crate) fn expr(&mut self, e: &Expr, pre: &mut LBlock) -> LExpr {
        match &e.kind {
            ExprKind::Number(n) => LExpr::Number(n.clone()),
            ExprKind::Str(s) => LExpr::Str(s.clone()),
            ExprKind::Bool(b) => LExpr::Bool(*b),
            ExprKind::Null => LExpr::Nil,
            ExprKind::Ident(n) => self.module_binding(e).unwrap_or_else(|| name(n)),
            ExprKind::Unary { op, expr } => {
                let v = self.expr(expr, pre);
                match op.as_str() {
                    "typeof" => lua::call(name("__selu_type"), vec![v]),
                    "!" => not(v),
                    _ => LExpr::Unary(UnOp::Neg, Box::new(v)),
                }
            }
            ExprKind::Binary { op, left, right } => self.binary(e, op, left, right, pre),
            // A declared other addon: a checked field read, a protected call.
            ExprKind::Member { .. } if self.types.foreign_read(e.id).is_some() => {
                self.foreign_read(e, pre).expect("checked foreign read")
            }
            ExprKind::Call { .. } if self.types.foreign_call(e.id).is_some() => {
                self.foreign_call(e, pre).expect("checked foreign call")
            }
            // Another addon's field: a protected read (`unknown`), or a raw
            // field another addon put on a frame.
            ExprKind::Member {
                object,
                name: field,
            } if self.types.is_unknown_field(e.id) => {
                let v = self.expr(object, pre);
                lua::call(name("__selu_field"), vec![v, LExpr::Str(field.clone())])
            }
            ExprKind::Member {
                object,
                name: field,
            } if self.types.is_addon_field(e.id) => {
                let ExprKind::Call { args, .. } = &object.kind else {
                    unreachable!("checked addonFields")
                };
                let v = self.expr(&args[0], pre);
                lua::call(name("__selu_rawfield"), vec![v, LExpr::Str(field.clone())])
            }
            ExprKind::Member {
                object,
                name: field,
            } => self.data_field(object, field, pre),
            // `addon("Name").global("Global")`: the global only while that
            // addon is loaded; the load check cannot raise.
            ExprKind::Call { .. } if self.types.addon_global(e.id).is_some() => {
                let read = self.types.addon_global(e.id).expect("checked").clone();
                let t = self.temp();
                pre.push(LStmt::new(
                    LStmtKind::Local(vec![t.clone()], vec![]),
                    &e.span,
                ));
                pre.push(LStmt::new(
                    LStmtKind::Raw(format!(
                        "do
  local ok, loaded = pcall({}, {:?})
  if ok and loaded == true then {t} = {} end
end",
                        read.loaded, read.addon, read.global
                    )),
                    &e.span,
                ));
                name(&t)
            }
            // The folder name the WoW loader passes as the first chunk argument.
            ExprKind::Call { .. } if self.types.is_addon_name(e.id) => name("__selu_addon"),
            ExprKind::Call { callee: _, args } if self.types.is_frame_family(e.id) => {
                let prefix = self.expr(&args[0], pre);
                let index = self.expr(&args[1], pre);
                let mut key = bin(BinOp::Concat, prefix, index);
                if let Some(suffix) = args.get(2) {
                    key = bin(BinOp::Concat, key, self.expr(suffix, pre));
                }
                LExpr::Index(Box::new(name("_G")), Box::new(key))
            }
            ExprKind::Call { callee, args }
                if matches!(callee.kind, ExprKind::OptionalMember { .. }) =>
            {
                self.optional_call(e, callee, args, pre)
            }
            ExprKind::Call { callee, args } => {
                let v = self.restricted_call(callee, args, pre);
                // One repeating result: `{ frame:GetRegions() }` collects
                // every value the engine returns.
                if matches!(v, LExpr::Call(..) | LExpr::Method(..))
                    && matches!(
                        self.types.get(callee.id),
                        Some(selu_check::Type::Function { api_semantics, .. })
                            if api_semantics.packed_results
                    )
                {
                    return LExpr::Table(vec![crate::lua::LField::Pos(v)]);
                }
                if self.types.is_single_value(e.id)
                    && matches!(v, LExpr::Call(..) | LExpr::Method(..))
                    && matches!(
                        self.types.get(callee.id),
                        Some(
                            selu_check::Type::Function {
                                multiple_results: true,
                                ..
                            } | selu_check::Type::Untyped
                        )
                    )
                {
                    LExpr::Paren(Box::new(v))
                } else {
                    v
                }
            }
            ExprKind::Assign { .. } => {
                unreachable!("the checker allows assignments only as statements")
            }
            ExprKind::Conditional { .. }
            | ExprKind::CompoundAssign { .. }
            | ExprKind::Update { .. } => self.control_expr(e, pre),
            ExprKind::Template { .. }
            | ExprKind::Array(_)
            | ExprKind::Object(_)
            | ExprKind::Index { .. }
            | ExprKind::OptionalMember { .. }
            | ExprKind::New { .. } => self.data_expr(e, pre),
            ExprKind::Function { .. } => self.function_expr(e, pre),
            ExprKind::TaggedTemplate { .. } => self.tagged_template(e, pre),
            ExprKind::This | ExprKind::Super => self.class_expr(e, pre),
        }
    }

    fn binary(&mut self, e: &Expr, op: &str, left: &Expr, right: &Expr, pre: &mut LBlock) -> LExpr {
        if let "&&" | "||" | "??" = op {
            return self.short_circuit(e, op, left, right, pre);
        }
        let vals = self.seq(&[left, right], pre);
        let [l, r]: [LExpr; 2] = vals.try_into().expect("two operands");
        let op = match op {
            // The checker allows `+` on two strings only; Lua joins strings
            // with `..`.
            "+" if self.types.is_string(left.id) || self.types.is_string(right.id) => BinOp::Concat,
            "+" => BinOp::Add,
            "-" => BinOp::Sub,
            "*" => BinOp::Mul,
            "/" => BinOp::Div,
            "%" => BinOp::Mod,
            "**" => BinOp::Pow,
            "==" => BinOp::Eq,
            "!=" => BinOp::Ne,
            "<" => BinOp::Lt,
            "<=" => BinOp::Le,
            ">" => BinOp::Gt,
            ">=" => BinOp::Ge,
            other => unreachable!("unknown operator {other}"),
        };
        bin(op, l, r)
    }

    /// `&&`, `||` and `??`: the right side runs only when needed. When it
    /// needs statements of its own, the whole thing becomes an `if` on a
    /// temporary.
    fn short_circuit(
        &mut self,
        e: &Expr,
        op: &str,
        left: &Expr,
        right: &Expr,
        pre: &mut LBlock,
    ) -> LExpr {
        let a = self.expr(left, pre);
        let a = if op == "??" {
            self.read_if_secret(left, a)
        } else {
            a
        };
        // Literal nullish branches have no reads or calls to preserve.
        if op == "??" && !self.may_run_code(left) {
            match &a {
                LExpr::Nil => return self.expr(right, pre),
                LExpr::Bool(_) | LExpr::Number(_) | LExpr::Str(_) => return a,
                _ => {}
            }
        }
        let mut right_pre = Vec::new();
        let b = self.expr(right, &mut right_pre);
        if right_pre.is_empty() {
            match op {
                "&&" => return bin(BinOp::And, a, b),
                "||" => return bin(BinOp::Or, a, b),
                // `a == nil and b or a` is exact when `a` can be read twice and
                // `b` is never false or nil.
                _ if a.is_pure() && never_false_or_nil(&b) => {
                    return bin(
                        BinOp::Or,
                        bin(BinOp::And, bin(BinOp::Eq, a.clone(), LExpr::Nil), b),
                        a,
                    );
                }
                _ => {}
            }
        }
        let t = self.hold(a, &e.span, pre);
        let test = match op {
            "&&" => name(&t),
            "||" => not(name(&t)),
            _ => bin(BinOp::Eq, name(&t), LExpr::Nil),
        };
        right_pre.push(LStmt::new(
            LStmtKind::Assign(vec![name(&t)], vec![b]),
            &e.span,
        ));
        pre.push(LStmt::new(
            LStmtKind::If(vec![(test, right_pre)], None),
            &e.span,
        ));
        name(&t)
    }

    fn call(&mut self, callee: &Expr, args: &[Expr], pre: &mut LBlock) -> LExpr {
        if let Some(v) = self.builtin_call(callee, args, pre) {
            return v;
        }
        if let Some(v) = self.data_call(callee, args, pre) {
            return v;
        }
        if let Some(v) = self.class_super_call(callee, args, pre) {
            return v;
        }
        let mut all: Vec<&Expr> = Vec::new();
        match &callee.kind {
            // Method call: `a.B(x)` -> `a:B(x)`, unless the whole dotted name
            // is a manifest namespace function (`C_Foo.Bar(x)` stays a plain
            // call).
            ExprKind::Member {
                object,
                name: method,
            } => {
                let is_namespace = callee
                    .dotted_name()
                    .is_some_and(|full| self.known.contains(&full));
                all.push(object);
                all.extend(args);
                let mut vals = self.seq(&all, pre);
                let obj = vals.remove(0);
                if self.types.is_method_call(callee.id) {
                    return LExpr::Method(Box::new(obj), method.clone(), vals);
                }
                if is_namespace
                    || (matches!(self.types.get(object.id), Some(selu_check::Type::Class(..)))
                        && !self.types.is_method_call(callee.id))
                    || matches!(
                        self.types.get(object.id),
                        Some(selu_check::Type::ClassObject(_))
                    )
                    || matches!(self.types.get(object.id), Some(selu_check::Type::Object(fields)) if fields.get(method).is_some_and(|t| matches!(selu_check::shape_of(t), selu_check::Type::Function { .. })))
                {
                    lua::call(LExpr::Field(Box::new(obj), method.clone()), vals)
                } else {
                    LExpr::Method(Box::new(obj), method.clone(), vals)
                }
            }
            _ => {
                all.push(callee);
                all.extend(args);
                let mut vals = self.seq(&all, pre);
                let f = vals.remove(0);
                lua::call(f, vals)
            }
        }
    }
}

fn is_constant(e: &LExpr) -> bool {
    matches!(
        e,
        LExpr::Nil | LExpr::Bool(_) | LExpr::Number(_) | LExpr::Str(_)
    )
}

/// Literals that are never `false` or `nil`.
fn never_false_or_nil(e: &LExpr) -> bool {
    matches!(
        e,
        LExpr::Number(_) | LExpr::Str(_) | LExpr::Bool(true) | LExpr::Table(_) | LExpr::Function(_)
    )
}

/// Names declared in `stmts` that an earlier statement of the same block
/// refers to, in declaration order. The checker only allows that from inside
/// a function body; a shadowed use in an inner block may be counted too,
/// which costs one harmless extra forward declaration.
fn used_before_declaration(stmts: &[Stmt]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut early = Vec::new();
    for s in stmts {
        let s = match &s.kind {
            StmtKind::Export(inner) => inner.as_ref(),
            _ => s,
        };
        let class_early = matches!(s.kind, StmtKind::Class(_))
            .then(|| declaration_names(s).iter().any(|n| seen.contains(n)));
        visit::idents_in_stmt(s, &mut |n| {
            seen.insert(n.to_string());
        });
        classes::class_uses(s, &mut |n| {
            seen.insert(n.to_string());
        });
        let names = declaration_names(s);
        if class_early.unwrap_or_else(|| names.iter().any(|n| seen.contains(n))) {
            for name in names {
                if !early.contains(&name) {
                    early.push(name);
                }
            }
        }
    }
    early
}

fn declaration_names(s: &Stmt) -> Vec<String> {
    match &s.kind {
        StmtKind::Export(inner) => declaration_names(inner),
        StmtKind::Saved { name, .. }
        | StmtKind::VarDecl { name, .. }
        | StmtKind::FuncDecl { name, .. } => vec![name.clone()],
        StmtKind::Class(c) => vec![c.name.clone()],
        StmtKind::Destructure { pattern, .. } => match pattern {
            selu_syntax::Pattern::Array(items) => {
                items.iter().flatten().map(|(n, _)| n.clone()).collect()
            }
            selu_syntax::Pattern::Object(items) => {
                items.iter().map(|(_, n, _, _)| n.clone()).collect()
            }
        },
        _ => Vec::new(),
    }
}

/// `a ?? b ?? c` (left-associative) as `[a, b, c]`.
fn flatten_coalesce<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    match &e.kind {
        ExprKind::Binary { op, left, right } if op == "??" => {
            flatten_coalesce(left, out);
            out.push(right);
        }
        _ => out.push(e),
    }
}

/// The interface number comes from the target manifest, so the `.toc`
/// cannot drift from the API set the code was checked against.
pub fn generate_toc(addon_name: &str, manifest: &Manifest, lua_files: &[String]) -> String {
    let mut out = String::new();
    out.push_str(&format!("## Interface: {}\n", manifest.interface));
    out.push_str(&format!("## Title: {addon_name}\n"));
    out.push_str("## Notes: Built with Selu\n");
    out.push_str(&format!("## X-Selu-Target: {}\n", manifest.target_string()));
    for f in lua_files {
        out.push_str(f);
        out.push('\n');
    }
    out
}

impl Lowerer<'_> {
    // Every protected runtime capture is emitted once, before source bindings.
    fn file_prelude(&mut self, program: &Program, out: &mut LBlock) {
        fn inspect(c: visit::Child<'_>, found: &mut bool, types: &TypeTable) {
            match c {
                visit::Child::Expr(e) => {
                    if matches!(&e.kind, ExprKind::Unary {op,..} if op == "typeof")
                        || patterns::needs_type(e, types)
                    {
                        *found = true;
                    }
                    visit::child_exprs_and_stmts(e, &mut |c| inspect(c, found, types));
                }
                visit::Child::Stmt(s) => {
                    visit::child_stmts_and_exprs(s, &mut |c| inspect(c, found, types))
                }
            }
        }
        let mut found = false;
        for s in &program.stmts {
            inspect(visit::Child::Stmt(s), &mut found, self.types);
        }
        if found {
            out.push(LStmt {
                kind: LStmtKind::Local(vec!["__selu_type".into()], vec![name("type")]),
                span: None,
            });
        }
        // Reads across the other-addon boundary: a field of an unknown value
        // never raises (a metatable's __index runs protected), and a field
        // another addon put on a frame is read raw.
        // Declared other addons: the runtime checks of crossing values.
        if self.types.uses_foreign() {
            out.push(LStmt {
                kind: LStmtKind::Raw(foreign::RUNTIME.into()),
                span: None,
            });
        }
        if self.types.uses_boundary_reads() {
            out.push(LStmt {
                kind: LStmtKind::Raw(
                    [
                        "local __selu_boundary_type, __selu_boundary_pcall, __selu_boundary_rawget = type, pcall, rawget",
                        "local function __selu_boundary_index(value, key) return value[key] end",
                        "local function __selu_field(value, key)",
                        "  if __selu_boundary_type(value) ~= \"table\" then return nil end",
                        "  local ok, result = __selu_boundary_pcall(__selu_boundary_index, value, key)",
                        "  if ok then return result end",
                        "  return nil",
                        "end",
                        "local function __selu_rawfield(frame, key) return __selu_boundary_rawget(frame, key) end",
                    ]
                    .join("\n"),
                ),
                span: None,
            });
        }
        self.error_captures(program, out);
        self.target_prelude(program, out);
        self.class_prelude(&program.stmts, out);
        out.extend(saved::captures(program));
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

    fn lua(src: &str) -> Lowered {
        let p = selu_syntax::parse("main.selu", src).unwrap();
        let checked = selu_check::check_program(&p, &manifest());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        lower(&p, &manifest(), &checked.types)
    }

    #[test]
    fn lowers_subset_with_method_colons_and_events() {
        let out = lua(
            "const limit = 10;\nconst frame = CreateFrame(\"Frame\");\nfunction greet() { return \"hi\"; }\non PLAYER_LOGIN() {\n  print(greet());\n  frame.SetPoint(\"TOP\", null);\n  if (limit > 5 && limit != 9) { return; }\n}\n",
        );
        for want in [
            "local limit = 10",
            "local function greet()",
            "__selu_on(\"PLAYER_LOGIN\", function()",
            "  frame:SetPoint(\"TOP\", nil)",
            "  if limit > 5 and limit ~= 9 then",
            "__selu_frame:RegisterEvent(event)",
        ] {
            assert!(out.lua.contains(want), "missing {want:?} in\n{}", out.lua);
        }
        assert!(
            out.statement_lines
                .iter()
                .all(|(_, span)| span.file == "main.selu")
        );
        // The handler's `print` line maps back to the source.
        let line = out
            .lua
            .lines()
            .position(|l| l.contains("print(greet())"))
            .unwrap() as u32
            + 1;
        assert!(
            out.statement_lines
                .iter()
                .any(|(generated, _)| *generated == line)
        );
    }

    #[test]
    fn output_has_only_needed_brackets() {
        let out = lua(
            "function f(a: number, b: number) { return (a + b) * 2 - a / b + 1; }\nfunction g(s: string) { return s + \"x\" + s; }\n",
        );
        assert!(
            out.lua.contains("return (a + b) * 2 - a / b + 1"),
            "{}",
            out.lua
        );
        assert!(out.lua.contains("return s .. \"x\" .. s"), "{}", out.lua);
    }

    #[test]
    fn toc_interface_comes_from_manifest() {
        let toc = generate_toc("Hello", &manifest(), &["main.lua".into()]);
        assert!(toc.starts_with("## Interface: 100000\n"), "{toc}");
        assert!(toc.contains("main.lua"), "{toc}");
    }
}

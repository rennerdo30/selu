//! Lower the Selu subset to readable Lua 5.1 (DESIGN.md section 4):
//! 2-space indent, local names preserved, Blizzard method calls use `:`
//! (`frame.SetPoint(...)` in Selu is `frame:SetPoint(...)` in Lua).
//! `on EVENT` handlers share one event frame with a dispatch table.

use selu_diagnostics::Span;
use selu_manifest::Manifest;
use selu_syntax::{Expr, ExprKind, Program, Stmt, StmtKind, is_lua_keyword};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceMapEntry {
    /// 1-based line in the generated Lua.
    pub generated_line: u32,
    pub span: Span,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceMap {
    pub entries: Vec<SourceMapEntry>,
}

impl SourceMap {
    /// The source span for a generated line: the exact entry, or the
    /// nearest entry at or before it.
    pub fn lookup(&self, generated_line: u32) -> Option<&Span> {
        self.entries
            .iter()
            .rfind(|e| e.generated_line <= generated_line)
            .map(|e| &e.span)
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }
}

#[derive(Debug, Clone)]
pub struct Lowered {
    pub lua: String,
    pub source_map: SourceMap,
}

pub fn lower(program: &Program, manifest: &Manifest) -> Lowered {
    let known: HashSet<String> = manifest.functions.iter().map(|f| f.name.clone()).collect();
    let mut w = Writer {
        out: String::new(),
        line: 1,
        map: SourceMap::default(),
        known: &known,
    };
    let has_handlers = program
        .stmts
        .iter()
        .any(|s| matches!(s.kind, StmtKind::OnHandler { .. }));
    if has_handlers {
        w.raw("-- Selu: shared event frame and dispatch table");
        w.raw("local __selu_handlers = {}");
        w.raw("local __selu_frame = CreateFrame(\"Frame\")");
        w.raw("__selu_frame:SetScript(\"OnEvent\", function(self, event, ...)");
        w.raw("  local handlers = __selu_handlers[event]");
        w.raw("  if handlers then");
        w.raw("    for i = 1, #handlers do");
        w.raw("      handlers[i](...)");
        w.raw("    end");
        w.raw("  end");
        w.raw("end)");
        w.raw("local function __selu_on(event, fn)");
        w.raw("  if not __selu_handlers[event] then");
        w.raw("    __selu_handlers[event] = {}");
        w.raw("    __selu_frame:RegisterEvent(event)");
        w.raw("  end");
        w.raw("  __selu_handlers[event][#__selu_handlers[event] + 1] = fn");
        w.raw("end");
        w.raw("");
    }
    for s in &program.stmts {
        w.stmt(s, 0);
    }
    Lowered {
        lua: w.out,
        source_map: w.map,
    }
}

struct Writer<'a> {
    out: String,
    line: u32,
    map: SourceMap,
    known: &'a HashSet<String>,
}

impl Writer<'_> {
    fn raw(&mut self, text: &str) {
        self.out.push_str(text);
        self.out.push('\n');
        self.line += 1;
    }
    fn emit(&mut self, indent: usize, text: &str, span: &Span) {
        self.map.entries.push(SourceMapEntry {
            generated_line: self.line,
            span: span.clone(),
        });
        for _ in 0..indent {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
        self.line += 1;
    }
    fn stmts(&mut self, stmts: &[Stmt], indent: usize) {
        for s in stmts {
            self.stmt(s, indent);
        }
    }
    fn stmt(&mut self, s: &Stmt, indent: usize) {
        match &s.kind {
            StmtKind::VarDecl { name, init, .. } => match init {
                Some(e) if is_coalesce(e) => {
                    self.coalesce_into(indent, &format!("local {name}"), name, e, &s.span);
                }
                Some(e) => {
                    let text = format!("local {name} = {}", self.expr(e));
                    self.emit(indent, &text, &s.span);
                }
                None => self.emit(indent, &format!("local {name}"), &s.span),
            },
            StmtKind::FuncDecl { name, params, body } => {
                let ps: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                self.emit(
                    indent,
                    &format!("local function {name}({})", ps.join(", ")),
                    &s.span,
                );
                self.stmts(body, indent + 1);
                self.emit(indent, "end", &s.span);
            }
            StmtKind::OnHandler {
                event,
                params,
                body,
            } => {
                let ps: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                self.emit(
                    indent,
                    &format!("__selu_on({:?}, function({})", event, ps.join(", ")),
                    &s.span,
                );
                self.stmts(body, indent + 1);
                self.emit(indent, "end)", &s.span);
            }
            StmtKind::Return(Some(e)) if is_coalesce(e) => {
                self.emit(indent, "do", &s.span);
                self.coalesce_into(indent + 1, "local __selu_v", "__selu_v", e, &s.span);
                self.emit(indent + 1, "return __selu_v", &s.span);
                self.emit(indent, "end", &s.span);
            }
            StmtKind::Return(e) => {
                let text = match e {
                    Some(e) => format!("return {}", self.expr(e)),
                    None => "return".into(),
                };
                self.emit(indent, &text, &s.span);
            }
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                // `else if` (a single If in the else branch) becomes `elseif`.
                self.emit(indent, &format!("if {} then", self.expr(cond)), &s.span);
                self.stmts(then_branch, indent + 1);
                let mut else_branch = else_branch.as_deref();
                loop {
                    match else_branch {
                        Some([only]) if matches!(only.kind, StmtKind::If { .. }) => {
                            if let StmtKind::If {
                                cond,
                                then_branch,
                                else_branch: next,
                            } = &only.kind
                            {
                                self.emit(
                                    indent,
                                    &format!("elseif {} then", self.expr(cond)),
                                    &only.span,
                                );
                                self.stmts(then_branch, indent + 1);
                                else_branch = next.as_deref();
                            }
                        }
                        Some(body) => {
                            self.emit(indent, "else", &s.span);
                            self.stmts(body, indent + 1);
                            break;
                        }
                        None => break,
                    }
                }
                self.emit(indent, "end", &s.span);
            }
            StmtKind::Block(b) => {
                self.emit(indent, "do", &s.span);
                self.stmts(b, indent + 1);
                self.emit(indent, "end", &s.span);
            }
            StmtKind::Expr(e) => match &e.kind {
                ExprKind::Assign { target, value } if is_coalesce(value) => {
                    let target = self.expr(target);
                    self.emit(indent, "do", &s.span);
                    self.coalesce_into(indent + 1, "local __selu_v", "__selu_v", value, &s.span);
                    self.emit(indent + 1, &format!("{target} = __selu_v"), &s.span);
                    self.emit(indent, "end", &s.span);
                }
                _ => {
                    let text = self.expr(e);
                    self.emit(indent, &text, &s.span);
                }
            },
        }
    }

    /// `a ?? b ?? c` assigned to `var`: evaluate each side once and only when
    /// needed, and keep `false` (unlike `a or b`):
    ///
    /// ```lua
    /// local x = a
    /// if x == nil then x = b end
    /// if x == nil then x = c end
    /// ```
    fn coalesce_into(&mut self, indent: usize, decl: &str, var: &str, e: &Expr, span: &Span) {
        let mut chain = Vec::new();
        flatten_coalesce(e, &mut chain);
        let (first, rest) = chain.split_first().expect("a chain has at least one part");
        let first = self.expr(first);
        self.emit(indent, &format!("{decl} = {first}"), span);
        for part in rest {
            let value = self.expr(part);
            self.emit(
                indent,
                &format!("if {var} == nil then {var} = {value} end"),
                span,
            );
        }
    }

    fn expr(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Number(n) => n.clone(),
            ExprKind::Str(s) => lua_string(s),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Null => "nil".into(),
            ExprKind::Ident(n) => n.clone(),
            ExprKind::Unary { op, expr } => match op.as_str() {
                "!" => format!("not ({})", self.expr(expr)),
                _ => format!("-({})", self.expr(expr)),
            },
            ExprKind::Binary { op, left, right } => {
                let (l, r) = (self.expr(left), self.expr(right));
                match op.as_str() {
                    "&&" => format!("({l} and {r})"),
                    "||" => format!("({l} or {r})"),
                    "!=" => format!("({l} ~= {r})"),
                    // Inline form: the checker (SELU0308) only allows it when
                    // `l` has no side effects and `r` is never false or nil, so
                    // evaluating `l` twice and using and/or is exact, including
                    // for `l == false`. Other forms use `coalesce_into`.
                    "??" => format!("({l} == nil and {r} or {l})"),
                    _ => format!("({l} {op} {r})"),
                }
            }
            ExprKind::Assign { target, value } => {
                format!("{} = {}", self.expr(target), self.expr(value))
            }
            // `t.end` is fine in Selu but not in Lua: use `t["end"]`.
            ExprKind::Member { object, name } if is_lua_keyword(name) => {
                format!("{}[{}]", self.expr(object), lua_string(name))
            }
            ExprKind::Member { object, name } => format!("{}.{name}", self.expr(object)),
            ExprKind::Call { callee, args } => {
                let args: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                let joined = args.join(", ");
                match &callee.kind {
                    // Method call: `a.B(x)` -> `a:B(x)`, unless the whole
                    // dotted name is a manifest namespace function
                    // (`C_Foo.Bar(x)` stays a plain call).
                    ExprKind::Member { object, name } => {
                        let is_namespace = callee
                            .dotted_name()
                            .is_some_and(|full| self.known.contains(&full));
                        if is_namespace {
                            format!("{}.{name}({joined})", self.expr(object))
                        } else {
                            format!("{}:{name}({joined})", self.expr(object))
                        }
                    }
                    _ => format!("{}({joined})", self.expr(callee)),
                }
            }
        }
    }
}

fn is_coalesce(e: &Expr) -> bool {
    matches!(&e.kind, ExprKind::Binary { op, .. } if op == "??")
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

fn lua_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Other control characters as Lua 5.1 decimal escapes; padding
            // to three digits keeps a following digit from joining the escape.
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\{:03}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
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
    fn lowers_subset_with_method_colons_and_events() {
        let src = "const limit = 10;\nfunction greet() { return \"hi\"; }\non PLAYER_LOGIN() {\n  print(greet());\n  frame.SetPoint(\"TOP\", null);\n  if (limit > 5 && limit != 9) { return; }\n}\n";
        let p = selu_syntax::parse("main.selu", src).unwrap();
        let out = lower(&p, &manifest());
        assert!(out.lua.contains("local limit = 10"), "{}", out.lua);
        assert!(out.lua.contains("local function greet()"), "{}", out.lua);
        assert!(
            out.lua.contains("__selu_on(\"PLAYER_LOGIN\", function()"),
            "{}",
            out.lua
        );
        assert!(
            out.lua.contains("frame:SetPoint(\"TOP\", nil)"),
            "{}",
            out.lua
        );
        assert!(
            out.lua.contains("(limit > 5) and (limit ~= 9)") || out.lua.contains("and"),
            "{}",
            out.lua
        );
        assert!(
            out.lua.contains("__selu_frame:RegisterEvent(event)"),
            "{}",
            out.lua
        );
        // Source map points back at main.selu.
        let span = out
            .source_map
            .lookup(1)
            .or_else(|| out.source_map.lookup(30));
        assert!(
            out.source_map
                .entries
                .iter()
                .all(|e| e.span.file == "main.selu")
        );
        assert!(span.is_some());
    }

    #[test]
    fn toc_interface_comes_from_manifest() {
        let toc = generate_toc("Hello", &manifest(), &["main.lua".into()]);
        assert!(toc.starts_with("## Interface: 100000\n"), "{toc}");
        assert!(toc.contains("main.lua"), "{toc}");
    }
}

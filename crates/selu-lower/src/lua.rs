//! A small Lua 5.1 syntax tree and its printer. The lowering builds this
//! tree instead of text, so it can put statements before an expression
//! (temporaries), nest function bodies inside expressions, and let the
//! printer decide which parentheses are needed.

use selu_diagnostics::Span;
use selu_syntax::is_lua_keyword;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    Concat,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

impl BinOp {
    fn text(self) -> &'static str {
        match self {
            BinOp::Or => "or",
            BinOp::And => "and",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Eq => "==",
            BinOp::Ne => "~=",
            BinOp::Concat => "..",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Pow => "^",
        }
    }
    /// Lua 5.1 precedence, higher binds tighter.
    fn prec(self) -> u8 {
        match self {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne => 3,
            BinOp::Concat => 4,
            BinOp::Add | BinOp::Sub => 5,
            BinOp::Mul | BinOp::Div | BinOp::Mod => 6,
            BinOp::Pow => 8,
        }
    }
    fn right_assoc(self) -> bool {
        matches!(self, BinOp::Concat | BinOp::Pow)
    }
}

const UNARY_PREC: u8 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
    Len,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LExpr {
    Nil,
    Bool(bool),
    /// A number literal as written (`1`, `0.5`).
    Number(String),
    Str(String),
    Name(String),
    Vararg,
    /// `a.b` (printed as `a["end"]` when `b` is a Lua keyword).
    Field(Box<LExpr>, String),
    /// `a[b]`
    Index(Box<LExpr>, Box<LExpr>),
    Call(Box<LExpr>, Vec<LExpr>),
    /// `a:b(...)`
    Method(Box<LExpr>, String, Vec<LExpr>),
    Binary(BinOp, Box<LExpr>, Box<LExpr>),
    Unary(UnOp, Box<LExpr>),
    Function(LFunction),
    Table(Vec<LField>),
    /// `(e)`: keeps only the first value of a call.
    Paren(Box<LExpr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LFunction {
    pub params: Vec<String>,
    pub vararg: bool,
    pub body: LBlock,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LField {
    /// `value` (the next array slot).
    Pos(LExpr),
    /// `name = value`
    Named(String, LExpr),
    /// `[key] = value`
    Keyed(LExpr, LExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LStmtKind {
    /// `local a, b = x, y` (no values: `local a, b`).
    Local(Vec<String>, Vec<LExpr>),
    Assign(Vec<LExpr>, Vec<LExpr>),
    /// A call used as a statement.
    Call(LExpr),
    /// `if c then ... elseif c2 then ... else ... end`
    If(Vec<(LExpr, LBlock)>, Option<LBlock>),
    While(LExpr, LBlock),
    Repeat(LBlock, LExpr),
    /// `for v = start, stop, step do`
    NumFor {
        var: String,
        start: LExpr,
        stop: LExpr,
        step: Option<LExpr>,
        body: LBlock,
    },
    /// `for a, b in exprs do`
    GenFor {
        names: Vec<String>,
        exprs: Vec<LExpr>,
        body: LBlock,
    },
    Do(LBlock),
    Return(Vec<LExpr>),
    Break,
    /// `local function name(...)`
    LocalFunction(String, LFunction),
    /// `function name(...)`: assigns an existing variable or a global.
    Function(String, LFunction),
    /// Lines written as they are (the shared event-frame prologue).
    Raw(String),
    /// `-- text`
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LStmt {
    pub kind: LStmtKind,
    /// The Selu source this statement came from, for the source map.
    pub span: Option<Span>,
}

pub type LBlock = Vec<LStmt>;

impl LStmt {
    pub fn new(kind: LStmtKind, span: &Span) -> LStmt {
        LStmt {
            kind,
            span: Some(span.clone()),
        }
    }
}

// ---- convenience constructors --------------------------------------------

pub fn name(n: &str) -> LExpr {
    LExpr::Name(n.to_string())
}
pub fn bin(op: BinOp, l: LExpr, r: LExpr) -> LExpr {
    LExpr::Binary(op, Box::new(l), Box::new(r))
}
pub fn not(e: LExpr) -> LExpr {
    LExpr::Unary(UnOp::Not, Box::new(e))
}
pub fn call(f: LExpr, args: Vec<LExpr>) -> LExpr {
    LExpr::Call(Box::new(f), args)
}

impl LExpr {
    /// Evaluating it has no effect and gives the same value every time it is
    /// read within one statement, so it may be repeated or reordered.
    pub fn is_pure(&self) -> bool {
        matches!(
            self,
            LExpr::Nil | LExpr::Bool(_) | LExpr::Number(_) | LExpr::Str(_) | LExpr::Name(_)
        )
    }
}

// ---- printing ----------------------------------------------------------

/// Printed Lua and, for each generated line (1-based), the span it came
/// from.
pub struct Printed {
    pub text: String,
    pub lines: Vec<(u32, Span)>,
}

pub fn print_block(block: &LBlock) -> Printed {
    let mut p = Printer {
        column: 0,
        out: String::new(),
        line: 1,
        lines: Vec::new(),
        embedded: Vec::new(),
    };
    p.block(block, 0);
    Printed {
        text: p.out,
        lines: p.lines,
    }
}

/// One expression on its own (for tests and messages).
pub fn print_expr(e: &LExpr) -> String {
    let mut p = Printer {
        column: 0,
        out: String::new(),
        line: 1,
        lines: Vec::new(),
        embedded: Vec::new(),
    };
    p.expr(e, 0, 0)
}

struct Printer {
    column: usize,
    out: String,
    line: u32,
    lines: Vec<(u32, Span)>,
    embedded: Vec<(String, Vec<(u32, Span)>)>,
}

impl Printer {
    fn line(&mut self, indent: usize, text: &str, span: &Option<Span>) {
        for offset in 0..=text.matches('\n').count() as u32 {
            if let Some(s) = span {
                self.lines.push((self.line + offset, s.clone()));
            }
        }
        let mut cursor = 0;
        for (fragment, lines) in std::mem::take(&mut self.embedded) {
            let offset = text[cursor..]
                .find(&fragment)
                .expect("embedded function in emitted expression")
                + cursor;
            let base = self.line + text[..offset].matches('\n').count() as u32;
            self.lines
                .extend(lines.into_iter().map(|(line, span)| (base + line, span)));
            cursor = offset + fragment.len();
        }
        if !text.is_empty() {
            for _ in 0..indent {
                self.out.push_str("  ");
            }
        }
        self.out.push_str(text);
        self.out.push('\n');
        // A function body inside an expression adds lines of its own.
        self.line += 1 + text.matches('\n').count() as u32;
    }

    fn block(&mut self, b: &LBlock, indent: usize) {
        for (i, s) in b.iter().enumerate() {
            // Lua requires `break` and `return` to end their block; one
            // that doesn't gets a `do ... end` of its own.
            if i + 1 < b.len() && matches!(s.kind, LStmtKind::Break | LStmtKind::Return(_)) {
                self.line(indent, "do", &s.span);
                self.stmt(s, indent + 1);
                self.line(indent, "end", &s.span);
            } else {
                self.stmt(s, indent);
            }
        }
    }

    fn func_head(&mut self, f: &LFunction, indent: usize) -> String {
        let mut ps = f.params.clone();
        if f.vararg {
            ps.push("...".into());
        }
        let _ = indent;
        format!("({})", ps.join(", "))
    }

    fn stmt(&mut self, s: &LStmt, indent: usize) {
        let sp = &s.span;
        match &s.kind {
            LStmtKind::Local(names, values) => {
                let mut text = format!("local {}", names.join(", "));
                if !values.is_empty() {
                    text.push_str(" = ");
                    self.column = text.len() + indent * 2;
                    text.push_str(&if names.len() <= values.len() {
                        self.scalar_list(values, indent)
                    } else {
                        self.list(values, indent)
                    });
                    self.column = 0;
                }
                self.line(indent, &text, sp);
            }
            LStmtKind::Assign(targets, values) => {
                let t = self.list(targets, indent);
                self.column = t.len() + 3 + indent * 2;
                let v = if targets.len() <= values.len() {
                    self.scalar_list(values, indent)
                } else {
                    self.list(values, indent)
                };
                self.column = 0;
                self.line(indent, &format!("{t} = {v}"), sp);
            }
            LStmtKind::Call(e) => {
                let text = self.scalar_expr(e, indent);
                self.line(indent, &text, sp);
            }
            LStmtKind::If(arms, otherwise) => {
                // `if x == nil then x = 0 end` on one line when it is that short.
                if let ([(cond, body)], None) = (arms.as_slice(), otherwise)
                    && let [only] = body.as_slice()
                    && matches!(
                        only.kind,
                        LStmtKind::Assign(..)
                            | LStmtKind::Return(_)
                            | LStmtKind::Break
                            | LStmtKind::Call(_)
                    )
                {
                    let mut inner = Printer {
                        column: 0,
                        out: String::new(),
                        line: 0,
                        lines: Vec::new(),
                        embedded: Vec::new(),
                    };
                    inner.stmt(only, 0);
                    let body_text = inner.out.trim_end().to_string();
                    let pending = self.embedded.len();
                    let c = self.scalar_expr(cond, indent);
                    let text = format!("if {c} then {body_text} end");
                    if !body_text.contains('\n')
                        && text.len() + indent * 2 <= 100
                        && (c.contains('\n') || only.span == s.span)
                    {
                        let generated = self.line + c.matches('\n').count() as u32;
                        self.line(indent, &text, sp);
                        self.lines
                            .extend(inner.lines.into_iter().map(|(_, span)| (generated, span)));
                        return;
                    }
                    self.embedded.truncate(pending);
                }
                for (i, (cond, body)) in arms.iter().enumerate() {
                    let kw = if i == 0 { "if" } else { "elseif" };
                    let c = self.scalar_expr(cond, indent);
                    self.line(indent, &format!("{kw} {c} then"), sp);
                    self.block(body, indent + 1);
                }
                if let Some(b) = otherwise {
                    self.line(indent, "else", sp);
                    self.block(b, indent + 1);
                }
                self.line(indent, "end", sp);
            }
            LStmtKind::While(cond, body) => {
                let c = self.scalar_expr(cond, indent);
                self.line(indent, &format!("while {c} do"), sp);
                self.block(body, indent + 1);
                self.line(indent, "end", sp);
            }
            LStmtKind::Repeat(body, cond) => {
                self.line(indent, "repeat", sp);
                self.block(body, indent + 1);
                let c = self.scalar_expr(cond, indent);
                self.line(indent, &format!("until {c}"), sp);
            }
            LStmtKind::NumFor {
                var,
                start,
                stop,
                step,
                body,
            } => {
                let mut text = format!(
                    "for {var} = {}, {}",
                    self.scalar_expr(start, indent),
                    self.scalar_expr(stop, indent)
                );
                // A step of 1 is Lua's default.
                if let Some(step) = step
                    && *step != LExpr::Number("1".into())
                {
                    text.push_str(&format!(", {}", self.scalar_expr(step, indent)));
                }
                self.line(indent, &format!("{text} do"), sp);
                self.block(body, indent + 1);
                self.line(indent, "end", sp);
            }
            LStmtKind::GenFor { names, exprs, body } => {
                let e = self.list(exprs, indent);
                self.line(indent, &format!("for {} in {e} do", names.join(", ")), sp);
                self.block(body, indent + 1);
                self.line(indent, "end", sp);
            }
            LStmtKind::Do(body) => {
                self.line(indent, "do", sp);
                self.block(body, indent + 1);
                self.line(indent, "end", sp);
            }
            LStmtKind::Return(values) => {
                let text = if values.is_empty() {
                    "return".to_string()
                } else {
                    format!("return {}", self.list(values, indent))
                };
                self.line(indent, &text, sp);
            }
            LStmtKind::Break => self.line(indent, "break", sp),
            LStmtKind::LocalFunction(n, f) | LStmtKind::Function(n, f) => {
                let local = if matches!(s.kind, LStmtKind::LocalFunction(..)) {
                    "local "
                } else {
                    ""
                };
                let head = self.func_head(f, indent);
                self.line(indent, &format!("{local}function {n}{head}"), sp);
                self.block(&f.body, indent + 1);
                self.line(indent, "end", sp);
            }
            LStmtKind::Raw(text) => {
                for l in text.lines() {
                    self.line(indent, l, sp);
                }
            }
            LStmtKind::Comment(text) => self.line(indent, &format!("-- {text}"), sp),
        }
    }

    fn scalar_expr(&mut self, e: &LExpr, indent: usize) -> String {
        match e {
            LExpr::Paren(inner)
                if matches!(
                    inner.as_ref(),
                    LExpr::Call(..) | LExpr::Method(..) | LExpr::Vararg
                ) =>
            {
                self.expr(inner, 0, indent)
            }
            _ => self.expr(e, 0, indent),
        }
    }
    fn scalar_list(&mut self, es: &[LExpr], indent: usize) -> String {
        es.iter()
            .map(|e| self.scalar_expr(e, indent))
            .collect::<Vec<_>>()
            .join(", ")
    }
    fn list(&mut self, es: &[LExpr], indent: usize) -> String {
        let parts: Vec<String> = es
            .iter()
            .enumerate()
            .map(|(i, e)| {
                if i + 1 < es.len() {
                    self.scalar_expr(e, indent)
                } else {
                    self.expr(e, 0, indent)
                }
            })
            .collect();
        parts.join(", ")
    }

    /// A prefix expression (the object of a call, field or index) must be a
    /// name, a call, a field, an index or parenthesised.
    fn prefix(&mut self, e: &LExpr, indent: usize) -> String {
        if let LExpr::Paren(inner) = e
            && matches!(inner.as_ref(), LExpr::Call(..) | LExpr::Method(..))
        {
            return self.prefix(inner, indent);
        }
        let text = self.expr(e, 0, indent);
        match e {
            LExpr::Name(_)
            | LExpr::Field(..)
            | LExpr::Index(..)
            | LExpr::Call(..)
            | LExpr::Method(..)
            | LExpr::Paren(_) => text,
            _ => format!("({text})"),
        }
    }

    /// `min` is the precedence the context needs; lower binds get brackets.
    /// Expressions containing function bodies print them on new lines at
    /// `indent + 1`.
    fn expr(&mut self, e: &LExpr, min: u8, indent: usize) -> String {
        match e {
            LExpr::Nil => "nil".into(),
            LExpr::Bool(b) => b.to_string(),
            LExpr::Number(n) => n.clone(),
            LExpr::Str(s) => lua_string(s),
            LExpr::Name(n) => n.clone(),
            LExpr::Vararg => "...".into(),
            LExpr::Field(obj, f) if is_lua_keyword(f) || !is_lua_name(f) => {
                format!("{}[{}]", self.prefix(obj, indent), lua_string(f))
            }
            LExpr::Field(obj, f) => format!("{}.{f}", self.prefix(obj, indent)),
            LExpr::Index(obj, i) => {
                format!(
                    "{}[{}]",
                    self.prefix(obj, indent),
                    self.scalar_expr(i, indent)
                )
            }
            LExpr::Call(f, args) => {
                format!("{}({})", self.prefix(f, indent), self.list(args, indent))
            }
            LExpr::Method(obj, m, args) => {
                format!(
                    "{}:{m}({})",
                    self.prefix(obj, indent),
                    self.list(args, indent)
                )
            }
            LExpr::Paren(inner)
                if min > 0
                    && matches!(
                        inner.as_ref(),
                        LExpr::Call(..) | LExpr::Method(..) | LExpr::Vararg
                    ) =>
            {
                self.expr(inner, min, indent)
            }
            LExpr::Paren(inner) => format!("({})", self.expr(inner, 0, indent)),
            LExpr::Unary(op, inner) => {
                let operand = self.expr(inner, UNARY_PREC, indent);
                let text = match op {
                    UnOp::Not => format!("not {operand}"),
                    UnOp::Len => format!("#{operand}"),
                    // `- -x` would start a comment (`--`).
                    UnOp::Neg if operand.starts_with('-') => format!("-({operand})"),
                    UnOp::Neg => format!("-{operand}"),
                };
                wrap(text, UNARY_PREC, min)
            }
            LExpr::Binary(op, l, r) => {
                let p = op.prec();
                // Equal precedence on the side that doesn't associate needs
                // brackets: `a - (b - c)`, `(a .. b) .. c` stays flat.
                // `..` only ever joins strings here, where grouping doesn't
                // matter, so a chain prints flat either way.
                let (lmin, rmin) = if *op == BinOp::Concat {
                    (p, p)
                } else if op.right_assoc() {
                    (p + 1, p)
                } else {
                    (p, p + 1)
                };
                let text = format!(
                    "{} {} {}",
                    self.expr(l, lmin, indent),
                    op.text(),
                    self.expr(r, rmin, indent)
                );
                wrap(text, p, min)
            }
            LExpr::Function(f) => {
                let head = self.func_head(f, indent);
                // Carry body positions with the expression until its containing line is emitted.
                let mut sub = Printer {
                    column: 0,
                    out: String::new(),
                    line: 0,
                    lines: Vec::new(),
                    embedded: Vec::new(),
                };
                sub.block(&f.body, indent + 1);
                let mut text = format!("function{head}\n{}", sub.out);
                for _ in 0..indent {
                    text.push_str("  ");
                }
                text.push_str("end");
                self.embedded.push((
                    text.clone(),
                    sub.lines
                        .into_iter()
                        .map(|(line, span)| (line + 1, span))
                        .collect(),
                ));
                text
            }
            LExpr::Table(fields) => {
                if fields.is_empty() {
                    return "{}".into();
                }
                let parts: Vec<String> = fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| match f {
                        LField::Pos(v) => {
                            if i + 1 < fields.len() {
                                self.scalar_expr(v, indent)
                            } else {
                                self.expr(v, 0, indent)
                            }
                        }
                        LField::Named(k, v) if is_lua_name(k) && !is_lua_keyword(k) => {
                            format!("{k} = {}", self.scalar_expr(v, indent))
                        }
                        LField::Named(k, v) => {
                            format!("[{}] = {}", lua_string(k), self.scalar_expr(v, indent))
                        }
                        LField::Keyed(k, v) => {
                            format!(
                                "[{}] = {}",
                                self.scalar_expr(k, indent),
                                self.scalar_expr(v, indent)
                            )
                        }
                    })
                    .collect();
                let flat = format!("{{ {} }}", parts.join(", "));
                if flat.contains('\n') || flat.len() + self.column.max(indent * 2) > 100 {
                    // Break long table literals without changing field order.
                    format!(
                        "{{\n{}\n{}}}",
                        parts
                            .iter()
                            .map(|p| format!("{}{},", "  ".repeat(indent + 1), p))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        "  ".repeat(indent)
                    )
                } else {
                    flat
                }
            }
        }
    }
}

fn wrap(text: String, prec: u8, min: u8) -> String {
    if prec < min {
        format!("({text})")
    } else {
        text
    }
}

fn is_lua_name(s: &str) -> bool {
    let mut cs = s.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn lua_string(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn n(x: &str) -> LExpr {
        LExpr::Number(x.into())
    }

    #[test]
    fn brackets_only_where_needed() {
        let e = bin(BinOp::Add, n("1"), bin(BinOp::Mul, n("2"), n("3")));
        assert_eq!(print_expr(&e), "1 + 2 * 3");
        let e = bin(BinOp::Mul, bin(BinOp::Add, n("1"), n("2")), n("3"));
        assert_eq!(print_expr(&e), "(1 + 2) * 3");
        let e = bin(BinOp::Sub, n("1"), bin(BinOp::Sub, n("2"), n("3")));
        assert_eq!(print_expr(&e), "1 - (2 - 3)");
        let e = bin(BinOp::Sub, bin(BinOp::Sub, n("1"), n("2")), n("3"));
        assert_eq!(print_expr(&e), "1 - 2 - 3");
        let s = |x: &str| LExpr::Str(x.into());
        let e = bin(BinOp::Concat, bin(BinOp::Concat, s("a"), s("b")), s("c"));
        assert_eq!(print_expr(&e), "\"a\" .. \"b\" .. \"c\"");
        let e = bin(BinOp::Concat, s("a"), bin(BinOp::Concat, s("b"), s("c")));
        assert_eq!(print_expr(&e), "\"a\" .. \"b\" .. \"c\"");
        let e = not(bin(BinOp::Eq, name("a"), LExpr::Nil));
        assert_eq!(print_expr(&e), "not (a == nil)");
        let e = bin(BinOp::Or, bin(BinOp::And, name("a"), name("b")), name("c"));
        assert_eq!(print_expr(&e), "a and b or c");
        let e = LExpr::Unary(
            UnOp::Neg,
            Box::new(LExpr::Unary(UnOp::Neg, Box::new(name("x")))),
        );
        assert_eq!(print_expr(&e), "-(-x)");
    }

    #[test]
    fn break_and_return_end_their_block() {
        let sp = Span::new("t.selu", 0, 0);
        let s = |kind| LStmt::new(kind, &sp);
        let call = || s(LStmtKind::Call(call(name("f"), vec![])));
        let block = vec![
            s(LStmtKind::Break),
            call(),
            s(LStmtKind::Return(vec![])),
            call(),
        ];
        assert_eq!(
            print_block(&block).text,
            "do\n  break\nend\nf()\ndo\n  return\nend\nf()\n"
        );
        let last = vec![call(), s(LStmtKind::Break)];
        assert_eq!(print_block(&last).text, "f()\nbreak\n");
    }

    #[test]
    fn prefixes_fields_and_tables() {
        let e = LExpr::Method(Box::new(LExpr::Str("x".into())), "upper".into(), vec![]);
        assert_eq!(print_expr(&e), "(\"x\"):upper()");
        let e = LExpr::Field(Box::new(name("t")), "end".into());
        assert_eq!(print_expr(&e), "t[\"end\"]");
        let e = LExpr::Table(vec![
            LField::Pos(n("1")),
            LField::Named("a".into(), n("2")),
            LField::Named("end".into(), n("3")),
        ]);
        assert_eq!(print_expr(&e), "{ 1, a = 2, [\"end\"] = 3 }");
    }
}

//! The Selu formatter (`selu fmt`, Studio's Format Document).
//!
//! A conservative, token-based formatter, in the spirit of gofmt rather
//! than Prettier: it keeps the author's line breaks and changes only
//! layout that has one right answer.
//!
//! - Indentation is recomputed from bracket nesting (`indent` spaces per
//!   level, default 2); a line that continues an unfinished statement is
//!   indented one extra level.
//! - Spacing between tokens on a line is normalised (`a = b + c;`,
//!   `f(x, y)`, `if (x) {`, `x: number`, `cond ? a : b`, `-x`, `i++`).
//! - A block whose braces are on different lines gets its body on its own
//!   lines; `}` followed by `else`, `catch` or `finally` on the next line is
//!   joined (`} else {`); several statements on one line are split, except
//!   inside parentheses (`for (;;)`) and inside one-line blocks.
//! - At most one blank line in a row, no trailing whitespace, no blank lines
//!   at the start of a block or the file, exactly one final newline.
//! - Comments, strings and template strings are kept verbatim.
//!
//! Formatting never changes the token sequence, so it can't change what a
//! program means (Selu requires semicolons, so line breaks carry no
//! meaning); the tests check this for every example. Lines are not wrapped
//! at a width yet.

/// Formatter settings, from `[format]` in `selu.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Spaces per indentation level.
    pub indent: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options { indent: 2 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError {
    pub message: String,
    /// 1-based line.
    pub line: usize,
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for FormatError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Identifier or keyword.
    Word,
    Number,
    /// String or template literal, verbatim.
    Str,
    LineComment,
    BlockComment,
    Punct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    /// Blank lines before this token (0 when on the same line as the
    /// previous token or directly below it).
    pub newlines_before: usize,
    /// Whitespace directly before it in the source (same line).
    pub space_before: bool,
}

const PUNCT: &[&str] = &[
    "...", "===", "!==", "**=", "<<=", ">>=", "&&=", "||=", "??=", "==", "!=", "<=", ">=", "&&",
    "||", "??", "?.", "=>", "++", "--", "+=", "-=", "*=", "/=", "%=", "**", "<<", ">>", "&=", "|=",
    "^=",
];

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Split `src` into tokens, keeping comments and verbatim literals.
pub fn tokenize(src: &str) -> Result<Vec<Token>, FormatError> {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let n = chars.len();
    let at = |i: usize| chars.get(i).map(|&(_, c)| c);
    let byte = |i: usize| chars.get(i).map_or(src.len(), |&(b, _)| b);
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let mut newlines = 0;
    let mut space = false;
    while i < n {
        let c = chars[i].1;
        if c == '\n' {
            newlines += 1;
            line += 1;
            space = false;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            space = true;
            i += 1;
            continue;
        }
        let start = i;
        let start_line = line;
        let kind;
        if c == '/' && at(i + 1) == Some('/') {
            while i < n && chars[i].1 != '\n' {
                i += 1;
            }
            kind = Kind::LineComment;
        } else if c == '/' && at(i + 1) == Some('*') {
            i += 2;
            loop {
                match at(i) {
                    None => {
                        return Err(FormatError {
                            message: "unclosed block comment".into(),
                            line: start_line,
                        });
                    }
                    Some('*') if at(i + 1) == Some('/') => {
                        i += 2;
                        break;
                    }
                    Some('\n') => {
                        line += 1;
                        i += 1;
                    }
                    Some(_) => i += 1,
                }
            }
            kind = Kind::BlockComment;
        } else if c == '"' || c == '\'' {
            i += 1;
            loop {
                match at(i) {
                    None | Some('\n') => {
                        return Err(FormatError {
                            message: "unclosed string".into(),
                            line: start_line,
                        });
                    }
                    Some('\\') => i += 2,
                    Some(q) if q == c => {
                        i += 1;
                        break;
                    }
                    Some(_) => i += 1,
                }
            }
            kind = Kind::Str;
        } else if c == '`' {
            // Template: verbatim up to the matching backtick, through
            // nested `${ ... }` (which may hold strings and templates).
            i += 1;
            let mut depth: Vec<usize> = Vec::new();
            loop {
                match at(i) {
                    None => {
                        return Err(FormatError {
                            message: "unclosed template string".into(),
                            line: start_line,
                        });
                    }
                    Some('\n') => {
                        line += 1;
                        i += 1;
                    }
                    Some('\\') if depth.is_empty() => i += 2,
                    Some('`') if depth.is_empty() => {
                        i += 1;
                        break;
                    }
                    Some('$') if depth.is_empty() && at(i + 1) == Some('{') => {
                        depth.push(0);
                        i += 2;
                    }
                    Some('{') if !depth.is_empty() => {
                        *depth.last_mut().unwrap() += 1;
                        i += 1;
                    }
                    Some('}') if !depth.is_empty() => {
                        let d = depth.last_mut().unwrap();
                        if *d == 0 {
                            depth.pop();
                        } else {
                            *d -= 1;
                        }
                        i += 1;
                    }
                    Some(q @ ('"' | '\'')) if !depth.is_empty() => {
                        i += 1;
                        while let Some(x) = at(i) {
                            i += 1;
                            if x == '\\' {
                                i += 1;
                            } else if x == q || x == '\n' {
                                break;
                            }
                        }
                    }
                    Some(_) => i += 1,
                }
            }
            kind = Kind::Str;
        } else if c.is_ascii_digit() || (c == '.' && at(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            while let Some(d) = at(i) {
                if d.is_ascii_alphanumeric() || d == '.' || d == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            kind = Kind::Number;
        } else if is_ident_start(c) {
            while at(i).is_some_and(is_ident) {
                i += 1;
            }
            kind = Kind::Word;
        } else {
            let rest = &src[byte(i)..];
            let len = PUNCT
                .iter()
                .find(|p| rest.starts_with(**p))
                .map_or(1, |p| p.chars().count());
            i += len;
            kind = Kind::Punct;
        }
        out.push(Token {
            kind,
            text: src[byte(start)..byte(i)].to_string(),
            newlines_before: newlines,
            space_before: space,
        });
        newlines = 0;
        space = false;
    }
    Ok(out)
}

const CONTROL: &[&str] = &["if", "for", "while", "switch", "catch", "with"];
const BINARY: &[&str] = &[
    "=", "==", "!=", "===", "!==", "<=", ">=", "&&", "||", "??", "=>", "+=", "-=", "*=", "/=",
    "%=", "**=", "<<=", ">>=", "&&=", "||=", "??=", "&=", "|=", "^=", "*", "/", "%", "**", "<<",
    ">>", "&", "|", "^",
];
const EXPR_KEYWORDS: &[&str] = &[
    "return",
    "typeof",
    "throw",
    "case",
    "in",
    "of",
    "instanceof",
    "new",
    "await",
    "yield",
    "else",
    "do",
];

/// Keywords after which `[` starts an array or a destructuring pattern
/// rather than indexing (`const [a, b] = f();`, `return [1, 2];`).
const DECL_KEYWORDS: &[&str] = &[
    "const",
    "let",
    "var",
    "export",
    "import",
    "from",
    "as",
    "is",
    "extends",
    "implements",
    "static",
    "readonly",
    "declare",
    "function",
    "class",
    "interface",
    "type",
    "enum",
    "on",
    "saved",
    "global",
    "if",
    "for",
    "while",
    "switch",
    "catch",
];

fn is_operand_end(t: &Token) -> bool {
    match t.kind {
        Kind::Word => {
            !EXPR_KEYWORDS.contains(&t.text.as_str()) && !DECL_KEYWORDS.contains(&t.text.as_str())
        }
        Kind::Number | Kind::Str => true,
        Kind::Punct => matches!(t.text.as_str(), ")" | "]" | "}" | "++" | "--"),
        _ => false,
    }
}

/// Does `-`/`+` at `i` act as a sign (unary)? Decided by the previous
/// token in the file, so `a` + newline + `+ b` stays binary.
fn is_unary(tokens: &[Token], i: usize) -> bool {
    i == 0 || !is_operand_end(&tokens[i - 1])
}

/// The spacing between `prev` and `cur` on the same line.
fn space_between(tokens: &[Token], i: usize, ternary: &[bool]) -> bool {
    let prev = &tokens[i - 1];
    let cur = &tokens[i];
    let p = prev.text.as_str();
    let c = cur.text.as_str();
    if cur.kind == Kind::LineComment || cur.kind == Kind::BlockComment {
        return true;
    }
    // Tagged templates (`luaPattern`...``, `L`...``) stay attached.
    if cur.kind == Kind::Str && c.starts_with('`') && !cur.space_before && is_operand_end(prev) {
        return false;
    }
    if prev.kind == Kind::BlockComment {
        return true;
    }
    // Ambiguous `<` `>` (comparison or type arguments): keep the source's choice.
    if matches!(c, "<" | ">") {
        return cur.space_before;
    }
    if matches!(p, "<" | ">") {
        return cur.space_before;
    }
    // Tight pairs.
    if matches!(p, "(" | "[" | "." | "?." | "...")
        || matches!(c, ")" | "]" | "," | ";" | "." | "?.")
    {
        return false;
    }
    if c == "}" {
        return p != "{";
    }
    if p == "{" {
        return true;
    }
    if p == "," || p == ";" {
        return true;
    }
    // Unary signs and prefix operators.
    if matches!(p, "!" | "~") {
        return false;
    }
    if matches!(p, "-" | "+") && is_unary(tokens, i - 1) {
        return false;
    }
    if matches!(p, "++" | "--") && !(i >= 2 && is_operand_end(&tokens[i - 2])) {
        return false; // prefix ++x
    }
    if matches!(c, "++" | "--") && is_operand_end(prev) {
        return false; // postfix x++
    }
    // `?` and `:`
    if c == "?" {
        return ternary.get(i).copied().unwrap_or(false);
    }
    if p == "?" {
        return ternary.get(i - 1).copied().unwrap_or(false);
    }
    if c == ":" {
        return ternary.get(i).copied().unwrap_or(false);
    }
    if p == ":" {
        return true;
    }
    if c == "(" {
        return match prev.kind {
            Kind::Word => CONTROL.contains(&p) || EXPR_KEYWORDS.contains(&p),
            Kind::Punct => !matches!(p, ")" | "]"),
            _ => true,
        };
    }
    if c == "[" {
        return !is_operand_end(prev);
    }
    if c == "{" {
        return true;
    }
    if BINARY.contains(&c) || BINARY.contains(&p) {
        return true;
    }
    if matches!(c, "-" | "+") {
        // Binary when it follows an operand; a sign gets its space from
        // the previous token's rule.
        return true;
    }
    true
}

/// Mark which `?` and `:` tokens belong to a conditional expression.
fn ternary_marks(tokens: &[Token]) -> Vec<bool> {
    let mut marks = vec![false; tokens.len()];
    // Stack of (bracket depth, `?` index) for open conditionals.
    let mut open: Vec<usize> = Vec::new();
    let mut depth_stack: Vec<usize> = Vec::new();
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate() {
        match t.text.as_str() {
            "(" | "[" | "{" if t.kind == Kind::Punct => depth += 1,
            ")" | "]" | "}" if t.kind == Kind::Punct => {
                depth = depth.saturating_sub(1);
                while depth_stack.last().is_some_and(|d| *d > depth) {
                    depth_stack.pop();
                    open.pop();
                }
            }
            "?" if t.kind == Kind::Punct => {
                // `x?: T` and `x?)` are optional markers, not conditionals.
                let next = tokens.get(i + 1).map(|n| n.text.as_str());
                if !matches!(next, Some(":" | ")" | "," | "=")) {
                    marks[i] = true;
                    open.push(i);
                    depth_stack.push(depth);
                }
            }
            ":" if t.kind == Kind::Punct && depth_stack.last() == Some(&depth) => {
                marks[i] = true;
                open.pop();
                depth_stack.pop();
            }
            _ => {}
        }
    }
    marks
}

struct Line {
    /// Tokens on this output line (indices into the token list).
    tokens: Vec<usize>,
    blank_before: bool,
}

/// Format a whole file.
pub fn format(src: &str, options: &Options) -> Result<String, FormatError> {
    let tokens = tokenize(src)?;
    if tokens.is_empty() {
        return Ok(String::new());
    }
    // 1. Group tokens into lines as the author wrote them.
    let mut lines: Vec<Line> = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if i == 0 || t.newlines_before > 0 {
            lines.push(Line {
                tokens: vec![i],
                blank_before: t.newlines_before > 1,
            });
        } else {
            lines.last_mut().unwrap().tokens.push(i);
        }
    }

    // 2. Matching brackets, to tell one-line blocks from multi-line ones.
    let mut partner = vec![usize::MAX; tokens.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if t.kind != Kind::Punct {
            continue;
        }
        match t.text.as_str() {
            "(" | "[" | "{" => stack.push(i),
            ")" | "]" | "}" => {
                if let Some(o) = stack.pop() {
                    partner[o] = i;
                    partner[i] = o;
                }
            }
            _ => {}
        }
    }
    let mut line_of = vec![0usize; tokens.len()];
    for (l, line) in lines.iter().enumerate() {
        for &t in &line.tokens {
            line_of[t] = l;
        }
    }

    // 3. Split lines: after `{` whose `}` is on a later line, before such a
    // `}`, after `;` at statement level when more code follows.
    let mut split: Vec<Line> = Vec::new();
    for line in lines {
        let mut cur = Line {
            tokens: Vec::new(),
            blank_before: line.blank_before,
        };
        let mut paren = 0i32;
        let mut inline_braces = 0i32;
        let n = line.tokens.len();
        for (k, &t) in line.tokens.iter().enumerate() {
            let tok = &tokens[t];
            let text = tok.text.as_str();
            let is_p = tok.kind == Kind::Punct;
            let multi_close = is_p
                && text == "}"
                && partner[t] != usize::MAX
                && line_of[partner[t]] != line_of[t];
            if multi_close && !cur.tokens.is_empty() {
                split.push(std::mem::replace(
                    &mut cur,
                    Line {
                        tokens: Vec::new(),
                        blank_before: false,
                    },
                ));
            }
            cur.tokens.push(t);
            if is_p {
                match text {
                    "(" | "[" => paren += 1,
                    ")" | "]" => paren -= 1,
                    "{" if partner[t] != usize::MAX && line_of[partner[t]] == line_of[t] => {
                        inline_braces += 1
                    }
                    "}" if partner[t] != usize::MAX && line_of[partner[t]] == line_of[t] => {
                        inline_braces -= 1
                    }
                    _ => {}
                }
            }
            let next = line.tokens.get(k + 1).map(|&x| &tokens[x]);
            let next_is_comment = next.is_some_and(|x| x.kind == Kind::LineComment);
            let more = k + 1 < n && !next_is_comment;
            let multi_open = is_p
                && text == "{"
                && partner[t] != usize::MAX
                && line_of[partner[t]] != line_of[t];
            let stmt_end = is_p && text == ";" && paren == 0 && inline_braces == 0;
            // `}` of a multi-line block followed by more code: keep
            // `} else {`, `});`, `},` and `})` together; split otherwise.
            let close_then_code = multi_close
                && next.is_some_and(|x| {
                    !matches!(
                        x.text.as_str(),
                        ")" | "]" | "," | ";" | "." | "?." | "else" | "catch" | "finally" | "while"
                    ) && x.kind != Kind::LineComment
                });
            if more && (multi_open || stmt_end || close_then_code) {
                split.push(std::mem::replace(
                    &mut cur,
                    Line {
                        tokens: Vec::new(),
                        blank_before: false,
                    },
                ));
            }
        }
        if !cur.tokens.is_empty() {
            split.push(cur);
        }
    }

    // 4. Join `}` + `else`/`catch`/`finally` written on separate lines.
    let mut joined: Vec<Line> = Vec::new();
    for line in split {
        let first = &tokens[line.tokens[0]];
        let joinable = first.kind == Kind::Word
            && matches!(first.text.as_str(), "else" | "catch" | "finally")
            && joined.last().is_some_and(|prev: &Line| {
                let last = &tokens[*prev.tokens.last().unwrap()];
                last.kind == Kind::Punct && last.text == "}"
            });
        if joinable {
            joined.last_mut().unwrap().tokens.extend(line.tokens);
        } else {
            joined.push(line);
        }
    }

    // 5. Print. Each bracket left open at the end of a line indents the
    // following lines one level past that line, however many brackets the
    // line opened (`hook(f, () => {` indents its body once).
    let ternary = ternary_marks(&tokens);
    let unit = " ".repeat(options.indent);
    let mut out = String::new();
    // Indent level to return to for each open bracket.
    let mut stack: Vec<usize> = Vec::new();
    let mut prev_line_last: Option<usize> = None;
    let mut prev_opened = false;
    for (l, line) in joined.iter().enumerate() {
        let mut lead_close = 0;
        for &t in &line.tokens {
            let tok = &tokens[t];
            if tok.kind == Kind::Punct && matches!(tok.text.as_str(), ")" | "]" | "}") {
                lead_close += 1;
            } else {
                break;
            }
        }
        for _ in 0..lead_close {
            stack.pop();
        }
        let first = &tokens[line.tokens[0]];
        let continuation = match prev_line_last {
            Some(p) => {
                let pt = &tokens[p];
                let ends = (pt.kind == Kind::Punct
                    && matches!(pt.text.as_str(), ";" | "{" | "}" | "(" | "[" | ","))
                    || matches!(pt.kind, Kind::LineComment | Kind::BlockComment);
                !ends
                    && lead_close == 0
                    && !prev_opened
                    && !(first.kind == Kind::Word
                        && matches!(first.text.as_str(), "else" | "catch" | "finally"))
            }
            None => false,
        };
        let base = stack.last().copied().unwrap_or(0);
        let level = base + usize::from(continuation);
        if line.blank_before && l > 0 {
            out.push('\n');
        }
        for _ in 0..level {
            out.push_str(&unit);
        }
        let mut opened_here = 0usize;
        for (k, &t) in line.tokens.iter().enumerate() {
            let tok = &tokens[t];
            if k > 0 && space_between(&tokens, t, &ternary) {
                out.push(' ');
            }
            // Block comments keep their inner lines as written.
            out.push_str(if tok.kind == Kind::BlockComment {
                tok.text.trim_end()
            } else {
                &tok.text
            });
            if tok.kind == Kind::Punct && k >= lead_close {
                match tok.text.as_str() {
                    "(" | "[" | "{" => {
                        stack.push(level + 1);
                        opened_here += 1;
                    }
                    ")" | "]" | "}" => {
                        stack.pop();
                        opened_here = opened_here.saturating_sub(1);
                    }
                    _ => {}
                }
            }
        }
        out.push('\n');
        prev_opened = opened_here > 0;
        prev_line_last = line.tokens.last().copied();
    }
    // No blank lines right after an opening `{` or before a closing `}`.
    let mut cleaned: Vec<&str> = Vec::new();
    let all: Vec<&str> = out.lines().collect();
    for (i, l) in all.iter().enumerate() {
        if l.trim().is_empty() {
            let after_open = cleaned.last().is_some_and(|p| p.trim_end().ends_with('{'));
            let before_close = all
                .get(i + 1)
                .is_some_and(|n| n.trim_start().starts_with('}'));
            if after_open || before_close || cleaned.is_empty() {
                continue;
            }
        }
        cleaned.push(l.trim_end());
    }
    let mut result = cleaned.join("\n");
    result.push('\n');
    Ok(result)
}

/// The code tokens of `src` (no comments), for checking that formatting
/// kept the program unchanged.
pub fn code_tokens(src: &str) -> Result<Vec<String>, FormatError> {
    Ok(tokenize(src)?
        .into_iter()
        .filter(|t| !matches!(t.kind, Kind::LineComment | Kind::BlockComment))
        .map(|t| t.text)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(s: &str) -> String {
        let out = format(s, &Options::default()).unwrap();
        assert_eq!(
            code_tokens(s).unwrap(),
            code_tokens(&out).unwrap(),
            "tokens changed"
        );
        let again = format(&out, &Options::default()).unwrap();
        assert_eq!(out, again, "not idempotent");
        out
    }

    #[test]
    fn destructuring_and_tagged_templates() {
        assert_eq!(fmt("const [a, b] = f(x)[0];"), "const [a, b] = f(x)[0];\n");
        assert_eq!(fmt("return [1];"), "return [1];\n");
        assert_eq!(
            fmt("s.match(luaPattern`%d+`); print(L`hi ${n}`);"),
            "s.match(luaPattern`%d+`);\nprint(L`hi ${n}`);\n"
        );
        assert_eq!(fmt("const t = `a`;"), "const t = `a`;\n");
    }

    #[test]
    fn spacing() {
        assert_eq!(fmt("const  x=1+2*y;"), "const x = 1 + 2 * y;\n");
        assert_eq!(fmt("f( a ,b );"), "f(a, b);\n");
        assert_eq!(fmt("if(x){y();}"), "if (x) { y(); }\n");
        assert_eq!(fmt("let n:number=-1;"), "let n: number = -1;\n");
        assert_eq!(fmt("x=a?b:c;"), "x = a ? b : c;\n");
        assert_eq!(fmt("i ++;--j;"), "i++;\n--j;\n");
        assert_eq!(fmt("const t = a.b?.c[0];"), "const t = a.b?.c[0];\n");
        assert_eq!(
            fmt("function f(x?: number) {}"),
            "function f(x?: number) {}\n"
        );
        assert_eq!(
            fmt("const s: Secret<number> = v;"),
            "const s: Secret<number> = v;\n"
        );
        assert_eq!(fmt("if (a < b && c > d) {}"), "if (a < b && c > d) {}\n");
        assert_eq!(fmt("f(...args, !x, -y);"), "f(...args, !x, -y);\n");
        assert_eq!(fmt("return -x;"), "return -x;\n");
        assert_eq!(
            fmt("const o = {a: 1, b: [1, 2]};"),
            "const o = { a: 1, b: [1, 2] };\n"
        );
    }

    #[test]
    fn indentation_and_blocks() {
        let src =
            "on PLAYER_LOGIN() {\nprint(1);\n    if (x) {\n  y();\n}\n  else {\nz();\n      }\n}\n";
        assert_eq!(
            fmt(src),
            "on PLAYER_LOGIN() {\n  print(1);\n  if (x) {\n    y();\n  } else {\n    z();\n  }\n}\n"
        );
        // A block opened on one line and closed later gets its body split.
        assert_eq!(
            fmt("function f() { a(); b();\n}"),
            "function f() {\n  a();\n  b();\n}\n"
        );
        // Callbacks keep `});` together.
        assert_eq!(
            fmt("hook(f, () => {\nprint(1);\n});"),
            "hook(f, () => {\n  print(1);\n});\n"
        );
        // Statement continuation gets one extra level.
        assert_eq!(
            fmt("const total = a +\nb +\nc;\nnext();"),
            "const total = a +\n  b +\n  c;\nnext();\n"
        );
        // for(;;) isn't split.
        assert_eq!(
            fmt("for (let i = 0; i < 3; i++) { f(i); }"),
            "for (let i = 0; i < 3; i++) { f(i); }\n"
        );
    }

    #[test]
    fn comments_blank_lines_and_literals() {
        let src = "\n\n// lead\nconst a = 1;   // trailing\n\n\n\n/* block\n   comment */\nconst s = 'it\\'s';\n";
        assert_eq!(
            fmt(src),
            "// lead\nconst a = 1; // trailing\n\n/* block\n   comment */\nconst s = 'it\\'s';\n"
        );
        // A binary operator starting a continuation line stays binary.
        assert_eq!(fmt("x = a\n+ b;"), "x = a\n  + b;\n");
        assert_eq!(
            fmt("const t = `a ${ x+1 } b`;"),
            "const t = `a ${ x+1 } b`;\n"
        );
        assert_eq!(fmt("f() {\n\n  a();\n\n}"), "f() {\n  a();\n}\n");
    }

    #[test]
    fn broken_code_is_refused_not_mangled() {
        assert_eq!(
            format("x = \"abc", &Options::default()).unwrap_err().line,
            1
        );
        assert!(format("/* open", &Options::default()).is_err());
        assert!(format("a = `x ${ y", &Options::default()).is_err());
    }

    #[test]
    fn indent_width() {
        let out = format("{\nx;\n}", &Options { indent: 4 }).unwrap();
        assert_eq!(out, "{\n    x;\n}\n");
    }
}

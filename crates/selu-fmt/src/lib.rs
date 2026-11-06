//! The Selu formatter (`selu fmt`, Studio's Format Document).
//!
//! An opinionated, token-based formatter in the spirit of clang-format:
//!
//! - Indentation from bracket nesting (`indent` spaces, default 2); a line
//!   continuing an unfinished statement gets one extra level.
//! - Normalised spacing (`a = b + c;`, `f(x, y)`, `if (x) {`, `x: number`,
//!   `cond ? a : b`, `-x`, `i++`, `luaPattern`...``).
//! - Blocks of statements and class bodies always span lines
//!   (`if (x) { y(); }` becomes three lines); short object literals stay
//!   inline. `} else {` is joined; one statement per line.
//! - A blank line between top-level items and between class members when
//!   either side spans several lines; comments directly above an item stay
//!   attached to it. At most one blank line in a row, none right inside
//!   brackets.
//! - Lines longer than `width` (default 100) are wrapped at their outermost
//!   bracket list, one item per line.
//! - Comments, strings and template strings are kept verbatim; the author's
//!   line breaks are otherwise kept.
//!
//! Formatting never changes the token sequence, so it can't change what a
//! program means (Selu requires semicolons, so line breaks carry no
//! meaning); the tests check this for every example.

/// Formatter settings, from `[format]` in `selu.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Spaces per indentation level.
    pub indent: usize,
    /// Lines longer than this are wrapped at their outermost list.
    pub width: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            indent: 2,
            width: 100,
        }
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
        let kind = if c == '/' && at(i + 1) == Some('/') {
            while i < n && chars[i].1 != '\n' {
                i += 1;
            }
            Kind::LineComment
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
            Kind::BlockComment
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
            Kind::Str
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
            Kind::Str
        } else if c.is_ascii_digit() || (c == '.' && at(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            while let Some(d) = at(i) {
                if d.is_ascii_alphanumeric() || d == '.' || d == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            Kind::Number
        } else if is_ident_start(c) {
            while at(i).is_some_and(is_ident) {
                i += 1;
            }
            Kind::Word
        } else {
            let rest = &src[byte(i)..];
            let len = PUNCT
                .iter()
                .find(|p| rest.starts_with(**p))
                .map_or(1, |p| p.chars().count());
            i += len;
            Kind::Punct
        };
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

/// Is `{` at `open` a block of statements (or a class body) rather than an
/// object literal or type? Blocks are always laid out over several lines.
fn block_braces(tokens: &[Token], partner: &[usize]) -> Vec<bool> {
    let mut block = vec![false; tokens.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if t.kind != Kind::Punct {
            continue;
        }
        match t.text.as_str() {
            "(" | "[" | "{" => stack.push(i),
            ")" | "]" | "}" => {
                if let Some(o) = stack.pop()
                    && tokens[o].text == "{"
                    && block[o]
                    && let Some(&parent) = stack.last()
                    && tokens[parent].text == "{"
                {
                    // A brace holding a block holds statements or members.
                    block[parent] = true;
                }
            }
            ";" => {
                if let Some(&top) = stack.last()
                    && tokens[top].text == "{"
                {
                    block[top] = true;
                }
            }
            _ => {}
        }
    }
    // Class, interface and enum bodies, even with one member and no `;`.
    for (i, t) in tokens.iter().enumerate() {
        if t.kind != Kind::Punct || t.text != "{" || partner[i] == usize::MAX || partner[i] == i + 1
        {
            continue;
        }
        let mut k = i;
        while k > 0 {
            k -= 1;
            let p = &tokens[k];
            if p.kind == Kind::Punct && matches!(p.text.as_str(), ";" | "{" | "}" | "=" | "(" | ",")
            {
                break;
            }
            if p.kind == Kind::Word && matches!(p.text.as_str(), "class" | "interface" | "enum") {
                block[i] = true;
                break;
            }
        }
    }
    block
}

/// Is the brace at `open` the body of a class, interface or enum?
fn member_container(tokens: &[Token], open: usize) -> bool {
    let mut k = open;
    while k > 0 {
        k -= 1;
        let p = &tokens[k];
        if p.kind == Kind::Punct && matches!(p.text.as_str(), ";" | "{" | "}" | "=" | "(" | ",") {
            return false;
        }
        if p.kind == Kind::Word && matches!(p.text.as_str(), "class" | "interface" | "enum") {
            return true;
        }
    }
    false
}

/// Is `<` at `i` the start of type arguments (`Map<number, boolean>`)?
/// Written tight after a name, as Selu code is.
fn opens_type_args(tokens: &[Token], i: usize) -> bool {
    let t = &tokens[i];
    t.kind == Kind::Punct
        && t.text == "<"
        && !t.space_before
        && i > 0
        && tokens[i - 1].kind == Kind::Word
        && tokens.get(i + 1).is_some_and(|n| !n.space_before)
}

/// The text of one line's tokens with spacing applied (no indentation).
fn render(tokens: &[Token], line: &[usize], ternary: &[bool]) -> String {
    let mut out = String::new();
    for (k, &t) in line.iter().enumerate() {
        if k > 0 && space_between(tokens, t, ternary) {
            out.push(' ');
        }
        out.push_str(&tokens[t].text);
    }
    out
}

/// Split a line that is too long at the outermost bracket pair on it that
/// holds a comma-separated list: the opener ends the first line, each item
/// gets a line, and the closer starts the last one. `None` if nothing fits
/// that pattern.
fn wrap_once(tokens: &[Token], line: &[usize], partner: &[usize]) -> Option<Vec<Vec<usize>>> {
    let pos: std::collections::HashMap<usize, usize> =
        line.iter().enumerate().map(|(k, &t)| (t, k)).collect();
    let mut best: Option<(usize, usize, Vec<usize>)> = None; // (open k, close k, comma ks)
    for (k, &t) in line.iter().enumerate() {
        let tok = &tokens[t];
        if tok.kind != Kind::Punct || !matches!(tok.text.as_str(), "(" | "[" | "{") {
            continue;
        }
        let Some(&ck) = pos.get(&partner[t]) else {
            continue;
        };
        if ck <= k + 1 {
            continue;
        }
        // Top-level commas inside this pair.
        let mut depth = 0;
        let mut angle = 0;
        let mut commas = Vec::new();
        for (j, &u) in line.iter().enumerate().take(ck).skip(k + 1) {
            let ut = &tokens[u];
            if opens_type_args(tokens, u) {
                angle += 1;
            } else if ut.kind == Kind::Punct && ut.text == ">" && angle > 0 {
                angle -= 1;
            } else if ut.kind == Kind::Punct {
                match ut.text.as_str() {
                    "(" | "[" | "{" => depth += 1,
                    ")" | "]" | "}" => depth -= 1,
                    "," if depth == 0 && angle == 0 => commas.push(j),
                    _ => {}
                }
            }
        }
        if commas.is_empty() {
            continue;
        }
        let span = ck - k;
        if best.as_ref().is_none_or(|(bo, bc, _)| span > bc - bo) {
            best = Some((k, ck, commas));
        }
    }
    let (open, close, commas) = best?;
    let mut parts = vec![line[..=open].to_vec()];
    let mut start = open + 1;
    for c in commas {
        parts.push(line[start..=c].to_vec());
        start = c + 1;
    }
    // The last item may end with a trailing comma already.
    if start < close {
        parts.push(line[start..close].to_vec());
    }
    parts.push(line[close..].to_vec());
    Some(parts)
}

struct Out {
    text: String,
    level: usize,
    /// The line sits directly in a top-level or class/interface/enum body.
    container: bool,
    comment_only: bool,
    /// Brackets opened on this line are still open after it.
    opens: bool,
    /// The line starts with a closing bracket.
    closes: bool,
    /// The line starts a statement or member (not a continuation).
    stmt_start: bool,
    /// It starts a function, class, interface, enum or event handler.
    decl: bool,
    blank_before: bool,
}

/// Format a whole file.
pub fn format(src: &str, options: &Options) -> Result<String, FormatError> {
    let tokens = tokenize(src)?;
    if tokens.is_empty() {
        return Ok(String::new());
    }
    // 1. Lines as the author wrote them.
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

    // 2. Matching brackets and blocks.
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
    let block = block_braces(&tokens, &partner);
    let mut line_of = vec![0usize; tokens.len()];
    for (l, line) in lines.iter().enumerate() {
        for &t in &line.tokens {
            line_of[t] = l;
        }
    }
    // A brace is laid out over several lines when it is a block, or when
    // the author already wrote it that way.
    let multi = |t: usize| -> bool {
        let o = if matches!(tokens[t].text.as_str(), "{" | "[") {
            t
        } else {
            partner[t]
        };
        o != usize::MAX
            && partner[o] != usize::MAX
            && partner[o] != o + 1
            && (block[o] || line_of[o] != line_of[partner[o]])
    };

    // 3. Split: after a multi-line `{`, before its `}`, after `;` at
    // statement level when more code follows on the line.
    let mut split: Vec<Line> = Vec::new();
    // Open brackets across lines: is it a multi-line object or array
    // literal, laid out one item per line?
    let mut lists: Vec<bool> = Vec::new();
    for line in lines {
        let mut cur = Line {
            tokens: Vec::new(),
            blank_before: line.blank_before,
        };
        let mut paren = 0i32;
        let mut inline_braces = 0i32;
        let mut angle = 0i32;
        let n = line.tokens.len();
        for (k, &t) in line.tokens.iter().enumerate() {
            let tok = &tokens[t];
            let text = tok.text.as_str();
            let is_p = tok.kind == Kind::Punct;
            let multi_close =
                is_p && matches!(text, "}" | "]") && partner[t] != usize::MAX && multi(t);
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
                    "(" | "[" | "{" => lists.push(
                        matches!(text, "[" | "{")
                            && partner[t] != usize::MAX
                            && multi(t)
                            && !block[t],
                    ),
                    ")" | "]" | "}" => {
                        lists.pop();
                    }
                    _ => {}
                }
                match text {
                    "(" | "[" => paren += 1,
                    ")" | "]" => paren -= 1,
                    "{" if partner[t] != usize::MAX && !multi(t) => inline_braces += 1,
                    "}" if partner[t] != usize::MAX && !multi(t) => inline_braces -= 1,
                    _ => {}
                }
            }
            let next = line.tokens.get(k + 1).map(|&x| &tokens[x]);
            let next_is_comment = next.is_some_and(|x| x.kind == Kind::LineComment);
            let more = k + 1 < n && !next_is_comment;
            let multi_open =
                is_p && matches!(text, "{" | "[") && partner[t] != usize::MAX && multi(t);
            let stmt_end = is_p && text == ";" && paren <= 0 && inline_braces == 0;
            let close_then_code = multi_close
                && next.is_some_and(|x| {
                    !matches!(
                        x.text.as_str(),
                        ")" | "]" | "," | ";" | "." | "?." | "else" | "catch" | "finally" | "while"
                    ) && x.kind != Kind::LineComment
                });
            if opens_type_args(&tokens, t) {
                angle += 1;
            } else if is_p && text == ">" && angle > 0 {
                angle -= 1;
            }
            let list_item_end = is_p && text == "," && angle == 0 && lists.last() == Some(&true);
            if more && (multi_open || stmt_end || close_then_code || list_item_end) {
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

    // 4. Join `}` + `else`/`catch`/`finally`.
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

    // 5. Wrap lines longer than the width at their outermost list.
    let ternary = ternary_marks(&tokens);
    let mut wrapped: Vec<Line> = Vec::new();
    let mut work: std::collections::VecDeque<Line> = joined.into_iter().collect();
    let mut budget = 10_000;
    while let Some(line) = work.pop_front() {
        let has_comment = line
            .tokens
            .iter()
            .any(|&t| matches!(tokens[t].kind, Kind::LineComment | Kind::BlockComment));
        let long = render(&tokens, &line.tokens, &ternary).chars().count() + 8 > options.width;
        if long && !has_comment && budget > 0 {
            budget -= 1;
            if let Some(parts) = wrap_once(&tokens, &line.tokens, &partner) {
                for p in parts.into_iter().rev() {
                    work.push_front(Line {
                        tokens: p,
                        blank_before: false,
                    });
                }
                work.front_mut().unwrap().blank_before = line.blank_before;
                continue;
            }
        }
        wrapped.push(line);
    }

    // 6. Indent. Each bracket left open at the end of a line indents the
    // following lines one level past that line.
    let mut outs: Vec<Out> = Vec::new();
    // (indent level to return to, is a member container)
    let mut stack: Vec<(usize, bool)> = Vec::new();
    let mut prev_line_last: Option<usize> = None;
    let mut prev_opened = false;
    for line in &wrapped {
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
        let prev_ends = prev_line_last.is_none_or(|p| {
            let pt = &tokens[p];
            (pt.kind == Kind::Punct
                && matches!(pt.text.as_str(), ";" | "{" | "}" | "(" | "[" | ","))
                || matches!(pt.kind, Kind::LineComment | Kind::BlockComment)
        });
        let continuation = !prev_ends
            && lead_close == 0
            && !prev_opened
            && !(first.kind == Kind::Word
                && matches!(first.text.as_str(), "else" | "catch" | "finally"));
        let (base, container) = stack.last().copied().unwrap_or((0, true));
        let level = base + usize::from(continuation);
        let text = render(&tokens, &line.tokens, &ternary);
        let mut opened_here = 0usize;
        for (k, &t) in line.tokens.iter().enumerate() {
            let tok = &tokens[t];
            if tok.kind == Kind::Punct && k >= lead_close {
                match tok.text.as_str() {
                    "(" | "[" | "{" => {
                        let member = tok.text == "{" && member_container(&tokens, t);
                        stack.push((level + 1, member));
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
        let decl = {
            let words: Vec<&str> = line
                .tokens
                .iter()
                .take(2)
                .map(|&t| tokens[t].text.as_str())
                .collect();
            let w = if words.first() == Some(&"export") {
                words.get(1).copied()
            } else {
                words.first().copied()
            };
            matches!(w, Some("function" | "class" | "interface" | "enum" | "on"))
        };
        let comment_only = line
            .tokens
            .iter()
            .all(|&t| matches!(tokens[t].kind, Kind::LineComment | Kind::BlockComment));
        outs.push(Out {
            text,
            level,
            container,
            comment_only,
            opens: opened_here > 0,
            closes: lead_close > 0,
            stmt_start: prev_ends && !continuation && lead_close == 0,
            decl,
            blank_before: line.blank_before,
        });
        prev_opened = opened_here > 0;
        prev_line_last = line.tokens.last().copied();
    }

    // 7. Blank lines between top-level items and class members when either
    // side spans several lines. A comment block directly above an item
    // stays attached to it.
    // The statement each line belongs to (its first line).
    let mut start_of = vec![0usize; outs.len()];
    let mut last_start = 0usize;
    for (i, o) in outs.iter().enumerate() {
        if o.stmt_start && !o.comment_only {
            last_start = i;
        }
        start_of[i] = last_start;
    }
    for i in 0..outs.len() {
        let o = &outs[i];
        if !(o.container && o.stmt_start) || o.comment_only {
            continue;
        }
        // Attached comments above this line.
        let mut k = i;
        while k > 0
            && outs[k - 1].comment_only
            && outs[k - 1].level == o.level
            && !outs[k].blank_before
        {
            k -= 1;
        }
        if k == 0 || outs[k].blank_before {
            continue;
        }
        let prev = &outs[k - 1];
        // The previous line must belong to the same container (not its
        // opening line).
        if prev.level != o.level || prev.opens && !prev.closes {
            continue;
        }
        let prev_multi = prev.closes;
        let prev_decl = outs[start_of[k - 1]].decl;
        let o = &outs[i];
        if prev_multi || o.opens || o.decl || prev_decl {
            outs[k].blank_before = true;
        }
    }

    // 8. Print.
    let unit = " ".repeat(options.indent);
    let mut out = String::new();
    for (i, o) in outs.iter().enumerate() {
        if o.blank_before && i > 0 {
            out.push('\n');
        }
        for _ in 0..o.level {
            out.push_str(&unit);
        }
        out.push_str(&o.text);
        out.push('\n');
    }
    // No blank lines right after an opening `{` or before a closing `}`.
    let mut cleaned: Vec<&str> = Vec::new();
    let all: Vec<&str> = out.lines().collect();
    for (i, l) in all.iter().enumerate() {
        if l.trim().is_empty() {
            let after_open = cleaned
                .last()
                .is_some_and(|p| p.trim_end().ends_with(['{', '(', '[']));
            let before_close = all
                .get(i + 1)
                .is_some_and(|n| n.trim_start().starts_with(['}', ')', ']']));
            if after_open
                || before_close
                || cleaned.is_empty()
                || cleaned.last().is_some_and(|p| p.trim().is_empty())
            {
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
    fn blank_lines_between_items() {
        let src = "const a = 1;\nconst b = 2;\nfunction f() { a(); }\nfunction g() { b(); }\n// Handles login.\non PLAYER_LOGIN() { f(); }\nconst c = 3;\n";
        assert_eq!(
            fmt(src),
            "const a = 1;\nconst b = 2;\n\nfunction f() {\n  a();\n}\n\nfunction g() {\n  b();\n}\n\n// Handles login.\non PLAYER_LOGIN() {\n  f();\n}\n\nconst c = 3;\n"
        );
        // Class members: methods are separated, simple fields stay together.
        let src = "class A {\n  x = 1;\n  y = 2;\n  m() { return 1; }\n  n() { return 2; }\n}\n";
        assert_eq!(
            fmt(src),
            "class A {\n  x = 1;\n  y = 2;\n\n  m() {\n    return 1;\n  }\n\n  n() {\n    return 2;\n  }\n}\n"
        );
        // Statements inside functions keep the author's grouping.
        let src = "function f() {\n  a();\n  if (x) {\n    b();\n  }\n  c();\n}\n";
        assert_eq!(fmt(src), src);
        // Short object literals stay inline; empty blocks stay `{}`.
        assert_eq!(
            fmt("const o = { a: 1 };\nfunction e() {}\n"),
            "const o = { a: 1 };\n\nfunction e() {}\n"
        );
    }

    #[test]
    fn multi_line_literals_get_one_item_per_line() {
        let src = "const s = {
  a: 1, b: 2,
  c: [1, 2, 3] };
const xs = [
1, 2];
const m = {
  locks: new Map<number, boolean>() };
";
        assert_eq!(
            fmt(src),
            "const s = {
  a: 1,
  b: 2,
  c: [1, 2, 3]
};

const xs = [
  1,
  2
];

const m = {
  locks: new Map<number, boolean>()
};
"
        );
    }

    #[test]
    fn long_lines_wrap_at_the_outermost_list() {
        let src = "const settings = { enabled: true, maxQuality: 0, maxValue: 0, announce: true, autoRepair: true, guild: false };\n";
        assert_eq!(
            fmt(src),
            "const settings = {\n  enabled: true,\n  maxQuality: 0,\n  maxValue: 0,\n  announce: true,\n  autoRepair: true,\n  guild: false\n};\n"
        );
        let src = "print(someLongFunctionName(alpha), anotherLongFunctionName(beta), yetAnotherFunction(gamma, delta));\n";
        assert_eq!(
            fmt(src),
            "print(\n  someLongFunctionName(alpha),\n  anotherLongFunctionName(beta),\n  yetAnotherFunction(gamma, delta)\n);\n"
        );
        // Under the width: untouched.
        assert_eq!(fmt("print(a, b);\n"), "print(a, b);\n");
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
        assert_eq!(
            format("type V=string|number;", &Options::default()).unwrap(),
            "type V = string | number;\n"
        );
        assert_eq!(fmt("const  x=1+2*y;"), "const x = 1 + 2 * y;\n");
        assert_eq!(fmt("f( a ,b );"), "f(a, b);\n");
        assert_eq!(fmt("if(x){y();}"), "if (x) {\n  y();\n}\n");
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
        // for(;;) headers keep their semicolons; the body is a block.
        assert_eq!(
            fmt("for (let i = 0; i < 3; i++) { f(i); }"),
            "for (let i = 0; i < 3; i++) {\n  f(i);\n}\n"
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
        let out = format(
            "{\nx;\n}",
            &Options {
                indent: 4,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(out, "{\n    x;\n}\n");
    }
}

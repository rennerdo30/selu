//! Syntax highlighting for `.selu` files. A small tolerant scanner, not the
//! compiler's lexer: it must colour half-typed code (unclosed strings,
//! comments) without failing, and it keeps comments, which the lexer drops.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Keyword,
    Type,
    Constant,
    String,
    Number,
    Comment,
    /// An identifier followed by `(`.
    Call,
    /// A name the game API defines (function, namespace, widget, enum).
    Api,
    /// The event name after `on`.
    Event,
    Punct,
}

const KEYWORDS: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "declare",
    "default",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "finally",
    "for",
    "from",
    "function",
    "get",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "is",
    "let",
    "new",
    "of",
    "on",
    "private",
    "protected",
    "public",
    "readonly",
    "return",
    "set",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "try",
    "type",
    "typeof",
    "var",
    "while",
    "yield",
];
const TYPES: &[&str] = &[
    "any", "boolean", "never", "number", "object", "string", "unknown", "void", "Error", "Secret",
    "Record", "Array",
];
const CONSTANTS: &[&str] = &["true", "false", "null", "undefined", "NaN", "Infinity"];

pub fn keywords() -> impl Iterator<Item = &'static str> {
    KEYWORDS
        .iter()
        .chain(TYPES.iter())
        .chain(CONSTANTS.iter())
        .copied()
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80
}

fn is_ident(c: u8) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

/// Byte ranges and their kinds, covering `text` without gaps. `is_api`
/// tells whether a bare identifier is a game API name.
pub fn spans(text: &str, is_api: &dyn Fn(&str) -> bool) -> Vec<(Range<usize>, Kind)> {
    let b = text.as_bytes();
    let mut out: Vec<(Range<usize>, Kind)> = Vec::new();
    let mut push = |r: Range<usize>, k: Kind| {
        if r.is_empty() {
            return;
        }
        if let Some(last) = out.last_mut()
            && last.1 == k
            && last.0.end == r.start
        {
            last.0.end = r.end;
            return;
        }
        out.push((r, k));
    };
    let mut i = 0;
    // Inside `${ ... }` of a template: brace depth per open template.
    let mut templates: Vec<usize> = Vec::new();
    let mut after_on = false;
    while i < b.len() {
        let c = b[i];
        let start = i;
        // Continue a template after its `${ ... }` closes.
        if c == b'}' && templates.last() == Some(&0) {
            templates.pop();
            i += 1;
            push(start..i, Kind::Punct);
            let s = i;
            i = template_body(b, i, &mut templates);
            push(s..i, Kind::String);
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            push(start..i, Kind::Comment);
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(b.len());
            push(start..i, Kind::Comment);
        } else if c == b'"' || c == b'\'' {
            i += 1;
            while i < b.len() && b[i] != c && b[i] != b'\n' {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(b.len());
            push(start..i, Kind::String);
        } else if c == b'`' {
            i = template_body(b, i + 1, &mut templates);
            push(start..i, Kind::String);
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            push(start..i, Kind::Number);
        } else if is_ident_start(c) {
            while i < b.len() && is_ident(b[i]) {
                i += 1;
            }
            // Identifiers are ASCII or whole UTF-8 sequences; `i` stays on a
            // char boundary because continuation bytes are >= 0x80 too.
            let word = &text[start..i];
            let mut j = i;
            while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
                j += 1;
            }
            let member = start > 0 && b[start - 1] == b'.';
            let kind = if after_on {
                Kind::Event
            } else if !member && KEYWORDS.contains(&word) {
                Kind::Keyword
            } else if !member && CONSTANTS.contains(&word) {
                Kind::Constant
            } else if !member && TYPES.contains(&word) {
                Kind::Type
            } else if !member && is_api(word) {
                Kind::Api
            } else if b.get(j) == Some(&b'(') {
                Kind::Call
            } else {
                Kind::Plain
            };
            after_on = word == "on" && !member;
            push(start..i, kind);
            continue;
        } else {
            if c == b'{'
                && let Some(depth) = templates.last_mut()
            {
                *depth += 1;
            } else if c == b'}'
                && let Some(depth) = templates.last_mut()
            {
                *depth -= 1;
            }
            // Advance by a whole character.
            let len = text[i..].chars().next().map_or(1, char::len_utf8);
            i += len;
            let kind = if c.is_ascii_whitespace() {
                Kind::Plain
            } else {
                Kind::Punct
            };
            push(start..i, kind);
            if !c.is_ascii_whitespace() {
                after_on = false;
            }
        }
    }
    out
}

/// Scan template text from `i` (just after a backtick or a closing `}`) to
/// the closing backtick (included) or to a `${` (included, opening a new
/// expression level).
fn template_body(b: &[u8], mut i: usize, templates: &mut Vec<usize>) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'`' => return i + 1,
            b'$' if b.get(i + 1) == Some(&b'{') => {
                templates.push(0);
                return i + 2;
            }
            _ => i += 1,
        }
    }
    b.len()
}

/// The identifier around byte offset `at` (for hover), with its range.
pub fn word_at(text: &str, at: usize) -> Option<(Range<usize>, &str)> {
    let b = text.as_bytes();
    if at > b.len() {
        return None;
    }
    let mut s = at;
    while s > 0 && is_ident(b[s - 1]) {
        s -= 1;
    }
    let mut e = at;
    while e < b.len() && is_ident(b[e]) {
        e += 1;
    }
    (s < e && is_ident_start(b[s])).then(|| (s..e, &text[s..e]))
}

/// A dotted name ending at the identifier around `at`: `C_Timer.After`.
pub fn dotted_at(text: &str, at: usize) -> Option<(Range<usize>, String)> {
    let (range, _) = word_at(text, at)?;
    let b = text.as_bytes();
    let mut s = range.start;
    while s > 1 && matches!(b[s - 1], b'.' | b':') && is_ident(b[s - 2]) {
        let mut k = s - 1;
        while k > 0 && is_ident(b[k - 1]) {
            k -= 1;
        }
        s = k;
    }
    Some((range.clone(), text[s..range.end].replace(':', ".")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(&str, Kind)> {
        spans(text, &|w| w == "CreateFrame")
            .into_iter()
            .filter(|(_, k)| *k != Kind::Plain && *k != Kind::Punct)
            .map(|(r, k)| (&text[r], k))
            .collect()
    }

    #[test]
    fn colours_a_line_of_selu() {
        assert_eq!(
            kinds("const f = CreateFrame(\"Frame\"); // note"),
            [
                ("const", Kind::Keyword),
                ("CreateFrame", Kind::Api),
                ("\"Frame\"", Kind::String),
                ("// note", Kind::Comment)
            ]
        );
        assert_eq!(
            kinds("on PLAYER_LOGIN() { print(1.5, true); }"),
            [
                ("on", Kind::Keyword),
                ("PLAYER_LOGIN", Kind::Event),
                ("print", Kind::Call),
                ("1.5", Kind::Number),
                ("true", Kind::Constant)
            ]
        );
    }

    #[test]
    fn templates_and_unclosed_text_dont_break_it() {
        assert_eq!(
            kinds("`a ${x + 1} b` let"),
            [
                ("`a ${", Kind::String),
                ("x", Kind::Plain),
                ("1", Kind::Number),
                (" b`", Kind::String),
                ("let", Kind::Keyword)
            ]
            .into_iter()
            .filter(|(_, k)| *k != Kind::Plain)
            .collect::<Vec<_>>()
        );
        // Unclosed string and comment: no panic, everything covered.
        for text in ["\"abc", "/* open", "`open ${ a", "ä = 1;"] {
            let s = spans(text, &|_| false);
            assert_eq!(s.last().unwrap().0.end, text.len(), "{text}");
        }
    }

    #[test]
    fn words_and_dotted_names() {
        let t = "C_Timer.After(1, f)";
        assert_eq!(word_at(t, 10).unwrap().1, "After");
        assert_eq!(dotted_at(t, 10).unwrap().1, "C_Timer.After");
        assert_eq!(dotted_at(t, 2).unwrap().1, "C_Timer");
        assert!(word_at(t, 13).is_none() || word_at(t, 13).unwrap().1 == "After");
    }
}

/// Tolerant Lua scanner, including long strings and comments.
pub fn lua_spans(text: &str, api: &dyn Fn(&str) -> bool) -> Vec<(Range<usize>, Kind)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let comment = text[i..].starts_with("--");
        if comment {
            i += 2;
        }
        let mut k = i;
        if b.get(k) == Some(&b'[') {
            k += 1;
            while b.get(k) == Some(&b'=') {
                k += 1;
            }
        }
        let long = k > i && b.get(k) == Some(&b'[');
        let kind = if long {
            let close = format!("]{}]", "=".repeat(k - i - 1));
            i = text[k + 1..]
                .find(&close)
                .map_or(text.len(), |n| k + 1 + n + close.len());
            if comment { Kind::Comment } else { Kind::String }
        } else if comment {
            i = text[i..].find('\n').map_or(text.len(), |n| i + n);
            Kind::Comment
        } else if matches!(b[i], b'\'' | b'"') {
            let quote = b[i];
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                } else if b[i] == quote {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            while !text.is_char_boundary(i) {
                i += 1;
            }
            Kind::String
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &text[start..i];
            if [
                "and", "break", "do", "else", "elseif", "end", "for", "function", "if", "in",
                "local", "not", "or", "repeat", "return", "then", "until", "while",
            ]
            .contains(&word)
            {
                Kind::Keyword
            } else if ["true", "false", "nil"].contains(&word) {
                Kind::Constant
            } else if api(word) {
                Kind::Api
            } else {
                Kind::Plain
            }
        } else {
            i += text[i..].chars().next().unwrap().len_utf8();
            if b[start].is_ascii_digit() {
                Kind::Number
            } else {
                Kind::Punct
            }
        };
        out.push((start..i, kind));
    }
    out
}

pub fn xml_spans(text: &str) -> Vec<(Range<usize>, Kind)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let start = i;
        let (end, kind) = if text[i..].starts_with("<!--") {
            (
                text[i + 4..]
                    .find("-->")
                    .map_or(text.len(), |n| i + 4 + n + 3),
                Kind::Comment,
            )
        } else if text.as_bytes()[i] == b'<' {
            (
                text[i..].find('>').map_or(text.len(), |n| i + n + 1),
                Kind::Keyword,
            )
        } else {
            (
                text[i..].find('<').map_or(text.len(), |n| i + n),
                Kind::Plain,
            )
        };
        i = end;
        out.push((start..i, kind));
    }
    out
}

#[cfg(test)]
mod addon_tests {
    use super::*;
    #[test]
    fn lua_comments_long_strings_and_xml_comments() {
        let text = "local name = [=[hello]=] -- comment\nreturn nil";
        let got: Vec<_> = lua_spans(text, &|_| false)
            .into_iter()
            .map(|(r, k)| (text[r].to_string(), k))
            .collect();
        assert!(got.contains(&("local".into(), Kind::Keyword)));
        assert!(got.contains(&("[=[hello]=]".into(), Kind::String)));
        assert!(got.contains(&("-- comment".into(), Kind::Comment)));
        assert_eq!(xml_spans("<!-- hello --><Ui/> ")[0].1, Kind::Comment);
    }
}

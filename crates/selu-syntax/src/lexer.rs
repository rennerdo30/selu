use selu_diagnostics::{Diagnostic, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Number(String),
    Str(String),
    /// A template string without `${}`.
    Template(String),
    /// The text of a template up to its first `${`.
    TemplateHead(String),
    /// The text between a `}` and the next `${` of a template.
    TemplateMiddle(String),
    /// The text after the last `}` of a template, up to the closing backtick.
    TemplateTail(String),
    Const,
    Let,
    Function,
    Return,
    If,
    Else,
    On,
    True,
    False,
    Null,
    While,
    Do,
    For,
    Break,
    Continue,
    New,
    Class,
    Extends,
    Import,
    Export,
    This,
    Super,
    Semi,
    Comma,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Dot,
    DotDotDot,
    QuestionDot,
    Arrow,
    Eq,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AndAnd,
    OrOr,
    Bang,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusPlus,
    MinusMinus,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    Question,
    QuestionQuestion,
    Eof,
}

impl Tok {
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(n) => format!("`{n}`"),
            Tok::Number(n) => format!("number `{n}`"),
            Tok::Str(_) => "a string".into(),
            Tok::Template(_) | Tok::TemplateHead(_) => "a template string".into(),
            Tok::TemplateMiddle(_) | Tok::TemplateTail(_) => "the rest of a template string".into(),
            Tok::Eof => "end of file".into(),
            t => match PUNCT.iter().find(|(_, p)| p == t) {
                Some((text, _)) => format!("`{text}`"),
                None => format!("`{}`", format!("{t:?}").to_lowercase()),
            },
        }
    }
}

/// Punctuation, longest first so `...` wins over `.` and `===` is caught
/// before `==`.
const PUNCT: &[(&str, Tok)] = &[
    ("...", Tok::DotDotDot),
    ("==", Tok::EqEq),
    ("!=", Tok::NotEq),
    ("<=", Tok::LtEq),
    (">=", Tok::GtEq),
    ("&&", Tok::AndAnd),
    ("||", Tok::OrOr),
    ("??", Tok::QuestionQuestion),
    ("?.", Tok::QuestionDot),
    ("=>", Tok::Arrow),
    ("++", Tok::PlusPlus),
    ("--", Tok::MinusMinus),
    ("+=", Tok::PlusEq),
    ("-=", Tok::MinusEq),
    ("*=", Tok::StarEq),
    ("/=", Tok::SlashEq),
    ("%=", Tok::PercentEq),
    (";", Tok::Semi),
    (",", Tok::Comma),
    ("(", Tok::LParen),
    (")", Tok::RParen),
    ("{", Tok::LBrace),
    ("}", Tok::RBrace),
    ("[", Tok::LBracket),
    ("]", Tok::RBracket),
    (":", Tok::Colon),
    (".", Tok::Dot),
    ("=", Tok::Eq),
    ("<", Tok::Lt),
    (">", Tok::Gt),
    ("!", Tok::Bang),
    ("+", Tok::Plus),
    ("-", Tok::Minus),
    ("*", Tok::Star),
    ("/", Tok::Slash),
    ("%", Tok::Percent),
    ("?", Tok::Question),
];

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
    pub template_error: Option<Diagnostic>,
}

struct Lexer<'a> {
    file: &'a str,
    src: &'a str,
    bytes: &'a [u8],
    i: usize,
    out: Vec<Token>,
    /// One entry per template whose `${ ... }` we are inside: the number of
    /// `{` opened in that expression and not yet closed.
    templates: Vec<usize>,
}

pub fn lex(file: &str, src: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut lx = Lexer {
        file,
        src,
        bytes: src.as_bytes(),
        i: 0,
        out: Vec::new(),
        templates: Vec::new(),
    };
    lx.run()?;
    let end = lx.bytes.len();
    lx.push(Tok::Eof, end, end);
    Ok(lx.out)
}

impl Lexer<'_> {
    fn err(&self, code: &str, msg: impl Into<String>, s: usize, e: usize) -> Diagnostic {
        Diagnostic::error(code, msg).with_span(Span::new(self.file, s, e))
    }

    fn push(&mut self, tok: Tok, s: usize, e: usize) {
        self.out.push(Token {
            tok,
            span: Span::new(self.file, s, e),
            template_error: None,
        });
    }

    fn at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.i + offset).copied()
    }

    fn run(&mut self) -> Result<(), Diagnostic> {
        while self.i < self.bytes.len() {
            let start = self.i;
            let c = self.bytes[self.i] as char;
            match c {
                c if c.is_ascii_whitespace() => self.i += 1,
                '/' if self.at(1) == Some(b'/') => {
                    while self.i < self.bytes.len() && self.bytes[self.i] != b'\n' {
                        self.i += 1;
                    }
                }
                '/' if self.at(1) == Some(b'*') => {
                    self.i += 2;
                    while self.i + 1 < self.bytes.len()
                        && !(self.bytes[self.i] == b'*' && self.bytes[self.i + 1] == b'/')
                    {
                        self.i += 1;
                    }
                    if self.i + 1 >= self.bytes.len() {
                        return Err(self.err(
                            "SELU0102",
                            "unterminated block comment",
                            start,
                            self.bytes.len(),
                        ));
                    }
                    self.i += 2;
                }
                '"' | '\'' => {
                    let s = self.string(start)?;
                    self.push(Tok::Str(s), start, self.i);
                }
                '`' => {
                    self.i += 1;
                    self.template_part(start, true)?;
                }
                '{' => {
                    if let Some(depth) = self.templates.last_mut() {
                        *depth += 1;
                    }
                    self.i += 1;
                    self.push(Tok::LBrace, start, self.i);
                }
                // The `}` that ends a `${ ... }` continues its template.
                '}' if self.templates.last() == Some(&0) => {
                    self.templates.pop();
                    self.i += 1;
                    self.template_part(start, false)?;
                }
                '}' => {
                    if let Some(depth) = self.templates.last_mut() {
                        *depth -= 1;
                    }
                    self.i += 1;
                    self.push(Tok::RBrace, start, self.i);
                }
                // `.5` is a number too.
                c if c.is_ascii_digit()
                    || c == '.' && self.at(1).is_some_and(|b| b.is_ascii_digit()) =>
                {
                    self.i += 1;
                    while self.i < self.bytes.len()
                        && (self.bytes[self.i].is_ascii_digit()
                            || self.bytes[self.i] == b'.'
                                && self.at(1).is_some_and(|b| b.is_ascii_digit()))
                    {
                        self.i += 1;
                    }
                    let n = self.src[start..self.i].to_string();
                    self.push(Tok::Number(n), start, self.i);
                }
                c if c.is_ascii_alphabetic() || c == '_' => {
                    while self.i < self.bytes.len()
                        && (self.bytes[self.i].is_ascii_alphanumeric()
                            || self.bytes[self.i] == b'_')
                    {
                        self.i += 1;
                    }
                    let tok = keyword(&self.src[start..self.i]);
                    self.push(tok, start, self.i);
                }
                _ => self.punct(start)?,
            }
        }
        if !self.templates.is_empty() {
            return Err(self.err(
                "SELU0103",
                "unterminated template string: a `${` has no closing `}`",
                self.bytes.len(),
                self.bytes.len(),
            ));
        }
        Ok(())
    }

    fn punct(&mut self, start: usize) -> Result<(), Diagnostic> {
        let rest = &self.src[start..];
        for bad in ["===", "!=="] {
            if rest.starts_with(bad) {
                let good = &bad[..2];
                return Err(self
                    .err(
                        "SELU0109",
                        format!(
                            "`{bad}` isn't Selu: `{good}` already compares without converting types"
                        ),
                        start,
                        start + 3,
                    )
                    .with_fix(format!("use `{good}`")));
            }
        }
        // `a ?.5 : 1` is a conditional with the number `.5`, not `?.`.
        if rest.starts_with("?.") && rest.as_bytes().get(2).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
            self.push(Tok::Question, start, self.i);
            return Ok(());
        }
        if let Some((text, tok)) = PUNCT.iter().find(|(text, _)| rest.starts_with(text)) {
            self.i += text.len();
            self.push(tok.clone(), start, self.i);
            return Ok(());
        }
        // Decode the real character: a span must end on a char boundary.
        let ch = rest.chars().next().unwrap_or('?');
        let end = start + ch.len_utf8();
        if ch.is_alphabetic() {
            return Err(self
                .err(
                    "SELU0107",
                    format!("names can only use ASCII letters, digits and `_`, not `{ch}`"),
                    start,
                    end,
                )
                .with_help("Lua 5.1 only accepts ASCII names; strings can contain any character"));
        }
        Err(self.err(
            "SELU0104",
            format!("unexpected character `{ch}`"),
            start,
            end,
        ))
    }

    /// One escape after a `\` at `self.i`; advances past it.
    fn escape(&mut self, extra: &[(u8, char)]) -> Result<char, Diagnostic> {
        let c = match self.at(1) {
            Some(b'n') => '\n',
            Some(b't') => '\t',
            Some(b'r') => '\r',
            Some(b'0') => '\0',
            Some(b'\\') => '\\',
            Some(b) if extra.iter().any(|(e, _)| *e == b) => extra
                .iter()
                .find(|(e, _)| *e == b)
                .map(|(_, c)| *c)
                .unwrap_or('?'),
            _ => {
                let len = self.src[self.i + 1..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8);
                let known: String = extra.iter().map(|(_, c)| format!(" \\{c}")).collect();
                return Err(self
                    .err(
                        "SELU0108",
                        format!("unknown escape `{}`", &self.src[self.i..self.i + 1 + len]),
                        self.i,
                        self.i + 1 + len,
                    )
                    .with_help(format!(
                        r"known escapes are \n \t \r \0 \\{known}; write a backslash as \\"
                    )));
            }
        };
        self.i += 2;
        Ok(c)
    }

    fn string(&mut self, start: usize) -> Result<String, Diagnostic> {
        let quote = self.bytes[self.i];
        self.i += 1;
        let mut s = String::new();
        loop {
            match self.at(0) {
                None | Some(b'\n') => {
                    return Err(self.err("SELU0103", "unterminated string", start, self.i));
                }
                Some(b'\\') => s.push(self.escape(&[(b'"', '"'), (b'\'', '\'')])?),
                Some(b) if b == quote => {
                    self.i += 1;
                    return Ok(s);
                }
                Some(_) => s.push(self.char()),
            }
        }
    }

    /// Consumes one UTF-8 character.
    fn char(&mut self) -> char {
        let c = self.src[self.i..].chars().next().unwrap_or('?');
        self.i += c.len_utf8();
        c
    }

    /// Template text from `self.i` up to the closing backtick or the next
    /// `${`. `first`: right after the opening backtick (else after a `}`).
    fn template_part(&mut self, start: usize, first: bool) -> Result<(), Diagnostic> {
        let mut s = String::new();
        let mut invalid = None;
        loop {
            match self.at(0) {
                None => {
                    return Err(self.err(
                        "SELU0103",
                        "unterminated template string",
                        start,
                        self.i,
                    ));
                }
                Some(b'\\') => match self.escape(&[(b'`', '`'), (b'$', '$')]) {
                    Ok(c) => s.push(c),
                    Err(e) => {
                        invalid.get_or_insert(e);
                        s.push(self.char());
                    }
                },
                Some(b'`') => {
                    self.i += 1;
                    let tok = if first {
                        Tok::Template(s)
                    } else {
                        Tok::TemplateTail(s)
                    };
                    self.push(tok, start, self.i);
                    self.out.last_mut().unwrap().template_error = invalid;
                    return Ok(());
                }
                Some(b'$') if self.at(1) == Some(b'{') => {
                    self.i += 2;
                    let tok = if first {
                        Tok::TemplateHead(s)
                    } else {
                        Tok::TemplateMiddle(s)
                    };
                    self.push(tok, start, self.i);
                    self.templates.push(0);
                    return Ok(());
                }
                Some(_) => s.push(self.char()),
            }
        }
    }
}

fn keyword(word: &str) -> Tok {
    match word {
        "const" => Tok::Const,
        "let" => Tok::Let,
        "function" => Tok::Function,
        "return" => Tok::Return,
        "if" => Tok::If,
        "else" => Tok::Else,
        "on" => Tok::On,
        "true" => Tok::True,
        "false" => Tok::False,
        "null" => Tok::Null,
        "while" => Tok::While,
        "do" => Tok::Do,
        "for" => Tok::For,
        "break" => Tok::Break,
        "continue" => Tok::Continue,
        "new" => Tok::New,
        "class" => Tok::Class,
        "extends" => Tok::Extends,
        "import" => Tok::Import,
        "export" => Tok::Export,
        "this" => Tok::This,
        "super" => Tok::Super,
        _ => Tok::Ident(word.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex("t.selu", src)
            .unwrap()
            .into_iter()
            .map(|t| t.tok)
            .collect()
    }

    #[test]
    fn templates_with_nested_braces_and_escapes() {
        assert_eq!(
            toks("`a${ {x: 1}.x }b${c}\\`d`"),
            [
                Tok::TemplateHead("a".into()),
                Tok::LBrace,
                Tok::Ident("x".into()),
                Tok::Colon,
                Tok::Number("1".into()),
                Tok::RBrace,
                Tok::Dot,
                Tok::Ident("x".into()),
                Tok::TemplateMiddle("b".into()),
                Tok::Ident("c".into()),
                Tok::TemplateTail("`d".into()),
                Tok::Eof,
            ]
        );
        assert_eq!(
            toks("`plain $ {}`"),
            [Tok::Template("plain $ {}".into()), Tok::Eof]
        );
        // A template inside a template.
        assert_eq!(toks("`${`${x}`}`").len(), 6);
        assert_eq!(lex("t.selu", "`${x").unwrap_err().code, "SELU0103");
    }

    #[test]
    fn punctuation_takes_the_longest_match() {
        assert_eq!(
            toks("a?.b ?? c ... => ++ += ?"),
            [
                Tok::Ident("a".into()),
                Tok::QuestionDot,
                Tok::Ident("b".into()),
                Tok::QuestionQuestion,
                Tok::Ident("c".into()),
                Tok::DotDotDot,
                Tok::Arrow,
                Tok::PlusPlus,
                Tok::PlusEq,
                Tok::Question,
                Tok::Eof,
            ]
        );
    }
}

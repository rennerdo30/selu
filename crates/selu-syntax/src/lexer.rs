use selu_diagnostics::{Diagnostic, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Number(String),
    Str(String),
    Template(String),
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
    Semi,
    Comma,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Colon,
    Dot,
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
    Question,
    QuestionQuestion,
    Eof,
}

impl Tok {
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(n) => format!("`{n}`"),
            Tok::Number(n) => format!("number `{n}`"),
            Tok::Str(_) | Tok::Template(_) => "a string".into(),
            Tok::Semi => "`;`".into(),
            Tok::Eof => "end of file".into(),
            t => format!("{t:?}").to_lowercase(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

pub fn lex(file: &str, src: &str) -> Result<Vec<Token>, Diagnostic> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    let span = |s: usize, e: usize| Span::new(file, s, e);
    let err = |code: &str, msg: String, s: usize, e: usize| {
        Diagnostic::error(code, msg).with_span(Span::new(file, s, e))
    };
    while i < bytes.len() {
        let start = i;
        let c = bytes[i] as char;
        match c {
            c if c.is_whitespace() => i += 1,
            '/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            '/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 >= bytes.len() {
                    return Err(err(
                        "SELU0102",
                        "unterminated block comment".into(),
                        start,
                        i,
                    ));
                }
                i += 2;
            }
            '"' | '\'' => {
                let quote = bytes[i];
                i += 1;
                let mut s = String::new();
                loop {
                    match bytes.get(i) {
                        None => {
                            return Err(err("SELU0103", "unterminated string".into(), start, i));
                        }
                        Some(b'\\') => {
                            let n = bytes.get(i + 1).copied().unwrap_or(b' ');
                            s.push(match n {
                                b'n' => '\n',
                                b't' => '\t',
                                b'r' => '\r',
                                other => other as char,
                            });
                            i += 2;
                        }
                        Some(&b) if b == quote => {
                            i += 1;
                            break;
                        }
                        Some(_) => {
                            // Consume one UTF-8 char.
                            let ch_len = src[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                            s.push_str(&src[i..i + ch_len]);
                            i += ch_len;
                        }
                    }
                }
                out.push(Token {
                    tok: Tok::Str(s),
                    span: span(start, i),
                });
            }
            '`' => {
                i += 1;
                let content_start = i;
                while i < bytes.len() && bytes[i] != b'`' {
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(err(
                        "SELU0103",
                        "unterminated template string".into(),
                        start,
                        i,
                    ));
                }
                let s = src[content_start..i].to_string();
                i += 1;
                out.push(Token {
                    tok: Tok::Template(s),
                    span: span(start, i),
                });
            }
            c if c.is_ascii_digit() => {
                while i < bytes.len()
                    && (bytes[i].is_ascii_digit()
                        || bytes[i] == b'.' && bytes.get(i + 1).is_some_and(|b| b.is_ascii_digit()))
                {
                    i += 1;
                }
                out.push(Token {
                    tok: Tok::Number(src[start..i].to_string()),
                    span: span(start, i),
                });
            }
            c if c.is_alphabetic() || c == '_' => {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let word = &src[start..i];
                let tok = match word {
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
                    _ => Tok::Ident(word.to_string()),
                };
                out.push(Token {
                    tok,
                    span: span(start, i),
                });
            }
            ';' => {
                i += 1;
                out.push(Token {
                    tok: Tok::Semi,
                    span: span(start, i),
                });
            }
            ',' => {
                i += 1;
                out.push(Token {
                    tok: Tok::Comma,
                    span: span(start, i),
                });
            }
            '(' => {
                i += 1;
                out.push(Token {
                    tok: Tok::LParen,
                    span: span(start, i),
                });
            }
            ')' => {
                i += 1;
                out.push(Token {
                    tok: Tok::RParen,
                    span: span(start, i),
                });
            }
            '{' => {
                i += 1;
                out.push(Token {
                    tok: Tok::LBrace,
                    span: span(start, i),
                });
            }
            '}' => {
                i += 1;
                out.push(Token {
                    tok: Tok::RBrace,
                    span: span(start, i),
                });
            }
            ':' => {
                i += 1;
                out.push(Token {
                    tok: Tok::Colon,
                    span: span(start, i),
                });
            }
            '.' => {
                i += 1;
                out.push(Token {
                    tok: Tok::Dot,
                    span: span(start, i),
                });
            }
            '+' => {
                i += 1;
                out.push(Token {
                    tok: Tok::Plus,
                    span: span(start, i),
                });
            }
            '-' => {
                i += 1;
                out.push(Token {
                    tok: Tok::Minus,
                    span: span(start, i),
                });
            }
            '*' => {
                i += 1;
                out.push(Token {
                    tok: Tok::Star,
                    span: span(start, i),
                });
            }
            '%' => {
                i += 1;
                out.push(Token {
                    tok: Tok::Percent,
                    span: span(start, i),
                });
            }
            '/' => {
                i += 1;
                out.push(Token {
                    tok: Tok::Slash,
                    span: span(start, i),
                });
            }
            '=' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    i += 2;
                    out.push(Token {
                        tok: Tok::EqEq,
                        span: span(start, i),
                    });
                } else {
                    i += 1;
                    out.push(Token {
                        tok: Tok::Eq,
                        span: span(start, i),
                    });
                }
            }
            '!' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    i += 2;
                    out.push(Token {
                        tok: Tok::NotEq,
                        span: span(start, i),
                    });
                } else {
                    i += 1;
                    out.push(Token {
                        tok: Tok::Bang,
                        span: span(start, i),
                    });
                }
            }
            '<' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    i += 2;
                    out.push(Token {
                        tok: Tok::LtEq,
                        span: span(start, i),
                    });
                } else {
                    i += 1;
                    out.push(Token {
                        tok: Tok::Lt,
                        span: span(start, i),
                    });
                }
            }
            '>' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    i += 2;
                    out.push(Token {
                        tok: Tok::GtEq,
                        span: span(start, i),
                    });
                } else {
                    i += 1;
                    out.push(Token {
                        tok: Tok::Gt,
                        span: span(start, i),
                    });
                }
            }
            '&' if bytes.get(i + 1) == Some(&b'&') => {
                i += 2;
                out.push(Token {
                    tok: Tok::AndAnd,
                    span: span(start, i),
                });
            }
            '|' if bytes.get(i + 1) == Some(&b'|') => {
                i += 2;
                out.push(Token {
                    tok: Tok::OrOr,
                    span: span(start, i),
                });
            }
            '?' => {
                if bytes.get(i + 1) == Some(&b'?') {
                    i += 2;
                    out.push(Token {
                        tok: Tok::QuestionQuestion,
                        span: span(start, i),
                    });
                } else {
                    i += 1;
                    out.push(Token {
                        tok: Tok::Question,
                        span: span(start, i),
                    });
                }
            }
            other => {
                return Err(err(
                    "SELU0104",
                    format!("unexpected character `{other}`"),
                    start,
                    start + 1,
                ));
            }
        }
    }
    out.push(Token {
        tok: Tok::Eof,
        span: span(i, i),
    });
    Ok(out)
}

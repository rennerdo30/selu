use crate::ast::*;
use crate::lexer::{Tok, Token, lex};
use selu_diagnostics::{Diagnostic, Span};

pub fn parse(file: &str, src: &str) -> Result<Program, Diagnostic> {
    let tokens = lex(file, src)?;
    let mut p = Parser {
        tokens,
        pos: 0,
        file: file.to_string(),
    };
    let mut stmts = Vec::new();
    while !p.at(&Tok::Eof) {
        stmts.push(p.stmt()?);
    }
    Ok(Program { stmts })
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    file: String,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].tok
    }
    fn peek_span(&self) -> Span {
        self.tokens[self.pos.min(self.tokens.len() - 1)]
            .span
            .clone()
    }
    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }
    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }
    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: &Tok, what: &str) -> Result<Token, Diagnostic> {
        if self.at(t) {
            Ok(self.bump())
        } else {
            Err(Diagnostic::error(
                "SELU0105",
                format!("expected {what}, found {}", self.peek().describe()),
            )
            .with_span(self.peek_span()))
        }
    }
    fn ident(&mut self, what: &str) -> Result<(String, Span), Diagnostic> {
        match self.peek().clone() {
            Tok::Ident(n) => {
                let s = self.peek_span();
                self.bump();
                Ok((n, s))
            }
            _ => Err(Diagnostic::error(
                "SELU0105",
                format!("expected {what}, found {}", self.peek().describe()),
            )
            .with_span(self.peek_span())),
        }
    }
    /// Semicolons are required (DESIGN.md section 9); a missing one is a
    /// diagnostic with a fix, never a guess.
    fn semi(&mut self, stmt_span: &Span) -> Result<(), Diagnostic> {
        if self.eat(&Tok::Semi) {
            Ok(())
        } else {
            Err(Diagnostic::error(
                "SELU0101",
                "missing `;` at the end of this statement; Selu requires a semicolon after every statement",
            )
            .with_span(Span::new(&self.file, stmt_span.end, stmt_span.end))
            .with_fix("add `;` here")
            .with_help("like C#, Java and Rust, Selu never guesses where a statement ends"))
        }
    }
    fn ty(&mut self) -> Result<Option<String>, Diagnostic> {
        if !self.eat(&Tok::Colon) {
            return Ok(None);
        }
        let (mut name, _) = self.ident("a type name")?;
        while self.eat(&Tok::Dot) {
            let (part, _) = self.ident("a type name")?;
            name.push('.');
            name.push_str(&part);
        }
        if self.eat(&Tok::Question) {
            name.push('?');
        }
        Ok(Some(name))
    }
    fn params(&mut self) -> Result<Vec<Param>, Diagnostic> {
        self.expect(&Tok::LParen, "`(`")?;
        let mut params = Vec::new();
        while !self.at(&Tok::RParen) {
            let (name, span) = self.ident("a parameter name")?;
            let ty = self.ty()?;
            params.push(Param { name, ty, span });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RParen, "`)`")?;
        Ok(params)
    }
    fn block(&mut self) -> Result<(Vec<Stmt>, Span), Diagnostic> {
        let open = self.expect(&Tok::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
            stmts.push(self.stmt()?);
        }
        let close = self.expect(&Tok::RBrace, "`}`")?;
        Ok((
            stmts,
            Span::new(&self.file, open.span.start, close.span.end),
        ))
    }
    /// A `if` branch: a `{ }` block, or one statement (`if (x) return;`).
    fn branch(&mut self) -> Result<Vec<Stmt>, Diagnostic> {
        if self.at(&Tok::LBrace) {
            Ok(self.block()?.0)
        } else {
            Ok(vec![self.stmt()?])
        }
    }

    fn stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.peek_span();
        let kind = match self.peek().clone() {
            Tok::Const | Tok::Let => {
                let is_const = self.at(&Tok::Const);
                self.bump();
                let (name, _) = self.ident("a variable name")?;
                let ty = self.ty()?;
                let init = if self.eat(&Tok::Eq) {
                    Some(Box::new(self.expr()?))
                } else {
                    None
                };
                let end = init
                    .as_ref()
                    .map(|e| e.span.clone())
                    .unwrap_or_else(|| start.clone());
                // Span so far ends where the (missing) ; belongs.
                let upto = Span::new(&self.file, start.start, end.end);
                self.semi(&upto)?;
                StmtKind::VarDecl {
                    is_const,
                    name,
                    ty,
                    init,
                }
            }
            Tok::Function => {
                self.bump();
                let (name, _) = self.ident("a function name")?;
                let params = self.params()?;
                let (body, _) = self.block()?;
                StmtKind::FuncDecl { name, params, body }
            }
            Tok::On => {
                self.bump();
                let (event, _) = self.ident("an event name like `PLAYER_LOGIN`")?;
                let params = self.params()?;
                let (body, _) = self.block()?;
                StmtKind::OnHandler {
                    event,
                    params,
                    body,
                }
            }
            Tok::Return => {
                self.bump();
                if self.at(&Tok::Semi) {
                    self.bump();
                    StmtKind::Return(None)
                } else if self.at(&Tok::RBrace) || self.at(&Tok::Eof) {
                    self.semi(&start)?;
                    unreachable!("semi() errors when the next token is not `;`")
                } else {
                    let e = self.expr()?;
                    let upto = Span::new(&self.file, start.start, e.span.end);
                    self.semi(&upto)?;
                    StmtKind::Return(Some(Box::new(e)))
                }
            }
            Tok::If => {
                self.bump();
                self.expect(&Tok::LParen, "`(` after `if`")?;
                let cond = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                let then_branch = self.branch()?;
                let else_branch = if self.eat(&Tok::Else) {
                    Some(self.branch()?)
                } else {
                    None
                };
                StmtKind::If {
                    cond: Box::new(cond),
                    then_branch,
                    else_branch,
                }
            }
            Tok::LBrace => StmtKind::Block(self.block()?.0),
            _ => {
                let e = self.expr()?;
                let upto = Span::new(&self.file, start.start, e.span.end);
                self.semi(&upto)?;
                StmtKind::Expr(Box::new(e))
            }
        };
        let end = self.tokens[self.pos.saturating_sub(1).min(self.tokens.len() - 1)]
            .span
            .end;
        Ok(Stmt {
            kind,
            span: Span::new(&self.file, start.start, end),
        })
    }

    fn expr(&mut self) -> Result<Expr, Diagnostic> {
        self.assign()
    }
    fn assign(&mut self) -> Result<Expr, Diagnostic> {
        let left = self.coalesce()?;
        if self.eat(&Tok::Eq) {
            let value = self.assign()?;
            let span = Span::new(&self.file, left.span.start, value.span.end);
            return Ok(Expr {
                kind: ExprKind::Assign {
                    target: Box::new(left),
                    value: Box::new(value),
                },
                span,
            });
        }
        Ok(left)
    }
    fn binary_level<F: Fn(&mut Self) -> Result<Expr, Diagnostic>>(
        &mut self,
        next: F,
        ops: &[(&Tok, &str)],
    ) -> Result<Expr, Diagnostic> {
        let mut left = next(self)?;
        loop {
            let op = ops.iter().find(|(t, _)| self.at(t)).map(|(_, s)| *s);
            let Some(op) = op else { return Ok(left) };
            self.bump();
            let right = next(self)?;
            let span = Span::new(&self.file, left.span.start, right.span.end);
            left = Expr {
                kind: ExprKind::Binary {
                    op: op.to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
    }
    fn coalesce(&mut self) -> Result<Expr, Diagnostic> {
        self.binary_level(Self::logic_or, &[(&Tok::QuestionQuestion, "??")])
    }
    fn logic_or(&mut self) -> Result<Expr, Diagnostic> {
        self.binary_level(Self::logic_and, &[(&Tok::OrOr, "||")])
    }
    fn logic_and(&mut self) -> Result<Expr, Diagnostic> {
        self.binary_level(Self::equality, &[(&Tok::AndAnd, "&&")])
    }
    fn equality(&mut self) -> Result<Expr, Diagnostic> {
        self.binary_level(Self::comparison, &[(&Tok::EqEq, "=="), (&Tok::NotEq, "!=")])
    }
    fn comparison(&mut self) -> Result<Expr, Diagnostic> {
        self.binary_level(
            Self::additive,
            &[
                (&Tok::Lt, "<"),
                (&Tok::LtEq, "<="),
                (&Tok::Gt, ">"),
                (&Tok::GtEq, ">="),
            ],
        )
    }
    fn additive(&mut self) -> Result<Expr, Diagnostic> {
        self.binary_level(
            Self::multiplicative,
            &[(&Tok::Plus, "+"), (&Tok::Minus, "-")],
        )
    }
    fn multiplicative(&mut self) -> Result<Expr, Diagnostic> {
        self.binary_level(
            Self::unary,
            &[(&Tok::Star, "*"), (&Tok::Slash, "/"), (&Tok::Percent, "%")],
        )
    }
    fn unary(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.peek_span();
        let op = if self.eat(&Tok::Bang) {
            Some("!")
        } else if self.eat(&Tok::Minus) {
            Some("-")
        } else {
            None
        };
        match op {
            Some(op) => {
                let e = self.unary()?;
                let span = Span::new(&self.file, start.start, e.span.end);
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: op.into(),
                        expr: Box::new(e),
                    },
                    span,
                })
            }
            None => self.postfix(),
        }
    }
    fn postfix(&mut self) -> Result<Expr, Diagnostic> {
        let mut e = self.primary()?;
        loop {
            if self.eat(&Tok::Dot) {
                let (name, name_span) = self.ident("a member name")?;
                let span = Span::new(&self.file, e.span.start, name_span.end);
                e = Expr {
                    kind: ExprKind::Member {
                        object: Box::new(e),
                        name,
                    },
                    span,
                };
            } else if self.at(&Tok::LParen) {
                self.bump();
                let mut args = Vec::new();
                while !self.at(&Tok::RParen) {
                    args.push(self.expr()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                let close = self.expect(&Tok::RParen, "`)`")?;
                let span = Span::new(&self.file, e.span.start, close.span.end);
                e = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(e),
                        args,
                    },
                    span,
                };
            } else {
                return Ok(e);
            }
        }
    }
    fn primary(&mut self) -> Result<Expr, Diagnostic> {
        let tok = self.bump();
        let span = tok.span.clone();
        let kind = match tok.tok {
            Tok::Number(n) => ExprKind::Number(n),
            Tok::Str(s) | Tok::Template(s) => ExprKind::Str(s),
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::Null => ExprKind::Null,
            Tok::Ident(n) => ExprKind::Ident(n),
            Tok::LParen => {
                let e = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                return Ok(e);
            }
            other => {
                return Err(Diagnostic::error(
                    "SELU0106",
                    format!("expected a value, found {}", other.describe()),
                )
                .with_span(span));
            }
        };
        Ok(Expr { kind, span })
    }
}

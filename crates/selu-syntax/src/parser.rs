use crate::ast::*;
use crate::lexer::{Tok, Token, lex};
use selu_diagnostics::{Diagnostic, Span};

pub fn parse(file: &str, src: &str) -> Result<Program, Diagnostic> {
    let tokens = lex(file, src)?;
    let mut p = Parser {
        tokens,
        source: src.to_string(),
        pos: 0,
        file: file.to_string(),
        next_id: 0,
    };
    let mut stmts = Vec::new();
    while !p.at(&Tok::Eof) {
        stmts.push(p.stmt()?);
    }
    Ok(Program { stmts })
}

struct Parser {
    source: String,
    tokens: Vec<Token>,
    pos: usize,
    file: String,
    next_id: ExprId,
}

type PResult<T> = Result<T, Diagnostic>;

/// Keywords that are still fine as a field name (`t.for`, `{ new: 1 }`).
fn keyword_text(t: &Tok) -> Option<&'static str> {
    Some(match t {
        Tok::Const => "const",
        Tok::Let => "let",
        Tok::Function => "function",
        Tok::Return => "return",
        Tok::If => "if",
        Tok::Else => "else",
        Tok::On => "on",
        Tok::True => "true",
        Tok::False => "false",
        Tok::Null => "null",
        Tok::While => "while",
        Tok::Do => "do",
        Tok::For => "for",
        Tok::Break => "break",
        Tok::Continue => "continue",
        Tok::New => "new",
        Tok::Class => "class",
        Tok::Extends => "extends",
        Tok::Import => "import",
        Tok::Export => "export",
        Tok::This => "this",
        Tok::Super => "super",
        _ => return None,
    })
}

impl Parser {
    fn next_id(&mut self) -> ExprId {
        self.next_id += 1;
        self.next_id
    }
    fn mk(&mut self, kind: ExprKind, span: Span) -> Expr {
        let string_offsets = if matches!(kind, ExprKind::Str(_)) {
            let raw = &self.source[span.start..span.end];
            let mut offsets = Vec::new();
            let mut i = 1;
            while i + 1 < raw.len() {
                let start = i;
                if raw.as_bytes()[i] == b'\\' {
                    i += 2;
                    offsets.push(span.start + start);
                } else {
                    let len = raw[i..].chars().next().unwrap().len_utf8();
                    offsets.extend((0..len).map(|j| span.start + start + j));
                    i += len;
                }
            }
            offsets.push(span.start + raw.len() - 1);
            Some(offsets)
        } else {
            None
        };
        Expr {
            string_offsets,
            id: self.next_id(),
            kind,
            span,
        }
    }
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].tok
    }
    fn peek_at(&self, n: usize) -> Option<&Tok> {
        self.tokens.get(self.pos + n).map(|t| &t.tok)
    }
    fn peek_span(&self) -> Span {
        self.tokens[self.pos.min(self.tokens.len() - 1)]
            .span
            .clone()
    }
    /// The end of the last consumed token.
    fn prev_end(&self) -> usize {
        self.tokens[self.pos.saturating_sub(1).min(self.tokens.len() - 1)]
            .span
            .end
    }
    fn span_from(&self, start: usize) -> Span {
        Span::new(&self.file, start, self.prev_end().max(start))
    }
    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }
    fn at_ident(&self, word: &str) -> bool {
        matches!(self.peek(), Tok::Ident(w) if w == word)
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
    fn unexpected(&self, what: &str) -> Diagnostic {
        Diagnostic::error(
            "SELU0105",
            format!("expected {what}, found {}", self.peek().describe()),
        )
        .with_span(self.peek_span())
    }
    fn expect(&mut self, t: &Tok, what: &str) -> PResult<Token> {
        if self.at(t) {
            Ok(self.bump())
        } else {
            Err(self.unexpected(what))
        }
    }
    fn ident(&mut self, what: &str) -> PResult<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(n) => {
                let s = self.peek_span();
                self.bump();
                Ok((n, s))
            }
            _ => Err(self.unexpected(what)),
        }
    }
    /// A field name: an identifier or a keyword.
    fn field_name(&mut self, what: &str) -> PResult<(String, Span)> {
        if let Some(k) = keyword_text(self.peek()) {
            let s = self.peek_span();
            self.bump();
            return Ok((k.to_string(), s));
        }
        self.ident(what)
    }
    /// Semicolons are required (DESIGN.md section 9); a missing one is a
    /// diagnostic with a fix, never a guess.
    fn semi(&mut self, stmt_start: usize) -> PResult<()> {
        if self.eat(&Tok::Semi) {
            Ok(())
        } else {
            let end = self.prev_end().max(stmt_start);
            Err(Diagnostic::error(
                "SELU0101",
                "missing `;` at the end of this statement; Selu requires a semicolon after every statement",
            )
            .with_span(Span::new(&self.file, end, end))
            .with_fix("add `;` here")
            .with_help("like C#, Java and Rust, Selu never guesses where a statement ends"))
        }
    }

    // ---- types ----------------------------------------------------------

    /// `: T` if present.
    fn annotation(&mut self) -> PResult<Option<TypeExpr>> {
        if self.eat(&Tok::Colon) {
            Ok(Some(self.ty()?))
        } else {
            Ok(None)
        }
    }

    fn ty(&mut self) -> PResult<TypeExpr> {
        let start = self.peek_span().start;
        let mut t = self.primary_ty()?;
        loop {
            let kind = if self.at(&Tok::LBracket) && self.peek_at(1) == Some(&Tok::RBracket) {
                self.bump();
                self.bump();
                TypeExprKind::Array(Box::new(t))
            } else if self.eat(&Tok::Question) {
                TypeExprKind::Optional(Box::new(t))
            } else {
                return Ok(t);
            };
            t = TypeExpr {
                kind,
                span: self.span_from(start),
            };
        }
    }

    fn primary_ty(&mut self) -> PResult<TypeExpr> {
        let start = self.peek_span().start;
        let kind = match self.peek().clone() {
            Tok::LParen => {
                // `(a: A, b: B) => R`
                self.bump();
                let mut params = Vec::new();
                let mut rest = false;
                while !self.at(&Tok::RParen) {
                    rest = self.eat(&Tok::DotDotDot);
                    self.ident("a parameter name")?;
                    self.expect(&Tok::Colon, "`:` and the parameter's type")?;
                    params.push(self.ty()?);
                    if rest {
                        break;
                    }
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RParen, "`)`")?;
                self.expect(&Tok::Arrow, "`=>` and the return type")?;
                let ret = self.ty()?;
                TypeExprKind::Function {
                    params,
                    ret: Box::new(ret),
                    rest,
                }
            }
            Tok::LBracket => {
                self.bump();
                let mut items = Vec::new();
                while !self.at(&Tok::RBracket) {
                    items.push(self.ty()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RBracket, "`]`")?;
                TypeExprKind::Tuple(items)
            }
            Tok::LBrace => {
                self.bump();
                let mut fields = Vec::new();
                while !self.at(&Tok::RBrace) {
                    let (name, name_span) = self.field_name("a field name")?;
                    let optional = self.eat(&Tok::Question);
                    self.expect(&Tok::Colon, "`:` and the field's type")?;
                    let ty = self.ty()?;
                    fields.push(FieldType {
                        name,
                        name_span,
                        optional,
                        ty,
                    });
                    if !self.eat(&Tok::Semi) && !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RBrace, "`}`")?;
                TypeExprKind::Object(fields)
            }
            Tok::Null => {
                self.bump();
                TypeExprKind::Named {
                    name: "null".into(),
                    args: Vec::new(),
                }
            }
            _ => {
                let (mut name, _) = self.ident("a type")?;
                while self.eat(&Tok::Dot) {
                    let (part, _) = self.ident("a type name")?;
                    name.push('.');
                    name.push_str(&part);
                }
                let mut args = Vec::new();
                if self.eat(&Tok::Lt) {
                    loop {
                        args.push(self.ty()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.type_gt()?;
                }
                TypeExprKind::Named { name, args }
            }
        };
        Ok(TypeExpr {
            kind,
            span: self.span_from(start),
        })
    }

    // ---- parameters and patterns ------------------------------------------

    fn params(&mut self) -> PResult<Vec<Param>> {
        self.expect(&Tok::LParen, "`(`")?;
        let mut params = Vec::new();
        while !self.at(&Tok::RParen) {
            let start = self.peek_span().start;
            let rest = self.eat(&Tok::DotDotDot);
            let (name, _) = self.ident("a parameter name")?;
            let ty = self.annotation()?;
            let default = if self.eat(&Tok::Eq) {
                Some(Box::new(self.expr_no_assign()?))
            } else {
                None
            };
            params.push(Param {
                name,
                ty,
                default,
                rest,
                span: self.span_from(start),
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RParen, "`)`")?;
        Ok(params)
    }

    fn pattern(&mut self) -> PResult<Pattern> {
        if self.eat(&Tok::LBracket) {
            let mut items = Vec::new();
            while !self.at(&Tok::RBracket) {
                if self.at(&Tok::Comma) {
                    items.push(None);
                } else {
                    items.push(Some(self.ident("a name")?));
                }
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RBracket, "`]`")?;
            Ok(Pattern::Array(items))
        } else {
            self.expect(&Tok::LBrace, "`[` or `{`")?;
            let mut items = Vec::new();
            while !self.at(&Tok::RBrace) {
                let (field, field_span) = self.field_name("a field name")?;
                let (name, span) = if self.eat(&Tok::Colon) {
                    self.ident("the name to bind")?
                } else {
                    (field.clone(), field_span.clone())
                };
                items.push((field, name, span, field_span));
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RBrace, "`}`")?;
            Ok(Pattern::Object(items))
        }
    }

    // ---- statements -----------------------------------------------------

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect(&Tok::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
            stmts.push(self.stmt()?);
        }
        self.expect(&Tok::RBrace, "`}`")?;
        Ok(stmts)
    }

    /// The body of `if`, a loop: a `{ }` block, or one statement.
    fn branch(&mut self) -> PResult<Vec<Stmt>> {
        if self.at(&Tok::LBrace) {
            self.block()
        } else {
            Ok(vec![self.stmt()?])
        }
    }

    fn cond(&mut self, after: &str) -> PResult<Box<Expr>> {
        self.expect(&Tok::LParen, &format!("`(` after `{after}`"))?;
        let cond = self.expr()?;
        self.expect(&Tok::RParen, "`)`")?;
        Ok(Box::new(cond))
    }

    /// `const x = 1;` / `let [a, b] = f();`, starting at `const`/`let`.
    fn declaration(&mut self, start: usize) -> PResult<StmtKind> {
        let is_const = self.at(&Tok::Const);
        self.bump();
        if self.at(&Tok::LBracket) || self.at(&Tok::LBrace) {
            let pattern = self.pattern()?;
            self.expect(&Tok::Eq, "`=`: destructuring needs a value")?;
            let init = self.expr()?;
            self.semi(start)?;
            return Ok(StmtKind::Destructure {
                is_const,
                pattern,
                init: Box::new(init),
            });
        }
        let (name, name_span) = self.ident("a variable name")?;
        let ty = self.annotation()?;
        let init = if self.eat(&Tok::Eq) {
            Some(Box::new(self.expr()?))
        } else {
            None
        };
        self.semi(start)?;
        Ok(StmtKind::VarDecl {
            is_const,
            name,
            name_span,
            ty,
            init,
        })
    }

    fn saved_version(&mut self) -> PResult<u32> {
        if let Tok::Number(n) = self.peek().clone()
            && let Ok(v) = n.parse::<u32>()
            && v > 0
        {
            self.bump();
            return Ok(v);
        }
        Err(
            Diagnostic::error("SELU0461", "a saved version must be a positive integer")
                .with_span(self.peek_span()),
        )
    }
    fn stmt(&mut self) -> PResult<Stmt> {
        let start = self.peek_span().start;
        let kind = match self.peek().clone() {
            Tok::Ident(ref w) if w == "throw" => {
                self.bump();
                let value = Box::new(self.expr()?);
                self.semi(start)?;
                StmtKind::Throw { value }
            }
            Tok::Ident(ref w) if w == "try" => {
                self.bump();
                let body = self.block()?;
                let catch = if self.at_ident("catch") {
                    self.bump();
                    let param = if self.eat(&Tok::LParen) {
                        let (name, span) = self.ident("an error name")?;
                        self.expect(&Tok::RParen, "`)`")?;
                        Some(Param {
                            name,
                            span,
                            ty: None,
                            default: None,
                            rest: false,
                        })
                    } else {
                        None
                    };
                    Some((param, self.block()?))
                } else {
                    None
                };
                let finally = if self.at_ident("finally") {
                    self.bump();
                    Some(self.block()?)
                } else {
                    None
                };
                if catch.is_none() && finally.is_none() {
                    return Err(self.unexpected("`catch` or `finally`"));
                }
                StmtKind::Try {
                    body,
                    catch,
                    finally,
                }
            }

            Tok::Ident(ref w)
                if (w == "type" || w == "interface")
                    && matches!(self.peek_at(1), Some(Tok::Ident(_))) =>
            {
                self.type_decl()?
            }
            Tok::Ident(ref w) if w == "saved" && matches!(self.peek_at(1), Some(Tok::Ident(_))) => {
                self.bump();
                let adopted = self.at_ident("global");
                if adopted {
                    self.bump();
                }
                let per_character = self.at_ident("perCharacter");
                if per_character {
                    self.bump();
                }
                let (name, name_span) = self.ident("a saved name")?;
                let version = if self.at_ident("version") {
                    self.bump();
                    self.saved_version()?
                } else {
                    1
                };
                let version_key = if self.at_ident("using") {
                    self.bump();
                    Some(self.ident("a persisted version key")?.0)
                } else {
                    None
                };
                let legacy = if self.at_ident("legacy") {
                    self.bump();
                    Some(self.ident("a legacy saved global")?.0)
                } else {
                    None
                };
                self.expect(&Tok::Eq, "`=`")?;
                let defaults = Box::new(self.expr()?);
                self.semi(start)?;
                StmtKind::Saved {
                    name,
                    name_span,
                    per_character,
                    adopted,
                    version_key,
                    legacy,
                    version,
                    defaults,
                }
            }
            Tok::Ident(ref w)
                if w == "migrate" && matches!(self.peek_at(1), Some(Tok::Ident(_))) =>
            {
                self.bump();
                let (name, name_span) = self.ident("a saved name")?;
                let step_start = self.peek_span().start;
                let from = self.saved_version()?;
                self.expect(&Tok::Minus, "`->`")?;
                self.expect(&Tok::Gt, "`->`")?;
                let to = self.saved_version()?;
                let step_span = self.span_from(step_start);
                self.expect(&Tok::LParen, "`(`")?;
                let (n, span) = self.ident("the old data parameter")?;
                let ty = self.annotation()?;
                let context = if self.at(&Tok::Comma) {
                    self.bump();
                    let (name, span) = self.ident("a migration context parameter")?;
                    let ty = self.annotation()?;
                    Some(Box::new(Param {
                        name,
                        span,
                        ty,
                        default: None,
                        rest: false,
                    }))
                } else {
                    None
                };
                self.expect(&Tok::RParen, "`)`")?;
                let param = Param {
                    name: n,
                    span,
                    ty,
                    default: None,
                    rest: false,
                };
                let body = self.block()?;
                StmtKind::Migrate {
                    name,
                    name_span,
                    step_span,
                    from,
                    to,
                    param,
                    context,
                    body,
                }
            }
            Tok::Const | Tok::Let => self.declaration(start)?,
            Tok::Function => {
                self.bump();
                let (name, name_span) = self.ident("a function name")?;
                let params = self.params()?;
                let ret = self.annotation()?;
                let body = self.block()?;
                StmtKind::FuncDecl {
                    name,
                    name_span,
                    params,
                    ret,
                    body,
                }
            }
            Tok::On => {
                self.bump();
                let (event, _) = self.ident("an event name like `PLAYER_LOGIN`")?;
                let params = self.params()?;
                let body = self.block()?;
                StmtKind::OnHandler {
                    event,
                    params,
                    body,
                }
            }
            Tok::Return => {
                self.bump();
                if self.eat(&Tok::Semi) {
                    StmtKind::Return(None)
                } else if self.at(&Tok::RBrace) || self.at(&Tok::Eof) {
                    self.semi(start)?;
                    unreachable!("semi() errors when the next token is not `;`")
                } else {
                    let e = self.expr()?;
                    self.semi(start)?;
                    StmtKind::Return(Some(Box::new(e)))
                }
            }
            Tok::If => {
                self.bump();
                let cond = self.cond("if")?;
                let then_branch = self.branch()?;
                let else_branch = if self.eat(&Tok::Else) {
                    Some(self.branch()?)
                } else {
                    None
                };
                StmtKind::If {
                    cond,
                    then_branch,
                    else_branch,
                }
            }
            Tok::While => {
                self.bump();
                let cond = self.cond("while")?;
                let body = self.branch()?;
                StmtKind::While { cond, body }
            }
            Tok::Do => {
                self.bump();
                let body = self.block()?;
                self.expect(&Tok::While, "`while (...)` after the `do { }` body")?;
                let cond = self.cond("while")?;
                self.semi(start)?;
                StmtKind::DoWhile { body, cond }
            }
            Tok::For => self.for_stmt(start)?,
            Tok::Break => {
                self.bump();
                self.semi(start)?;
                StmtKind::Break
            }
            Tok::Continue => {
                self.bump();
                self.semi(start)?;
                StmtKind::Continue
            }
            Tok::LBrace => StmtKind::Block(self.block()?),
            Tok::Import => self.import(start)?,
            Tok::Export => {
                self.bump();
                let inner = self.stmt()?;
                match inner.kind {
                    StmtKind::TypeDecl(_)
                    | StmtKind::Saved { .. }
                    | StmtKind::VarDecl { .. }
                    | StmtKind::FuncDecl { .. }
                    | StmtKind::Class(_)
                    | StmtKind::Global { .. } => StmtKind::Export(Box::new(inner)),
                    _ => {
                        return Err(Diagnostic::error(
                            "SELU0111",
                            "`export` can only come before `function`, `const`, `let`, `class` or `global`",
                        )
                        .with_span(inner.span));
                    }
                }
            }
            Tok::Class => StmtKind::Class(self.class()?),
            // `global` is a contextual keyword: only `global <name>` starts a
            // global declaration, so `global` stays usable as a plain name.
            Tok::Ident(ref w)
                if w == "global" && matches!(self.peek_at(1), Some(Tok::Ident(_))) =>
            {
                self.bump();
                let (name, name_span) = self.ident("a global name")?;
                self.expect(&Tok::Eq, "`=`: a global needs a starting value")?;
                let init = self.expr()?;
                self.semi(start)?;
                StmtKind::Global {
                    name,
                    name_span,
                    init: Box::new(init),
                }
            }
            _ => {
                let e = self.expr()?;
                self.semi(start)?;
                StmtKind::Expr(Box::new(e))
            }
        };
        Ok(Stmt {
            kind,
            span: self.span_from(start),
        })
    }

    /// `import { a, b as c } from "./x";` / `import name from "./x";`
    fn import(&mut self, start: usize) -> PResult<StmtKind> {
        self.bump();
        let mut names = Vec::new();
        let mut default = None;
        if self.eat(&Tok::LBrace) {
            while !self.at(&Tok::RBrace) {
                let (exported, exported_span) = self.field_name("a name to import")?;
                let (local, span) = if self.at_ident("as") {
                    self.bump();
                    self.ident("the local name after `as`")?
                } else {
                    (exported.clone(), exported_span.clone())
                };
                names.push((exported, local, span, exported_span));
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RBrace, "`}`")?;
        } else {
            default = Some(self.ident("`{` or a name to import")?);
        }
        if !self.at_ident("from") {
            return Err(self.unexpected("`from` and the file to import from"));
        }
        self.bump();
        let (from, from_span) = match self.peek().clone() {
            Tok::Str(s) => {
                let span = self.peek_span();
                self.bump();
                (s, span)
            }
            _ => return Err(self.unexpected("the file as a string, for example `\"./other\"`")),
        };
        self.semi(start)?;
        Ok(StmtKind::Import {
            names,
            default,
            from,
            from_span,
        })
    }

    /// `class Name extends Base { members }`
    fn class(&mut self) -> PResult<ClassDecl> {
        self.bump();
        let (name, name_span) = self.ident("a class name")?;
        let extends = if self.eat(&Tok::Extends) {
            Some(self.ident("the class to extend")?)
        } else {
            None
        };
        self.expect(&Tok::LBrace, "`{`")?;
        let mut members = Vec::new();
        while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
            let start = self.peek_span().start;
            // `static` is contextual: `static()` would be a method named static.
            let is_static = self.at_ident("static")
                && self.peek_at(1) != Some(&Tok::LParen)
                && self.peek_at(1) != Some(&Tok::Colon);
            if is_static {
                self.bump();
            }
            let kind = if self.at_ident("constructor") && self.peek_at(1) == Some(&Tok::LParen) {
                self.bump();
                let params = self.params()?;
                let body = self.block()?;
                ClassMemberKind::Constructor { params, body }
            } else {
                let (name, name_span) = self.field_name("a field or method name")?;
                if self.at(&Tok::LParen) {
                    let params = self.params()?;
                    let ret = self.annotation()?;
                    let body = self.block()?;
                    ClassMemberKind::Method {
                        name,
                        name_span,
                        params,
                        ret,
                        body,
                    }
                } else {
                    let ty = self.annotation()?;
                    let init = if self.eat(&Tok::Eq) {
                        Some(Box::new(self.expr()?))
                    } else {
                        None
                    };
                    self.semi(start)?;
                    ClassMemberKind::Field {
                        name,
                        name_span,
                        ty,
                        init,
                    }
                }
            };
            members.push(ClassMember {
                is_static,
                kind,
                span: self.span_from(start),
            });
        }
        self.expect(&Tok::RBrace, "`}`")?;
        Ok(ClassDecl {
            name,
            name_span,
            extends,
            members,
        })
    }

    fn for_stmt(&mut self, start: usize) -> PResult<StmtKind> {
        self.bump();
        self.expect(&Tok::LParen, "`(` after `for`")?;
        // `for (const x of xs)`
        if self.at(&Tok::Const) || self.at(&Tok::Let) {
            let save = self.pos;
            let is_const = self.at(&Tok::Const);
            self.bump();
            let binding = if self.at(&Tok::LBracket) || self.at(&Tok::LBrace) {
                Some(ForBinding::Pattern(self.pattern()?))
            } else if let Tok::Ident(_) = self.peek() {
                let (n, s) = self.ident("a name")?;
                Some(ForBinding::Name(n, s))
            } else {
                None
            };
            if let Some(binding) = binding
                && self.at_ident("of")
            {
                self.bump();
                let iter = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                let body = self.branch()?;
                return Ok(StmtKind::ForOf {
                    is_const,
                    binding,
                    iter: Box::new(iter),
                    body,
                });
            }
            if self.at_ident("in") {
                return Err(Diagnostic::error("SELU0110", "Selu has no `for...in`")
                    .with_span(self.peek_span())
                    .with_help("loop over values with `for (const x of xs)`, or over a map with `for (const [key, value] of map)`")
                    .with_fix("use `of`"));
            }
            self.pos = save;
        }
        let init = if self.eat(&Tok::Semi) {
            None
        } else {
            let init_start = self.peek_span().start;
            let kind = if self.at(&Tok::Const) || self.at(&Tok::Let) {
                self.declaration(init_start)?
            } else {
                let e = self.expr()?;
                self.semi(init_start)?;
                StmtKind::Expr(Box::new(e))
            };
            Some(Box::new(Stmt {
                kind,
                span: self.span_from(init_start),
            }))
        };
        let cond = if self.at(&Tok::Semi) {
            None
        } else {
            Some(Box::new(self.expr()?))
        };
        self.semi(start)?;
        let update = if self.at(&Tok::RParen) {
            None
        } else {
            Some(Box::new(self.expr()?))
        };
        self.expect(&Tok::RParen, "`)`")?;
        let body = self.branch()?;
        Ok(StmtKind::For {
            init,
            cond,
            update,
            body,
        })
    }

    // ---- expressions ----------------------------------------------------

    pub fn expr(&mut self) -> PResult<Expr> {
        self.assign()
    }

    /// An expression without a top-level assignment (default values).
    fn expr_no_assign(&mut self) -> PResult<Expr> {
        self.conditional()
    }

    fn assign(&mut self) -> PResult<Expr> {
        let left = self.conditional()?;
        let op = match self.peek() {
            Tok::Eq => None,
            Tok::PlusEq => Some("+"),
            Tok::MinusEq => Some("-"),
            Tok::StarEq => Some("*"),
            Tok::SlashEq => Some("/"),
            Tok::PercentEq => Some("%"),
            _ => return Ok(left),
        };
        self.bump();
        let value = self.assign()?;
        let span = Span::new(&self.file, left.span.start, value.span.end);
        let kind = match op {
            None => ExprKind::Assign {
                target: Box::new(left),
                value: Box::new(value),
            },
            Some(op) => ExprKind::CompoundAssign {
                op: op.into(),
                target: Box::new(left),
                value: Box::new(value),
            },
        };
        Ok(self.mk(kind, span))
    }

    fn conditional(&mut self) -> PResult<Expr> {
        let cond = self.coalesce()?;
        if !self.eat(&Tok::Question) {
            return Ok(cond);
        }
        let then = self.assign()?;
        self.expect(
            &Tok::Colon,
            "`:` and the value for when the condition is false",
        )?;
        let otherwise = self.assign()?;
        let span = Span::new(&self.file, cond.span.start, otherwise.span.end);
        Ok(self.mk(
            ExprKind::Conditional {
                cond: Box::new(cond),
                then: Box::new(then),
                otherwise: Box::new(otherwise),
            },
            span,
        ))
    }

    fn binary_level(
        &mut self,
        next: fn(&mut Self) -> PResult<Expr>,
        ops: &[(Tok, &str)],
    ) -> PResult<Expr> {
        let mut left = next(self)?;
        loop {
            let op = ops.iter().find(|(t, _)| self.at(t)).map(|(_, s)| *s);
            let Some(op) = op else { return Ok(left) };
            self.bump();
            let right = next(self)?;
            let span = Span::new(&self.file, left.span.start, right.span.end);
            left = self.mk(
                ExprKind::Binary {
                    op: op.to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
    }
    fn coalesce(&mut self) -> PResult<Expr> {
        self.binary_level(Self::logic_or, &[(Tok::QuestionQuestion, "??")])
    }
    fn logic_or(&mut self) -> PResult<Expr> {
        self.binary_level(Self::logic_and, &[(Tok::OrOr, "||")])
    }
    fn logic_and(&mut self) -> PResult<Expr> {
        self.binary_level(Self::equality, &[(Tok::AndAnd, "&&")])
    }
    fn equality(&mut self) -> PResult<Expr> {
        self.binary_level(Self::comparison, &[(Tok::EqEq, "=="), (Tok::NotEq, "!=")])
    }
    fn comparison(&mut self) -> PResult<Expr> {
        self.binary_level(
            Self::additive,
            &[
                (Tok::Lt, "<"),
                (Tok::LtEq, "<="),
                (Tok::Gt, ">"),
                (Tok::GtEq, ">="),
            ],
        )
    }
    fn additive(&mut self) -> PResult<Expr> {
        self.binary_level(Self::multiplicative, &[(Tok::Plus, "+"), (Tok::Minus, "-")])
    }
    fn multiplicative(&mut self) -> PResult<Expr> {
        self.binary_level(
            Self::unary,
            &[(Tok::Star, "*"), (Tok::Slash, "/"), (Tok::Percent, "%")],
        )
    }
    fn type_gt(&mut self) -> PResult<()> {
        if self.at(&Tok::GtEq) {
            let mut equal = self.bump();
            equal.tok = Tok::Eq;
            equal.span.start += 1;
            self.tokens.insert(self.pos, equal);
            Ok(())
        } else {
            self.expect(&Tok::Gt, "`>`")?;
            Ok(())
        }
    }
    fn type_decl(&mut self) -> PResult<StmtKind> {
        let interface = self.at_ident("interface");
        self.bump();
        let (name, name_span) = self.ident("a type name")?;
        let mut params = Vec::new();
        let mut param_spans = Vec::new();
        if self.eat(&Tok::Lt) {
            loop {
                let (n, span) = self.ident("a type parameter")?;
                param_spans.push(span);
                let bound = if self.eat(&Tok::Extends) {
                    Some(self.ty()?)
                } else {
                    None
                };
                params.push((n, bound));
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.type_gt()?;
        }
        let mut bases = Vec::new();
        if interface && self.eat(&Tok::Extends) {
            loop {
                bases.push(self.ty()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        let body = if interface {
            let start = self.expect(&Tok::LBrace, "`{`")?.span.start;
            let mut fields = Vec::new();
            while !self.at(&Tok::RBrace) {
                let (name, name_span) = self.field_name("a field or method")?;
                let optional = self.eat(&Tok::Question);
                let ty = if self.at(&Tok::LParen) {
                    let ps = self.params()?;
                    for (i, p) in ps.iter().enumerate() {
                        if p.default.is_some() || (p.rest && i + 1 != ps.len()) {
                            return Err(self.unexpected("a final rest parameter with no default"));
                        }
                    }
                    let rest = ps.last().is_some_and(|p| p.rest);
                    self.expect(&Tok::Colon, "`:`")?;
                    let ret = self.ty()?;
                    TypeExpr {
                        span: self.span_from(start),
                        kind: TypeExprKind::Function {
                            params: ps
                                .into_iter()
                                .map(|p| p.ty.ok_or_else(|| self.unexpected("a parameter type")))
                                .collect::<PResult<_>>()?,
                            ret: Box::new(ret),
                            rest,
                        },
                    }
                } else {
                    self.expect(&Tok::Colon, "`:`")?;
                    self.ty()?
                };
                fields.push(FieldType {
                    name,
                    name_span,
                    optional,
                    ty,
                });
                if !self.at(&Tok::RBrace) {
                    self.expect(&Tok::Semi, "`;`")?;
                } else {
                    self.eat(&Tok::Semi);
                }
            }
            self.expect(&Tok::RBrace, "`}`")?;
            self.eat(&Tok::Semi);
            TypeExpr {
                kind: TypeExprKind::Object(fields),
                span: self.span_from(start),
            }
        } else {
            self.expect(&Tok::Eq, "`=`")?;
            let t = self.ty()?;
            self.semi(name_span.start)?;
            t
        };
        Ok(StmtKind::TypeDecl(TypeDecl {
            name,
            name_span,
            params,
            param_spans,
            bases,
            body,
        }))
    }

    fn unary(&mut self) -> PResult<Expr> {
        let start = self.peek_span().start;
        let op = match self.peek() {
            Tok::Ident(w) if w == "typeof" => "typeof",
            Tok::Bang => "!",
            Tok::Minus => "-",
            Tok::PlusPlus => "++",
            Tok::MinusMinus => "--",
            _ => return self.postfix(),
        };
        self.bump();
        let e = self.unary()?;
        let span = Span::new(&self.file, start, e.span.end);
        let kind = if op == "++" || op == "--" {
            ExprKind::Update {
                op: op.into(),
                target: Box::new(e),
            }
        } else {
            ExprKind::Unary {
                op: op.into(),
                expr: Box::new(e),
            }
        };
        Ok(self.mk(kind, span))
    }

    fn args(&mut self) -> PResult<Vec<Expr>> {
        self.expect(&Tok::LParen, "`(`")?;
        let mut args = Vec::new();
        while !self.at(&Tok::RParen) {
            args.push(self.expr()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RParen, "`)`")?;
        Ok(args)
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        let start = e.span.start;
        loop {
            let kind = match self.peek().clone() {
                Tok::Dot | Tok::QuestionDot => {
                    let optional = self.at(&Tok::QuestionDot);
                    self.bump();
                    let (name, _) = self.field_name("a field or method name")?;
                    let object = Box::new(e);
                    if optional {
                        ExprKind::OptionalMember { object, name }
                    } else {
                        ExprKind::Member { object, name }
                    }
                }
                Tok::LBracket => {
                    self.bump();
                    let index = self.expr()?;
                    self.expect(&Tok::RBracket, "`]`")?;
                    ExprKind::Index {
                        object: Box::new(e),
                        index: Box::new(index),
                    }
                }
                Tok::LParen => {
                    let args = self.args()?;
                    ExprKind::Call {
                        callee: Box::new(e),
                        args,
                    }
                }
                Tok::Template(s) => {
                    if e.dotted_name().as_deref() != Some("luaPattern") {
                        self.validate_templates(self.pos, self.pos + 1)?;
                    }
                    let raw_quasis = self.raw_templates(self.pos, self.pos + 1);
                    self.bump();
                    ExprKind::TaggedTemplate {
                        raw_quasis,
                        tag: Box::new(e),
                        quasis: vec![s],
                        exprs: Vec::new(),
                    }
                }
                Tok::TemplateHead(s) => {
                    let raw_start = self.pos;
                    self.bump();
                    let (quasis, exprs) = self.template_rest(s)?;
                    if e.dotted_name().as_deref() != Some("luaPattern") {
                        self.validate_templates(raw_start, self.pos)?;
                    }
                    ExprKind::TaggedTemplate {
                        raw_quasis: self.raw_templates(raw_start, self.pos),
                        tag: Box::new(e),
                        quasis,
                        exprs,
                    }
                }
                Tok::PlusPlus | Tok::MinusMinus => {
                    let op = if self.at(&Tok::PlusPlus) { "++" } else { "--" };
                    self.bump();
                    ExprKind::Update {
                        op: op.into(),
                        target: Box::new(e),
                    }
                }
                _ => return Ok(e),
            };
            let span = self.span_from(start);
            e = self.mk(kind, span);
        }
    }

    fn raw_templates(&self, start: usize, end: usize) -> Vec<(String, Span)> {
        let mut depth = 0usize;
        self.tokens[start..end]
            .iter()
            .filter_map(|t| {
                let outer = depth == 0
                    || depth == 1 && matches!(t.tok, Tok::TemplateMiddle(_) | Tok::TemplateTail(_));
                match t.tok {
                    Tok::TemplateHead(_) => depth += 1,
                    Tok::TemplateTail(_) => depth -= 1,
                    _ => {}
                }
                if !outer {
                    return None;
                }
                let trim = match t.tok {
                    Tok::Template(_) | Tok::TemplateTail(_) => 1,
                    Tok::TemplateHead(_) | Tok::TemplateMiddle(_) => 2,
                    _ => return None,
                };
                let sp = Span::new(&self.file, t.span.start + 1, t.span.end - trim);
                Some((self.source[sp.start..sp.end].to_string(), sp))
            })
            .collect()
    }
    fn validate_templates(&self, start: usize, end: usize) -> PResult<()> {
        let spans = self.raw_templates(start, end);
        for t in &self.tokens[start..end] {
            if spans.iter().any(|(_, sp)| sp.start == t.span.start + 1)
                && let Some(e) = &t.template_error
            {
                return Err(e.clone());
            }
        }
        Ok(())
    }

    /// After a `TemplateHead`: the expressions and text parts up to the tail.
    fn template_rest(&mut self, head: String) -> PResult<(Vec<String>, Vec<Expr>)> {
        let mut quasis = vec![head];
        let mut exprs = Vec::new();
        loop {
            exprs.push(self.expr()?);
            match self.peek().clone() {
                Tok::TemplateMiddle(s) => {
                    self.bump();
                    quasis.push(s);
                }
                Tok::TemplateTail(s) => {
                    self.bump();
                    quasis.push(s);
                    return Ok((quasis, exprs));
                }
                _ => return Err(self.unexpected("`}` to close the `${`")),
            }
        }
    }

    /// `(params) => body` / `(params): T => body`, if that is what follows;
    /// otherwise restores the position and returns `None`.
    fn try_arrow(&mut self) -> PResult<Option<Expr>> {
        let save = (self.pos, self.next_id);
        let start = self.peek_span().start;
        let head = (|| -> PResult<(Vec<Param>, Option<TypeExpr>)> {
            let params = self.params()?;
            let ret = self.annotation()?;
            self.expect(&Tok::Arrow, "`=>`")?;
            Ok((params, ret))
        })();
        let Ok((params, ret)) = head else {
            (self.pos, self.next_id) = save;
            return Ok(None);
        };
        self.arrow_body(start, params, ret).map(Some)
    }

    fn arrow_body(
        &mut self,
        start: usize,
        params: Vec<Param>,
        ret: Option<TypeExpr>,
    ) -> PResult<Expr> {
        let body = if self.at(&Tok::LBrace) {
            FnBody::Block(self.block()?)
        } else {
            FnBody::Expr(Box::new(self.assign()?))
        };
        let span = self.span_from(start);
        Ok(self.mk(
            ExprKind::Function {
                params,
                ret,
                body,
                is_arrow: true,
            },
            span,
        ))
    }

    fn primary(&mut self) -> PResult<Expr> {
        let start_span = self.peek_span();
        let start = start_span.start;
        let kind = match self.peek().clone() {
            Tok::LParen => {
                if let Some(arrow) = self.try_arrow()? {
                    return Ok(arrow);
                }
                self.bump();
                let e = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                return Ok(e);
            }
            // `x => x + 1`
            Tok::Ident(name) if self.peek_at(1) == Some(&Tok::Arrow) => {
                self.bump();
                self.bump();
                let param = Param {
                    name,
                    ty: None,
                    default: None,
                    rest: false,
                    span: start_span,
                };
                return self.arrow_body(start, vec![param], None);
            }
            Tok::Function => {
                self.bump();
                let params = self.params()?;
                let ret = self.annotation()?;
                let body = self.block()?;
                ExprKind::Function {
                    params,
                    ret,
                    body: FnBody::Block(body),
                    is_arrow: false,
                }
            }
            Tok::LBracket => {
                self.bump();
                let mut items = Vec::new();
                while !self.at(&Tok::RBracket) {
                    items.push(self.expr()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RBracket, "`]`")?;
                ExprKind::Array(items)
            }
            Tok::LBrace => {
                self.bump();
                let mut fields = Vec::new();
                while !self.at(&Tok::RBrace) {
                    let (key, key_span) = match self.peek().clone() {
                        Tok::Str(s) => {
                            let span = self.peek_span();
                            self.bump();
                            (s, span)
                        }
                        _ => self.field_name("a field name")?,
                    };
                    let value = if self.eat(&Tok::Colon) {
                        self.expr()?
                    } else {
                        // `{ x }` is `{ x: x }`.
                        self.mk(ExprKind::Ident(key.clone()), key_span.clone())
                    };
                    fields.push(ObjectField {
                        key,
                        key_span,
                        value,
                    });
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RBrace, "`}`")?;
                ExprKind::Object(fields)
            }
            Tok::New => {
                let start = self.peek_span().start;
                let line = self.source[..start].bytes().filter(|b| *b == b'\n').count() + 1;
                self.bump();
                let (class, _) = self.ident("a class name after `new`")?;
                let mut type_args = Vec::new();
                if self.eat(&Tok::Lt) {
                    loop {
                        type_args.push(self.ty()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.type_gt()?;
                }
                let args = self.args()?;
                ExprKind::New {
                    class,
                    type_args,
                    args,
                    line,
                }
            }
            Tok::TemplateHead(s) => {
                let raw_start = self.pos;
                self.bump();
                let (quasis, exprs) = self.template_rest(s)?;
                self.validate_templates(raw_start, self.pos)?;
                ExprKind::Template { quasis, exprs }
            }
            _ => {
                let tok = self.bump();
                if let Some(e) = tok.template_error {
                    return Err(e);
                }
                match tok.tok {
                    Tok::Number(n) => ExprKind::Number(n),
                    Tok::Str(s) | Tok::Template(s) => ExprKind::Str(s),
                    Tok::True => ExprKind::Bool(true),
                    Tok::False => ExprKind::Bool(false),
                    Tok::Null => ExprKind::Null,
                    Tok::This => ExprKind::This,
                    Tok::Super => ExprKind::Super,
                    Tok::Ident(n) => ExprKind::Ident(n),
                    other => {
                        return Err(Diagnostic::error(
                            "SELU0106",
                            format!("expected a value, found {}", other.describe()),
                        )
                        .with_span(tok.span));
                    }
                }
            }
        };
        let span = self.span_from(start);
        Ok(self.mk(kind, span))
    }
}

#[cfg(test)]
mod function_type_tests {
    use super::*;

    #[test]
    fn preserves_written_rest_function_type() {
        let src = "const f: (...xs: number[]) => void = (...xs: number[]) => {};";
        let p = parse("t.selu", src).unwrap();
        let StmtKind::VarDecl { ty: Some(ty), .. } = &p.stmts[0].kind else {
            panic!("expected declaration")
        };
        assert!(
            matches!(&ty.kind, TypeExprKind::Function { rest: true, params, .. } if params.len() == 1)
        );
        assert!(
            parse(
                "t.selu",
                "const f: (...xs: number[], n: number) => void = g;"
            )
            .is_err()
        );
    }
}

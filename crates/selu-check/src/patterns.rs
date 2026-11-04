//! Compile-time Lua 5.1 patterns. No pattern object exists at runtime.
use crate::{Ctx, Type, err};
use selu_diagnostics::Span;
use selu_syntax::{Expr, ExprKind};

pub fn text(e: &Expr) -> Option<(&str, &Span)> {
    match &e.kind {
        ExprKind::TaggedTemplate {
            tag,
            raw_quasis,
            exprs,
            ..
        } if tag.dotted_name().as_deref() == Some("luaPattern") && exprs.is_empty() => {
            raw_quasis.first().map(|(s, sp)| (s.as_str(), sp))
        }
        _ => None,
    }
}

// Each entry records whether a capture is a position capture.
pub fn captures(s: &str) -> Result<Vec<bool>, (usize, &'static str)> {
    let b = s.as_bytes();
    let mut caps = Vec::new();
    let mut stack = Vec::new();
    let mut closed = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => return Err((i, "use Lua classes such as %d instead of \\d")),
            b'|' => {
                return Err((
                    i,
                    "use %| for a literal pipe; Lua patterns have no alternation",
                ));
            }
            b'{' => {
                return Err((
                    i,
                    "use %{ for a literal brace; Lua patterns have no counted repetition",
                ));
            }
            b'(' => {
                if b.get(i + 1) == Some(&b'?') {
                    return Err((
                        i + 1,
                        "use (...) for a Lua capture; non-capturing groups are unavailable",
                    ));
                }
                if caps.len() == 32 {
                    return Err((i, "split the pattern; Lua permits at most 32 captures"));
                }
                let pos = b.get(i + 1) == Some(&b')');
                caps.push(pos);
                closed.push(pos);
                if pos {
                    i += 1;
                } else {
                    stack.push((caps.len() - 1, i));
                }
            }
            b')' => {
                let Some((n, _)) = stack.pop() else {
                    return Err((i, "escape a literal closing parenthesis as %)"));
                };
                closed[n] = true;
            }
            b'[' => {
                i = set(b, i)?;
            }
            b'%' => {
                let Some(&c) = b.get(i + 1) else {
                    return Err((i, "write %% for a literal percent"));
                };
                match c {
                    b'0' => return Err((i + 1, "back-references range from %1 to %9")),
                    b'1'..=b'9' => {
                        let n = (c - b'1') as usize;
                        if !closed.get(n).copied().unwrap_or(false) || caps[n] {
                            return Err((i + 1, "reference a previously closed string capture"));
                        }
                    }
                    b'b' => {
                        if i + 3 >= b.len() {
                            return Err((i, "write %b followed by two delimiter characters"));
                        }
                        i += 2;
                    }
                    b'f' => {
                        return Err((
                            i,
                            "%f is not enabled across all supported clients; use an explicit boundary capture",
                        ));
                    }
                    b'g' | b'G' => return Err((i + 1, "%g is unavailable in Lua 5.1; use [!-~]")),
                    c if c.is_ascii_alphabetic() && !b"aAcCdDlLpPsSuUwWxXzZ".contains(&c) => {
                        return Err((i + 1, "use a Lua 5.1 character class or escape punctuation"));
                    }
                    _ => {}
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    if let Some((_, i)) = stack.first() {
        return Err((*i, "close this capture with )"));
    }
    Ok(caps)
}
fn set(b: &[u8], start: usize) -> Result<usize, (usize, &'static str)> {
    let mut i = start + 1;
    if b.get(i) == Some(&b'^') {
        i += 1;
    }
    if b.get(i) == Some(&b']') {
        i += 1;
    }
    while i < b.len() {
        if b[i] == b']' {
            return Ok(i);
        }
        if b[i] == b'%' {
            i += 1;
            if i == b.len() {
                return Err((i - 1, "escape a literal percent as %% and close the set"));
            }
            if b[i] == b'g' || b[i] == b'G' {
                return Err((i, "%g is unavailable in Lua 5.1; use [!-~]"));
            }
        }
        i += 1;
    }
    Err((start, "close this character set with ]"))
}
fn function(params: Vec<Type>, ret: Type, multiple_results: bool) -> Type {
    Type::Function {
        required: params.len(),
        params,
        ret: Box::new(ret),
        multiple_results,
        unspecified_callback: false,
        api: Default::default(),
        api_semantics: Default::default(),
        rest: None,
    }
}
fn capture_types(s: &str, optional: bool) -> Vec<Type> {
    let mut ts: Vec<_> = captures(s)
        .unwrap_or_default()
        .into_iter()
        .map(|p| if p { Type::Number } else { Type::String })
        .collect();
    if ts.is_empty() {
        ts.push(Type::String);
    }
    if optional {
        ts = ts.into_iter().map(Type::optional).collect();
    }
    ts
}
impl Ctx<'_> {
    pub(crate) fn pattern_tag(&mut self, e: &Expr) -> Type {
        if let Some((s, sp)) = text(e) {
            if let Err((i, help)) = captures(s) {
                let len = s[i..].chars().next().map_or(1, char::len_utf8);
                self.out.push(
                    err(
                        "SELU0560",
                        "Invalid Lua 5.1 pattern.",
                        &Span::new(&sp.file, sp.start + i, sp.start + i + len),
                    )
                    .with_help(help),
                );
            }
        } else {
            self.out.push(
                err(
                    "SELU0561",
                    "A Lua pattern cannot contain interpolation.",
                    &e.span,
                )
                .with_help("write a static luaPattern template"),
            );
        }
        // Pattern tags are consumed syntactically by pattern methods.
        Type::ApiOpaque("luaPattern".into(), vec![])
    }
    pub(crate) fn pattern_call(
        &mut self,
        e: &Expr,
        callee: &Expr,
        method: &str,
        args: &[Expr],
    ) -> Option<Type> {
        if !matches!(
            method,
            "match" | "search" | "matchAll" | "replace" | "replaceAll"
        ) {
            return None;
        }
        let replace = matches!(method, "replace" | "replaceAll");
        if args.len() != if replace { 2 } else { 1 } {
            self.out.push(
                err(
                    "SELU0562",
                    "This pattern method has the wrong number of arguments.",
                    &e.span,
                )
                .with_help("pass a luaPattern and, for replacements, a string or callback"),
            );
            return Some(Type::Error);
        }
        self.expr(&args[0]);
        let Some((s, _)) = text(&args[0]) else {
            self.out.push(
                err(
                    "SELU0562",
                    "This method needs a luaPattern template.",
                    &args[0].span,
                )
                .with_help("write luaPattern`...` instead of a plain string"),
            );
            return Some(Type::Error);
        };
        if self.lookup("luaPattern").is_some() {
            return Some(Type::Error);
        }
        if method == "matchAll" && s.starts_with('^') {
            self.out.push(
                err(
                    "SELU0563",
                    "matchAll cannot use a leading anchor.",
                    text(&args[0]).unwrap().1,
                )
                .with_help("remove ^ or use match"),
            );
        }
        let ts = capture_types(s, method == "match");
        let ret = match method {
            "search" => Type::optional(Type::Number),
            "replace" | "replaceAll" => {
                let expected =
                    function(capture_types(s, false), Type::optional(Type::String), false);
                let got = self.value_expected(&args[1], Some(&expected));
                if !got.assignable_to(&Type::String) && !got.assignable_to(&expected) {
                    self.out.push(err("SELU0564", "A replacement needs a string or a capture callback returning string or null.", &args[1].span).with_help("return null to keep the original match"));
                }
                Type::Tuple(vec![Type::String, Type::Number])
            }
            "matchAll" => Type::ApiOpaque("luaPatternIterator".into(), ts),
            _ if captures(s).unwrap_or_default().is_empty() => ts[0].clone(),
            _ => Type::Tuple(ts),
        };
        self.types.set(
            callee.id,
            function(vec![], ret.clone(), replace || method == "match"),
        );
        Some(ret)
    }
    pub(crate) fn pattern_for_of(&mut self, s: &selu_syntax::Stmt, t: &Type) -> bool {
        use selu_syntax::{ForBinding, Pattern, StmtKind};
        let StmtKind::ForOf {
            is_const,
            binding,
            iter,
            body,
        } = &s.kind
        else {
            return false;
        };
        let Type::ApiOpaque(n, ts) = t else {
            return false;
        };
        if n != "luaPatternIterator" {
            return false;
        }
        let names = match binding {
            ForBinding::Name(n, sp) => vec![Some((n.clone(), sp.clone()))],
            ForBinding::Pattern(Pattern::Array(v)) => v.clone(),
            _ => {
                self.out.push(
                    err(
                        "SELU0565",
                        "Use a name or capture destructuring for matchAll.",
                        &iter.span,
                    )
                    .with_help("write for (const [a, b] of s.matchAll(luaPattern`...`))"),
                );
                return true;
            }
        };
        self.scopes.push(crate::Scope::default());
        for (i, v) in names.iter().enumerate() {
            if let Some((n, sp)) = v {
                let t = ts.get(i).cloned().unwrap_or_else(|| {
                    self.out.push(
                        err(
                            "SELU0565",
                            "The pattern has fewer captures than this binding.",
                            sp,
                        )
                        .with_help("remove the extra binding"),
                    );
                    Type::Error
                });
                self.declare(
                    n,
                    sp,
                    if *is_const {
                        crate::Kind::Const
                    } else {
                        crate::Kind::Let
                    },
                    t.clone(),
                );
                self.finish_decl(n, t);
            }
        }
        self.loops.push(crate::loops::LoopCtx);
        self.block(body, &[], vec![]);
        self.loops.pop();
        self.scopes.pop();
        true
    }
}

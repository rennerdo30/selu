//! Hand-written lexer and recursive-descent parser for the Selu subset in
//! DESIGN.md section 9. Good errors matter more than speed here.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod visit;

pub use ast::*;
pub use parser::parse;

/// Lua 5.1 reserved words. Selu source may spell them as names, but the
/// generated Lua can't use them as identifiers.
pub const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

pub fn is_lua_keyword(name: &str) -> bool {
    LUA_KEYWORDS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_decl_call_and_handler() {
        let src = r#"
const limit: number = 10;
function greet(name: string) { return "hi"; }
on PLAYER_LOGIN() {
  print(greet("x"));
  frame.SetPoint("TOP", UIParent, "TOP", 0, -40);
}
"#;
        let p = parse("t.selu", src).unwrap();
        assert_eq!(p.stmts.len(), 3);
        match &p.stmts[2].kind {
            StmtKind::OnHandler { event, body, .. } => {
                assert_eq!(event, "PLAYER_LOGIN");
                assert_eq!(body.len(), 2);
            }
            other => panic!("expected handler, got {other:?}"),
        }
    }

    #[test]
    fn missing_semicolon_is_a_diagnostic_with_fix() {
        let e = parse("t.selu", "const x = 1\nconst y = 2;\n").unwrap_err();
        assert_eq!(e.code, "SELU0101");
        assert_eq!(e.fix.as_deref(), Some("add `;` here"));
        let rendered = e.render("const x = 1\nconst y = 2;\n");
        assert!(rendered.contains("SELU0101"), "{rendered}");
    }

    #[test]
    fn method_call_shape_and_precedence() {
        let p = parse("t.selu", "const x = 1 + 2 * 3;\nlet y = a.B(x, null);\n").unwrap();
        // 1 + (2*3): top op is +
        match &p.stmts[0].kind {
            StmtKind::VarDecl {
                init: Some(init), ..
            } => match &init.kind {
                ExprKind::Binary { op, .. } => assert_eq!(op, "+"),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        match &p.stmts[1].kind {
            StmtKind::VarDecl {
                init: Some(init), ..
            } => match &init.kind {
                ExprKind::Call { callee, args } => {
                    assert_eq!(callee.dotted_name().as_deref(), Some("a.B"));
                    assert_eq!(args.len(), 2);
                    assert!(matches!(args[1].kind, ExprKind::Null));
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn if_else_and_bare_return_and_template() {
        let src = "function f(x) { if (x > 0) { return `big`; } else if (x == 0) { return; } else { return \"small\"; } }\n";
        // `else if` is an else branch holding one if statement.
        let p = parse("t.selu", src).unwrap();
        assert_eq!(p.stmts.len(), 1);
        let err = parse("t.selu", "function f() { return 1 }\n").unwrap_err();
        assert_eq!(err.code, "SELU0101");
    }

    #[test]
    fn unterminated_string_is_an_error() {
        let e = parse("t.selu", "const s = \"oops;\n").unwrap_err();
        assert_eq!(e.code, "SELU0103");
    }

    #[test]
    fn non_ascii_name_is_an_error_not_a_hang() {
        // This used to loop forever: the identifier arm accepted the first
        // byte of a multi-byte character but never advanced past it.
        let e = parse("t.selu", "const größe = 1;\n").unwrap_err();
        assert_eq!(e.code, "SELU0107");
        let rendered = e.render("const größe = 1;\n");
        assert!(rendered.contains('ö'), "{rendered}");
        // Any character, any position: still an error, never a panic.
        assert_eq!(parse("t.selu", "§").unwrap_err().code, "SELU0104");
    }

    #[test]
    fn strings_keep_unicode_and_reject_unknown_escapes() {
        let p = parse("t.selu", r#"const s = "größe \"q\" \\ \0";"#).unwrap();
        match &p.stmts[0].kind {
            StmtKind::VarDecl { init: Some(e), .. } => {
                assert_eq!(e.kind, ExprKind::Str("größe \"q\" \\ \0".into()));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            parse("t.selu", r#"const s = "\q";"#).unwrap_err().code,
            "SELU0108"
        );
        assert_eq!(
            parse("t.selu", r#"const s = "\ö";"#).unwrap_err().code,
            "SELU0108"
        );
    }
}

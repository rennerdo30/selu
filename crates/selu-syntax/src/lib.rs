//! Hand-written lexer and recursive-descent parser for the Selu subset in
//! DESIGN.md section 9. Good errors matter more than speed here.

pub mod ast;
pub mod lexer;
pub mod parser;

pub use ast::*;
pub use parser::parse;

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
}

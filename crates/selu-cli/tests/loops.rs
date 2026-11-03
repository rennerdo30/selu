mod common;
use common::{codes, compile, run};

#[test]
fn loops_and_updates() {
    assert_eq!(
        run(
            "on PLAYER_LOGIN() { let x = 0; while (x < 3) { x++; if (x == 2) { continue; } print(x); } do { x--; if (x == 1) { break; } } while (x > 0); print(x); let s = \"a\"; s += \"b\"; print(s); x += 9; x -= 2; x *= 2; x /= 4; x %= 3; print(x); }"
        ),
        ["1", "3", "1", "ab", "1"]
    );
}
#[test]
fn do_scope_and_repeated_condition() {
    assert_eq!(
        run(
            "let n = 0; function next(): number? { n++; if (n < 3) { return n; } return null; } on PLAYER_LOGIN() { while ((next() ?? 0) > 0) { print(n); } let x = false; do { let x = true; print(x); } while (x); print(n); }"
        ),
        ["1", "2", "true", "3"]
    );
}
#[test]
fn counted_forms() {
    let src = "on PLAYER_LOGIN() { for (let i = 0; i < 3; i++) { if (i == 1) { continue; } print(i); } let bound = 5; for (let j = 0; j < bound; j++) { print(j); bound = 2; } for (let k = 3; k >= 1; k--) { print(k); } }";
    assert_eq!(run(src), ["0", "2", "0", "1", "3", "2", "1"]);
    assert!(compile(src).contains("for i ="));
}
#[test]
fn ternary_branches() {
    assert_eq!(
        run(
            "on PLAYER_LOGIN() { print(true ? 0 : 9); print(true ? false : null); print(false ? false : true); }"
        ),
        ["0", "false", "true"]
    );
    assert!(codes("let x = true ? false : 1;").contains(&"SELU0342".into()));
}
#[test]
fn loop_errors() {
    assert!(codes("break;").contains(&"SELU0340".into()));
    assert!(codes("continue;").contains(&"SELU0340".into()));
    assert!(codes("while (true) { function bad() { break; } }").contains(&"SELU0340".into()));
    assert!(codes("while (1) {}").contains(&"SELU0328".into()));
    assert!(codes("let x = false; x++;").contains(&"SELU0341".into()));
    assert!(codes("const x = 1; x++;").contains(&"SELU0311".into()));
}
#[test]
fn closures_rejected_until_exact() {
    assert!(
        codes("for (let i = 0; i < 3; i++) { function capture() { print(i); } }")
            .contains(&"SELU0343".into())
    );
}

#[test]
fn receiver_once_and_old_value_before_rhs() {
    assert_eq!(
        run(
            "let calls = 0; let frame = CreateFrame(\"Frame\"); function obj() { calls++; return frame; } function rhs(): number { frame.value = 100; return 2; } on PLAYER_LOGIN() { frame.value = 3; obj().value += rhs(); print(frame.value); print(calls); }"
        ),
        ["5", "1"]
    );
}
#[test]
fn continue_break_all_loop_forms() {
    assert_eq!(
        run(
            "on PLAYER_LOGIN() { let i = 0; do { i++; if (i == 1) { continue; } if (i == 3) { break; } print(i); } while (i < 5); let limit = 6; for (let j = 0; j < limit; j++) { if (j == 0) { continue; } if (j == 3) { break; } print(j); } for (let k = 0; k <= 4; k += 2) { if (k == 0) { continue; } if (k == 4) { break; } print(k); } let n = 0; while (n < 3) { n++; if (n == 1) { continue; } while (true) { break; } if (n == 3) { break; } print(n); } }"
        ),
        ["2", "1", "2", "2", "2"]
    );
}
#[test]
fn fallback_float_bound_and_body_write() {
    assert_eq!(
        run(
            "on PLAYER_LOGIN() { for (let i = 0; i < 2.5; i++) { print(i); } for (let j = 0; j < 4; j++) { print(j); j++; } }"
        ),
        ["0", "1", "2", "0", "2"]
    );
}
#[test]
fn lazy_ternary_and_optional_narrowing() {
    assert_eq!(
        run(
            "let calls = 0; function yes(): boolean { calls++; return false; } on PLAYER_LOGIN() { print(true ? yes() : false); print(false ? yes() : true); let n: number? = 7; print(n != null ? n + 1 : 0); print(calls); }"
        ),
        ["false", "true", "8", "1"]
    );
}

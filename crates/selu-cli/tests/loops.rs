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
fn each_turn_has_its_own_binding() {
    // Numeric and fallback forms: every closure keeps its own turn's value.
    let src = "on PLAYER_LOGIN() { const fs: (() => number)[] = []; for (let i = 0; i < 3; i++) { fs.push(() => i); } let limit = 3; for (let j = 0; j < limit; j++) { fs.push(() => j * 10); } for (const f of fs) { print(f()); } }";
    assert_eq!(run(src), ["0", "1", "2", "0", "10", "20"]);
}
#[test]
fn per_turn_writes_and_continue() {
    // A body write carries into the next turn; a closure's write after its
    // turn does not; continue still runs the update.
    let src = "on PLAYER_LOGIN() { let limit = 6; const bumps: (() => void)[] = []; const reads: (() => number)[] = []; for (let i = 0; i < limit; i++) { if (i == 1) { continue; } bumps.push(() => { i += 100; }); reads.push(() => i); i++; } for (const b of bumps) { b(); } for (const r of reads) { print(r()); } }";
    assert_eq!(run(src), ["101", "103", "105"]);
}
#[test]
fn per_turn_condition_and_update_closures() {
    // The condition and the update run on the turn's own binding.
    let src = "const seen: (() => number)[] = []; function note(f: () => number): boolean { seen.push(f); return true; } on PLAYER_LOGIN() { let limit = 3; let k = 0; for (let i = 0; note(() => i) && i < limit; i = i + 1) { k++; } print(seen.length, k); print(seen[0](), seen[3]()); }";
    assert_eq!(run(src), ["4	3", "0	3"]);
}
#[test]
fn breaks_and_returns_in_per_turn_loops() {
    let src = "function find(limit: number): (() => number)? { for (let i = 0; i < limit; i++) { if (i == 2) { return () => i; } } return null; } on PLAYER_LOGIN() { const f = find(5); print(f != null ? f() : -1); let limit = 9; const fs: (() => number)[] = []; for (let i = 0; i < limit; i++) { fs.push(() => i); if (i == 1) { break; } } print(fs.length, fs[1]()); }";
    assert_eq!(run(src), ["2", "2	1"]);
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

#[test]
fn fallback_return_body() {
    let src = "function first(limit: number): number { for (let i = 0; i < limit; i++) { return i; } return -1; } on PLAYER_LOGIN() { print(first(2)); }";
    assert_eq!(run(src), ["0"]);
    println!(
        "{}",
        compile(
            "function first(limit: number): number { for (let i = 0; i < limit; i++) { return i; } return -1; }"
        )
    );
}
#[test]
fn fallback_shadow_body() {
    assert_eq!(
        run(
            "on PLAYER_LOGIN() { let limit = 3; let turns = 0; for (let i = 0; i < limit; i++) { turns++; let i = 10; if (turns > 4) { break; } } print(turns); }"
        ),
        ["3"]
    );
}
#[test]
fn fallback_return_and_shadow_with_continue() {
    assert_eq!(
        run(
            "function first(limit: number): number { for (let i = 0; i < limit; i++) { if (i == 0) { continue; } return i; } return -1; } on PLAYER_LOGIN() { print(first(3)); let limit = 3; let turns = 0; for (let i = 0; i < limit; i++) { turns++; if (turns == 1) { continue; } let i = 10; print(i); if (turns > 4) { break; } } print(turns); }"
        ),
        ["1", "10", "10", "3"]
    );
}
#[test]
fn for_of_requires_known_collection() {
    assert_eq!(
        codes("let xs = CreateFrame(\"Frame\"); for (const x of xs) {}"),
        ["SELU0357"]
    );
}
#[test]
fn compound_string_untyped_field() {
    assert_eq!(
        run(
            "on PLAYER_LOGIN() { let frame = CreateFrame(\"Frame\"); frame.text = \"a\"; frame.text += \"x\"; print(frame.text); }"
        ),
        ["ax"]
    );
}
#[test]
fn compound_binary_diagnostics() {
    assert!(codes("let b = true; b += CreateFrame(\"Frame\");").contains(&"SELU0323".into()));
    assert!(
        codes("let frame = CreateFrame(\"Frame\"); frame.x += frame.y;")
            .contains(&"SELU0324".into())
    );
}
#[test]
fn event_handlers_capture_their_turn() {
    assert!(
        codes("let limit = 3; for (let i = 0; i < limit; i++) { on PLAYER_LOGIN() { print(i); } }")
            .is_empty()
    );
}
#[test]
fn update_condition_narrowing() {
    assert_eq!(
        run("on PLAYER_LOGIN() { for (let n: number? = 0; n != null && n < 2; n++) {} print(2); }"),
        ["2"]
    );
}
#[test]
fn plain_updates_are_readable() {
    let lua = compile("let i = 0; i++; i += 1;");
    assert_eq!(lua.matches("i = i + 1").count(), 2, "{lua}");
}
#[test]
fn fractional_and_negative_steps() {
    assert_eq!(
        run(
            "on PLAYER_LOGIN() { for (let i = 0; i < 1; i += 0.5) { print(i); } for (let j = 2; j >= 0; j += -1) { print(j); } }"
        ),
        ["0", "0.5", "2", "1", "0"]
    );
}
#[test]
fn null_then_ternary() {
    assert_eq!(
        run("on PLAYER_LOGIN() { print(true ? null : 1); print(false ? null : 1); }"),
        ["nil", "1"]
    );
}
#[test]
fn update_narrowing_invalidated_by_body() {
    assert!(
        codes("for (let n: number? = 0; n != null && n < 2; n++) { n = null; }")
            .contains(&"SELU0341".into())
    );
    assert!(
        codes("for (let n: number? = 0; n != null && n < 2; n++) { n = null; continue; }")
            .contains(&"SELU0341".into())
    );
}

#[test]
fn compound_index_once() {
    assert_eq!(
        run(
            "let calls = 0; function index(): number { calls++; return 0; } on PLAYER_LOGIN() { let t = [3]; t[index()] += 1; print(t[0]); print(calls); }"
        ),
        ["4", "1"]
    );
}

#[test]
fn update_narrowing_survives_an_unmodified_continue() {
    assert_eq!(
        run("let count=0;for(let n:number?=0;n!=null&&n<2;n++){count++;continue;}print(count);"),
        ["2"]
    );
}

#[test]
fn exponent_operator() {
    let src = "on PLAYER_LOGIN() { let x = 3; x **= 2; print(2 ** 10, 2 ** 3 ** 2, (-2) ** 2, -(2 ** 2), x, 2 * 3 ** 2); }";
    assert_eq!(run(src), ["1024\t512\t4\t-4\t9\t18"]);
    assert_eq!(
        selu_syntax::parse("t.selu", "const y = -2 ** 2;")
            .unwrap_err()
            .code,
        "SELU0112"
    );
    assert!(codes("const z = 2 ** \"a\";").contains(&"SELU0325".into()));
}

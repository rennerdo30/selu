# Control flow decisions (package A)

Conditions use the existing boolean/optional truthiness rule, including
branch narrowing. Where paths meet, a name keeps a fact only when every
path has one; the facts are joined (widgets of one class that differ only in
provenance merge to that class) or, when they share no common type, united
(`k = 3` on one path and `k = "x"` on the other leave `number | string`). Loop condition preludes execute on every evaluation.
A do-loop body is a Lua `do` block inside `repeat`, so body declarations
cannot shadow the names read by `until`.

Counted loops use Lua numeric for only for a mutable local initialized with
a small integer literal, a literal bound, a matching directional comparison,
and a nonzero constant integer update. Integers are limited conservatively
to absolute values at most one billion. Strict comparisons adjust the bound
by one. Any assignment to the binding anywhere in the body disables this
optimization. Fractional, dynamic and other headers use while; dynamic
bounds are evaluated each turn. No bound invariance is guessed across calls.

A `let` header binding gets one binding per turn, as in TypeScript. A
numeric for already has one: Lua makes a fresh loop variable each turn. When
a fallback loop's body, condition or update names the binding inside a
function, method, class or event handler, the loop takes a per-turn form.
Each turn declares a fresh local copied from a carrier. From the second turn
on, the update runs on that fresh copy, then the condition, then the body.
The turn's final value goes back to the carrier. So a closure keeps the
value of its own turn, and a closure's later write does not reach the next
turn. Loops without such a capture keep the plain while form. Detection
goes by spelling, so a nested function's own same-named variable only costs
the per-turn form.

## Helper for package B

`Lowerer::loop_body(&mut self, body: &[Stmt], span: &Span) -> LBlock`
(in selu-lower/src/loops.rs, pub(crate)) lowers a loop body and establishes
its transfer context. Put the returned statements directly inside the actual
Lua loop. A body with continue gets an inner `repeat ... until true`; a body
with both continue and break also gets a local break flag and its outer-loop
break check. Nested loops and functions do not contribute transfers to their
parent. The checker counterpart uses `loops::LoopCtx` markers; package B
must push a marker before checking its body and pop it afterwards.

Function boundaries save, empty and restore the loop stack. Break and
continue are terminating for early-exit narrowing, but never count as
function returns.

Ternaries join branch types; incompatible branches produce SELU0342.
Thus `c ? false : 1` is an error (there is no boolean/number union).
False/null-capable branches use a temporary and if; known truthy then values
with no preludes use `c and a or b`. Only the selected branch executes.

Compound assignments evaluate the receiver, index, old value and RHS in that
order, once each. Array/tuple indices get the zero-to-one-based adjustment.
The existing checker enforces mutability for names. Compound operators reuse the binary checker and binary lowering directly,
including both operand types for += string concatenation and SELU0324 for
ambiguous untyped operands. Other arithmetic requires numbers. Updates require numbers.
SELU0340 diagnoses transfers without a loop, SELU0341 operand errors, and
SELU0344 invalid assignment targets. Each includes a span and help.

## Pending package B integration

Index reads and ordinary indexed targets still depend on package B in this
base. Parent approved leaving the following exact end-to-end test pending;
receiver-once and old-value-before-RHS behavior is tested now with frames.
Add after B merges:

```rust
#[test]
fn compound_index_once() {
    assert_eq!(run("let calls = 0; function index(): number { calls++; return 0; } on PLAYER_LOGIN() { let t = [3]; t[index()] += 1; print(t[0]); print(calls); }"), ["4", "1"]);
}
```

No package A construct retains SELU0390. Data/index and function-expression
support still produce the base packages' SELU0390 until B/C merge. Their
integration and callback tests remain with the parent.

## Independent review fixes

Fallback for bodies have their own `do ... end` before the update. This keeps
body declarations out of the update's scope and makes a body-ending return
legal Lua 5.1. Continue wrappers stay inside this body scope.

For-of still reports SELU0390 until B implements it; it must never reach the
control-flow checker's unreachable arm.

Updates receive the condition-true narrowings in a separate scope. A fact is
removed if any body subtree writes that name, contains a call, or contains
continue. This intentionally includes nested functions and loops and does
not attempt path-sensitive proofs: even an unrelated call or a continue
with unchanged facts can cause otherwise valid optional updates to be
rejected. It cannot accept an update on the assumption that a nullable
binding remained non-null across these uncertain paths.

Plain-local updates with pure RHS expressions print directly as `i = i + 1`.
Receivers, indices, effectful right operands and expression preludes retain
snapshots to preserve evaluation order and the old value.

## Exponent

`a ** b` raises `a` to the power `b` and lowers to Lua's `^`; `x **= y` is its
compound form. It is right-associative (`2 ** 3 ** 2` is `2 ** 9`) and binds
tighter than `*`. As in TypeScript, a unary operator on its left needs
parentheses: `(-2) ** 2` or `-(2 ** 2)`, never `-2 ** 2` (SELU0112). Both
operands must be numbers (SELU0325).

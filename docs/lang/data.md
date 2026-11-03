# Data lowering decisions

Package B starts at `c4d0580527550bdff44c1676053d5178d7b683a2`.

- Arrays are dense Lua sequences exposed through 0-based numeric indices.
  Reads have type T, not T?: callers must keep indices in bounds. Numeric
  indices must be integers; no runtime bounds or integrality checks are added.
  Use Map for sparse data. Null elements and null collection type arguments
  are errors. Empty arrays require a written array type on the declaration.
- Only a call or vararg in the final array slot is parenthesized, to prevent
  multiple-return expansion. Other values print without extra brackets. Structural object fields use the existing checker.
  Optional access tests nil explicitly, evaluates the receiver once, and
  preserves false. Length is read-only; strings count bytes.
- push, insert, set, add, clear and sort return void. pop/removeAt return T?.
  delete returns whether an entry existed. count walks the table in O(n).
  WeakMap uses a fresh metatable with __mode = "k". Map and Set iteration order
  follows Lua pairs and is unspecified. Array iteration uses numeric for;
  the collection is evaluated once and its length is captured at entry.
  If shrinking the array exposes a nil element, iteration stops before the body;
  growth beyond the captured length is not visited.
- slice takes one or two bounds, floors fractional bounds, clamps them to
  the collection length, interprets negative bounds from the end, and excludes
  the end bound. String operations use bytes and literal substring matching,
  never Lua patterns supplied by the caller. split preserves empty fields;
  an empty separator splits bytes. trim removes Lua whitespace.
- Default array sort uses Lua ordering (ascending numbers or strings).
  join defaults to comma and handles numeric, string and boolean sequences.
  slice/split and boolean join allocate result tables. push/index operations
  introduce no tables, closures or iterator allocations; array capacity growth
  is live storage, distinct from garbage. The GC test warms capacity, stops GC,
  then checks unchanged memory across 1024 pushes and reads/writes.
- Math.round is floor(x + 0.5), including negative ties; random is Lua's
  zero-argument random. Template strings use one string.format, escape literal
  percent characters, use %s, and convert booleans through tostring.

## Integration hooks authorized by the parent

Checker lib.rs: stmt sends ForOf directly here; declared_type passes the written
initializer type; expr_inner Assign checks field/index stores; member delegates
object fields and length; call delegates collection/string/Math operations.
Lowerer lib.rs: stmt sends ForOf directly here; expr and target route field
access through data_field; call has a separate early data-call return, ahead of
package C's method-call branch. Untyped Blizzard receivers and manifest namespace
calls keep their existing paths. No loops.rs file is modified.
The stale unsupported-construct test drops exactly the array-literal and template
fixtures with parent approval; destructuring and other fixtures remain.

## Pending A and C

Callback methods now accept existing function values. Callbacks may ignore trailing
arguments: element, 0-based index, array; reduce prepends the accumulator. Predicate
results must be boolean; comparators return a number and are adapted to Lua's less
predicate. reduce without an initial value starts with the first element and errors
at runtime on an empty array. map rejects nullable results; map/filter allocate new
tables and comparator sort allocates one adapter closure per call.

Package C still supplies arrow/function-expression lowering and contextual typing
for arrows without written parameter types, plus ordinary object function calls.
B's early library dispatch excludes Object receivers. On this base the exact source
`function f(x: string): string { return x; } const o = { slice: f }; print(o.slice("hi"));`
still uses the existing colon-call path; its runtime result `hi` is pending C.
Package A integration is complete after rebasing onto 47e3874: the checker pushes
and pops A's LoopCtx, and lowering uses A's loop_body helper. Break, continue and
both in one body work for arrays, maps and sets. Functions retain A's empty-loop
boundary. For-of bindings use fresh per-turn locals, so captures are allowed;
simulator tests collect array and map closures and call them after the loop.

Optional access to a missing field is a checker error. Unsupported collection or
Math method names retain SELU0390, rather than falling through to Blizzard method
lowering. No publishing, pushes, benchmarks or native launches are performed.

## Verification

Data tests cover runtime behavior, diagnostics and the allocation check.
Both initial tests failed on the base. With data_expr deliberately disabled,
seven tests failed; strings_and_math had independently failed on literal-receiver
method dispatch before that bug was fixed. The deliberate break was restored.
Full cargo test, cargo fmt --check and cargo clippy --all-targets -- -D warnings
are the required final gates; exact results are in the implementation report.

## Review corrections

Written types dispatch through data_annotation, recursively rejecting nullable
array elements in declarations, parameters, returns and nested types with SELU0350
and the Map hint. Receiver types are checked once and reused on fallback paths.
Missing-field diagnostics span the member expression and include spelling help.
Optional length uses the same typed field lowering inside its nil guard.
Empty pop/removeAt calls are parenthesized to retain a single nil return.
Integer literal indices fold only below 2^53.

Shared seq now snapshots names as well as impure values before a later operand's
pre-statements; only literals remain stable. LExpr::is_pure is unchanged. The
additional plain-call regression f(x, g() ?? 0) proves this independently of data.
Store-specific name capture in data.rs protects push/set/index operands and
receivers before later impure arguments are hoisted into temporary stores;
pure pushes remain direct. The endsWith receiver-reassignment regression proves
the same rule for argument stabilization. Earlier map key names are protected too. expr_stmt now emits only
pre when the resulting expression is pure, omitting discarded local _ = nil.
No general seq snapshot for later impure expressions was added.

All review tests were red before their fixes. The statement-elision and callback
signature checks were also deliberately disabled to show their assertions fail,
then restored. Full gates and commit hashes are recorded in the implementation
report. No loops.rs or package C implementation was changed.

## Package A integration verification

The nine array/map/set transfer cases each failed with SELU0340 before integration.
The closure test already passed; deliberately replacing the fresh array local with
a shared assignment made all array closures return 6, proving the test can fail.
A's exact compound_index_once source is now in CLI tests/loops.rs and passes; it
was also demonstrated red with array checking deliberately disabled, then restored.
The checker loops.rs ForOf unsupported arm was removed; A's lowerer loops.rs had
no explicit ForOf arm to remove and was left unchanged. reject_capture stays private
and is not applied to for-of. The stale for_of_is_diagnostic test was renamed to
for_of_requires_known_collection and now expects exactly SELU0357 with approval.
The old B pending-continue SELU0390 fixture was removed in favor of runtime tests.

Final integration gates: cargo fmt --check, cargo clippy --all-targets -- -D warnings,
and cargo test all passed (114 tests). Rebase conflict resolution preserved A's
loop stack fields and function take/restore pairs plus B's dispatch hooks. The two
previously approved obsolete array/template unsupported fixtures were removed
after the full suite exposed their retention during conflict resolution.

# Data lowering decisions

Package B starts at `c4d0580527550bdff44c1676053d5178d7b683a2`.

- Arrays are dense Lua sequences exposed through 0-based numeric indices.
  Reads have type T, not T?: callers must keep indices in bounds. Numeric
  indices must be integers; no runtime bounds or integrality checks are added.
  Use Map for sparse data. Null elements and null collection type arguments
  are errors. Empty arrays require a written array type on the declaration.
- Literal elements are parenthesized in Lua to prevent a final API call from
  expanding multiple returns into extra array elements. Object fields are
  similarly single-valued. Structural object fields use the existing checker.
  Optional access tests nil explicitly, evaluates the receiver once, and
  preserves false. Length is read-only; strings count bytes.
- push, insert, set, add, clear and sort return void. pop/removeAt return T?.
  delete returns whether an entry existed. count walks the table in O(n).
  WeakMap uses a fresh metatable with __mode = "k". Map and Set iteration order
  follows Lua pairs and is unspecified. Array iteration uses numeric for;
  the collection is evaluated once and its length is captured at entry.
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

Callback array methods forEach/map/filter/find/some/every/reduce and sort with a
comparator remain SELU0390 pending package C. No replacement arrow implementation
is built here. Contextual callback typing and callback allocation warnings remain
part of that integration. Continue and break in for-of bodies currently reach
package A's loop_stmt and remain SELU0390 on this base. A's loop-body helper must
be integrated after A lands. for-of itself is implemented here.
Optional access to a missing field is a checker error. Unsupported collection or
Math method names retain SELU0390, rather than falling through to Blizzard method
lowering. No publishing, pushes, benchmarks or native launches are performed.

## Verification

Eight data tests cover runtime behavior, diagnostics and the allocation check.
Both initial tests failed on the base. With data_expr deliberately disabled,
seven tests failed; strings_and_math had independently failed on literal-receiver
method dispatch before that bug was fixed. The deliberate break was restored.
Full cargo test, cargo fmt --check and cargo clippy --all-targets -- -D warnings
are the required final gates; exact results are in the implementation report.

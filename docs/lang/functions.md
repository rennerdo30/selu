# Functions

Arrow functions and anonymous function expressions compile to Lua closures. They
capture variables, so later assignments are visible. Parameter types come from
annotations or the expected function type in a written variable declaration or a
call argument. The checker helper for callback implementations is
`Ctx::value_expected(&Expr, Option<&Type>) -> Type`; it records the expression type.
Package B can pass a callback signature to this helper.

A default runs only when the argument is nil, in parameter order, and can use
earlier parameters. It does not run for false. Inside the body the parameter
has its written type (or the inferred default type); callers may omit trailing default parameters. Required
parameters after a default still determine the minimum argument count. Null is
not accepted for a non-null parameter merely because it has a default.

A final `...rest: T[]` collects remaining arguments into a fresh Lua sequence.
This allocates one table per invocation, including calls with no rest arguments.
The compiler should warn for calls to rest functions in hot event/update/loop
paths when hot-path analysis is available; this package adds no such analysis.
Prefer fixed arguments in hot paths. Rest must be last, have an array annotation,
and have no default. Function types retain fixed parameter types, minimum arity
and the rest element type; calls check every rest argument against that element.

Array destructuring of a tuple-returning call, or an untyped game API call,
uses Lua multiple assignment without allocating a table. Holes use distinct
compiler temporaries rather than a shared `_`, avoiding collisions. Destructuring
an array/tuple value reads its 1-based Lua slots. Object destructuring reads named
fields, supports renaming, and evaluates its initializer once. Array and object
literals remain package B's responsibility.

A function with a written tuple return type can return `[a, b]`; this emits
`return a, b`, with no table. Forwarding `return f()` is allowed when the return
types agree. Returning a tuple table value reads its slots into separate results. A tuple call can also stand alone as a statement or be destructured.
Using it as one ordinary value is SELU0377: write `const [a, b] = f();`.
Tuple values obtained from host tables can still be destructured as tables.
Tuple return types must be written; general array-return inference belongs to B.

A call `object.field(args)` uses `.` when the checked object type has a function
field of that name. It passes only the written arguments. Untyped game objects
keep Blizzard's `:` convention and receive the object as `self`. Manifest
namespace functions retain plain calls. No method binding or implicit `this`
is introduced for Selu object function fields.

The package's diagnostic range currently uses SELU0370-0377 for invalid rest
parameters, rest position, invalid default values, out-of-range tuple bindings,
invalid destructuring, missing fields and single-value tuple calls.

## Review clarifications

An explicit `null` also triggers a default: `function f(x: number? = 7)`
receives 7 for `f(null)`. Lua 5.1 represents both an omitted argument and
`null` as nil; Selu intentionally uses the same nil-based default rule for
both. TypeScript preserves an explicit null, so this is a deliberate difference.

Written function types preserve a final rest parameter, for example
`(...xs: number[]) => void`. Nullable rest elements such as `number?[]` are
rejected: Lua sequences cannot preserve nil holes. Function assignment checks
every position contravariantly, including rest arguments that land in optional
fixed parameters of the assigned function.

Tuple calls are also rejected as condition operands, including `!`, `&&` and
`||`. Destructure first and test the desired result. Self-referential closures
and references to later destructuring declarations share forward-declared Lua
locals. Empty destructuring evaluates its initializer once without bindings.

An optional tuple return (`[A, B]?`) returns either nothing (`return null`)
or both values. Destructuring such a call binds each element as optional
(`const [a, b] = f();` gives `a: A?`, `b: B?`); using the call as one value
is SELU0377 like any tuple call. A typed tuple variable (`const t: [A, B] =
[a, b];`) is a table: `t[0]` is `A`, `t[1]` is `B`, and a literal index past
the end is SELU0352.

A conditional with an expected type checks each branch against it, so
`return ok ? "in" : "out";` fits a `"in" | "out"` result, and a template
string accepts literal types and unions of strings, numbers and booleans.


## Generic functions

```selu
export function first<T>(xs: T[]): T? { return xs.length > 0 ? xs[0] : null; }
export function mapAll<T, U>(xs: T[], f: (x: T) => U): U[] { ... }

const n = first([3, 4]) ?? 0;                     // T is number
const lengths = mapAll(["a", "bc"], (s) => s.length); // T string, U number
```

- Inside the function a type parameter is opaque: values of `T` can be
  passed along, stored in `T[]`, `Map<K, T>` or `Set<T>` and returned, but not
  read as a number, string or table (`v + 1` on a `T` is SELU0323).
- Each call infers the type arguments from its arguments. Literals bind their
  base type (`first(["a"])` gives `string`). Callback arguments are checked
  after the others, so `(s) => s.length` sees `s: string`; an uninferred
  result type is taken from the callback's body.
- A type parameter that appears in the result but in no argument is
  SELU0651; one declared twice is SELU0650.
- Generic functions are exported and imported like any function; the
  generated Lua has no trace of the type parameters.

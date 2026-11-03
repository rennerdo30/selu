# Work packages for the language core

SPEC-V1 section 11 step 2 is split into packages that can run side by side.
The parser, AST, scopes, basic types and the lowering machinery are done;
each package fills in checking and lowering for its constructs in **its own
files**. Read this whole file, then `SPEC-V1.md` sections 2-3,
`docs/ARCHITECTURE.md` and `docs/TOOLKIT-SURVEY.md` before starting.

## How every package works

**Files.** Each package owns these files and edits nothing else, except
where its section says so:

| Package | Checker | Lowering | Tests | Decisions |
|---|---|---|---|---|
| A control flow | `crates/selu-check/src/loops.rs` | `crates/selu-lower/src/loops.rs` | `crates/selu-cli/tests/loops.rs` | `docs/lang/control-flow.md` |
| B data | `crates/selu-check/src/data.rs` | `crates/selu-lower/src/data.rs` | `crates/selu-cli/tests/data.rs` | `docs/lang/data.md` |
| C functions | `crates/selu-check/src/functions.rs` | `crates/selu-lower/src/functions.rs` | `crates/selu-cli/tests/functions.rs` | `docs/lang/functions.md` |

The parser already produces every construct (`crates/selu-syntax/src/ast.rs`).
The central matches in `selu-check/src/lib.rs` (`stmt`, `expr_inner`) and
`selu-lower/src/lib.rs` (`stmt`, `expr`) already send each construct to your
file; today your functions report `SELU0390` ("isn't supported yet") or are
`unreachable!`. Replace them. If the parser is wrong for your constructs,
stop and report it to the parent instead of working around it.

**What you build on (don't duplicate it):**

- Checker (`selu-check/src/lib.rs`, `types.rs`): `Ctx::expr` (records the
  type), `value` (an expression whose value is used), `condition` (truthiness
  rule plus narrowing), `block`, `function_body` (return inference),
  `declare`/`finish_decl`/`lookup`/`type_of`, `write` (assignment checks),
  `unsupported`; `Type` with `Array`, `Tuple`, `Object`, `Map`, `Set`,
  `WeakMap`, `Function`; `types::resolve` for written types.
- Lowering (`selu-lower/src/lib.rs`, `lua.rs`): build Lua with the `lua.rs`
  tree, never with strings. `Lowerer::expr(e, pre)` lowers an expression and
  may push statements into `pre`; `seq` lowers several in order (keeps
  left-to-right order when one needs statements); `hold` makes a temporary;
  `finish` puts a statement after its `pre`; `assign_into`, `block`,
  `function`, `target`. The printer handles brackets, `t["end"]` and
  one-line `if`s. Read how `short_circuit` lowers `??`: it is the pattern for
  any expression that needs statements.
- Tests: `crates/selu-cli/tests/common/mod.rs` has `run(src)` (compile, load
  in the simulator, fire `PLAYER_LOGIN`, return printed lines),
  `compile(src)` and `codes(src)`.

**Rules (all binding):**

1. Rule one: a TypeScript developer reads it without a tutorial. Rule two:
   no garbage in hot paths, no idle cost. Rule three: the Lua reads like a
   careful person wrote it. When the spec doesn't decide a detail, pick the
   simplest option that fits rule one and write it in your decisions file.
2. Every new diagnostic has a code from your range, an exact span, one plain
   sentence, and a `help`/`fix` line where possible. Ranges: A `SELU0340`-
   `0349`, B `SELU0350`-`0369`, C `SELU0370`-`0389`.
3. Every lowering rule has a behaviour test that runs the Lua in the
   simulator (`run`), plus checker tests for its errors (`codes`). **A test
   must be able to fail:** show it red on the base commit or with the fix
   temporarily broken, and say so in your report.
4. No stubs that make tests pass. If something can't be done in your
   package, leave the `SELU0390` error in place and list it in your report.
5. Before you report: `cargo fmt --check`, `cargo clippy --all-targets --
   -D warnings` and `cargo test` are clean. The workspace is small; the full
   `cargo test` is fine. No benchmarks or load generators.
6. Commits: small and logical, on your branch, author `rennerdo30
   <9086097+rennerdo30@users.noreply.github.com>`. Don't write git config.
   No AI mentions, `Co-Authored-By` or "Generated with" lines. Don't push
   or publish anything; the parent merges.

## Package A: control flow

Constructs: `while`, `do...while`, counted `for`, `break`, `continue`,
`cond ? a : b`, `x += v` (all of `+= -= *= /= %=`), `x++`/`x--`.

Acceptance:

1. `while` and `do...while` with the truthiness rule on the condition. A
   condition that needs statements (`while ((f() ?? 0) > 0)`) still runs
   them on every turn (`while true do <pre> if not c then break end ... end`).
2. `do { } while (c);` lowers to `repeat ... until not c`, but the condition
   must not see names declared in the body (Lua's `until` does; Selu's
   block scoping doesn't). Prove it with a test where the body shadows a
   name the condition reads.
3. Counted `for`: lower to Lua's numeric `for i = a, b do` only when that
   is exact (the loop variable is a number not assigned in the body, the
   bound can't change during the loop, a simple `< <= > >=` condition and a
   `++`/`--`/`+= const` update); otherwise a `while` form. Both forms
   tested, including a bound that changes in the body.
4. `continue` in every loop kind, without `goto` (Lua 5.1 has none): the
   usual `repeat ... until true` inner block, plus a flag when the same body
   also uses `break`. Provide this as a helper that package B's `for...of`
   can call (document its signature in your decisions file).
5. `break`/`continue` outside a loop are errors; `always_exits` in
   `selu-check/src/lib.rs` learns `break`/`continue` (the one function in
   `lib.rs` you may edit).
6. `cond ? a : b`: condition by the truthiness rule; result type is the
   join of both sides (`SELU0339`-style error when there is none). Lower to
   `c and a or b` only when `a` can never be false or nil (by type);
   otherwise an `if` on a temporary. Both tested, including `c ? false : 1`.
7. `+=` etc.: types as for the operator (`+=` joins strings with `..`), the
   target is evaluated once (`t[f()] += 1` calls `f` once). `x++`/`x--` on
   numbers. All only as statements or as the `for` update (the rules pass
   already enforces where they may appear).
8. A test that a loop with a closure capturing the loop variable behaves as
   in TypeScript (`let` gives each turn its own variable), or a documented
   decision with an error if you can't make it exact.

## Package B: data

Constructs: array literals, object literals, `xs[i]`, `a?.b`, `new
Map/Set/WeakMap`, template strings, `.length`, `for...of`, the array,
string, `Map`/`Set` methods and `Math` from SPEC-V1 2.3.

Acceptance:

1. Arrays: `[1, 2]` is a `number[]`; elements can't be null (error, with
   the `Map` hint from SPEC 2.2); `[]` needs a written type. `xs[i]` lowers to
   `xs[i + 1]`, folded for literals (`xs[0]` -> `xs[1]`); assignment
   `xs[i] = v`. `xs.length` -> `#xs`. Reading an index gives `T` (not `T?`);
   document that decision or choose `T?` with a reason.
2. Array methods from SPEC 2.3. `push` lowers to `xs[#xs + 1] = v` (no
   function call), `pop`, `insert(i, x)`, `removeAt(i)` with 0-based
   indices, `indexOf` (-1 when missing), `includes`, `sort(compare?)`,
   `slice`, `join`, `forEach`, `map`, `filter`, `find`, `some`, `every`,
   `reduce`. Methods taking callbacks need arrow functions from package C;
   implement them so they work with any function value, and test them once
   C has merged (list them as pending in your report if C isn't merged).
3. Objects: `{ a: 1, b: "x" }` gets an object type; field reads and writes
   are checked against it (`SELU0330` style errors); optional fields read as
   `T?`. Lua keyword field names keep working (`t["end"]`).
4. `a?.b` gives `T?` and is exact when `a.b` is `false`.
5. `Map<K, V>` (`get` returns `V?`, `set`, `has`, `delete`, `clear`,
   `count()`), `Set<T>` (`add`, `has`, `delete`, `clear`, `count()`),
   `WeakMap<K, V>` (as `Map` with a `__mode = "k"` metatable). Keys and values
   can't be null. All are plain Lua tables.
6. `for (const x of xs)` over arrays (a numeric `for` over `#xs`, no
   iterator closure), `for (const [k, v] of map)`, `for (const x of set)`.
   `continue` inside it uses package A's helper once A has merged.
7. Template strings lower to one `string.format` call: literal `%` escaped,
   numbers and strings with `%s`, booleans through `tostring`. Interpolating
   a `T?` is an error with the fix `?? ""` or a comparison.
8. Strings: `s.length` (bytes), `split`, `trim`, `startsWith`, `endsWith`,
   `includes`, `upper`, `lower`, `slice(from, to)` 0-based. `Math.floor`,
   `ceil`, `round`, `min`, `max`, `abs`, `clamp`, `random`.
9. A test proves `push` and indexing in a loop create no garbage, using the
   simulator's Lua (`collectgarbage("count")` before and after).

## Package C: functions

Constructs: arrow functions and function expressions, default and rest
parameters, destructuring declarations, function values and calls through
them, own-object method calls.

Acceptance:

1. Arrow and function expressions become Lua closures; they capture
   variables as in TypeScript (closures see later assignments). Their type
   is a `Function` type; calling a function value checks arguments as for
   declared functions.
2. Contextual typing: an arrow passed where a function type is expected
   (`const f: (n: number) => string = n => ...`) gets its parameter types
   from that type, so parameters need no annotation there. Provide a helper
   that package B's callback methods can call.
3. Default parameters (`if x == nil then x = d end` at the start of the
   body; the parameter's type is `T`, callers may omit it) and rest
   parameters (`...rest: T[]` collects the remaining arguments into an
   array; document the garbage this creates and when the compiler should
   warn in hot code).
4. Destructuring: `const [a, , b] = f()` from a call that returns several
   values (game APIs are untyped: treat their results as multiple returns)
   lowers to `local a, _, b = f()` with no table; from a tuple or array
   value it reads the elements; `const { x, y } = p` reads the fields, `p`
   evaluated once.
5. Functions returning tuples: `function f(): [number, string] { return
   [1, "a"]; }` lowers to `return 1, "a"` (no table) and destructuring its
   call gets both values. Decide and document what using such a call as a
   single value means.
6. Calls on objects: today every `a.b(x)` lowers to `a:b(x)` (Blizzard
   style). For Selu values whose type is known (an object type with a
   function field), call with `.`; for untyped values (game objects) keep
   `:`. Document the rule; test both.

## Package D (later): WoW features

Tagged templates (`L`, `log`), `saved` with migrations and
`selu.saved.toml`, `command`, `hook`, `hookScript`, modules with `.toc` order
from the import graph. Starts once A-C have merged.

## Merge order and reviews

A merges first (B's `for...of` uses its `continue` helper), then B and C in
either order. Every branch gets an independent review before merge; the
parent does the final check and merges. Findings outside a package become
new issues, not more commits on the branch.

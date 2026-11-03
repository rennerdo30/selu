# Work packages for the language core

> **Status (2026-11-04):** packages A, B and C are done, reviewed and merged
> into `master`. Their decisions live in `docs/lang/control-flow.md`,
> `docs/lang/data.md` and `docs/lang/functions.md`. Round 2 (D1, D3, D4)
> is described at the end of this file.

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

## Round 2: modules, classes, WoW built-ins

Round 2 runs D1, D3 and D4 side by side; D2 (`saved` with migrations)
follows once D1's project pipeline is merged. "How every package works"
above applies unchanged, except for the files and code ranges below.
Round 1 showed that packages need small hooks in shared files; the ones
each package may make are listed in its section. Anything else in a shared
file: ask the parent first, with a concrete proposal.

| Package | Checker | Lowering | Other owned files | Tests | Decisions | Codes |
|---|---|---|---|---|---|---|
| D1 modules | `selu-check/src/modules.rs` | `selu-lower/src/modules.rs` | `selu-cli/src/main.rs` (the build pipeline), `examples/modules/` | `selu-cli/tests/modules.rs` | `docs/lang/modules.md` | `SELU0400`-`0419` |
| D3 classes | `selu-check/src/classes.rs` | `selu-lower/src/classes.rs` | | `selu-cli/tests/classes.rs` | `docs/lang/classes.md` | `SELU0420`-`0439` |
| D4 WoW built-ins | `selu-check/src/wow.rs` | `selu-lower/src/wow.rs` | `selu-sim/src/wow_api.rs` | `selu-cli/tests/wow.rs` | `docs/lang/wow.md` | `SELU0440`-`0459` |

The parser already reads `import`/`export`, `class` (fields, `static`,
`constructor`, methods, `extends`), `this`, `super`, and tagged templates
(see `crates/selu-syntax/src/ast.rs`). The dispatch points exist: in the
checker `declare_imports`, `import_stmt`, `export_stmt` (D1),
`declare_class`, `class_decl`, `class_expr` (D3), `builtin_call`,
`tagged_template` (D4); the same names in the lowering. `Sim::load_addon`
loads files the way the client does (`...` = addon name and one shared
table).

### D1: modules

Acceptance:

1. A project is the files reachable from `src/main.selu` through imports
   (one file is one module; paths are relative, without the `.selu`
   extension). A `.selu` file under `src/` that nothing imports is a warning
   naming it.
2. `import { a, b as c } from "./x";` checks that `./x` exists and exports
   `a` and `b` (exact spans on the bad name or path, a "did you mean"
   where cheap). Imported names keep their types across files: an imported
   function keeps its signature, an imported `const` its type. Imports are
   read-only (`SELU04xx` when assigned).
3. Modules share the addon's private table (`local ADDON, ns = ...`), never
   globals. Decide and document how an export is stored and read so that it
   stays readable and fast: an exported `let` must behave as a live binding
   (an importer sees later assignments), while functions and `const`s may be
   cached in a local at the top of the importing file.
4. The `.toc` lists the files in dependency order from the import graph
   (ties broken by path, so builds are reproducible). An import cycle is an
   error that shows the whole cycle, each import with its span.
5. `export global name = value;` is the way Lua code outside the addon
   reaches Selu code (SPEC-V1 section 5): it lowers to a plain Lua global.
6. `selu build` compiles every module per target, renders each diagnostic
   with its own file's source, writes one Lua file and one source map per
   module, and stops on errors in any file. `examples/modules/` is a small
   multi-file addon; an end-to-end test builds it with the real `selu`
   binary and runs it in the simulator with `load_addon` in `.toc` order.
7. `import name from "./file.ext"` (assets) stays `SELU0390` for now.

Shared edits allowed: new public entry functions in `selu-check/src/lib.rs`
and `selu-lower/src/lib.rs` for checking and lowering one module with the
exports of its imports (keep `check_program`/`lower` working for single
files); `used_before_declaration` in `selu-lower/src/lib.rs` so it sees
names inside `export`; `generate_toc` in `selu-lower/src/lib.rs`.

### D3: classes

SPEC-V1 2.4: one metatable per class with `__index`, methods called with
`:`, `this` becomes `self`, arrow functions capture `this`, `static`,
single `extends` and `super`.

Acceptance:

1. A class declares a type: the class name works in written types
   (`let p: Point`), and instances are checked against their fields and
   methods (missing field, wrong type, wrong arity: errors with exact
   spans). Fields need a written type or a starting value.
2. `new Point(1, 2)` checks the constructor's parameters and lowers to
   `Point.new(1, 2)`. Field initialisers run in declaration order before
   the constructor body; in a class with `extends`, `super(...)` must come
   first in the constructor (an error otherwise), and initialisers run right
   after it, as in TypeScript.
3. Lowering reads like hand-written Lua:
   ```lua
   local Point = {}
   Point.__index = Point
   function Point.new(x, y)
     local self = setmetatable({}, Point)
     self.x = x
     self.y = y
     return self
   end
   function Point:length()
     return math.sqrt(self.x * self.x + self.y * self.y)
   end
   ```
   Method calls on instances lower to `p:length()`; static members to
   `Point.make()`. One table per `new`, nothing per method call.
4. `this` is allowed in methods, constructors and arrow functions nested in
   them (arrows capture it); in a `function` expression or outside a class
   it is an error.
5. `extends`: the derived class's metatable falls back to the base
   (`setmetatable(Derived, { __index = Base })` or equivalent), an instance
   of the derived class is assignable to the base type, `super.method(...)`
   lowers to `Base.method(self, ...)`, and overriding a method with an
   incompatible signature is an error.
6. Static fields and methods (`static count = 0;`, `static make()`), with
   `this` inside a static method an error (or the class itself; decide and
   document).
7. Classes work with `export` once D1 lands; until then test them in one
   file.

Shared edits allowed: written-type resolution in
`selu-check/src/types.rs` (`resolve`/`named`) may consult user-declared
class names (pass them in; keep the function usable for built-in types);
a `Type` variant for class instances (and its `assignable_to`, `Display`);
the `New` branch in `selu-check/src/data.rs` and `selu-lower/src/data.rs`
may dispatch user classes to your files; the method-call branch in
`Lowerer::call` (`selu-lower/src/lib.rs`) may send class instances to `:`;
`used_before_declaration` may see class names.

### D4: WoW built-ins

SPEC-V1 3.3 and 3.4.

Acceptance:

1. `command("/myaddon", "/ma", (args) => { ... });` registers a slash
   command (`SLASH_<KEY>1`, `SLASH_<KEY>2` and `SlashCmdList[KEY]`); the key
   is unique per addon and command. `args` is a `string`. The simulator gets
   `SlashCmdList` and a `Sim::slash("/ma hello")` helper that dispatches the
   way the client does.
2. `hook(GameTooltip, "SetUnit", (tip, unit) => { ... });` and
   `hook("GlobalFunction", (...) => { ... });` lower to `hooksecurefunc`;
   `hookScript(frame, "OnShow", (self) => { ... });` to `frame:HookScript`.
   Hooks never replace functions. The simulator models `hooksecurefunc`
   (runs the hook after the original, with the same arguments) and
   `HookScript`.
3. `log.debug`, `log.info`, `log.warn`, `log.error` tagged templates and
   `log.warnOnce` write to the addon's debug log (a bounded buffer in the
   addon's table; the simulator can read it). Rule two: a disabled level
   (debug is off by default) builds no string. Decide and document the
   runtime shape.
4. `` L`${missing} missing` `` marks a user-facing sentence: the English text
   is the key, arguments become `{0}`, `{1}`. Lower it to one
   `string.format` with the active locale's format string, falling back to
   English, and design it so a translation can reorder arguments. Collect
   the keys of a file into a new `locale_keys: Vec<String>` field on
   `Lowered` (`selu-lower/src/lib.rs`, the one shared edit), so the build
   can later write `locales/enUS.toml`; writing that file is not part of
   this package. Joining two `L` results with `+` is a warning.
5. A local name that shadows a built-in (`const log = ...;`) turns the
   built-in off in that scope: `builtin_call` returns `None` there.
6. Callback parameters are typed from context where the built-in fixes them
   (`command`: `string`); hooked functions are untyped game values.

Shared edits allowed: the `locale_keys` field on `Lowered`; `HookScript`
on frames in `selu-sim/src/lib.rs`'s `CreateFrame` (the method list), and
`Sim::slash` plus any helper the tests need to read the debug log.

### Merge order

D1, D3 and D4 in any order; each branch gets an independent review, then
the parent rebases the later ones. D2 starts after D1 is merged.

## Merge order and reviews (round 1)

A merges first (B's `for...of` uses its `continue` helper), then B and C in
either order. Every branch gets an independent review before merge; the
parent does the final check and merges. Findings outside a package become
new issues, not more commits on the branch.

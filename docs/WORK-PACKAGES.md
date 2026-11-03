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
5. **Every package adds at least one example** under `examples/` (owner,
   2026-11-04): a small addon using the package's features the way a real
   addon would, listed in `examples/README.md`, with an `expected.txt` of
   the lines it prints after `PLAYER_LOGIN` (see
   `crates/selu-cli/tests/examples.rs`). Work the expected values out by hand,
   not from the compiler's output.
6. Before you report: `cargo fmt --check`, `cargo clippy --all-targets --
   -D warnings` and `cargo test` are clean. The workspace is small; the full
   `cargo test` is fine. No benchmarks or load generators.
7. Commits: small and logical, on your branch, author `rennerdo30
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

## Round 3: saved data, manifests

Round 3 runs D2 and M1 side by side (owner, 2026-11-04: "sure do that").
"How every package works" applies unchanged, including the example rule,
except for the files and code ranges below. D3 (classes) and D4 (built-ins)
from round 2 are still being merged; stay out of their files
(`classes.rs`, `wow.rs`, `selu-sim/src/wow_api.rs`).

| Package | Owned files | Tests | Decisions | Codes |
|---|---|---|---|---|
| D2 saved data | `selu-check/src/saved.rs`, `selu-lower/src/saved.rs` (new), `selu-sim/src/saved.rs` (new), `examples/saved-settings/` | `selu-cli/tests/saved.rs` | `docs/lang/saved.md` | `SELU0460`-`0479` |
| M1 manifests | `crates/selu-manifest/`, a new crate `crates/selu-manifest-gen/` (the generator binary), `manifests/retail/`, `manifests/forever/` | in those crates | `docs/manifests.md` | `SELU02xx` (manifest range) |

### D2: `saved` with checked migrations

SPEC-V1 3.2. Acceptance:

1. Syntax (you own these parser and AST additions): `saved name = { ... };`,
   `saved perCharacter name = { ... };`, `saved name version 2 = { ... };`,
   `migrate name 1 -> 2 (old) { ... }`. Top level of a file only.
2. Each `saved` declares one SavedVariables (or SavedVariablesPerCharacter)
   global with an object type from its defaults; the `.toc` gets the
   `## SavedVariables:` / `## SavedVariablesPerCharacter:` lines. The global's
   Lua name is unique per addon (document the rule; it must not collide
   with other addons' globals).
3. On `ADDON_LOADED` for this addon: run the pending migrations in order,
   then fill missing keys from the defaults (recursively for nested objects;
   existing values, including `false`, are never overwritten; arrays and
   maps in defaults are copied, not shared). A migration that throws stops
   the chain, leaves the data at the last good version and retries at the
   next login. Data from a newer version is left untouched and a warning is
   logged. Document which event the addon's code may first read the data
   in.
4. Reading a saved value where it can't be loaded yet (top-level statements
   of a file) is a compile error with a clear message; reads inside
   functions and event handlers are fine.
5. **Checked migrations:** every build writes the saved shapes to
   `selu.saved.toml` in the project root (TOML, deterministic, meant to be
   committed). If a shape changed and the version wasn't raised, or the
   version was raised without a `migrate` from the previous version, the
   build fails with a message that shows the migration to write (old and new
   fields). Adding a field with a default needs no new version (defaults fill
   it); removing or retyping a field does.
6. The simulator models SavedVariables: `ADDON_LOADED` with the addon name
   (fired by a helper that loads the addon), values saved at logout and
   loaded at the next login (`Sim` helpers to log out, inspect and log in
   again with the saved values), so tests can run a whole migration across
   two "sessions".
7. Example `examples/saved-settings/`: settings with defaults and one
   migration, printing at `PLAYER_LOGIN`.

Shared edits allowed: parser and AST for `saved`/`migrate` (add the
statement kinds, the parser arms and the `visit.rs` children); one dispatch
line each in the checker's and lowering's statement matches and in the
checker's block predeclaration; the build pipeline in
`crates/selu-cli/src/main.rs` (reading and writing `selu.saved.toml`, `.toc`
lines); `generate_toc` in `selu-lower/src/lib.rs`; in `selu-sim/src/lib.rs`
only the hooks your `saved.rs` needs to load an addon with saved data and
fire `ADDON_LOADED`.

### M1: generated manifests

SPEC-V1 section 7, DESIGN.md section 2. Data source: the local clone
`E:/Development/wow-ui-source` (branches `live` = retail, `forever`,
`classic_era`, `classic`, `classic_anniversary` and more), folder
`Interface/AddOns/Blizzard_APIDocumentationGenerated` (Lua tables: systems
with `Functions`, `Events`, `Tables`; `FrameAPI*` files for widget methods).
**That repository belongs to the owner's addon work: never check out,
switch, fetch, reset or modify it.** Read with `git -C E:/Development/wow-ui-source
show <branch>:<path>` or `git ls-tree`/`git archive` only.

Acceptance:

1. A generator binary (`crates/selu-manifest-gen`) takes a flavour, a
   source branch or commit and an output path, and writes one manifest per
   build: `manifests/<flavour>/<build>.json`. The build and version come
   from the branch's commit (`12.1.5 (70391)`), recorded with the source
   commit hash in the manifest. Read the documentation tables by running
   them in a sandboxed Lua 5.1 (the workspace has `mlua`), never by
   pattern-matching source text.
2. The manifest schema (in `crates/selu-manifest`) grows to carry: functions
   (global and namespaced) with typed parameters (name, type, nilable,
   default), returns (several, nilable), `MayReturnNothing`, secret and
   restriction flags as documented; events with typed payloads; structures
   (tables) as object types; enums with their values; widget types with
   their methods and inheritance; and the type names Blizzard uses kept
   faithfully (`luaIndex` stays distinct from `number`: Selu arrays are
   0-based, the checker will treat 1-based API indices specially later).
   Document the mapping to Selu types in `docs/manifests.md`. Existing
   fields and the current loader stay backward compatible: the checker on
   `master` must keep working unchanged with the new manifests (it reads
   names only today).
3. **Names, signatures and flags only:** no documentation text, comments or
   source are copied into manifests (SPEC-V1 7). Check the output for it.
4. The interface number per flavour comes from the source (a Blizzard
   addon's `## Interface:` line in that branch) or from an overlay; record
   which. Note: the owner's addon docs say WoW: Forever needs interface
   `16001` (`E:/Development/RennersAddons/docs/WOW_LUA_API.md`, read-only).
5. Overlays: `manifests/<flavour>/overlay.toml`, hand-edited, merged on top
   of generated data; every entry has a `note` saying where the knowledge
   comes from. Unknown keys are an error. Start with what the generator
   can't know (for example the interface number if needed).
6. Output is deterministic: sorted, stable formatting, byte-identical when
   run twice; a test proves it.
7. Generate and commit manifests for `retail` (branch `live`) and `forever`.
   `selu targets` lists them, `selu build --target retail` and `--target
   forever` build every example, and the hand-made fixture keeps driving
   the existing tests.
8. Tests: the generator on small hand-written documentation tables (not
   copied from Blizzard), schema round trips, overlay merging and errors,
   determinism, and one smoke test that loads each committed manifest and
   finds a few well-known entries (`C_Timer.After` or its real location,
   `CreateFrame`, `PLAYER_LOGIN`, a widget method like `SetPoint`).
9. Example: none needed (no language feature); instead `docs/manifests.md`
   explains how to regenerate a manifest for a new client build.

Shared edits allowed: `crates/selu-cli/src/main.rs` only where `selu
targets` needs to show new manifest fields; the workspace `Cargo.toml`
members list for the new crate. Nothing in `selu-check`, `selu-lower` or
`selu-syntax`.

### Merge order (round 3)

D2 and M1 in any order. Wiring the typed manifests into the checker (game
API values stop being untyped) is the next round, after D3 has merged.

## Round 4: typed game API, `has(...)` and targets, readability

Round 4 runs T1, T2 and R side by side (owner, 2026-11-04). "How every
package works" applies unchanged, including the example rule. All earlier
packages are merged; the code each round-4 package builds on is in place.

| Package | Owned files | Tests | Decisions | Codes |
|---|---|---|---|---|
| T1 typed game API | `selu-check/src/api.rs` (new), `examples/frames/` | `selu-cli/tests/api.rs` | `docs/lang/api.md` | `SELU0480`-`0499` |
| T2 `has(...)` and targets | `selu-check/src/targets.rs`, `selu-lower/src/targets.rs` (new), `examples/multi-target/` | `selu-cli/tests/targets.rs` | `docs/lang/targets.md` | `SELU0500`-`0519` |
| R readability | the lowering files `selu-lower/src/*.rs` for the items below | `selu-cli/tests/readability.rs`, `examples/*/expected.lua` | `docs/lang/output.md` | none |

### Decision: multiple results (owner, 2026-11-04; second opinion from Sol)

Game API functions return Lua *multiple results*, not tuples.
`const name = UnitName("player");` takes the first result, typed as the
first return; `const [name, realm] = UnitName("player");` takes all of
them. Selu's own tuple functions (`function f(): [A, B]`) keep the stricter
rule from `docs/lang/functions.md` (SELU0377). The difference is part of the
function type ("multiple results" versus "tuple"), never a special case for
Blizzard names, so it survives aliases, imports and callbacks. A call never
expands implicitly: in the last position of an argument list or array
literal it still contributes exactly one value (the lowering writes
`print((UnitName("player")))`); passing all results needs destructuring.
`MayReturnNothing` makes every result optional; zero results and a nil
result both read as `null`. SPEC-V1 2.4's wording ("tuples") is updated
accordingly by T1.

### T1: typed game API

Acceptance:

1. Calls to manifest functions with a signature are checked: argument count
   (from nilable and defaulted arguments) and types, with SELU errors naming
   the parameter and the target. Their results are typed per the decision
   above (`Nilable` returns are `T?`). Functions with evidence `source`
   (names only) stay untyped.
2. Type mapping from `docs/manifests.md`: `cstring`/`string` to `string`,
   `bool` to `boolean`, numeric aliases to `number`, structures to object
   types, callbacks to function types, enums: `Enum.X.Y` is a `number`
   constant, and parameters typed as an enum accept numbers. `luaIndex` is
   a `number` whose documentation says it is 1-based (record in `api.md`;
   no conversion).
3. Widgets: `CreateFrame("Button", ...)` returns the widget type for the
   literal frame type (an unknown literal is an error with "did you mean";
   a non-literal type gives `Frame`), with all inherited methods.
   `CreateTexture`, `CreateFontString` and other documented factory methods
   return their widget types. Method calls on widgets are checked (unknown
   method with "did you mean", arity, argument types) and keep the `:` call.
   Game globals with a documented widget type (`UIParent`, `GameTooltip`)
   use it.
4. Event handlers: parameters get the payload types; more parameters than
   the payload is an error.
5. One diagnostic per mistake: `C_Foo.Bar()` on a missing namespace gives
   only SELU0301, not also SELU0310.
6. Tests run against the real `retail` and `forever` manifests (calls,
   methods, events, an enum, a structure, multiple results in every position
   from the decision) and keep the fixture tests working.
7. Example `examples/frames/`: a small addon building a frame with a texture
   and a font string, setting points and text, hooking `OnShow`, printing
   at `PLAYER_LOGIN`.

Shared edits allowed: one dispatch each from `Ctx::call`, `Ctx::member` and
`Ctx::read` (for globals) into `api.rs`; the `OnHandler` parameter typing in
`selu-check/src/lib.rs`; a widget type variant and a "multiple results" flag
on function types in `selu-check/src/types.rs`; the lowering's call path
for the single-value parentheses; read-only accessors in
`crates/selu-manifest`; SPEC-V1 2.4's wording.

### T2: `has(...)` and several targets

DESIGN.md section 3 ("Different rules per version"), SPEC-V1 section 6.
Acceptance:

1. **Strictest target:** with several targets in `selu.toml`, every call is
   checked against every target; an error names the targets that lack the
   API.
2. **`has(X)`** for a manifest function, namespace, global or event is a
   `boolean`. Inside `if (has(X)) { ... }` (and the `else` of
   `if (!has(X))`, and after `if (!has(X)) return;`) uses of `X` are allowed
   even where some targets lack it; outside they are errors with the fix
   "guard it with `has(...)`".
3. **Per-target builds** (the default, one folder per target as today):
   `has(X)` is folded at compile time and the dead branch is removed.
4. **Shared build:** `[build] layout = "shared"` in `selu.toml` makes one
   addon folder for all targets: one `.toc` whose `## Interface:` lists every
   target's interface number comma-separated (as the owner's addon does),
   and `has(X)` becomes a runtime check (`C_Foo ~= nil and C_Foo.Bar ~= nil`,
   or the event's availability). Document it in `targets.md`; unknown keys in
   `[build]` are errors.
5. Example `examples/multi-target/` with a `has(...)` branch, built for
   `retail` and `forever`; its `expected.txt` holds the simulator output for
   one of them (say which in the README).

Shared edits allowed: `has` as a built-in through the existing
`builtin_call` dispatch (one line in `wow.rs` on both sides, logic in
`targets.rs`); narrowing hooks for `has` in the checker's condition
handling (one dispatch line); checking against several manifests in the
checker's entry points; `crates/selu-cli/src/main.rs` and `generate_toc` for
the build layout.

### R: readability

Rule three. Every item gets a test on the exact generated Lua (full-string
comparisons for small programs). The behaviour and every `expected.txt`
stay unchanged.

1. **Reference Lua for every example:** `examples/<name>/expected.lua` holds
   the generated Lua (one file per module, named like the output), and
   `crates/selu-cli/tests/examples.rs` compares it. A regeneration switch
   for tests (an environment variable) writes them anew; document it.
2. Long table literals break over several lines (one field per line when the
   one-line form passes 100 columns).
3. `filter`/`map`/`forEach`/`some`/`every`/`find`/`reduce` with an inline
   arrow inline the arrow's body into the loop, with no copies of receiver
   and callback into extra temporaries.
4. `m.set(k, (m.get(k) ?? 0) + n)` and similar map updates: at most one
   temporary.
5. `x += a.b * c` and `t.f += v` on stable receivers: no `do` block or
   temporaries (`x = x + a.b * c`).
6. No needless brackets: `return (f()), x` where Lua already truncates.
7. `log`/`L` text without placeholders is a plain string (no
   `string.format`); no `tostring` around values already typed `string`.
8. Named reserved keys in the addon table instead of `[true]`/`[false]`:
   `__selu_events`, `__selu_wow` (module names starting with `__selu` are
   already rejected).
9. One file prelude: the runtime captures of saved data (`__selu_saved_*`)
   and classes (`__selu_setmetatable`) come from one mechanism that emits
   each capture once per file, right after the loader line.
10. The event-frame prologue: shorter and only once per addon (D1 shares it;
    check the per-file guard reads clearly).

Shared edits allowed: any lowering file (`crates/selu-lower/src/*.rs`) for
these items, the examples harness, and `docs/lang/output.md`. Nothing in
`selu-check` or `selu-syntax`. Packages T1 and T2 run in parallel: T1
touches the call path for single-value parentheses, and T2 the `has`
lowering in `targets.rs`. Stay out of those.

### Merge order (round 4)

R first if it finishes first (it only touches output), then T1, then T2 (T2
builds on T1's typed manifests for the strictest-target errors).

## Merge order and reviews (round 1)

A merges first (B's `for...of` uses its `continue` helper), then B and C in
either order. Every branch gets an independent review before merge; the
parent does the final check and merges. Findings outside a package become
new issues, not more commits on the branch.

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

## M2: manifests for every client (owner, 2026-11-04)

The owner wants Selu to support every client through the source in
`E:/Development/wow-ui-source` ("support all versions via the decompiled
interface files"). M1's generator already reads any branch; M2 generates and
commits the remaining flavours. Owned: `manifests/*/`, the generator crate,
`crates/selu-manifest/`, `docs/manifests.md`, its tests. Shared edits: none
expected (flavour names come from manifests, never from compiler code); ask
first otherwise.

Acceptance:

1. Manifests for `classic-era` (branch `classic_era`, 1.15.9), `classic-anniversary`
   (`classic_anniversary`, 2.5.6), `classic-titan` (`classic_titan`, 3.80.2)
   and `classic` (`classic`, 5.5.4), from pinned commits, next to `retail`
   and `forever`. Target names follow Blizzard's branch names. Refresh
   `retail` and `forever` only if their pinned commits changed meaning;
   otherwise leave them.
2. Interface numbers from the version convention (`1.15.9` -> `11509`),
   recorded as such, with the note that the owner's shipping addon
   (`RennersAddons.toc`: 11509, 16001, 20506, 38002, 50504) loads with
   exactly these numbers.
3. Undocumented globals (`source` evidence) per flavour as for retail. The
   owner's frequently used globals (`docs/TOOLKIT-SURVEY.md`) checked on
   every flavour; list the ones that don't resolve, per flavour, with every
   location searched.
4. Every example builds against every flavour (a test), and `selu targets`
   lists them all.
5. `docs/manifests.md`: a table of flavours, branches, versions,
   interfaces and pinned commits, and how to add PTR/beta manifests on
   demand (not committed).
6. Report the sizes; plain JSON in git (it deltas well) unless the total
   exceeds 40 MB, then ask.

## Round 5: proof port, named types, more manifest data

Round 5 runs P, L and M3 side by side (owner's goal: port Renner's Toolkit;
the owner was away and the parent decided the scope). "How every package
works" applies unchanged.

| Package | Owned files | Codes |
|---|---|---|
| P proof port | `proof/junksell/` (a Selu project), `docs/proof/junksell.md` | none |
| L named types | `selu-check/src/aliases.rs` (new), `crates/selu-cli/tests/aliases.rs`, `docs/lang/types.md`, `examples/named-types/` | `SELU0520`-`0539` |
| M3 manifest data | the generator crate, `crates/selu-manifest/`, `manifests/*/`, `docs/manifests.md` | `SELU02xx` |

### P: proof port of `JunkSell`

SPEC-V1 section 11 step 8. Source: `E:/Development/RennersAddons/Modules/JunkSell.lua`
(read-only; the owner's MIT-licensed addon, which also needs the shared
helpers it calls from `Core.lua`). Acceptance:

1. `proof/junksell/` is a Selu project that reimplements the module's
   behaviour (selling junk at a merchant, auto-repair, the protected-item
   buyback, its settings and its English and German strings) as idiomatic
   Selu, targeting every flavour in `manifests/` with the shared layout,
   as the owner's addon does. A header comment names the source file and
   commit it was ported from, and its MIT licence.
2. **Don't change the compiler.** Every gap, bug, false error, missing API,
   missing manifest entry or readability problem met while porting goes into
   `docs/proof/junksell.md` as a numbered entry with a minimal Selu
   reproduction, what happened, what was expected, and a workaround if one
   was used. This list is the main result.
3. It builds for all six flavours (record the warnings), and a simulator
   test drives a merchant visit with mocked merchant and container APIs
   (`Sim::eval` stubs) and checks what got sold and repaired.
4. A comparison in `junksell.md`: lines of Selu versus the original Lua,
   size and readability of the generated Lua, the `pcall`/`type(...)`
   guards the original needed versus what `has(...)` and the manifests
   replace, and anything the port can't express yet.

Shared edits: none. Ask before touching anything outside `proof/` and
`docs/proof/`.

### L: named types

SPEC-V1 2.1 (`interface`, object types, `unknown` with `typeof`).
Acceptance:

1. `type Name = <type>;` and `interface Name { field: T; optional?: U; method(a: A): R; }`
   at the top level of a module, usable in every written type; generic
   parameters on both (`type Pair<T> = [T, T];`), constraints with
   `extends`. `export type`/`export interface` and importing them work with
   D1's modules. Interfaces are structural (as object types today); an
   interface may `extends` other interfaces. Recursive types
   (`interface Node { next: Node? }`) work.
2. `unknown`: a value of type `unknown` can only be passed around, stored or
   compared with `==`/`!=`, until narrowed with `typeof x == "string"`
   (`"number"`, `"boolean"`, `"table"`, `"function"`), `x != null`, or an
   assignment to a declared type with an explicit check. Untyped game values
   stay untyped (not `unknown`): `unknown` is for code that opts in, and for
   `*.d.selu` declarations later.
3. Diagnostics print type names, not their expansion, where a name exists.
4. Example `examples/named-types/`.

Shared edits allowed: parser and AST for `type`, `interface`, generic
parameter lists and `typeof` (you own those additions; they are new
syntax nobody else touches this round); one dispatch each in the checker's
block predeclaration, statement match and written-type resolution; the
`Type` representation in `types.rs` for named and generic types (keep
existing variants working); module export/import of types in
`modules.rs`.

### M3: more manifest data

Acceptance:

1. Named frames and regions created by Blizzard's XML in each branch
   (`<Frame name="MerchantFrame" ...>`, buttons such as
   `QuestFrameAcceptButton`, `WorldMapFrame`), including `$parent` name
   resolution, as globals with their widget type, evidence `source`, names
   only. Use a real XML parser.
2. Font objects (`GameFontNormal`, `GameFontHighlightSmall`...) from XML
   `<Font>`/`<FontFamily>` definitions as globals of a font-object type.
3. Global data tables assigned at top level in Blizzard Lua (`SOUNDKIT`,
   constants tables), as globals with field names where they are literal
   tables (values only for numbers and strings), evidence `source`.
4. Templates: members that XML templates (`BackdropTemplate`,
   `UIPanelButtonTemplate`, `GameTooltipTemplate`...) add through mixins
   (`mixin="BackdropTemplateMixin"`), recorded per template, so the
   checker can later type `CreateFrame("Frame", ..., "BackdropTemplate")`.
   The checker change is not part of M3; record the data and its schema.
5. Regenerate all six manifests from their pinned commits; deterministic;
   report counts and size (ask the parent if the total passes 45 MB).
   The owner's survey globals that were unresolved (`MerchantFrame`,
   `WorldMapFrame`, `QuestFrameAcceptButton`, `GameFontNormal`, `SOUNDKIT`,
   `ContainerFrameItemButton_Update`) resolve, or are listed with every
   location searched.

Shared edits: none outside the manifest crates and data. The data source
`E:/Development/wow-ui-source` is read-only as before.

### Merge order (round 5)

M3 and L in any order; P last (it reports against `master` at its base and
is merged as documentation plus the proof project).

## Round 6: errors, string patterns, timers

From the proof port's open gaps (`docs/proof/junksell.md`: 1, 4, 7). The
parent decided the scope while the owner was away. "How every package
works" applies unchanged, including the example rule.

| Package | Owned files | Codes |
|---|---|---|
| E errors | `selu-check/src/errors.rs`, `selu-lower/src/errors.rs` (new), `crates/selu-cli/tests/errors.rs`, `docs/lang/errors.md`, `examples/errors/` | `SELU0540`-`0559` |
| S string patterns | `selu-check/src/patterns.rs`, `selu-lower/src/patterns.rs` (new), `crates/selu-cli/tests/patterns.rs`, `docs/lang/patterns.md`, `examples/item-links/` | `SELU0560`-`0579` |
| Q simulator timers | `crates/selu-sim/src/timers.rs` (new), its tests, `proof/junksell/tests/` | none |

### Decision: string patterns (second opinion from Sol)

Plain strings stay literal everywhere. A Lua pattern is written as a
`luaPattern` tagged template: `` s.match(luaPattern`item:(%d+)`) ``. The tag is
a compiler intrinsic (no runtime object, no interpolation), checked at
compile time. `match` without captures gives `string?`; with captures it
gives multiple results (each `string?`, or `number?` for a position capture
`()`, 0-based), first value in single use, all by destructuring (the round-4
rule). No match makes every result `null`. `search` gives a 0-based
`number?`. `matchAll` iterates lazily (usable in `for...of`), yielding the
same capture types, non-null per turn; a leading `^` is an error there.
`replace` replaces once and `replaceAll` replaces every match; both return
multiple results `(string, number)`. The replacement is a literal string
(`%` escaped) or a typed callback receiving the captures. Regex-looking
syntax (`|`, `{n}`, `\d`, `(?:`) is a Selu error with a Lua-pattern
suggestion (`\d` -> `%d`; write `%|` or `%{` for the literal characters).
`%f[set]` is allowed only if every supported target and the simulator agree
(check). Document that Lua patterns can backtrack badly.

### E: `try`/`catch`/`finally`/`throw`

SPEC-V1 2.5. Acceptance:

1. `try { } catch (e) { } finally { }` (with `catch` and/or `finally`)
   lowers to `pcall` (or `xpcall` where a traceback is needed), readable,
   with no closure per execution of a hot loop body if avoidable (document
   the cost when not).
2. `return`, `break` and `continue` inside `try`, `catch` and `finally` work
   as in TypeScript, including `finally` running on every exit path and a
   `return` in `finally` overriding (or a Selu error forbidding it, the
   simpler choice; document).
3. `throw new Error("...")` lowers to `error(...)` with the Selu source
   position (`file:line:`), and `throw` of any other value is an error
   unless it's an `Error`. `catch (e)` gives `e` the type `Error` with a
   `message: string` (non-`Error` values from Lua code arrive wrapped).
   `e.message` works for errors from Lua (`error("x")`) too.
4. Errors that escape an event handler keep reaching the game's error
   handler with a source-mapped position (SPEC 2.5 last point; test with
   the simulator's error capture).
5. Example `examples/errors/`.

Shared edits: parser and AST for `try`/`catch`/`finally`/`throw`, and the
`Error` type and `new Error(...)` (one dispatch where `new` is handled), the
statement dispatch in checker and lowering, `always_exits` for `throw`,
and the loop-context interplay with A's `continue` machinery in
`loops.rs` (one hook).

### S: string patterns

Acceptance: the decision above, implemented, plus:

1. The pattern checker: balanced captures and sets, capture limit (32),
   `%1`-`%9` back-references only to closed captures, `%bxy`, trailing `%`,
   `%f` per the decision; spans pointing at the offending character.
2. Lowering to `string.match`/`string.find`/`string.gmatch`/`string.gsub`
   with no tables or adapter closures on the plain paths; position captures
   and `search` converted to 0-based; `matchAll` without per-turn garbage
   beyond what `gmatch` itself allocates.
3. Tests for: item links with the WoW `|H...|h` escapes, failure, mixed
   string and position captures, `%` in replacements, empty-match
   iteration, anchors, and multiple results in every position.
4. Example `examples/item-links/`: parsing item IDs and names from item
   links and counting words in a chat line.

Shared edits: the `luaPattern` tag in `selu-check/src/wow.rs`'s
tagged-template dispatch (one line) and the string method dispatch in
`data.rs` (one line each side). No parser changes needed.

### Q: simulator timers

Acceptance:

1. `C_Timer.After`, `C_Timer.NewTimer`, `C_Timer.NewTicker` (with
   iterations) and `:Cancel()`, plus frame `OnUpdate` scripts receiving
   `elapsed`, driven by the simulated clock: `Sim::advance_time(seconds)`
   runs every callback that becomes due, in due order, with `GetTime()`
   reporting the due time during each callback. Callbacks scheduled during a
   callback run in the same advance if due.
2. Errors in callbacks go through the simulator's error capture without
   stopping other timers.
3. The proof port's test (`proof/junksell/tests/`) uses the real simulator
   timers instead of its own stub, and builds the project itself instead of
   requiring a prior `selu build` (the current test silently depends on
   one).
4. Unit tests for ordering, cancel, tickers, nested scheduling and
   `OnUpdate` throttling.

Shared edits: the hook in `Sim::new` and `advance_time` in
`crates/selu-sim/src/lib.rs`; `proof/junksell/tests/` and the proof gap
test's status for gap 7.

### Merge order (round 6)

Q first, then E and S in either order.

## Round 7: frame templates, secret values and combat, leaner output

From the toolkit survey and DESIGN.md section 3 (owner: "next round",
2026-11-04). Codex is out of usage until 2026-11-08, so this round runs on
Claude agents. "How every package works" applies unchanged, including the
example rule.

| Package | Owned files | Codes |
|---|---|---|
| F frame templates | `selu-check/src/frames.rs` (new), `crates/selu-cli/tests/frames.rs`, `docs/lang/frames.md`, `examples/templated-frames/` | `SELU0580`-`0599` |
| G secret values and combat | `selu-check/src/restrictions.rs`, `selu-lower/src/restrictions.rs` (new), `crates/selu-cli/tests/restrictions.rs`, `docs/lang/restrictions.md`, `examples/combat-safe/` | `SELU0600`-`0619` |
| K leaner output | `selu-lower/src/runtime.rs` (new), `crates/selu-cli/tests/runtime.rs`, `docs/lang/output.md` | none |

### F: frame templates

The manifests record XML templates with their mixins and inheritance
(`templates`, `dataGlobals`, M3; `docs/manifests.md`). Acceptance:

1. `CreateFrame("Frame", name, parent, "BackdropTemplate")` (and a
   comma-separated template list, as the client accepts) returns the widget
   type plus every member the templates add through mixins, following
   template inheritance. Members from a template the target lacks are a
   missing-API error naming the target, like other APIs; with several
   targets, the strictest wins (T2's rule).
2. Named frames from Blizzard's XML (`MerchantFrame`, `GameTooltip`) get
   their templates' members too, and `$parent`-named children resolve as
   fields where M3 recorded them.
3. Unknown template names are a warning (manifests can't be complete; the
   T1 warning policy), and a string that isn't a literal gives the plain
   widget type.
4. Mixin members are untyped unless the manifest has a signature (no
   invented signatures); `strict_api` keeps its meaning.
5. Example `examples/templated-frames/`: a backdrop panel and a button made
   from `UIPanelButtonTemplate`, run in the simulator (extend the
   simulator's widget model only as far as the example needs, and say so).

Shared edits: one dispatch in `selu-check/src/api.rs` where `CreateFrame`'s
result and global widget types are computed; reading the manifest's template
data in `crates/selu-manifest/` if a loader accessor is missing; the
simulator's widget stubs for the example.

### G: secret values and combat lockdown

DESIGN.md section 3, items 2 and 3. Acceptance:

1. **Secret values.** A function whose manifest entry has `returnsSecret`
   (or a `SecretWhen...`/`SecretIn...` restriction) on any built target
   returns `T?` for the affected results (strictest target wins), so the
   author handles "no value" with `??` or `if`, like `undefined`. The
   lowering emits the guarded read the client needs: research how the
   client exposes secret values (Blizzard's UI Lua in the read-only
   `E:/Development/wow-ui-source`, e.g. `issecretvalue` / `canaccessvalue`
   or equivalent in the 12.x branches; existing refs only, never check out
   or modify it) and document the evidence. Where no runtime test exists on
   a target, no guard is emitted there. Untyped results stay untyped.
2. **Combat lockdown.** Calls the manifests mark as protected (find out
   which flag means it: `HasRestrictions`, `SecureOnly`, `restrictions`
   entries; document the mapping and the evidence) in code that can run in
   combat are queued automatically and run after `PLAYER_REGEN_ENABLED`,
   when `InCombatLockdown()` is true at the call; outside combat they run
   directly. Debug builds (`selu build --debug` if it exists; otherwise a
   `[build] debug = true` setting) log each deferral once. Calls whose
   result is used can't be deferred: those are an error with a help line.
   Document which contexts the compiler treats as "combat possible"
   (default: everything except code proven to run at setup).
3. **Hardware events.** Functions that need a hardware click (if the
   manifests record it; otherwise list the gap and stop) called outside a
   click handler are an error naming the target and the context.
4. Simulator: `InCombatLockdown`, entering and leaving combat
   (`Sim::set_combat(bool)` firing `PLAYER_REGEN_DISABLED`/`ENABLED`), and
   secret values for tests.
5. Example `examples/combat-safe/`.

Shared edits: one dispatch in the checker's call handling (`api.rs`) for
results and call contexts, one in the lowering's call lowering, the
simulator hooks (`crates/selu-sim/src/lib.rs`, `wow_api.rs`), and the build
setting if needed (`selu-cli`).

### K: leaner output (proof gap 8)

The JunkSell proof's generated Lua is dominated by runtime scaffolding
(`docs/proof/junksell.md`, gap 8): the event dispatcher, the saved loader,
the error runtime (about 45 lines per file using `try`), the `L` and log
helpers, optional-value temporaries. Acceptance:

1. Shared runtime pieces are emitted **once per addon**, in a generated
   first file (for example `Selu.lua`, first in the `.toc`), only the pieces
   the addon uses, and each module file aliases what it uses with one
   `local` line per helper. A one-file addon may keep everything in one file
   if that's smaller; decide and document.
2. Unify the file-prelude mechanisms (HANDOFF "Next"): one ordered prelude
   per file, built by one function.
3. Measure before and after: line counts of every example's
   `expected.lua` and of `proof/junksell`'s output; the proof's main file
   should shrink substantially. Report the numbers in `docs/lang/output.md`.
4. Fewer temporaries where a value is used once and has no side effects
   (the purity rule in `purity.rs` decides), without changing evaluation
   order.
5. Behaviour stays identical: every example's `expected.txt` unchanged;
   `expected.lua` files regenerate (`SELU_UPDATE_LUA=1`) and each diff is
   reviewed by you for readability.

Shared edits: the runtime emission points in the lowering crate (event
dispatch, saved loader, errors, `L`/log, `has`), the CLI's `.toc` and file
writing, the examples' `expected.lua` files.

### Merge order (round 7)

F and G in either order, K last (it rebases and regenerates every
`expected.lua`, including F's and G's new examples).

## Round 8: simulator core (groundwork for `selu test` and Studio)

Architecture and decisions: `docs/SIMULATOR.md` (read it first; it is part
of every brief below). Owner, 2026-11-04: "make the groundwork for the
simulator, studio and debug commands so Sol can take over". Starts after
round 7 is merged. "How every package works" applies, with one change: these
packages add at least one test addon under `crates/selu-sim/tests/addons/`
(plain Lua, written by hand like a real addon) instead of an `examples/`
entry, unless the package also adds language-facing features.

| Package | Owned files | Codes |
|---|---|---|
| S0 core contract (first, alone) | `crates/selu-sim/src/core.rs` (new), `crates/selu-sim/src/lib.rs`, its tests | none |
| S1 frames and layout | `crates/selu-sim/src/frames/` (new: `mod.rs`, `arena.rs`, `layout.rs`, `widgets.rs`, `input.rs`), its tests | none |
| S2 sessions and world | `crates/selu-sim/src/session.rs`, `loader.rs`, `world.rs`, `trace.rs`, `snapshot.rs` (new), its tests | none |
| S3 source maps | `crates/selu-sourcemap/` (new crate), `crates/selu-cli/tests/sourcemap.rs`, `docs/lang/output.md` (section "Source maps") | none |

**Groundwork done by the parent (2026-11-04):** S0 is implemented, and S1 to
S5 each start from a compiling skeleton with tests; see the table "Code
groundwork" in SIMULATOR.md. So round 8 is S1, S2 and S3, launched
together (S1 before S2 at merge time). Each package **extends** the
skeleton in its files rather than starting over; the shapes in `core.rs`,
`Session`, `selu_test::Report` and `selu_debug::{Command, Update}` are the
contract, and changing one goes to the parent first. Two traps found while
building it: Lua 5.1 hooks are per coroutine (budget code inside addon
coroutines is not counted yet: S2 wraps `coroutine.create`), and a test
that can hang must run under a timeout (a runaway loop once spun a test
binary for ten minutes before the hook fix).

### S0: core contract and wiring (done in the groundwork)

SIMULATOR.md decisions 5, 6, 15, 16, 17 and 18. Small and first, so S1 and
S2 can then work in parallel without touching the same files. Acceptance:

1. `core.rs`: the types and traits S1 and S2 share, with doc comments:
   `RegionId` (generational), the install context handed to each module
   (Lua handle, shared state, logging), the script-dispatch entry point with
   its flush points (decision 16), the layout-resolution call, the immutable
   snapshot view types (decision 8, serde), the trace record type, the
   session generation number, and the instruction-budget count hook
   (decision 17), installed once.
2. `lib.rs` wired so that `Sim` delegates to a `Session` skeleton, and the
   frames module (still the current Lua region model behind the new
   `RegionId` interface, so nothing changes yet) and the world module plug
   in through `core.rs`. The files S1 and S2 will own exist as empty modules
   with their owner noted at the top.
3. A runaway-loop test (hitting the budget is a logged error, not a hang),
   and the existing `Sim` API, every example and the proof test pass
   unchanged.

Shared edits: none outside `selu-sim`.

### S1: frames and layout in Rust

SIMULATOR.md decisions 3 and 4. Acceptance:

1. The region arena: Frame, Button, CheckButton, EditBox, StatusBar, Slider,
   ScrollFrame, Cooldown, Texture, FontString, MaskTexture (SPEC 8), created
   by `CreateFrame`/`CreateTexture`/`CreateFontString`, parented, named
   (named frames become globals; `$parent` names resolve), with per-object
   Lua tables for addon fields. The current Lua region model
   (`regions.rs`) is replaced; every existing test and example keeps
   passing unchanged.
2. Methods per widget type from the manifest's `widgets` and `inherits`;
   modelled methods implemented in Rust; the rest log `not simulated` once
   and return the documented empty value. List which methods are modelled.
3. Layout as decision 4: points, `SetAllPoints`, `ClearAllPoints`, size from
   two anchors, scale and effective scale, strata, levels, layers and
   sublevels, alpha (effective alpha), shown/visible, `IsVisible`,
   `GetRect`/`GetLeft`/..., anchor cycles logged. Tested against hand-worked
   numbers, not against the implementation.
4. Scripts: `SetScript`, `GetScript`, `HookScript`, `HasScript` for the
   widget's documented script types; `OnShow`/`OnHide`/`OnSizeChanged` fired
   at the right moments; input `click`, `hover` (`OnEnter`/`OnLeave`),
   `drag` (`OnDragStart`/`OnDragStop`, `StartMoving` changing points) driven
   from Rust with hit testing in draw order and `EnableMouse`.
5. Implements S0's snapshot view and layout-resolution contract from the
   arena; region identity, fields and lifetime as decision 15; layout
   callback timing as decision 16; cursor, capture, focus and text
   measurement as decision 19.

Shared edits: replacing `regions.rs`; the timers module's `OnUpdate`
visibility check (use the arena). Not `lib.rs` (S0 wired it); a needed
contract change goes to the parent first.

### S2: sessions, loader, world, trace and snapshots

SIMULATOR.md decisions 1, 5, 6 (the API shape; the worker thread itself is
Studio's), 7, 8 and 13. Acceptance:

1. `Session` with `SessionConfig` and the methods in decision 5 except
   input (S1) and debugging (round 10). `Sim` becomes a thin wrapper over
   `Session`; existing callers keep compiling.
2. The `.toc` loader for any addon folder, the boot sequence, several addons
   in one session, `## SavedVariables(PerCharacter)` loaded before
   `ADDON_LOADED` and written by `reload()`, which restarts the VM (test
   that a migration runs after a reload; reuse `saved.rs`).
3. World state for SPEC 8's units (`player`, `target`, `focus`, `party1-4`,
   `raid1-40`, `pet`: name, class, level, health, power, auras, in combat),
   money and a simple bag model, each setter firing the client's events
   (`UNIT_HEALTH`, `UNIT_AURA`, `PLAYER_MONEY`, `BAG_UPDATE`...) with
   payloads from the manifest. The `Unit*` API reads it. Secret values and
   combat from round 7 move into `world.rs` if they aren't there already.
4. The trace ring buffer and snapshots through S0's snapshot view. S2 may
   develop against a fake arena, but acceptance needs a real non-empty tree:
   if S1 isn't merged yet, the parent merges S1 first and S2 adds the
   integration test after rebasing.
4a. Loading scope, flavour `.toc` files, dependencies, load-on-demand and
   the XML "not supported yet" error as decision 18; saved-data namespacing
   and `run_until_idle` bounds as decision 20; test replay data as
   decision 22.
5. Determinism: seeded `math.random`, simulated `time()`/`date()`.
6. A plain Lua test addon under `crates/selu-sim/tests/addons/` that loads
   through its `.toc`, uses saved data across a reload, reacts to unit
   events and is checked through the trace.

Shared edits: `saved.rs`, `wow_api.rs`; the proof test and the CLI's tests
only if the wrapper can't keep them unchanged. Not `lib.rs` (S0 wired it).

### S3: source maps

SIMULATOR.md decision 9. Acceptance:

0. Already in the groundwork: the crate with the format, reader, writer,
   both lookups and `map_lua_error`, tested. S3 documents it, replaces the
   lowering's own `selu_lower::SourceMap` and its `.lua.map.json` files
   with it (keep the maps under `dist/.selu/`, out of the addon folder),
   and does items 2 and 3.
1. `crates/selu-sourcemap`: the format of decision 23 (documented in `docs/lang/output.md`),
   writer and reader, lookups both ways (generated line to source position,
   source line to generated lines), with unit tests.
2. The lowering records the source span for every emitted statement; the
   printer records generated lines; `selu build` writes `Name.lua.map`
   beside every generated file (not in the `.toc`). Runtime-only lines
   (shared runtime, preludes) map to no source.
3. A test that for every example each `.selu` statement line maps to a
   generated line that contains the corresponding code, and that a runtime
   error in generated Lua maps back to the right `.selu` line.
4. The error panel's need: a function `map_lua_error(message, maps)` that
   rewrites `[string "Name.lua"]:42:` or `Name.lua:42:` positions into
   `file.selu:7:` (used by `selu test` and Studio later).

Shared edits: the lowering's statement emission and the printer
(`selu-lower/src/lib.rs`, `lua.rs`), the CLI's file writing, the workspace
`Cargo.toml`, the examples harness (ignore `.map` files when comparing).

### Merge order (round 8)

S0 first. Then S1 before S2; S3 at any time (it may start alongside S0,
since it doesn't touch `selu-sim`).

## Round 9 and 10 (outline, briefed after round 8)

- **S4 `selu test`** (SIMULATOR.md decision 11; extends `crates/selu-test`): `*.test.selu`, `describe`,
  `test`, `beforeEach`, `expect` matchers from SPEC 8, `sim.*` helpers
  over the Session API, spies, human and `--json` output, source-mapped
  failures, exit codes. Codes `SELU0620`-`0639`.
- **S5 Studio shell** (decisions 2, 6, 12): crate `selu-debug`, binary
  `selu-studio` (the worker, protocol, headless mode and the `selu debug`
  launcher exist; S5 adds the eframe window over them); game screen from
  snapshots, frame inspector with anchors drawn, event timeline with
  scrubbing, console, world panel, rebuild on save. Manual check list plus
  snapshot-level tests (no screenshot tests in v1).
- **S6 debugger** (decision 10): breakpoints in `.selu` and `.lua`, stepping,
  stack, locals, console in the paused frame, "fire this event now" while
  paused.
- **S7 Studio panels**: saved data view with "/reload", target switch, error
  panel with links, "not simulated" panel.

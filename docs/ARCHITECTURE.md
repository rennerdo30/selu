# Architecture

Selu is a Rust workspace (edition 2024) under `crates/`. Data flows one way:

```
.selu source
  -> selu-syntax    lexer + recursive-descent parser -> AST (with spans)
  -> selu-check     scopes, types, language rules, target checks
                    -> diagnostics + a type for every expression
  -> selu-lower     Selu AST -> Lua syntax tree -> printed Lua 5.1,
                    source map, generated .toc
  -> dist/<Name>/  the addon folder
  -> selu-sim       the same Lua, loaded in TOC order into a Lua 5.1 VM
```

`selu-cli` (the `selu` binary) drives the pipeline. All crates report
problems as `selu-diagnostics` values: a stable code (`SELU0001`...), a
primary span, a plain message, and optional help/fix text, rendered for the
terminal with codespan-reporting.

Code ranges: `01xx` syntax, `02xx` manifests and targets, `03xx` the
language core (scopes, types, the per-area checks; `SELU0390` marks a
construct that isn't built yet), `04xx` modules, classes and the WoW
built-ins (ranges per package in `docs/WORK-PACKAGES.md`).

## Crates

| Crate | Job |
|---|---|
| `selu-diagnostics` | `Diagnostic`, `Span`, terminal rendering. No other Selu deps. |
| `selu-syntax` | Hand-written lexer and recursive-descent parser for all of SPEC-V1 section 2 except classes and modules (DESIGN.md section 9). Semicolons are required; a missing one is `SELU0101` with a fix suggestion. Every expression has an `id`; `visit.rs` walks any node's children. |
| `selu-manifest` | Serde types for a target manifest, compiled/JSON loading, `flavour@version` parsing and partial-version resolution to the newest matching manifest. Per-version differences (Lua dialect, availability, restriction flags, `returnsSecret`, evidence `documented`/`verified`) live in manifests, never in compiler code. |
| `selu-check` | Semantic pass: block scopes and name resolution (below), and a global call or `on EVENT` the target lacks is an error naming the target (`SELU0301`/`SELU0302`). `rules.rs` holds target-independent rules (Lua keywords and `__selu` names, where assignments may appear). One file per language area (`loops.rs`, `data.rs`, `functions.rs`, `wow.rs`; see `docs/WORK-PACKAGES.md`). |
| `selu-lower` | Builds a Lua syntax tree (`lua.rs`) and prints readable Lua 5.1 (2-space indent, local names preserved, only needed brackets, `a.B(x)` becomes `a:B(x)`; manifest namespace functions like `C_Foo.Bar(x)` stay plain calls), a generated-line -> source-span map, shared event-frame dispatch for `on EVENT`, and the `.toc` whose `## Interface:` comes from the manifest. Same per-area files as the checker. |
| `selu-sim` | `mlua` (Lua 5.1, vendored): boot a VM, load files in TOC order, simulated clock (`GetTime`), captured `print`, minimal `CreateFrame` (`RegisterEvent`/`UnregisterEvent`/`SetScript`/`Show`/`Hide`/`IsShown`), and `fire(event, ...)` from Rust. Unmodelled globals and frame methods log `not simulated: X` once and return a no-op. |
| `selu-cli` | `selu build` (working), `selu targets` (lists known manifests), `selu test`/`debug`/`probe` (honest stubs, exit non-zero). |

## Manifests and project files

- A project is a folder with `selu.toml` (`[addon]` with `name`, `targets`) and `.selu`
  files under `src/` (or the project root). `selu build` writes
  `dist/<Name>/` (one folder per flavour when several targets are built).
- Manifests are generated `.selum` files (versioned MessagePack+zstd; see docs/manifests.md) (not hand-edited config; those are TOML) found via `--manifest-dir`, `$SELU_MANIFEST_DIR`,
  or the nearest `manifests/` folder walking up from the project.
  `manifests/fixtures/` holds one small hand-made **test fixture**
  (`fixture: true`); real manifests will be generated, never hand-written.
- A build prints the resolution it used, e.g.
  `target forever@1.0 -> 1.0.0.1000`. An unknown build is an error listing
  the closest known builds; `@installed` is parsed but not implemented yet.

## Scopes and names (decided while building the language core)

- `let`/`const` are block-scoped as in TypeScript; an inner block may
  shadow an outer name. Function parameters share the body's scope, so
  `function f(a) { let a; }` is a redeclaration (`SELU0312`).
- A name can't be used before its declaration in the same function
  (`SELU0313`). Function declarations are **not** hoisted for direct calls:
  `f(); function f() {}` at the same level is an error ("move `function f`
  above this line"). From inside a function body that runs later, any name
  of an enclosing block may be used, whatever the textual order. The
  lowering then emits one `local a, b` at the top of that block and turns the
  declarations into assignments, because Lua would otherwise read a global.
- Reading an undeclared name is `SELU0310`. Declared means: a local, a
  `global` of this file, or a game global from the target manifest
  (functions, the manifest's `globals` list, and namespace roots such as
  `C_Timer`). Non-function game globals (`UIParent`, `SlashCmdList`...) live
  in the manifest's `globals` list, never in compiler code.
- Writing an undeclared name is `SELU0314` ("writing a global needs
  `global`"); writing a game global is `SELU0315`; `const` and function
  names can't be assigned (`SELU0311`).
- `global name = value;` is a statement allowed only at the top level of a
  file (`SELU0316`), always with a starting value. `global` is a contextual
  keyword, so it is still a valid variable name.

## Types (basic checker)

- `selu-check` gives every expression a type (`number`, `string`,
  `boolean`, `null`, `T?`, function types) and returns them in a
  `TypeTable` keyed by the parser's expression ids; `selu-lower` reads it
  (for example `+` on strings becomes `..`).
- Declarations infer their type from the value. `let x;` and `let x = null;`
  need a written type; `let x: T;` without a value is only allowed for `T?`.
  Function return types are inferred from the `return`s (`T?` when some
  paths return nothing) or written as `function f(): T`.
- Conditions take a `boolean` or a `T?` (`SELU0328` otherwise). `a && b`
  with a `T?` operand is condition-only (`SELU0327` as a value), because the
  Lua result isn't a boolean.
- `T?` narrows to `T` through `x != null`, `x == null` (else branch), a bare
  `x`, `!`, `&&`/`||` (also inside the right operand) and early `return`. An
  assignment that may be null undoes narrowings up to the declaration.
  Narrowings of a `let` don't reach into nested functions.
- `===`/`!==` are syntax errors (`SELU0109`) with the fix `==`/`!=`.

## Language areas

The detailed rules of each language area, with the decisions taken while
building it, are in `docs/lang/`: `control-flow.md` (loops, `continue`,
`? :`, `+=`), `data.md` (arrays, objects, `Map`/`Set`/`WeakMap`, template
strings, `for...of`, the string and `Math` libraries) and `functions.md`
(closures, default and rest parameters, destructuring, tuple returns, `.`
versus `:` calls).

## Lowering: the Lua tree and temporaries

- The lowering never builds Lua text. It builds `lua.rs` nodes and the
  printer decides brackets from Lua's precedence (`a + b * c`, `(a + b) * c`,
  `a - (b - c)`; `..` chains print flat because they only join strings),
  writes `t["end"]` for keyword fields, and prints a short single-statement
  `if` on one line.
- Lowering an expression may need statements first (a temporary for
  `f() ?? g()` inside a call). `Lowerer::expr(e, pre)` pushes them into
  `pre`; `seq` lowers operands in order and stores earlier, impure values in
  temporaries when a later operand needs statements, so evaluation stays
  left to right; `finish` wraps a statement and its temporaries in
  `do ... end` (declarations keep theirs in the block, since `do` would hide
  the declared name). Temporaries are `__selu_t1`, `__selu_t2`... (user
  names starting with `__selu` are `SELU0318`).
- `&&`, `||` and `??` keep short-circuiting: inline `a and b` / `a or b`
  when the right side needs no statements, else an `if` on a temporary.
  `a ?? b` is inline `a == nil and b or a` only when `a` is a plain name or
  literal and `b` a literal that is never false or nil. A `??` chain stored
  in a variable uses the variable itself (`local x = a` then
  `if x == nil then x = b end`), unless a later part reads that variable.
- `else if` becomes `elseif` unless its condition needs statements; then it
  stays a nested `if` inside `else`.
- Names that can't change (a `const`, a function declaration, a game
  global) are marked stable by the checker (`TypeTable::is_stable`); `seq`
  reads them again instead of copying them. `t.x = v` evaluates `t` first
  and holds a plain local `t` when `v` calls code, because Lua reads the
  local again at store time.
- The printer wraps a `break` or `return` that isn't the last statement of
  its block in `do ... end`, and leaves out a numeric `for`'s default step.

## Known simplifications in this base

- **Untyped values:** manifests don't carry types yet, so results of game
  API calls, game globals and parameters without a written type are
  "untyped" and skip checks. `untyped + untyped` is still an error
  (`SELU0324`), because the lowering must choose `+` or `..`.
- A function's inferred return type is known only after its body is
  checked; a call from an earlier function body sees it as untyped. There is
  no "not all paths return a value" check yet.
- `selu-check` checks plain global calls, `C_*` namespace calls, and event
  names against the target. Method calls on objects await the typed pass.
- Constructs whose package hasn't landed are `SELU0390` ("isn't supported
  yet"); see `docs/WORK-PACKAGES.md`. Default and rest parameters parse but
  are `SELU0390` until package C.
- Lines inside a function expression map to the statement that holds it in
  the source map.
- Temporaries in a declaration's `pre` stay in scope until the end of the
  block; a function with very many of them could reach Lua 5.1's limit of
  200 locals. Not seen in practice yet.
- Names Lua reserves (`end`, `local`, `then`...) are SELU0304; as field names
  they lower to `t["end"]`. Assignment is a statement only (SELU0305/0306).
- The simulator's frame is a plain Lua table, not the future frame/layout
  engine; unknown globals resolve to a callable no-op after one log line.
- Source maps are a JSON sidecar in Selu's own span format, written to
  `dist/.selu/<Name>/<name>.lua.map.json` (never into the addon folder), not yet the standard source-map VLQ encoding.

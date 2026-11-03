# Architecture

Selu is a Rust workspace (edition 2024) under `crates/`. Data flows one way:

```
.selu source
  -> selu-syntax    lexer + recursive-descent parser -> AST (with spans)
  -> selu-check     resolve calls/events against the target manifest
  -> selu-lower     readable Lua 5.1 + source map + generated .toc
  -> dist/<Name>/  the addon folder
  -> selu-sim       the same Lua, loaded in TOC order into a Lua 5.1 VM
```

`selu-cli` (the `selu` binary) drives the pipeline. All crates report
problems as `selu-diagnostics` values: a stable code (`SELU0001`...), a
primary span, a plain message, and optional help/fix text, rendered for the
terminal with codespan-reporting.

## Crates

| Crate | Job |
|---|---|
| `selu-diagnostics` | `Diagnostic`, `Span`, terminal rendering. No other Selu deps. |
| `selu-syntax` | Hand-written lexer and recursive-descent parser for the Selu subset (DESIGN.md section 9). Semicolons are required; a missing one is `SELU0101` with a fix suggestion. |
| `selu-manifest` | Serde types for a target manifest, JSON loading, `flavour@version` parsing and partial-version resolution to the newest matching manifest. Per-version differences (Lua dialect, availability, restriction flags, `returnsSecret`, evidence `documented`/`verified`) live in manifests, never in compiler code. |
| `selu-check` | Semantic pass: block scopes and name resolution (below), and a global call or `on EVENT` the target lacks is an error naming the target (`SELU0301`/`SELU0302`). `rules.rs` holds target-independent rules (Lua keywords as names, where assignment and `??` may appear). |
| `selu-lower` | Emits readable Lua 5.1 (2-space indent, local names preserved, `a.B(x)` becomes `a:B(x)`; manifest namespace functions like `C_Foo.Bar(x)` stay plain calls), a generated-line -> source-span map, shared event-frame dispatch for `on EVENT`, and the `.toc` whose `## Interface:` comes from the manifest. |
| `selu-sim` | `mlua` (Lua 5.1, vendored): boot a VM, load files in TOC order, simulated clock (`GetTime`), captured `print`, minimal `CreateFrame` (`RegisterEvent`/`UnregisterEvent`/`SetScript`/`Show`/`Hide`/`IsShown`), and `fire(event, ...)` from Rust. Unmodelled globals and frame methods log `not simulated: X` once and return a no-op. |
| `selu-cli` | `selu build` (working), `selu targets` (lists known manifests), `selu test`/`debug`/`probe` (honest stubs, exit non-zero). |

## Manifests and project files

- A project is a folder with `selu.toml` (`[addon]` with `name`, `targets`) and `.selu`
  files under `src/` (or the project root). `selu build` writes
  `dist/<Name>/` (one folder per flavour when several targets are built).
- Manifests are generated JSON files (not hand-edited config; those are TOML) found via `--manifest-dir`, `$SELU_MANIFEST_DIR`,
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

## Known simplifications in this base

- `selu-check` checks plain global calls, `C_*` namespace calls, and event
  names against the target. Method calls on objects await the typed pass.
- `??` keeps `false` and evaluates each side at most once. In a
  declaration, assignment or `return` it lowers to statements
  (`local x = a` then `if x == nil then x = b end`). Anywhere else only the
  exact inline form `(a == nil and b or a)` is allowed: `a` a plain name or
  field path, `b` a number, string or `true`; other forms are SELU0308 until
  the lowering can hoist temporaries.
- Names Lua reserves (`end`, `local`, `then`...) are SELU0304; as field names
  they lower to `t["end"]`. Assignment is a statement only (SELU0305/0306).
- The simulator's frame is a plain Lua table, not the future frame/layout
  engine; unknown globals resolve to a callable no-op after one log line.
- Source maps are a JSON sidecar in Selu's own span format, written to
  `dist/.selu/<Name>/<name>.lua.map.json` (never into the addon folder), not yet the standard source-map VLQ encoding.

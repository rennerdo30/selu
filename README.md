<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/logo/selu-wordmark-light.svg">
    <img alt="Selu" src="docs/logo/selu-wordmark.svg" width="420">
  </picture>
</p>

# Selu

**A TypeScript-style language for World of Warcraft addons.** Selu compiles
to readable Lua 5.1, checks your code against the API of the exact game
client you target, and comes with a simulator that runs your addon outside
the game.

> **Status: early development.** The compiler handles a small but growing
> part of the language (see [What works today](#what-works-today)). It is
> not ready for real addons yet. The name `selu` on crates.io is a
> placeholder until the first release.

## Why

Writing addons in plain Lua means finding mistakes in the game: a
misspelled event that never fires, a function that exists on retail but not
on the classic clients, `0` being true, an accidental global that taints
Blizzard's UI. Selu moves those mistakes to compile time, and keeps the
Lua it writes fast and readable.

- **Easy to read and write.** If you know TypeScript, you can read Selu
  without a tutorial. Semicolons are required, arrays start at 0, and types
  are inferred.
- **Checked against the real client.** Targets are pinned per game version
  (`retail@12.0.1`, `forever`). Calls and events the target doesn't have are
  compile errors that name the target.
- **Fast in the game.** Idle code costs nothing and hot code creates no
  garbage; the simulator tests both.
- **Readable output.** The generated Lua looks like a careful person wrote
  it. Blizzard methods keep Blizzard's names: `frame.SetPoint(...)` becomes
  `frame:SetPoint(...)`.

## A taste

```ts
const greeting = "Welcome back";

function title(name: string, level: number?) {
  const shown = level ?? 1;
  if (shown >= 10) {
    return name + " the Brave";
  }
  return name;
}

on PLAYER_LOGIN() {
  print(greeting + ", " + title("Renner", null) + "!");
}
```

compiles to (unchanged compiler output):

```lua
local greeting = "Welcome back"
local function title(name, level)
  local shown = level
  if shown == nil then shown = 1 end
  if shown >= 10 then return name .. " the Brave" end
  return name
end
__selu_on("PLAYER_LOGIN", function()
  print(greeting .. ", " .. title("Renner", nil) .. "!")
end)
```

`__selu_on` is a small shared event frame at the top of the file: one frame
for all handlers, and an event is registered only when something listens.

Mistakes are explained where they happen:

```
error[SELU0328]: a condition must be a `boolean` or a value that may be null, not a `number`
  ┌─ main.selu:3:7
  │
3 │   if (count) {
  │       ^^^^^
  │
  = help: Lua treats 0 and "" as true and JavaScript as false, so Selu asks you to say which you mean
  = fix: compare it: `count != 0` or `count > 0`
```

## What works today

- `const`/`let` with block scoping and shadowing, `function`, `if`/`else`,
  `return`, `on EVENT(...)` handlers, `global name = value;`
- Types: `number`, `string`, `boolean`, `null`, `T?`, inferred everywhere
  except parameters; narrowing through `!= null`, `&&`, `||` and early
  returns
- The rules that remove Lua's and JavaScript's traps: no truthy numbers or
  strings, `+` only for two numbers or two strings, strict `==` (`===` is an
  error), `??` that keeps `false` and works anywhere, no undeclared reads, no
  accidental globals
- Checks against a target manifest: unknown functions and events are errors
- `selu build`: writes the addon folder with the `.toc` (interface number from
  the target) and a source map
- A headless Lua 5.1 simulator that loads the addon in `.toc` order and fires
  events; the test suite runs every language rule through it
- `selu fmt [paths] [--check]`: formats `.selu` files (indentation, spacing,
  one statement per line; comments and line breaks kept; `[format] indent`
  in `selu.toml`)
- Selu Studio (`selu debug`, or `scripts\studio.bat`): a code editor with
  errors while typing, completion and hover from the game API, Format
  Document, Project Settings for `selu.toml`, and a live game preview

## Planned for version 1

Loops, arrays, maps and objects, template strings, arrow functions, classes,
modules, `try`/`catch`, SavedVariables with checked migrations, slash
commands, hooks, localisation, a JSX-style UI framework with no runtime
diffing, `selu test`, the `selu debug` desktop simulator, generated
manifests for `retail` and `forever`, asset conversion (SVG/PNG to TGA/BLP,
audio to OGG through ffmpeg), `selu export` (addon folder plus zip), and a
VS Code extension.

The full plan is in [SPEC-V1.md](SPEC-V1.md); the reasoning behind it is in
[DESIGN.md](DESIGN.md); how the compiler is built is in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Building from source

You need a recent stable Rust toolchain (edition 2024). Lua 5.1 is built
in, so nothing else is required.

```
cargo build
cargo test
cargo run -p selu-cli -- build examples/hello
```

The last command writes `examples/hello/dist/Hello/`, an addon folder you
can copy into `World of Warcraft/_<flavour>_/Interface/AddOns`. Target
manifests are found in the nearest `manifests/` folder; the one in this
repository is a small test fixture until the generated manifests land.

## Repository layout

| Path | Contents |
|---|---|
| `crates/selu-syntax` | lexer and parser |
| `crates/selu-check` | scopes, types, language rules, target checks |
| `crates/selu-lower` | Lua 5.1 output, source maps, `.toc` |
| `crates/selu-manifest` | target manifests and `flavour@version` resolution |
| `crates/selu-sim` | the Lua 5.1 simulator |
| `crates/selu-diagnostics` | error messages |
| `crates/selu-cli` | the `selu` command |
| `examples/` | example addons |
| `manifests/` | target manifests (a test fixture for now) |

## Licence

[MIT](LICENSE). The logo in `docs/logo/` is under the same licence.

Selu is an independent project and is not affiliated with or endorsed by
Blizzard Entertainment. World of Warcraft is a trademark of Blizzard
Entertainment, Inc.

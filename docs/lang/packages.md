# Packages

Status: implemented 2026-11-10 (SL-42; owner-approved). Package 0 of the UI
toolkit and standard-library work: the mechanism only, plus one small test
package. `selu-ui` and the standard-library modules come after it.

## What a package is

A package is a library of `.selu` modules that ships inside the Selu compiler.
Projects import it by name; no download, no registry and no version
resolution. A package's version is the compiler's version, so one compiler
build always gives the same package source.

Package sources live in the repository under `packages/<name>/`:

```
packages/<name>/
  package.toml      # name, description, products
  src/index.selu    # the entry point; its exports are the package API
  src/**.selu       # internal modules, reachable only from inside the package
```

The compiler embeds every package's `package.toml` and `src/**.selu` at
compile time (`crates/selu-test/build.rs`), so `selu.exe` alone is enough on a
user's machine, and Studio, `selu build`, `selu check` and `selu test` all see
the same sources. `packages/<name>/tests/` is not embedded.

`SELU_PACKAGES_DIR=<folder>` replaces the embedded set with the package
folders in `<folder>`, read from disk. Package authors use it to try changes
without rebuilding the compiler; the compiler's tests use it for packages that
must not ship. A bad `package.toml`, a folder whose name differs from its
`name`, or a package without `src/index.selu` is left out, and the first such
problem is named in the "unknown package" help.

`package.toml`:

```toml
name = "test-package"
description = "Small package that proves the package mechanism."
# Products the package supports; every product unless narrowed with a reason.
products = ["wow", "wowf", "wow_classic", "wow_classic_era", "wow_anniversary", "wow_classic_titan"]
```

## Importing

```selu
import { CreateButton } from "@selu/ui";
```

- A specifier that starts with `@selu/` names a package. `@selu/<name>`
  resolves to `packages/<name>/src/index.selu`.
- Only the entry point is importable from a project. `@selu/<name>/<path>`
  is an error (SELU0407: "import the package by its name; `<path>` is
  internal"). This keeps each package's API one file wide and stable.
- Inside a package, modules import each other with relative paths, exactly
  like a project. Relative paths cannot leave the package's `src/`.
- A package may import another package by name. Cycles between packages are
  SELU0403 like any module cycle.
- An unknown package is SELU0407 with the list of available packages in the
  help text.
- Any other `@` specifier is SELU0407 ("not a Selu package"). Project path
  segments may not start with `@` (SELU0318, like `__selu`), so package keys
  never collide with project keys.
- A relative path that would leave a package's `src/` is SELU0400.

## Checking and output

- Package modules join the project's module graph under the key
  `@selu/<name>/<relative path>` (for example `@selu/ui/button`). Only
  reachable package modules are checked and emitted, as with project modules.
- They are checked against every target of the project, with the same
  manifests and rules as project code. A package used on a product outside its
  `products` list is SELU0408 at the import.
- Lua output goes to `dist/<addon>/selu_packages/<name>/<relative path>.lua`,
  loads in graph order before the project modules that import it (through the
  same mechanism that orders project modules today), and uses the module namespace key `@selu/<name>/<relative path>`.
  Source maps go to `dist/.selu/<addon>/selu_packages/...` and name the
  package source path, so errors point into the package file.
- SELU0404 (unreachable file) never lists package files.
- Diagnostics in package code are reported with the package path
  (`@selu/ui/button.selu:12:3`), so a project author sees that the problem is in
  the package, not in their code.

## What packages may not do

Packages are libraries; the addon that imports them owns its global state.

- No `saved` declarations (SELU0409): a package keeps no SavedVariables of
  its own. It takes the state it needs from its caller (for example a theme
  setting passed in by the addon).
- No `export global` (SELU0409): globals belong to the addon.
- No bindings, TOC fields or assets of their own in package 0. Package art and
  strings are a later extension, designed when `selu-ui` needs them.
- Packages follow every Selu rule: the WoW interface fully typed, no Blizzard
  IP, every product.

## Tests

- A `selu test` file may import a package by name, like project code.
- Running `packages/<name>/tests/` as a package's own suite comes with
  `selu-ui`, the first real package.
- CLI tests (`crates/selu-cli/tests/packages.rs`, and the runner case in
  `testing.rs`) cover: import by name; unknown package; internal path refused;
  package-to-package import; package cycle; `saved` and `export global`
  refused; output paths, TOC order and namespace keys; source-map paths;
  SELU0404 silence; product restriction.

## Not in package 0

- Third-party or path dependencies (`[dependencies]` in `selu.toml`).
- Package assets, strings (locales) and bindings.
- `selu-ui` and the standard-library modules themselves.

# Packages

Status: design, owner-approved 2026-11-10 (SL-42). Package 0 of the UI
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

The CLI embeds every package's files at compile time (for example with a
`build.rs` that includes `packages/**`), so `selu.exe` alone is enough on a
user's machine and Studio, `selu build`, `selu check` and `selu test` all see
the same sources.

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
- Project paths may not start with `@` (SELU0318 already covers `__selu`;
  extend the reserved check), so package keys never collide with project keys.

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

- Package tests live in `packages/<name>/tests/*.test.selu` and run with
  `selu test` from the package directory (the package acts as its own
  project with `src/index.selu` as the root).
- CLI tests cover: import by name; unknown package; internal path refused;
  package-to-package import; package cycle; `saved` and `export global`
  refused; output paths, TOC order and namespace keys; source-map paths;
  SELU0404 silence; product restriction.

## Not in package 0

- Third-party or path dependencies (`[dependencies]` in `selu.toml`).
- Package assets, strings (locales) and bindings.
- `selu-ui` and the standard-library modules themselves.

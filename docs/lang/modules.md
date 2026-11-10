# Modules

A project starts at `src/main.selu`. Relative, extensionless named imports
form its graph; only reachable files are parsed and checked. Other `.selu`
files below `src/` are named in SELU0404 warnings. Paths cannot escape `src/`.
The build uses a lexicographically ordered ready set, so independent modules
are ordered by their complete relative path. Cycles report the full chain and
render every edge against its own source.

Each module has a private table `__selu_ns["relative/path"]`, using the shared
addon namespace supplied by the WoW loader. Functions and constants are
published directly and cached in importer locals before executable statements.
An exported mutable variable stays a normal local in its defining file. Its
export is one getter closure, allocated once when that declaration executes;
an importer caches that getter and calls it for each read. Reads allocate
nothing but pay one Lua call. This keeps defining-module writes and closures
ordinary Lua, preserves live nullable/false values, and requires no global
metatable. Imported names are read-only, including mutable exports. To change
state, export a function. Lexical shadowing disables getter resolution in the
shadowing scope. Getter calls participate in the existing expression sequencing
rules, so a later argument that mutates state cannot change an earlier read.

`export global name = value;` emits a plain global assignment and publishes
a getter for named imports, so changes made by Selu functions or external Lua
are visible on every imported read. Ordinary exports never create globals.
Single-file `check_program` and `lower` accept exports as local declarations;
project callers use `check_module` and `lower_module` with dependency metadata.
Modules without import/export syntax retain the original standalone output.

Named imports checked without dependency context produce SELU0400 at the path.
Missing names produce SELU0401 at the exported-name span, separate from the
local alias span. Read-only writes are SELU0402; cycles are SELU0403.
Asset/default imports intentionally remain SELU0390. Class semantics and WoW
built-ins remain the responsibilities of D3 and D4.

Source maps preserve each module's source name and are written under
`dist/.selu/<addon>/<relative/path>.lua.map.json`. Lua output retains relative
folders, avoiding collisions between equal basenames. Every reachable module
is checked for a target before any generated module is written for that target.

## Review corrections

Import local names, including unused aliases, use the normal Lua-keyword and
compiler-prefix name checks. Loader locals are `__selu_addon` and `__selu_ns`.
Imports and exports are only valid at file scope (SELU0406, with a move-to-top-level
hint). Missing exports receive generic help rather than an arbitrary suggestion.

Every import path component must match the directory entry's case exactly.
SELU0405 highlights the complete import path and names the actual source path
in its diagnostic and fix. Case variants cannot create duplicate module identities
or overwrite the same output on case-insensitive hosts.

The module event prologue creates its handler table, frame and registration
function only if `__selu_ns[true]` is absent, then stores the registration closure
there. Every file uses that same closure. A boolean namespace key cannot collide
with the string keys used for module paths; it is reserved for the event runtime.
The closure owns the handler table and frame. D4 can use a separate reserved slot
for its create-on-first-use runtime. Event-only modules use this prologue too.
Project output loaded without addon arguments uses a fresh fallback namespace;
`lower()` retains its original standalone event runtime without a namespace.

D3 integration belongs in `walk_stmt` in `selu-lower/src/modules.rs`: class
constructors and methods must remove their parameter bindings from the active
live-import set before walking defaults and bodies, as function declarations do.
Imported class names in written types belong in D3's checker name/type resolution;
module signatures already pass each exported Type through to import bindings.

Module path segments and filenames starting with `__selu` are reserved, like
user bindings (SELU0318). The diagnostic underlines the import path and asks to
rename the file or directory. This keeps named compiler runtime slots in the
addon namespace separate from module tables. Both project path resolution and
standalone module checking enforce the same validation.

## Re-exports

`export { a, b as c } from "./x";` imports names from another module and
publishes them as this module's exports, `b` under the name `c`. Values,
functions, classes and types all re-export with the defining module's
signature, so `new C()` and type annotations work in the importer. A mutable
export stays live: its getter is published as is. The names are also bound
locally, read-only, like any import. A missing name is SELU0401. The form is
how a package's `src/index.selu` offers an API that is spread over internal
files (docs/lang/packages.md). `export { ... }` without `from` is not
supported; export the declaration itself.

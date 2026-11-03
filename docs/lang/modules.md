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

`export global name = value;` still emits a plain global assignment, and also
publishes the value for named imports. Ordinary exports never create globals.
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

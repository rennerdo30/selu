# Generated Lua

Lua references live beside every example: `expected.lua` for main.lua and
`expected.<module>.lua` for each additional module (including relative paths).
The examples test compares every Lua byte and independently runs the addon
against its unchanged expected.txt. In PowerShell regenerate deliberately with:

```powershell
$env:SELU_UPDATE_LUA = "1"
cargo test -p selu-cli --test examples --test readability
Remove-Item Env:SELU_UPDATE_LUA
```

Long table literals split into one field per line above 100 columns, counting
indentation and the local declaration or assignment prefix. Field order,
including the last positional value's multiple-result restriction, is preserved.
Non-final tuple return operands need no parentheses: Lua truncates them already.
The final operand keeps its single-result parentheses.

Property reads can run code through Lua `__index`. An object annotation alone
proves nothing: `const obj: { n: number } = UIParent` still needs snapshots.
A read is effect-free only when its receiver is proven to be a table Selu
created with no code-running metatable: an object/array literal in an unreassigned
local, a `new` Map/Set or class instance, or `this` in a class member (whose
`__index` is the plain methods table). The origin pass conservatively rejects
names that are shadowed anywhere or assigned anywhere, including inside closures.
It does not infer origins from structural annotations, aliases, collection
elements or class-typed parameters. Those uncertain cases keep their snapshots.
`self.total = self.total + amount` remains direct.

Map updates on proven receivers with effect-free operands keep only the nullish
temporary. Every later property operand must have the same origin proof before
receiver/key snapshots can be removed. Otherwise a getter could reassign the key
between its evaluation and the Lua store. Compound assignments snapshot the old
value before any possibly effectful right operand. Lua reads a local receiver
again at store time; receiver captures cannot go merely because it is a name. One source-level
`may_run_code` implementation in `purity.rs` controls ordinary field stores,
compound assignments and map updates. Both property and indexed reads consult
the receiver-origin proof; the lowered Lua tree is not used to infer origin.
This keeps `t.x = obj.n` storing into the original table even if a getter in
`obj.n` reassigns `t`.

Inline callback bodies with a single-value terminal return, or no return for forEach, become
loop statements. Parameter locals preserve shadowing, assignment and per-turn
closure capture; only declared parameters are bound. Stable array bindings need
no receiver copy. Mutable bindings retain one copy since the callback can rebind
them. Named callbacks, rest callbacks, sort adapters and callbacks with early
returns or multiple-value returns retain a function boundary: removing it without modelling all return and
loop transfers would change behavior. Callback values retained this way are
captured once before iteration, never rebuilt on each turn.

Discarded inline forEach results remain valid Lua statements: calls stay calls,
literals/names may be dropped, and other expressions evaluate into `local _`.
A multiple-value callback return keeps its boundary so every operand evaluates.

Placeholder-free log messages are literal strings, including percent signs.
String-typed placeholders skip tostring; calls still keep single-value parentheses
when used as arguments. Other values retain conversion. L still
calls the translation helper even without placeholders, since returning the English
literal would disable translations. That helper returns a plain string without
formatting when no arguments exist.


## Shared addon runtime

Project builds emit `Selu.lua` first in the `.toc` whenever any module needs
runtime helpers, followed by source modules in dependency order. If a source
module is named Selu (case-insensitively), the runtime uses the reserved filename
`__selu_runtime.lua` instead, so it cannot overwrite authored output. Even a one-module
addon uses this arrangement: consistent debugging and a short main file take
precedence over saving a few wrapper lines. There is no installed runtime library.
Standalone lowering APIs remain self-contained; addon builders call `runtime::share`
on the complete set of lowered modules before writing files.

Every file receives the existing addon name and `__selu_ns` table through `...`.
Module files alias only helpers they reference, with plain local declarations.
The runtime is scoped to this addon namespace, including across multiple addons.
`[build] layout = "shared"` emits one runtime beside the common modules, used by
all targets listed in that TOC. Per-target layout emits one runtime per output
addon directory; folded-away capabilities require no helper. Native hook/hookScript calls alone
need no localisation runtime; command, L and log request it.

`runtime.rs` owns one registry with explicit dependencies and fixed ordered keys.
Error state and captures precede error operations, scalar protection helpers and
callback entry. The protected-call queue is piece `18-protected-calls`, before
the event dispatcher, preserving drain-before-user-regen-handler order. Saved
loading also precedes event dispatch; localisation/logging, shared event capability
probing and the secret-read helper follow. Each piece has a short comment and a
Lua `do` scope; dependencies become namespace aliases. Additional pieces can register their body, exports and dependencies here. Keep dependency keys earlier than consumers. Protection helpers remain
specialized by return width, preserving trailing nils without result tables.

`assemble_file` builds the single ordered file prelude for both lowering paths.
Sharing removes generated AST statements, replaces them with aliases, and prints
the module AST again to refresh `statement_lines`. The CLI constructs canonical
S3 maps from `Lowered::source_map` only after sharing; the runtime has no map. Runtime code has no Selu
source span. Class and typeof captures stay local to their module.

Saved declarations and migrations still register loaders while modules execute.
The saved runtime frame is created before the user dispatcher; `ADDON_LOADED`
runs saved loaders before user handlers. The simulator receives every TOC file
before firing that event. Existing rollback, newer-schema and character isolation
checks remain unchanged. The saved init table is exported only through
`__selu_saved`, which that event clears; primitive captures stay private so the
cold init helpers can be collected even if the namespace remains alive. The proof harness also reads the TOC and uses
`Sim::load_addon`, rather than evaluating just main.lua.

Temporary elimination is deliberately small: literal left operands of `??` are
folded after the purity check. `null ?? false` becomes false without a temporary;
non-null literals never evaluate the right operand. Effectful receivers, calls,
optional field reads and sequencing still use the existing purity/order machinery.

Physical line counts include comments. Existing expected.txt files are unchanged.
New Selu.lua files add wrapper/export lines; this is primarily a reduction in
module scaffolding, not a claim that tiny addons have fewer total lines.

| Example | Main before | Main after | Runtime after | Other module before -> after |
| --- | ---: | ---: | ---: | --- |
| classes | 78 | 57 | 22 | - |
| cooldowns | 85 | 64 | 22 | - |
| errors | 96 | 33 | 99 | - |
| frames | 100 | 19 | 22 | - |
| hello | 30 | 9 | 22 | - |
| item-links | 34 | 13 | 22 | - |
| loot-summary | 94 | 73 | 22 | - |
| modules | 32 | 11 | 22 | expected.counter.lua: 13 -> 12 |
| multi-target | 31 | 10 | 22 | - |
| named-types | 37 | 16 | 22 | - |
| saved-settings | 148 | 27 | 127 | - |
| shared-runtime | new | 7 | 99 | expected.worker.lua: new -> 20 |
| slash-and-hooks | 106 | 26 | 86 | - |
| timers | 30 | 9 | 22 | - |

The proof's checked-in baseline `reference.lua` has 439 lines. Its CLI shared
output now has 259 main lines and 191 runtime lines (450 total): the main file
shrinks by 180 lines (41%). The in-process proof test's standalone baseline had
432 lines; its new shared main omits the CLI module export table and can differ
from the CLI count. The checked-in reference is retained as historical evidence.

All expected Lua diffs were inspected: runtime bodies moved into scoped pieces,
helper aliases precede imports/source bindings, and user event bodies retain their
names and structure. The new shared-runtime example exercises two source modules.

## Source maps

`selu build` writes readable JSON maps to
`dist/.selu/<Addon>/<module>.lua.map` (including module subdirectories).
Maps never go inside the addon folder or its `.toc`. The shared runtime and
preludes have no source positions.

Version 1 has `version`, `chunk`, `sources` and `ranges`. `chunk` is the
reported Lua filename. Each source has a normalized project-relative `path`
and `hash` (`fnv1a64:` followed by 16 hexadecimal digits over the UTF-8 source).
Each range has `gen_start`, `gen_end` (inclusive, one-based generated lines),
`source` (zero-based source index), and one-based `line` and `column`.
Columns count Unicode characters. The printer emits a range for each printed
statement line, sorted by generated start; nested statement positions win on
shared lines. Callback bodies map to their own statements, including nested
callbacks. Synthetic statements carrying an originating span map to that span;
runtime statements without one are absent. Generated syntax such as `end`
maps to its containing statement. Erased types and imports emit no executable
statement and have no generated position.

`selu-sourcemap` reads and writes this format, looks up positions in both
directions, and rewrites Lua error positions through `map_lua_error`.
Unmapped positions remain unchanged. Hashes allow consumers to detect stale
source text; they are not security hashes.

Distinct source statements use distinct generated lines when a compact `if`
would otherwise make one position hide the other. Multi-line conditions retain
their body positions after all condition lines. Saved declarations and migration
registrations map to their declarations; shared saved runtime setup stays unmapped.
Error chunk selection prefers normalized exact paths, then a unique most-specific
path suffix. Ambiguous basenames stay unchanged. Quoted chunk names are delimited
by `[string "` and `"]`, so addon folder names may contain spaces.

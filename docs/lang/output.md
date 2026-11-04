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

The addon namespace uses __selu_events and __selu_wow for shared internal state.
Each file checks the event registration function before constructing the shared
frame. Dispatch preserves registration order and passes all event results through.
Both entry paths order their prelude as loader, protected class/saved captures,
namespace initialization, then runtimes. The class capture precedes the WoW
runtime even for a standalone lowering. Saved runtime setup follows namespace initialization; captures occur
once even when both saved declarations and classes occur in the same file.

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

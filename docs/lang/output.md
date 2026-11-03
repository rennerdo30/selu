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

Long table literals split into one field per line above 100 columns. Field order,
including the last positional value's multiple-result restriction, is preserved.
Non-final tuple return operands need no parentheses: Lua truncates them already.
The final operand keeps its single-result parentheses.

Map updates on stable receivers with effect-free operands keep only the nullish
temporary. A typed map get is a table read; reading ordinary object fields and
arithmetic cannot invoke source functions. Unknown objects, effectful operands,
mutable receivers, calls and computed stores retain their snapshots. Lua reads a
local receiver again at store time, so receiver captures cannot be removed merely
because its syntax is a name. Compound arithmetic reads its old value before the
right operand, and direct updates retain that order.

Inline callback bodies with a terminal return, or no return for forEach, become
loop statements. Parameter locals preserve shadowing, assignment and per-turn
closure capture; only declared parameters are bound. Stable array bindings need
no receiver copy. Mutable bindings retain one copy since the callback can rebind
them. Named callbacks, rest callbacks, sort adapters and callbacks with early
returns retain a function boundary: removing it without modelling all return and
loop transfers would change behavior. Callback values retained this way are
captured once before iteration, never rebuilt on each turn.

Placeholder-free log messages are literal strings, including percent signs.
String-typed placeholders skip tostring; other values retain conversion. L still
calls the translation helper even without placeholders, since returning the English
literal would disable translations. That helper returns a plain string without
formatting when no arguments exist.

The addon namespace uses __selu_events and __selu_wow for shared internal state.
Each file checks the event registration function before constructing the shared
frame. Dispatch preserves registration order and passes all event results through.
One file_prelude path plans the class and saved-runtime captures before source
bindings. Saved runtime setup follows namespace initialization; captures occur
once even when both saved declarations and classes occur in the same file.

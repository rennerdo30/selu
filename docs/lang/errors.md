# Structured errors

`try` requires `catch (e)`, `catch`, and/or `finally`. A catch binding is
block scoped and has the nominal built-in `Error` type. `Error` is reserved
for the built-in: declaring a value, parameter, import binding, class, type,
interface or type parameter with that name is SELU0542. Object member names
can still be Error.

`new Error("message")` creates one object with `message: string`.
`new Error()` uses the empty message. Invalid arguments are SELU0540;
throwing another type is SELU0541. Structural `{message: "..."}` objects do
not become Errors. Earlier expression errors suppress follow-on diagnostics.
`stack: string?` contains a protected client traceback when available. It is null when unavailable, inaccessible or secret. The compiler selects manifest-sourced debugstack, then debug.traceback. Catch and native boundaries retain Error identity and fill a missing stack. Simulator debugstack integration is supplied separately.

Throw and catch preserve the object. After `throw original`, the caught value
compares equal to original; its message is unchanged, and mutations affect
original. The position is where the Error was created: like JavaScript, which
records the stack at construction, `new Error(...)` stores a private source
position known at compile time. Throwing, rethrowing or reusing the object
never changes it. The position is independent of message and is rendered
exactly once when the Error escapes, as `file.selu:line: message`. A shared
metatable also gives the Error that textual representation through Lua
tostring. Source positions use the parser's original one-based line and
module-relative file name; generated Lua line numbers and source-map sidecars
are not needed.

Non-Error values thrown by Lua are wrapped. The wrapper's `message` comes from
a protected tostring conversion and retains Lua's own location text; a wrapper
has no Selu position, so rethrowing it shows only Lua's location. Its
`value: unknown` field keeps the original value, including null for Lua nil;
Errors constructed directly in Selu have value null. An Error already created
by Selu is recognized by shared metatable identity and passes through unchanged.
The metatable identity probe is protected too; no unfamiliar object's fields
are read. If tostring raises, the message is `"<error object>"` and value still
contains the original. tostring's result is validated by concatenating an empty
string within the same protected conversion.

A game error value may be a secret value. Selu does not inspect its contents:
conversion is limited to protected tostring, and retaining value does not make
it safe to inspect or branch on. A successful conversion can itself retain the
game's secret restrictions. General secret-value checking is a separate feature.

Every protected clause uses captured Lua pcall. The addon namespace shares an
Error metatable and a protection-depth counter across its modules. Each file's
fixed-arity `__selu_protect` saves the counter, increments it, calls the
captured pcall, then restores the saved value on both success and failure. It
forwards completion values through scalar locals, without a result table. Throw/propagation inside active
Selu protection passes the Error object to Lua error unchanged. With no active
Selu protection, `__selu_raise` converts it to safe text before Lua error. Thus
uncaught event throws and rethrows reach the normal game error path as strings;
Selu neither replaces geterrorhandler() nor swallows the error. Low-level Lua
pcall outside Selu protection sees that host-formatted text; use try/catch for
Selu Error identity.

The game runs callbacks behind its own boundary, even when they fire
synchronously inside a Selu try: `frame.Show()` runs an OnShow hook, a call
can fire an event or a hooked function. An error there goes to the game's
error handler, and the code that triggered the callback carries on. So in an
addon where any file uses try or throw, every generated callback entry point
starts with no active Selu protection: the shared event dispatcher sets the
counter to 0 before running handlers and restores it afterwards, and the
callbacks of `command`, `hook` and `hookScript` are wrapped in `__selu_entry`,
which does the same. An escaping throw inside a callback therefore reaches the
handler as text. If the callback fails, the enclosing `__selu_protect` restores
the counter. A Selu try around a direct call of a hooked function catches that
text as a wrapped error, not the original Error object. Files of such an addon
without try/throw get only the shared counter. Addons without try/throw emit
none of this. Callbacks passed directly to game APIs (`SetScript`,
`C_Timer.After`) are not generated entry points and are not wrapped.

This explicit boundary conversion avoids assuming every WoW handler accepts
tables: the read-only source at wow-ui-source commit
17ed2c3e8471cdc3806ab0d26a082be7c67d3d7c, in
`Interface/AddOns/Blizzard_ScriptErrors/Blizzard_ScriptErrors.lua` (HandleLuaError),
uses string.format with `%s`; `Blizzard_ScriptErrorsFrame.lua` also formats the
message with `%s`. Neither path explicitly calls tostring on the error first.
The simulator exposes escaping script errors as Rust errors, tested for one
Selu position even after rethrowing across modules.

Normal protected completion returns no status. Returns use `"return", ...values`;
a loop exit uses `"break"`. A's repeat wrapper and break flag distinguish
continue from a real break. Dispatch happens after cleanup. Nested protected
blocks propagate their completion through their parent; nested source functions
keep their own return semantics. False and trailing nil tuple values stay in
locals. Catch itself is protected so a new error cannot bypass finally.

Finally runs on normal completion, throw, return, break and continue. Its abrupt
completion overrides the previous one, including return and throw. The pending
loop break flag is saved while finally executes and restored only when finally
completes normally, so a finally-continue can override a pending break.

The common catch shape:

```lua
local __selu_ok, __selu_status = __selu_protect(function() work() end)
if not __selu_ok then
  __selu_ok, __selu_status = __selu_protect(function()
    local e = __selu_wrap_error(__selu_status)
    print(e.message)
  end)
end
if not __selu_ok then __selu_raise(__selu_status) end
```

A return with cleanup (the `__selu_` prefix omitted):

```lua
local ok, status, value1 = __selu_protect(function() return "return", value end)
local final_ok, final_status, final_value1 = __selu_protect(function() cleanup() end)
if not final_ok or final_status ~= nil then
  ok, status, value1 = final_ok, final_status, final_value1
end
if not ok then __selu_raise(status) end
if status == "return" then return value1 end
```

Each try's locals live in their own `do` block, so nested tries reuse the names.

Inside a loop, continue returns `"break"` from the protected function, runs
cleanup, then dispatches break out of the single-iteration repeat. Real breaks
also set A's break flag. `examples/errors/expected.lua` has complete output.

Generated protected functions are hoisted before their loop whenever free-name
analysis proves they capture no bindings created by an iteration. References to
module functions or cells outside the loop may be shared: their current values
are still read each time. Local declarations, loop variables, functions and
parameters within a protected function are tracked lexically, including nested
closures. The analysis covers all loop forms, including for-of. Source-written
callbacks are never hoisted by this pass. Functions capturing a loop binding
retain per-iteration allocation; catch wrappers capturing their pending error
also retain that cost. Catch allocates a wrapper only for non-Error failures.

The regression measures only execution of a 1,024-iteration loop, with GC stopped
after loading the module: capture-free try/finally clauses allocate no closures
per iteration. The initial module parse/setup and the few helper functions are
outside that measurement. Files using no Error/throw/try emit no error runtime.
Lua pcall, error, tostring, setmetatable and getmetatable are captured under
reserved names before source bindings, so local source names cannot replace them.

`onError((error: Error) => { ... })` installs a typed native diagnostic boundary.
SELU0543 rejects an incompatible callback or arity. Strings become messages;
existing Selu Errors retain identity; other values are described by Lua type
without invoking their conversion hooks. Secret values become `<secret error>`.
The compiler's adapter calls the prior native handler once with the original
arguments, even when the Selu callback throws. Native values are never exposed
by this adapter. Install early, retain errors in a bounded Selu array, and attach
storage/UI sinks later. The callback starts outside any surrounding try context.
This is registration, not a timer: disabling profiling needs no recurring work.
Native seterrorhandler/geterrorhandler semantics are sourced in the toolkit's
WOW_LUA_API.md:1887-1888; normalization is a Selu compiler contract.

Use GetTime for deterministic logical timestamps. `sim.callbackCounts()` reports
actual timer and OnUpdate callback dispatch counts for the current generation;
it is an observation, not CPU time or allocations. Native debugprofilestop's
millisecond contract comes from Blizzard FrameScriptDocumentation.lua. Simulator
clock advancement does not establish CPU precision or a performance improvement.

Simulator native callback failures reach an installed onError handler before
host-side formatting. Nested native boundaries forward an internal reported-error
marker, so the original error reaches the installed handler once. The owning test
still records the failure even if a caller catches the propagated exception.

`onError` returns a handle with `ensureInstalled(): void` and
`isInstalled(): boolean`. There is one adapter per addon namespace. Calling
onError again replaces the callback and returns that same handle; registrations
do not stack. ensureInstalled compares native handler identity and replaces
only a different handler, keeping that handler as the predecessor. A guard
covers callback and predecessor execution, so a wrapping addon calling back
into the adapter terminates. Repeated reassertion adds no captures. Use:

```selu
const errors = onError(e => { print(e.message); });
on ADDON_LOADED(name) { errors.ensureInstalled(); }
on PLAYER_ENTERING_WORLD() { errors.ensureInstalled(); }
```

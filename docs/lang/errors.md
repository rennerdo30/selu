# Structured errors

`try` requires `catch (e)` and/or `finally`. Catch parameters are block scoped
and have the built-in nominal `Error` type, with a writable `message: string`.
Construct it with `new Error("message")`. Other thrown values, including
structural `{message: "..."}` objects and untyped Lua values, are rejected
(SELU0541). Error construction requires exactly one string (SELU0540).
There is no stack/name property or inheritance requirement in this minimal type.

Throw emits `error("file.selu:line: " .. value.message, 0)`. The parser records
the original one-based line; this works with module-relative file names and
requires neither the generated Lua line number nor the source-map sidecar at
runtime. Lua errors caught by `pcall` are wrapped as `{message = tostring(value)}`;
this also handles numeric/table/nil error values. A rethrow adds its own Selu
position. Uncaught errors are re-raised with level zero and reach WoW's ordinary
script error handling (`geterrorhandler()`); generated code does not install or
replace that handler. The simulator exposes escaping event errors as Rust errors.
Lua-origin text retains the position supplied by Lua; only Selu `throw` creates a
Selu position.

Each protected clause uses `pcall`. Normal completion returns no status;
returns use `"return", ...values`; a loop exit uses `"break"`. A's loop wrapper
and break flag distinguish `continue` from a real break. Dispatch occurs after
cleanup. Nested protected blocks propagate their completion through their parent.
Scalar and tuple payloads stay in locals, including false and trailing nil;
there is no tuple table. Functions declared inside a clause retain their own
return semantics. Catch itself is protected so its errors cannot bypass finally.
Finally runs on normal completion, throw, return, break and continue. Its abrupt
completion overrides the previous one, including `return` and `throw`. A pending
loop break flag is saved while finally executes and restored only on normal
completion, so a finally-continue can override a pending break.

For example, `try { work(); } catch(e) { print(e.message); }` has this shape
(temporary names simplified here):

```lua
local ok, status = pcall(function() work() end)
if not ok then
  ok, status = pcall(function()
    local e = { message = tostring(status) }
    print(e.message)
  end)
end
if not ok then error(status, 0) end
```

`try { return value; } finally { cleanup(); }`:

```lua
local ok, status, value = pcall(function() return "return", value end)
local finalOk, finalStatus, finalValue = pcall(function() cleanup() end)
if not finalOk or finalStatus ~= nil then
  ok, status, value = finalOk, finalStatus, finalValue
end
if not ok then error(status, 0) end
if status == "return" then return value end
```

Inside a loop, `try { continue; } finally { cleanup(); }` returns `"break"`
from the protected function, runs cleanup, then dispatches `break` out of the
single-iteration repeat wrapper. Real breaks additionally set A's break flag.
`examples/errors/expected.lua` is a complete, executable generated example.

Protection has a cost: a closure for each executed protected clause, plus the
pcall boundary. Catch allocates its wrapper only on failure; new Error allocates
one message table. Empty and literal-completion protected closures in ordinary
loops are moved to the enclosing block, so those allocate once per loop entry.
Closures reading locals retain per-iteration allocation to preserve lexical
bindings and mutation. For-of currently keeps its clause closures in the body.
Keep a try around a whole batch/loop when per-item recovery is unnecessary.
No idle helper, runtime class registry, traceback construction or hot-path lint
is added. No Lua source-map runtime is installed.

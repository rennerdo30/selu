# WoW built-ins

D4 starts at 85ff2fe7000a23ff2529ef2970fd14ed925fa9a5.

`command` accepts literal slash aliases and a callback contextualised with one
string argument. Aliases have no whitespace. Callback arguments beyond the first
cannot be required. Hook callbacks get untyped game parameters. Both forms of
`hook` use hooksecurefunc; hookScript uses HookScript. Simulator hooks preserve
original results and invoke hooks afterward with the original arguments.

Names declared in a scope shadow built-ins throughout that scope. The checker
returns None for their calls, leaving normal function checking/lowering in charge.
Custom tagged templates remain SELU0390; this includes tags named L/log that are
shadowed. Supported logging tags are debug, info, warn, error and warnOnce.

The single Lowerer field WowState holds usage and unique locale keys in source
encounter order. `wow_prologue()` is self-contained and returns a runtime block
only after lowering encounters a built-in. Both lower and lower_module call it after lowering their body and insert its
result before executable code. lower_module retains D1's one loader line and
fallback namespace assignment; lower supplies those same reserved loader locals
only when the file uses the WoW runtime. The runtime itself has no loader line.
The shared lowering entry supplies `local __selu_addon, __selu_ns = ...`; `__selu_ns[false]` is created only once.
Multiple files loaded with the same namespace share commands, levels, buffer,
once set and translations. Standalone load_files falls back to a file-local table.

`__selu_ns[false].buffer` holds at most 200 `{level, message}` entries, oldest first.
`__selu_ns[false].levels` enables info/warn/error/warnOnce initially; debug is disabled.
Generated if guards surround interpolation evaluation and string.format, so a
disabled level evaluates no arguments and builds no string. warnOnce uses the
English template key (not interpolated values), shared across files. The once
set retains one entry per encountered template. Buffer overflow removes the
oldest entry; logging is diagnostic, not a zero-allocation hot-path operation.
Tests inspect the buffer using a Lua chunk receiving the same addon namespace.

Slash keys use SELU_<addon>_<monotonic command number>_, with aliases numbered
from 1. The per-addon counter prevents collisions between files. Generated calls
are `__selu_wow.command("/addon", "/a", function(args) ... end)`; the runtime
writes SLASH_<key>1 etc. and SlashCmdList[key]. Sim::slash dispatches aliases
case-insensitively, trimming surrounding whitespace from the line and message.
The simulator deliberately errors with "unknown slash command" for empty or
unmatched input. It calls handlers with (message, editBox), as the client does;
the registration adapter forwards exactly one string to the Selu callback,
preserving defaults after that argument. The first positional parameter or rest
element must accept string. Known primitive hook targets are rejected; untyped
game values and object tables are allowed.

L templates collect English keys using {0}, {1}, etc. in Lowered.locale_keys.
`__selu_ns[false].locales[GetLocale()][key]` contains translated sentences with those
placeholders. Literal braces in English key text are doubled: `{{` and `}}`.
Translations use the same escapes. For example L`literal {0} ${2}` has key
`literal {{0}} {0}`; translation `translated {{0}} {0}` renders
`translated {0} 2`. Single braces must enclose an argument index.
Missing entries use the English format before creating any tables. A literal-only
fallback returns the literal directly, including percent signs. Calls read as
`__selu_wow.text(key, fallback, ...)`; the helper returns the finished string.
Translations are parsed once per distinct translation string into `__selu_ns[false].formats`
(format, literal text, argument order and maximum index). Changing a translation
selects or creates the entry for its new text. Repeated/reordered arguments work.
Only translated interpolations build temporary argument tables per call.
Translation validation/file writing belongs to the later locale build pipeline.
Joining two direct L templates warns SELU0444; warning analysis is syntactic.
Interpolation accepts non-null strings, numbers and booleans; tostring gives
booleans Lua 5.1-compatible formatting. All literal percent signs are escaped.

Shared edits: checker lib.rs binary + calls wow_join; its unsupported-construct
fixture drops only L. Lowerer lib.rs adds WowState, initializes it, inserts the
optional prologue after lowering and transfers keys into Lowered.locale_keys.
Simulator lib.rs adds Sim::slash, HookScript in CreateFrame and dispatches OnShow
when Show transitions from hidden. No CLI pipeline changes are needed.

The Show -> OnShow simulator dispatch is explicitly approved, as is fixed raw
Lua runtime text (the same exception as the shared event-frame prologue).
User expressions still lower through the Lua syntax tree.

SELU0445 caps L and log templates at 64 interpolations, conservatively below
Lua 5.1 call/register limits. SELU0446 rejects invalid hook targets.
Diagnostics SELU0440-0444 cover arity, alias/hook name, callback, template types
and sentence joining. Other unsupported template tags retain SELU0390.

Verification: all seven D4 tests were demonstrated red by temporarily disabling
checker built-in/template dispatch (six failures) or runtime insertion and key
collection (four failures including keys). Breaks were restored. Translation
reordering independently failed before fixing a percent-format substitution.
Final gate results and commit identity are reported to the parent.

## Independent review corrections

Rebased onto master 8da0740; the original implementation rebased to 70e7b01.
The collision regression registers command 1 with eleven aliases in one file,
then commands 2 through 11 in another file of the same addon. Exact dispatch
must reach both command 1 alias 11 and command 11 alias 1.

The fallback allocation regression warms an interpolated and a literal label,
stops GC and checks growth below 1 KiB around 1,000 calls to each. The broken
runtime grew about 141 KiB. The log regression asserts exact info level and
`100% true` message. Literal-brace and cache tests check rendered text and
that a repeated translation retains the same parsed format entry.

The slash-and-hooks example shares its real slash callback with PLAYER_LOGIN,
shows a hooked frame, reads a hooked clock and logs both actions. Hand-computed
output is Status panel ready, 3 tasks ready, then Clock: 0.

## D1 integration

Rebased onto master 0778afe. The loader uses reserved __selu_addon/__selu_ns
locals, so user ADDON/ns variables cannot collide. The event registration
closure occupies __selu_ns[true]; WoW state occupies __selu_ns[false], avoiding
module path keys (strings) and retaining one log buffer, warn-once set, slash
counter, locale table and parsed-format cache for the entire addon.
The runtime closes over the first file's reserved addon identity and shared
namespace. Each later file aliases the existing runtime with local __selu_wow.

The public lower_module path initializes WowState and returns its locale_keys.
Its runtime block is inserted after the existing two loader statements, before
module declarations or user code. Files without import/export/event syntax use
lower's equivalent WoW initialization. Plain files without WoW uses retain their
original output. Standalone event-only lower output keeps its existing event
runtime; project event modules retain D1's shared registration closure.

The multi-module regression builds a real addon with imports, loads its files
in TOC order with Sim::load_addon, checks ordered shared logs and warnOnce,
dispatches both commands, shares translations, fires handlers, verifies user
ADDON/ns bindings, and checks exactly one reserved loader per generated file.
A direct lower_module call also checks collected locale keys. Before integration
loading first.lua failed with an attempt to index missing __selu_wow.

Keybindings are documented in [bindings.md](bindings.md).

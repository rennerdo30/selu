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
only after lowering encounters a built-in. D1 can call it after lowering its
module body, inserting the result before that body alongside the event prologue.
The runtime obtains `local ADDON, ns = ...`; `ns.wow` is created only once.
Multiple files loaded with the same namespace share commands, levels, buffer,
once set and translations. Standalone load_files falls back to a file-local table.

`ns.wow.buffer` holds at most 200 `{level, message}` entries, oldest first.
`ns.wow.levels` enables info/warn/error/warnOnce initially; debug is disabled.
Generated if guards surround interpolation evaluation and string.format, so a
disabled level evaluates no arguments and builds no string. warnOnce uses the
English template key (not interpolated values), shared across files. The once
set retains one entry per encountered template. Buffer overflow removes the
oldest entry; logging is diagnostic, not a zero-allocation hot-path operation.
Tests inspect the buffer using a Lua chunk receiving the same addon namespace.

Slash keys use SELU_<addon>_<monotonic command number>, with aliases numbered
from 1. The per-addon counter prevents collisions between files. Generated calls
are `__selu_wow.command("/addon", "/a", function(args) ... end)`; the runtime
writes SLASH_<key>1 etc. and SlashCmdList[key]. Sim::slash dispatches aliases
case-insensitively, passing the rest of the line as a string.

L templates collect English keys using {0}, {1}, etc. in Lowered.locale_keys.
`ns.wow.locales[GetLocale()][key]` contains translated sentences with those
placeholders. Missing entries use the English format. The format adapter returns
a percent-escaped format and reordered arguments to a single string.format call;
repeated placeholders also work. English fallback requires no temporary table.
Translation validation/file writing belongs to the later locale build pipeline.
Joining two direct L templates warns SELU0444; warning analysis is syntactic.
Interpolation accepts non-null strings, numbers and booleans; tostring gives
booleans Lua 5.1-compatible formatting. All literal percent signs are escaped.

Shared edits: checker lib.rs binary + calls wow_join; its unsupported-construct
fixture drops only L. Lowerer lib.rs adds WowState, initializes it, inserts the
optional prologue after lowering and transfers keys into Lowered.locale_keys.
Simulator lib.rs adds Sim::slash, HookScript in CreateFrame and dispatches OnShow
when Show transitions from hidden. No CLI pipeline changes are needed.

Diagnostics SELU0440-0444 cover arity, alias/hook name, callback, template types
and sentence joining. Other unsupported template tags retain SELU0390.

Verification: all seven D4 tests were demonstrated red by temporarily disabling
checker built-in/template dispatch (six failures) or runtime insertion and key
collection (four failures including keys). Breaks were restored. Translation
reordering independently failed before fixing a percent-format substitution.
Final gate results and commit identity are reported to the parent.

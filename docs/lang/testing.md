# Testing addons

Run `selu test [project] [--target flavour@version] [--filter text] [--ui core|full|none] [--jobs n] [--json]`.
The default target is the first configured target. The runner discovers sorted
`*.test.selu` files under `src/` and `tests/`. It builds the project with the same
compiler as `selu build`, including project modules imported by tests, then loads
the generated addon through its TOC. Test files are not shipped in the addon.

Each test file runs in its own fresh session. `--jobs n` (`-j n`) runs that
many files at once on worker threads; the default is half the cores, at most
4. A session with Blizzard's Core UI holds about 1 to 2 GB, so more jobs need
that much more memory. The report keeps file order whatever the job count,
and `--jobs 1` runs one file after another. `SELU_TEST_TIMINGS=1` prints each
phase's time and the process's private memory to stderr.

```selu
import { total } from "../src/prices";

describe("prices", () => {
  beforeEach(() => { sim.setMoney(100); });
  test("adds prices", () => {
    expect(total([10, 20])).toBe(30);
  });
});
```

## Blizzard's UI in tests

`--ui core` (the default), `--ui full` or `--ui none` chooses how much of
Blizzard's own UI each session runs before the addon loads. It is read at
runtime from the user's installed client of the target's product, found as
Studio finds it (`--client-install`, Studio's settings, or the usual install
folders), or from a configured source checkout on the product's branch.
Nothing of Blizzard's is shipped or cached in the project. Core is the same
lean real subset Studio loads by default, and Studio's `core_ui_addons`
setting applies too.

Without an installed client (for example on CI) the run prints
`running without Blizzard's UI: <reason>` and continues. Tests that use
Blizzard frames (such as `Minimap`) then fail honestly instead of passing
against a stand-in. Use `--ui none` for tests that never touch Blizzard's
UI; it is also faster, because Core takes about 3 seconds to start per file.

"not simulated" notes are filed by who reached the gap. Notes from
Blizzard's own startup, or reached only by Blizzard's secure code afterwards,
belong to the client. They stay out of the test's report and out of
`--strict`, though Studio still shows them. Notes reached by the addon's
code, by Blizzard code the addon called into, or by the test itself are the
addon's.

Each file has a fresh deterministic Session, seeded with 1. Tests within a file
share that Session. Hooks run outermost first before each test. Describe blocks
register tests immediately; hooks and bodies run later. Filtering matches the
joined describe and test names. Registration still runs when filtering.

`expect(value)` supports `toBe` (identity), `toEqual` (deep table comparison),
`toBeNull`, `toThrow`, `toContain`, `toBeLessThan`, `toBeGreaterThan`,
`toHaveBeenCalled`, `toHaveBeenCalledWith`, and `.not`. Pass a function to
`toThrow`. String containment is literal, not a Lua pattern.

`sim.spy("print")` wraps a named global function and preserves its result. Use
`expect(spy).toHaveBeenCalledWith("message")`. Spies belong to the VM generation
in which they were installed; install a new spy after reload.

Helpers:

| Helper | Action |
| --- | --- |
| `sim.fire(event, ...args)` | Dispatch an event |
| `sim.slash(line)` | Run a registered slash command |
| `sim.advanceTime(seconds)` | Advance the simulated clock; negative/nonfinite time is an error |
| `sim.reload()` | Save and reload the addon in a new VM |
| `sim.now()` | Read the current Session clock in seconds |
| `sim.setMoney(copper)` | Set player money |
| `sim.setCombat(boolean)` | Enter or leave combat |
| `sim.setUnit(token, fields)` | Set name, class, level, health, maxHealth, power, maxPower and combat; also `xp`, `maxXp`, `xpDisabled`, `played` and `levelPlayed` (player; `RequestTimePlayed()` answers with TIME_PLAYED_MSG on the next frame), `distance` (yards), `distanceChecked` and `visible`. A changed player XP fires PLAYER_XP_UPDATE |
| `sim.setSpell(id, { name, icon, known })` | Define a spell for IsPlayerSpell, IsSpellKnown and C_Spell (self-written test data; fires SPELLS_CHANGED) |
| `sim.setTracking([{ name, texture, active, type, spellID }])` | The minimap tracking list for C_Minimap.GetNumTrackingTypes/GetTrackingInfo (fires MINIMAP_UPDATE_TRACKING) |
| `sim.setZone({ zone, subzone, map, instance })` | Zone text, map and IsInInstance's type ("none", "party", "raid", ...) |
| `sim.setCVar(name, value)` | Change a CVar as the game's own settings would (fires CVAR_UPDATE); SetCVar from the addon fires it too |
| `sim.lockCVar(name)` | The addon's SetCVar on it returns but changes nothing, as for a CVar the client refuses |
| `sim.prints()` | Read captured prints as an array |
| `sim.frame(name)` | Look up a named frame; null if missing |

Test intrinsics are supplied by a test-only manifest overlay. Using them from
ordinary source is SELU0620. Invalid test imports are SELU0621; invalid time is
SELU0622. Stale generation use is SELU0623. Other argument/type errors use the compiler's existing diagnostics.

The Rust runner retains registrations and test locals across reload. Test bodies
remain in their original VM. Calling an obsolete game API (including a captured
API alias), invoking an imported function from that VM, or calling a method on
an old frame raises **SELU0623: stale reference**. Stale code never silently reads
an obsolete clock or world. This explicit rejection is the current fallback;
transparent rebinding of imported code across reload is not implemented.
If addon loading or login throws after VM replacement, the retired generation is
still invalidated. An error before replacement leaves the existing generation current.

Use Session helpers after reload. `sim.now()` reads the current clock, and
`sim.frame(name)` looks up the current generation's frame. Lookups are canonical
within a generation; old frames and replacement frames have different identities.
Frame methods preserve multiple results and translate frame arguments, returned
frames, scripts and callbacks between the Session and test VMs. For example,
`frame.GetParent()` returns the same handle as looking up that parent by name.

Spy arguments remain owned by their VM. Functions, frames, cyclic tables and
trailing null arguments are recorded without SavedVariables serialisation. Spy
wrappers preserve the original return values. Cross-VM table values are traversed
through an explicit Lua 5.1 bridge iterator, so deep comparison and negation
compare their entries, including nested and cyclic tables. Bridge handles use registry keys,
weak VM references and explicit cleanup when each test file ends.

Tests use strict API checking. Registration callbacks must be functions; matcher
arguments are required, and numeric comparisons require numbers. `null` remains
a valid required argument to equality matchers. Invalid argument calls are build
errors (exit 2), rather than runtime assertion failures.

Test imports currently refer to project modules under `src/`, not shared test
helper modules under `tests/`.

`perf.allocations(fn)` is explicitly unsupported: it fails with
`not simulated: perf.allocations`, also included in the report notes. No guessed
allocation counts are returned. Other SPEC performance helpers are future work.

Assertions and registration run under the simulator instruction budget. Structured runtime failures from timers, OnUpdate and queued/flushed frame callbacks fail their owning test, including after reload. Addon load/login exceptions become source-mapped setup failures (exit 1); other test files still run. Registration dispatch is checked before observations are cleared; failures raised during PLAYER_LOGOUT and its final flush are collected after every reload attempt, including failed attempts. Failures
include source-mapped Lua errors, expected/actual assertion text, simulator gap
notes, and a replay record containing the seed and Session host commands. Replay
records cover host interaction, not arbitrary mutations made by imported Lua
functions. Notes include addon setup gaps and gaps observed during that test, including before reload.

The default report is human-readable; `--json` emits `selu_test::Report` on stdout.
Exit codes: 0 for all selected tests passing, 1 for test failures, 2 for usage,
configuration, discovery or build errors. No matching tests is a successful empty
report. `selu_test::run` is a synchronous library API; it builds to the project's
`dist/` directory and does not start a subprocess or Studio.

See `examples/hello/tests/` and `examples/classes/tests/` for executable examples.

For port CI, `selu test --strict` fails empty selections and any scenario that
encounters a simulator gap, including gaps reported during boot or callbacks.
Ordinary `selu test` retains its report-only gap behavior.

A test file may have a data-only sibling: `tests/a.test.scenario.json` for
`tests/a.test.selu`. The checked fixture schema has optional `character` and
`saved` fields and rejects other fields. `saved` maps TOC-declared global names
to the serialized `SavedValue` data enum (`Nil`, `Boolean`, `Number`, `String`
with UTF-8 byte arrays, and `Table` with key/value pairs). For example:

```json
{"character":"Player","saved":{"MyAddon_settings":{"Table":[[{"String":[99,111,117,110,116]},{"Number":41}]]}}}
```

Fixtures seed the ordinary loader before addon load and ADDON_LOADED, including
malformed historical values; undeclared globals and late seeding fail. No Lua is
loaded from the fixture. `sim.switchCharacter("Other")` persists the old character,
loads the next character's saved tree and performs the usual login sequence.
Account data stays shared. Like reload, it retires old production handles. Use
fresh `sim.frame`, `sim.prints`, and `sim.callbackCounts` observations afterward;
imported old-generation functions remain invalid. Counts reset with the VM.
Inventory/aura/quest/taxi/CVar/message/native-frame fixtures belong to later waves.

SELU0631 identifies invalid scenario schemas or saved fixtures.
Fixture failures retain a structured diagnostic with the fixture pathname and a
source span. Schema failures underline the relevant JSON key where available;
saved-global validation underlines that global's key within `saved`, including
escaped JSON keys. Invalid JSON retains its parser position.

A scenario's `"character": "Name-Realm"` names the player and the realm
separately, as `UnitName("player")` and `GetRealmName()` answer in game, so a
realm with spaces (`"Kripz-Camp Realm 5"`) can be tested. Spells and tracking
not defined by a test keep the simulator's stand-in answers.


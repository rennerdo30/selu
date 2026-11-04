# Testing addons

Run `selu test [project] [--target flavour@version] [--filter text] [--json]`.
The default target is the first configured target. The runner discovers sorted
`*.test.selu` files under `src/` and `tests/`. It builds the project with the same
compiler as `selu build`, including project modules imported by tests, then loads
the generated addon through its TOC. Test files are not shipped in the addon.

```selu
import { total } from "../src/prices";

describe("prices", () => {
  beforeEach(() => { sim.setMoney(100); });
  test("adds prices", () => {
    expect(total([10, 20])).toBe(30);
  });
});
```

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
| `sim.setUnit(token, fields)` | Set name, class, level, health, maxHealth, power, maxPower and combat |
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

Use Session helpers after reload. `sim.now()` reads the current clock, and
`sim.frame(name)` looks up the current generation's frame. Lookups are canonical
within a generation; old frames and replacement frames have different identities.
Frame methods preserve multiple results and translate frame arguments, returned
frames, scripts and callbacks between the Session and test VMs. For example,
`frame.GetParent()` returns the same handle as looking up that parent by name.

Spy arguments remain owned by their VM. Functions, frames, cyclic tables and
trailing null arguments are recorded without SavedVariables serialisation. Spy
wrappers preserve the original return values. Bridge handles use registry keys,
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

Assertions and registration run under the simulator instruction budget. Structured runtime failures from timers, OnUpdate and queued/flushed frame callbacks fail their owning test, including after reload. Addon load/login exceptions become source-mapped setup failures (exit 1); other test files still run. Failures
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

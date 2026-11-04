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
| `sim.setMoney(copper)` | Set player money |
| `sim.setCombat(boolean)` | Enter or leave combat |
| `sim.setUnit(token, fields)` | Set name, class, level, health, maxHealth, power, maxPower and combat |
| `sim.prints()` | Read captured prints as an array |
| `sim.frame(name)` | Look up a named frame; null if missing |

Test intrinsics are supplied by a test-only manifest overlay. Using them from
ordinary source is SELU0620. Invalid test imports are SELU0621; invalid time is
SELU0622. Other argument/type errors use the compiler's existing diagnostics.

The Rust runner retains registrations across reload. Named frame proxies resolve
against the current Session after reload. Existing imported module references and
direct game API references in test closures still belong to the original VM;
after reload, use Session helpers to observe the reloaded addon. This is a known
limitation, not complete reload transparency. Frame proxy methods support scalar
and data-table arguments/results; passing other frame proxies is not supported.
Test imports currently refer to project modules under `src/`, not shared test
helper modules under `tests/`.

`perf.allocations(fn)` is explicitly unsupported: it fails with
`not simulated: perf.allocations`, also included in the report notes. No guessed
allocation counts are returned. Other SPEC performance helpers are future work.

Assertions and registration run under the simulator instruction budget. Failures
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

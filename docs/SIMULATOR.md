# Simulator, `selu test` and Selu Studio: architecture

Status: groundwork (2026-11-04). The goals come from DESIGN.md section 7 and
SPEC-V1.md section 8; this document decides how they are built so the work
can be split into packages (`docs/WORK-PACKAGES.md`, rounds 8 to 10).
Everything here can be revised by a package that finds a better way, but the
change goes into this file in the same commit.

## Where we are

`crates/selu-sim` (about 1,000 lines) is a headless core: an mlua Lua 5.1 VM,
captured `print`, a stateful but layout-free region model written in Lua
(`regions.rs`), events fired from Rust, timers and `OnUpdate` on a simulated
clock (`timers.rs`), SavedVariables helpers (`saved.rs`), `not simulated: X`
logging, and round 7's combat and secret-value hooks. Tests drive it through
`Sim::load_addon`, `fire`, `slash`, `eval`, `advance_time`, `prints`, `logs`.

The lowering already writes a simple line map per file
(`dist/.selu/<Addon>/<file>.lua.map.json`, `selu_lower::SourceMap`: one
entry per printed statement); generated Lua keeps descriptive names, and
thrown `Error`s carry `file.selu:line`.

### Code groundwork (2026-11-04, before round 8)

So round 8 starts from compiling, tested code, not from prose:

| Where | What exists | Who continues |
|---|---|---|
| `selu-sim/src/core.rs` | The S0 contract, done: `RegionId`, `Generation`, `Ctx` (install context: prints, logs, `not_simulated`, clock, trace, budget, flush queue), `dispatch` (outermost call resets the budget and flushes queued callbacks), `install_hooks` (count hook; once exhausted it fails on every instruction, so `pcall` can't keep a runaway loop alive), snapshot view types (serde), `RegionModel` trait, `Trace` ring buffer | contract changes go through the parent |
| `selu-sim/src/lib.rs` | `Sim` uses `Ctx`; `eval`, `load_files`, `load_addon`, `slash`, `fire`, `advance_time` go through `dispatch`; `print` and `fire` record trace entries; `Sim::snapshot()` | S2 inverts `Sim`/`Session` |
| `selu-sim/src/frames/` | `LuaTableModel`: `RegionModel` over today's Lua tables (frames, textures and font strings, names, parents, size, a basic layout from points and size, colour and file textures, text, alpha, shown/visible, scripts; no scale, strata or levels) | S1 replaces it with the arena |
| `selu-sim/src/session.rs`, `loader.rs`, `snapshot.rs` | `Session` wraps `Sim`: `SessionConfig`, `load_addon_dir` through `<Folder>.toc` (XML is a visible error), `login`, `reload` (new VM, new generation; no saved data yet), seeded `math.random`, `snapshot`, `trace_since` | S2 |
| `selu-sim/src/world.rs`, `trace.rs` | owner notes only | S2 |
| `selu-sim/tests/addons/Hello/` | plain Lua test addon with a `.toc` | everyone adds addons here |
| `crates/selu-sourcemap` | decision 23's format, reader and writer, both lookups, `map_lua_error` | S3 makes the lowering and `selu build` emit it |
| `crates/selu-test` | discovery, project compilation, test manifest overlay, Rust registrations and hooks, assertions, spies, Session helpers, source-mapped reports and replay; limitations in `docs/lang/testing.md` | S4 |
| `crates/selu-debug` | binary `selu-studio`; `Worker` owns a `Session` on its own thread; JSON `Command`/`Update` protocol; `--headless` mode; **the Studio window (S5's first version, built by the parent on the owner's request, 2026-11-04)**: eframe/egui on wgpu, toolbar (/reload, rebuild and rebuild on save by polling `src/`, play/pause, time steps, screenshot), frame tree, game screen drawn from snapshots with hit-testing, inspector, console (Lua and slash commands), event trace with "fire event"; `SELU_STUDIO_SCREENSHOT=file.png` saves one screenshot and closes, for checks; **the code editor** (owner, 2026-11-04: Studio is the place to write code): file list, tabs, syntax colours, line numbers, errors while typing from the real compiler (a background `selu build` of a mirror under `dist/.selu/check/` with the unsaved text), Problems list, completion and hover from the target manifest and the project, save with rebuild and reload | S5 polishes (strata/level draw order once S1 lands, real textures and fonts, anchors drawn, timeline scrubbing), S6 the debugger |
| `selu test`, `selu debug` | `selu test` calls `selu_test::run`, with filtering and JSON reports; `selu debug` builds, finds the built folder, starts `selu-studio` beside `selu` or on the path | S4, S5 |

## Decisions

1. **One core, three front ends.** `selu-sim` is the only place that knows
   how the client behaves. `selu test`, Selu Studio and the existing Rust
   tests all drive the same `Session` API. Nothing client-like is
   implemented in a front end.
2. **Crates.**

   | Crate | Role | Depends on |
   |---|---|---|
   | `selu-sim` | core: VM, world, frames, layout, events, timers, saved data, combat, secrets, trace, snapshots, debug hooks | `mlua`, `selu-manifest` |
   | `selu-sourcemap` (new, small) | the map format, writer and reader | none |
   | `selu-lower` / `selu-cli` | emit `.lua.map` next to each generated file | `selu-sourcemap` |
   | `selu-test` (new) | the `*.test.selu` runner, `expect`, spies, reporters | `selu-sim`, compiler crates |
   | `selu-debug` (new, binary `selu-studio`) | the Studio app | `selu-sim`, `selu-sourcemap`, GUI crates |

   The GUI dependencies live only in `selu-debug`. `selu` stays a lean
   compiler binary; `selu debug` builds the addon, then starts
   `selu-studio` (found next to the `selu` executable, or on the path)
   with the built folder and target. If it isn't installed, the error says
   how to get it. This keeps rule two for everyone who only compiles.
3. **Frames are Rust data.** The region model moves from Lua tables to a Rust
   arena: each frame, texture and font string is a Rust struct in a
   `Vec<Region>` with a generational id, and the Lua side holds a light
   userdata or a small table with that id, plus a per-object Lua table for
   addon fields (`frame.myField = 1` must keep working, as in the client).
   Methods are Rust functions registered once per widget type, following the
   manifest's widget method lists (`widgets`, `inherits`). The inspector,
   the layout engine and snapshots then read plain Rust state, with no Lua
   calls. A method the manifest lists but the core doesn't model logs `not
   simulated: Frame:Method` once and returns the documented empty value
   (SPEC 8).
4. **Layout** resolves anchors to rectangles in UIParent coordinates, lazily
   when something reads geometry or a snapshot is taken, cached until a
   dirty flag is set. It covers what SPEC 8 lists: points (including
   `SetAllPoints`, two-point stretching), size, scale and effective scale,
   strata, frame level, draw layer and sublevel, alpha, shown and visible
   propagation, mouse enable, hit testing in draw order, click, drag.
   UIParent defaults to 1920 x 1080 at scale 1 and can be changed per
   session. Anchor cycles and unresolvable anchors are logged and the frame
   gets no rectangle, as the client does.
5. **Session API** (the contract between core and front ends; names may
   change, the shape shouldn't):
   - `Session::new(SessionConfig { target, manifest, ui_size, seed })`.
   - `load_addon_dir(path)`: reads the `.toc` (any addon, not only Selu
     output; `## Interface`, `## SavedVariables`, `## SavedVariablesPerCharacter`,
     file list, XML files listed as unsupported for now), runs the files in
     order with `...` = addon name and table, then the boot sequence
     `ADDON_LOADED` (saved data loaded first), `PLAYER_LOGIN`,
     `PLAYER_ENTERING_WORLD`. Several addons can load into one session.
   - `reload()`: saves SavedVariables as the client does on logout, drops
     the VM, starts a new one and loads everything again (DESIGN `sim.reload()`).
   - World: `set_unit`, `set_combat`, `set_secret(api)`, `set_money`, bags
     and the rest of SPEC 8's units list; each change fires the events the
     client would.
   - Time: `advance_time`, `run_until_idle`.
   - Input: `click(region, button)`, `hover`, `drag`, `type_text`, `slash`.
   - Output: `prints`, `errors` (client-style, with stack), `trace`,
     `snapshot()`.
   - Debugging: `set_breakpoints(file, lines)`, `pause`, `step_in/over/out`,
     `stack()`, `locals(frame)`, `eval_in(frame, code)`.
6. **Threading.** The core is single-threaded and owns its `Lua`. Studio runs
   the session on a worker thread and talks to it through two channels:
   commands in, events/snapshots out. The GUI never touches Lua. This also
   makes pausing at a breakpoint simple: the worker blocks inside the Lua
   hook waiting for the next command.
7. **Trace.** Every event dispatch, script call (`OnEvent`, `OnUpdate`
   aggregated per second, `OnClick`...), hook, timer callback, protected call
   and its deferral, error and print is appended to a bounded ring buffer
   (`TraceRecord { time, frame_no, kind, target, args, source }`), with
   arguments rendered to short strings. Off by default in `selu test`
   (cheap counter only), on in Studio. "Scrub back to see the frame tree at
   an earlier moment" uses snapshots taken at trace marks (one per event
   dispatch by default, bounded), not a full VM rewind.
8. **Snapshots** are plain serialisable data (`serde`): the region tree with
   resolved rectangles, visibility, strata, level, layer, alpha, texture
   path or colour, text and font, scripts present, and the creating source
   location. Studio draws from snapshots only. Tests can compare snapshots
   too (`expect(frame).toMatchSnapshot()` later, not in v1).
9. **Source maps.** `selu build` writes `Name.lua.map` beside each generated
   file: JSON, one entry per generated line range (`gen_line_start`,
   `gen_line_end`, `source`, `line`, `column`), sorted, no VLQ (readable and
   small enough for addon sizes). The lowering already knows each
   statement's span; the printer records the generated line when it emits a
   statement. Maps aren't listed in the `.toc` and the game ignores them.
   Studio and `selu test` map Lua errors, stack frames and breakpoints
   through them; `selu export` (later) leaves them out.
10. **Debugger** uses Lua's `debug.sethook` with a line hook installed only
    while breakpoints exist or stepping is active (no cost otherwise), mapping
    `.selu` lines to generated lines through the source map, and plain `.lua`
    files map to themselves. Variables come from `debug.getlocal`, with
    Selu names (generated names keep the source names, temporaries start
    with `__selu` and are hidden by default). The Debug Adapter Protocol for
    VS Code is a later adapter over the same commands.
11. **`selu test`** finds `*.test.selu` files, compiles each with the
    project's modules for the chosen target, loads the addon into a fresh
    session per test file (per test with `beforeEach` semantics as in Jest:
    the session is fresh per *file*, and `beforeEach` runs in it), and runs
    `describe`/`test`/`expect` from SPEC 8. `sim` and `perf` are
    test-only intrinsics typed through a test manifest overlay, lowered to
    calls into the core's Lua bridge. Output: human by default, `--json`
    for tools; exit code 1 on any failure. A failing test can be opened in
    Studio at the failing moment (`selu debug --test file.test.selu --name "..."`).
12. **Studio GUI:** `eframe`/`egui` on the `wgpu` backend (native, GPU
    drawn, immediate mode, as SPEC 10 says), newest stable versions checked
    on crates.io when the package starts. The game screen is an egui canvas
    painting the snapshot (rectangles, texture images loaded from the addon's
    own TGA/PNG files, text with one bundled open font, labelled placeholders
    for Blizzard textures). Panels: game screen, frame inspector, event
    timeline, world, console, errors, saved data, later memory. Rebuild on
    save watches the project's sources (`notify`) and calls `reload()`.
13. **Determinism.** No wall clock, no OS randomness: `math.random` is
    seeded from `SessionConfig.seed`, `time()`/`date()` come from a simulated
    calendar, and `GetTime` from the simulated clock. The same test gives the
    same trace every run.
14. **Honest limits** stay as DESIGN section 7 says: the core models the
    client from manifests and documented behaviour; anything else is logged
    as not simulated, never invented.

Refinements after Sol's second opinion (2026-11-04):

15. **Frame identity (refines 3).** Each region is a canonical Lua **table**
    (not bare userdata) holding a private key with its arena id; addon fields
    live on that table, missing fields read `nil`, methods come from a shared
    per-widget metatable `__index`, and an addon may override a method on its
    own frame, as in the client. The same region always returns the same
    table. Regions are never freed while Lua can reach them (frames can't be
    destroyed in the client either). Rust never holds an arena borrow across
    a call into Lua.
16. **Layout timing (refines 4).** Reading geometry or taking a snapshot
    resolves layout but never runs addon code. Callbacks that layout causes
    (`OnSizeChanged`, `OnShow` from a parent being shown) are queued and
    flushed at defined dispatch points: after each event dispatch, timer
    callback, script call and input action, before control returns to the
    host.
17. **Hooks and pausing (refines 6 and 10).** A coarse count hook (every N
    instructions) is always installed by the host: it enforces an
    instruction budget per dispatch (a runaway loop becomes a logged error,
    not a hang) and lets `pause` interrupt a busy loop. Line hooks are added
    only while breakpoints exist or stepping is active. While paused, the
    worker serves a restricted command set: read stack, locals, upvalues and
    snapshots, set breakpoints, resume, step, disconnect. Evaluating code and
    firing events while paused are queued and run after resume (Lua 5.1
    disables hooks inside code a hook runs, so it couldn't hit breakpoints
    and could hang); evaluating in the paused frame may come later, behind an
    explicit "may run code" toggle. Simulated time is frozen while paused.
    Snapshot delivery to the GUI is coalesced and bounded in bytes; every
    snapshot carries a session generation number that changes on
    `reload()`, so a stale selection is dropped.
18. **Loading scope.** Loading an addon (files, `ADDON_LOADED`) is separate
    from the once-per-session login sequence (`PLAYER_LOGIN`,
    `PLAYER_ENTERING_WORLD`), which `Session::login()` fires after every
    initially loaded addon. Flavour-specific `.toc` files (`Name_Mainline.toc`,
    `Name_Vanilla.toc`, ...) are chosen per target as the client does;
    `## Dependencies`/`## RequiredDeps` order loading; `## LoadOnDemand`
    addons load through `C_AddOns.LoadAddOn`. **XML files** listed in a
    `.toc` are an explicit, visible "not supported yet" error in round 8 (a
    later package parses `<Frame>`, templates, `<Script>` and `<Include>`,
    reusing M3's XML reading); they are never skipped silently.
19. **Input and text.** Cursor coordinates are screen pixels with the origin
    at the bottom left, as `GetCursorPosition` returns them; addons divide by
    the effective scale as in the client. Mouse capture and keyboard focus
    (`EditBox`, `SetFocus`, `ClearFocus`, `OnEnterPressed`) are part of S1's
    input model. Text measurement (`GetStringWidth`, wrapping, font sizes)
    uses one bundled open font through one shared measuring function in the
    core, which the Studio renderer also uses, so both agree.
20. **Saved data and isolation.** Saved files are namespaced by session,
    account, realm and character, and target; two sessions never share
    state. `run_until_idle` stops at the next pending timer, or after a
    bounded simulated time when tickers or `OnUpdate` keep work pending.
21. **Secure templates.** Secure templates (`SecureActionButtonTemplate`,
    secure handlers, attribute-driven actions) are not modelled in v1:
    creating one logs `not simulated`, and protected actions fail the way
    round 7's model says. A test that depends on unmodelled behaviour reports
    it in its output, so it can't pass silently.
22. **Test replay.** `reload()` replaces the whole VM; the test runner's own
    state lives in Rust, outside it, and survives. A failing test records its
    seed and the ordered list of host commands; `selu debug --test` replays
    them into a fresh session to reach the failing moment.
23. **Source map details (refines 9).** Each map has a `version`, the chunk
    name the client reports for the generated file, normalised
    project-relative source paths, each source file's content hash, ranges
    sorted by generated line with the innermost range winning, and unmapped
    lines explicitly absent. Lines inside function expressions (callbacks)
    map to their own statements, not to the statement containing the
    function (the printer's current single-line simplification for function
    expressions in `selu-lower/src/lua.rs` has to go).

## Order of work

| Round | Packages | Depends on |
|---|---|---|
| 8a | S0 core contract and wiring (one agent, small); S3 source maps may start alongside | round 7 merged |
| 8b | S1 frames and layout in Rust; S2 sessions, loader, world, trace and snapshots | S0 |
| 9 | S4 `selu test`; S5 Studio shell (game screen, inspector, timeline, console, world panel, rebuild on save) | S1, S2 (S5 also S3 for error links) |
| 10 | S6 debugger (breakpoints, stepping, variables, console in frame); S7 Studio saved-data view, target switch, error panel polish | S3, S5 |

S1 and S2 both touch `selu-sim`, so S0 first fixes the shared contract
(region ids, the install context, script dispatch and flush points, layout
resolution, the immutable snapshot view) and does all wiring in `lib.rs`;
after that S1 and S2 own disjoint files. The existing `Sim` API keeps working
through round 8 as a thin wrapper over `Session`, so no other test has to
change at once.

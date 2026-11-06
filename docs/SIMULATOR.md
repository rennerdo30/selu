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
   with the built folder and target. A failed build still opens the
   project: Studio shows the errors and loads the addon once it builds
   (File > Open Project works the same way, in place). If Studio isn't
   installed, the error says how to get it. This keeps rule two for everyone who only compiles.
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
     file list, XML templates and includes), runs the files in
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
    `.toc` load frames, templates, scripts and includes; unsupported elements
    produce diagnostic notes. See the default UI implementation notes below.
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


## SIM2 rendering model (2026-11-06)

Region snapshots preserve arena indexing for selection and references. Studio
sorts a separate drawing view by strata, level, draw layer, sublevel and arena
index (creation order); the simulator uses the same key for input. Raise and
Lower choose an extreme level within the current strata. ScrollFrame owns a
scroll-child relationship and offsets its child's resolved anchor geometry.
Snapshots intersect every clipping ancestor; Studio applies that intersection
to all primitives and both front ends apply it to picking.

`VisualStyle` carries text and texture state. Font metrics and outlines use
ab_glyph 0.2.32 (Apache-2.0); shaping uses Studio's existing harfrust 0.12
(MIT) for OpenType kerning, ligatures and glyph clusters. Requested sizes are
em pixels. Line height is ascent minus descent plus line gap and SetSpacing.
The fallback is Studio's bundled Ubuntu Light, never an OS-dependent font.
Addon fonts resolve beneath roots registered by the single call in
Session::load_into before chunks run; reload and load-on-demand repeat it for
the new VM. Absolute paths, traversal and canonical symlink escapes are refused.

Simulator layout, text getters, wrapping and truncation use one font backend.
Snapshots carry its positioned glyphs and distinct raster masks. Studio draws
those exact masks at the supplied baselines, without a second text engine.
Text layout supports space-based wrapping, long-word splitting via
SetNonSpaceWrap, SetMaxLines, explicit height limits and ellipsis. Inline
colours preserve clusters; |T...|t inserts sandboxed textures with dimensions,
offsets and optional UVs. FontString templates automatically inherit through
the third CreateFontString argument. Font objects retain live font/style
inheritance and region overrides. Client fonts and atlas metadata are not
shipped, so exact native client typography remains an integration limit.

Media reads are canonicalized beneath explicitly loaded addon directories,
including after symlink resolution. Studio caches successful and failed texture
lookups. image 0.25.10 (MIT OR Apache-2.0) decodes PNG/TGA and the DXT blocks in
BLP2; the small BLP2 adapter also supports palette and BGRA mip zero. Reads are
limited to 32 MiB and 4096 pixels per dimension. BLP1, unavailable Blizzard
assets and atlases produce labelled placeholders. Four-value UV crops, tint
and effective alpha are supported; arbitrary eight-value UV transforms and
nonstandard blend modes remain fidelity limits.

Font objects use `sim_globals::create_font(lua, name)`. `fonts::create_font` is
only a re-export. Session stand-ins and CreateFont share the SIM1 constructor;
there is one constructor and one global installer after integration.

Manifest-free sessions resolve six standard GameFont Normal/Highlight templates
through a private native-template cache and the same constructor, without
inventing Lua globals. Named profiles use the Roman 12/10/16 shadow faces from
Blizzard's [FontStyles.xml](https://github.com/Gethe/wow-ui-source/blob/live/Interface/AddOns/Blizzard_Fonts_Shared/Shared/FontStyles.xml)
and [Fonts.xml](https://github.com/Gethe/wow-ui-source/blob/live/Interface/AddOns/Blizzard_Fonts_Shared/Shared/Fonts.xml).
Other missing templates are explicit errors; manifest-declared Font globals
continue through SIM1's typed shape lookup.
The committed visual fixture is `crates/selu-sim/tests/fixtures/rendering`.

## SIM1: loading real addons

### Default UI startup (SL-23)

Studio's Game Installs settings include Default UI (enabled when the source is
available for a wowf target) and a source checkout path. The initial source is
`E:/Development/wow-ui-source`; it is read-only. Library sessions remain opt-in
through `Session::load_default_ui`. With a configured client, TOC lists, TOCs,
Lua and XML bytes prefer local CASC; unavailable entries fall back to the source
checkout with a note. The checkout is still required for directory discovery
and canonical path containment. Sources for other targets are not selected by
switching that checkout's branch.

XML parsing uses roxmltree 0.21.2 (MIT OR Apache-2.0). Includes retain source
order and are bounded by canonical addon containment, a 64-file include depth,
a 128-element nesting depth and a 16 MiB input limit. External entities are not
enabled. Templates and intrinsic types belong only to the simulator VM; this
does not add Lua interop or declarations to the Selu language.

The loader promotes LoadFirst roots, traverses required dependencies, excludes
glue-only roots, and leaves LoadOnDemand roots unloaded unless required. Game
filters understand Camelot's Mainline family and Classic family members.
Each Blizzard source chunk has an error boundary; the receipt retains failures
and the worker continues. The user's addon loads afterward, followed by the
existing login sequence. Studio reports progress, counts, errors and wall time
in the console, plus the most frequently called manifest native stand-ins.
Reload rebuilds the default UI before the user's addon.

The synthetic profile exposes one visible General chat window and selects the
first of Blizzard's own edit-mode presets through `EDIT_MODE_LAYOUTS_UPDATED`.
It never reads the client's saved layouts. XML `relativeKey` anchors can walk
anonymous parents and child members. Animation groups exist before OnLoad;
Alpha and Translation use the existing simulator animation clock. Other
animation types remain diagnostic notes. Initial XML visibility does not fire
the instance's inherited visibility script before OnLoad.

Persistent arena tables and method-lookup tables use owning registry keys with
weak VM references, rather than Lua 5.1's bounded mlua auxiliary stack. Region
clones share ownership; the final owner removes the registry key. Session
replacement releases the old VM. No region destruction API is introduced.

This remains an incomplete fidelity implementation: unsupported XML elements,
missing templates/mixins and native APIs are reported. Roman FontFamily members
are selected for the synthetic enUS world. Typed native stand-ins preserve
primitive return shapes; record contents and native widget behavior need more
simulation. A successful tolerant load is not proof that every Blizzard chunk
initialized successfully or that the default UI matches the client.

Missing Lua files listed in a TOC produce `missing TOC file: <path>
(continuing)` and loading proceeds. Existing files still undergo canonical
sandbox validation; permission failures and malformed XML remain errors.
Historical addon-author reports describe missing TOC entries as logged load
failures that do not stop loading ([Excessive load times](https://wowinterface.com/forums/archive/index.php/t-36339.html)).
A [BfA beta report](https://www.wowinterface.com/forums/archive/index.php/https-/index.php/t-56283.html)
describes a regression from that behaviour. This is the simulator's chosen
compatibility behaviour, not native verification of every modern client.

Unknown globals remain nil and record `not simulated: X` once. Only manifest
function entries receive callable stand-ins. Manifest Font, table and widget
globals get the declared shape; `sim_globals::create_font` is the single Font
constructor boundary for SIM2. Native error handler routing from SB remains
unchanged. `debugprofilestop` returns simulated milliseconds; texture file IDs
are numeric IDs only, with nil for paths/atlases rather than invented IDs.

`StaticPopup_Show` retains the latest 16 popup records in `__selu_popups`
(text, two button labels and data), and emits their text to Studio's console.
Button actions and visual popup rendering are not implemented by SIM1.

Use `--manifest-dir <repository>/manifests --target wowf@1.60.1.70790` for external
Lua addon copies. `--manifest` instead takes a checked SELUMAN snapshot;
SELUSEG `.selus` history packs are rejected with an actionable explanation.
Session logs identify the resolved target/build or explicitly report none.

`debugstack(start?, count1?, count2?)` returns newline-separated Lua frame
locations and function names from the current coroutine, using defaults
1, 12 and 10 as described in the [client API documentation](https://warcraft.wiki.gg/wiki/API:debugstack).
It truncates the middle with `...` and bounds inspection at 1024 frames.
Addresses are excluded for deterministic tests. The optional explicit
coroutine overload is not implemented; current-coroutine calls are supported.

The SIM1 screen model uses `SessionConfig.ui_size` as physical resolution
(default 1920x1080). `GetPhysicalScreenSize` returns that pair;
`GetScreenWidth`/`GetScreenHeight` return resolution divided by UIParent's
effective scale. Scaling UIParent preserves its physical viewport by updating
its logical size. Children multiply their scales through their ancestors.
Non-positive and non-finite scales are rejected without mutation.
The screen signatures come from Blizzard's
[ScreenDocumentation.lua](https://github.com/Gethe/wow-ui-source/blob/live/Interface/AddOns/Blizzard_APIDocumentationGenerated/ScreenDocumentation.lua);
[GetScreenWidth](https://warcraft.wiki.gg/wiki/API_GetScreenWidth) describes
UIParent units, and a [native client error report](https://us.forums.blizzard.com/en/wow/t/disabled-all-addons-and-still-getting-this-lua-error/449049)
shows the positive-scale validation. No native-client NaN experiment was run.
UI.lua in the assessment copy does not read the UI scale CVars.

The actual Toolkit scale failure was caused by returning an invented callable
for its absent `RA_kitOriginalScale` field. Missing widget members now use only
known method signatures; unknown addon fields, including uppercase names, stay
nil. The former no-op tests now probe an actual known unmodelled method,
`SetMovable`; global fallback tests assert nil and retain log-once assertions.
Tooltip ownership is stateful. The native Minimap global has an explicitly
sourced widget shape from Blizzard's
[Minimap.xml](https://github.com/Gethe/wow-ui-source/blob/live/Interface/AddOns/Blizzard_Minimap/Mainline/Minimap.xml);
map imagery is not simulated. Hook callbacks retain registry keys rather than
auxiliary-stack function references, preserving error routing and flush order.

SIM1 also seeds a deterministic simulated player from the session character
and realm (level 1 Warrior, 100 health), without startup transition events.
This is synthetic scenario data, never a real saved file or game account.
`GetRealmName` reads the session realm. Ordinary text is retained in Rust and
materialized as a Lua string only for `GetText`; opaque secret values retain
identity. Transitions with no installed script do not queue frame handles.
These keep real addon construction beneath Lua 5.1's auxiliary reference limit
without changing snapshot text fields or SIM2's visual model.

The Toolkit copy has a visual skinning defect independent of native child
structure: `Modules/Skin/Collections.lua` assigns `sideTabs = SideTabs`
(a Lua function), while `Modules/Skin/Framework.lua` passes that value to
`ipairs` in `Parts` before looking up any child. No template field was invented
to bypass this. Animation groups remain a separate follow-up package.

### SIM3 visual runtime

`selu_sim::assets::Resolver` is the shared bounded media lookup for probes and
fonts. `Source::AddonMedia` reads only canonical files within registered addon
roots (16 MiB maximum per file); `Source::ClientStorage` optionally opens local CASC storage (see SL-11 below). `Lookup` distinguishes found
bytes, unavailable client data and missing addon media. Keys normalize slash
and case; adding a root invalidates the cache. Studio decodes the same lookup
bytes. Studio searches local installs by the manifest product; library sessions stay offline by default. Font fallback stays bundled.

Texture setter success is separate from availability. Modern SetTexture returns
true even for invalid assets ([reference](https://warcraft.wiki.gg/wiki/API:TextureBase_SetTexture));
GetTexture returns nil for invalid input ([reference](https://warcraft.wiki.gg/wiki/API:TextureBase_GetTexture)).
The simulator retains requested texture names in snapshots so Studio can label
unavailable assets. Addon media resolve as paths. Offline sessions report client textures unavailable.
With client storage enabled, extensionless texture names also try `.blp`.
Forever 1.60.1.70790 stores `Interface/Buttons/WHITE8X8.blp` as FileDataID
130871 (1228 bytes, decoded 8 by 8); it is not a synthesized engine texture.
The local client-storage integration test covers its extensionless lookup.

CreateColor implements the object methods from the read-only client source
`Interface/AddOns/Blizzard_SharedXMLBase/Color.lua`. Texture gradients carry
orientation and endpoint RGBA through snapshots to a four-vertex Studio mesh;
vertical minimum is at the bottom. EditBox uses the same positioned glyphs as
FontString, with text insets, local scroll offset and ancestor viewport clips.

Animation groups use the existing simulated OnUpdate grid. Alpha implements
ordered durations/delays, pause/resume, stop, repeat/bounce and play/loop/finished
scripts. Translation interpolates offsets in snapshots without changing anchor geometry.
FlipBook explicitly reports not simulated and returns nil. Native timing/fidelity
acceptance remains separate from deterministic simulator tests.

Layout invalidations seed a dirty-region set, expanded through parent and
anchor dependencies. Bulk external invalidations retain full resolution.
Unchanged font styles skip scanning; Font object mutations explicitly invalidate
inherited styles. Text measurements cache bounded results, and paint-only
texture mutations do not invalidate geometry. Geometry getters remain eager.

## Local client assets (SL-11 phase 2)

Studio detects a local install through the same discovery as Run in Game and
selects the product from the loaded manifest. `--client-install <root>` selects
the folder containing `.build.info`; `--no-client-assets` disables discovery
and storage entirely, including in headless mode. The Run in Game settings
window also saves a disable switch and install root (restart Studio to apply).
Library sessions remain offline unless `SessionConfig.client_install` is set.
No manifest/product means no client storage is opened.

Client storage uses selu-gamedata's read-only local CASC reader and verified
build/content cache outside the game folder. Paths are limited to Fonts and
Interface, plus numeric FileDataIDs. Fonts share the simulator measurement and
raster-mask drawing backend; the bundled font remains the fallback. Textures
are decoded lazily and cached by Studio, including inline text textures.
Missing local content stays missing, with no network fallback.

SetAtlas resolves installed UiTextureAtlas tables, uses the sheet FileDataID
and normalized crop, and changes region dimensions only with useAtlasSize.
Custom UVs are relative to the atlas crop; resetTexCoords resets them to the
full member. Unsupported phase-1 table layouts and encrypted/missing local
content remain explicit unavailable placeholders. Atlas metadata is cached.
Nine-slice rendering, atlas tiling and filter/wrap overrides remain fidelity
limits of the existing renderer.


## SIM4 widgets

Texture snapshots retain horizontal/vertical tiling and independent SetTexture
wrap modes. Studio splits textured quads at sampler boundaries for CLAMP,
REPEAT and MIRROR; tiling uses image pixel dimensions. Geometry is bounded to
512 sampler boundaries per axis. Filter overrides and atlas tiling remain
outside this implementation.

FontString colour getters retain Lua numeric precision, including inherited
font-object colours; snapshot colours remain render floats. Keyboard enabling,
propagation and wheel enabling are stateful. Host `key` dispatches topmost
visible enabled keyboard frames until propagation stops. Disabled or hidden
EditBoxes reject text and Enter input. Host `wheel` dispatches OnMouseWheel;
the installed script owns value-changing policy.

Sliders reclamp when ranges change and suppress unchanged-value callbacks.
SetValue's second argument controls OnValueChanged's user-input flag. Host drag
uses the thumb's travel span and rounds to the nearest range-relative step
when obey-step-on-drag is enabled. Horizontal minimum is left; vertical minimum
is top. Thumb textures are ordinary child regions in snapshots and Studio.

Sources: local read-only wow-ui-source
`Interface/AddOns/Blizzard_APIDocumentationGenerated/SimpleSliderAPIDocumentation.lua`
(SetValue's treatAsMouseEvent, step/orientation/thumb methods),
`SimpleTextureBaseAPIDocumentation.lua` (wrap arguments and tile methods),
`Blizzard_SharedXML/Shared/Slider/MinimalSlider.lua:6` (explicit obey-step policy),
`Blizzard_SharedXML/HybridScrollFrame.lua:263` (vertical scrolling starts at zero),
and `Blizzard_SharedXML/NineSlice.lua:76` (horizontal tiling).
Native tie-breaking and GPU filtering fidelity have not been measured in WoW.

## SIM5 callback and input model

API visibility (including inherited visibility), value, text, enabled and focus
transitions use a separate script queue, drained after arena borrows are released
and before the API returns. Deferred host work is not drained by this boundary.
Both queues retain the same outer instruction/task/callback budgets and error
custody. OnSizeChanged stays deferred to layout flush: synchronous native size
notification has not been established, and the owner explicitly requires deferral
until evidence establishes otherwise. Synchronous recursive scripts are bounded
by the existing nesting limit and the next outer dispatch recovers.

GetUnboundedStringWidth measures the complete text through the shared font
backend without max-line or height truncation. The generated client API has no
GetUnboundedStringHeight. Width semantics follow the
[FontString reference](https://warcraft.wiki.gg/wiki/API:FontString_GetStringWidth).

Host drag requires RegisterForDrag for OnDragStart/Stop and SetMovable for
StartMoving to select the moving frame. Screen clamps apply in physical pixels;
anchor deltas are converted by effective scale. Disabled hover uses the explicit
SetMotionScriptsWhileDisabled opt-in. EditBoxes retain autofocus policy, byte
cursor and selected range; typing replaces the selection. Studio consumes core
caret and selection geometry derived from the shared wrapped layout rows.
Snapshots also expose the byte selection range. Native visual fidelity remains unverified.

GameTooltip uses ordinary named left/right FontStrings and client tooltip
background/border assets, so Studio renders the same child snapshots. Artwork
is read from the local install and never bundled. Tooltip methods used by the
Toolkit port (SetOwner, SetText, AddLine, Show, Hide), plus ClearLines,
AddDoubleLine and NumLines, are represented.

The test CLI accepts --client-install and --no-client-assets, reads Studio's
client-asset opt-out and install settings, and shares registry/common-root
discovery with Studio. No detected install leaves sessions offline. Missing
artwork remains unavailable, including deliberate calibration textures. Discovery
reads the registry through bounded subprocesses and never starts a game.

Source evidence: Blizzard generated SimpleSlider/SimpleEditBox/SimpleButton APIs;
Blizzard_SharedXML/Shared/Slider/MinimalSlider.lua:126-141;
Blizzard_SharedXML/Shared/InputBox/InputBoxTemplates.lua:243-250;
Blizzard_SharedXML/Mainline/SharedUIPanelTemplates.lua:1526;
Blizzard_SharedXML/Backdrop.lua:125-153. Timing evidence is the owner's SIM5
client contract plus these source consumers; signatures alone do not prove native
scheduling. The implementation receipt lists per-script evidence and limits at
E:/AgentData/reports/selu-SIM5-report.md.

## Scenario panel

Studio's Scenario sidebar edits the same session-local unit tables read by the
simulator APIs. Quick actions create synthetic rare units, a nameplate frame,
and a vignette at normalized map position (0.5, 0.5). These are synthetic data,
not discoveries from a game client. Reaction runs from 1 (hated) to 8 (exalted).
The editor's fields apply with Add / update unit; clear removes the token.

Save/load uses `scenarios/<name>.toml` beneath the open project. The file contains
units, vignettes, combat, copper, bag item IDs and zone/subzone/map. Loading is a
replacement of that world setup, emitting transitions for the differences.
From a test script, call `sim.loadScenario("scenarios/rare-test.toml")` with a
path relative to the test runner's working directory (or an absolute path).
No language/checker declarations or Lua project interop are introduced.

Scenario API/event signatures are sourced from the read-only `wow-ui-source`
forever 1.60.1.70790 generated Unit, VignetteInfo, NamePlate, NamePlateManager,
Container, CurrencyInfo and Map documentation. The docs establish payload
shapes; native event ordering and default UI visual fidelity remain unverified.
Nameplate frames are simulator frames with `namePlateUnitToken`; no Blizzard
nameplate template is loaded here. Vignette positions return a vector with
`x`, `y`, `GetXY` and zero facing on the configured map only. Bag setups model
item IDs per slot, not inventory metadata or item stack counts.

## Default UI startup (stage 1, opt-in)

The Default UI setting remains **off by default until visual acceptance**.
An explicitly enabled session loads Blizzard's external Lua/XML before the
user addon. Local CASC files take precedence over the configured read-only
source checkout; missing locally installed files fall back with notes. The
tested source is Forever 1.60.1.70790, matching the installed wowf client.
Neither source nor game folders are modified. SavedVariables are not read.

XML parsing uses `roxmltree` (MIT OR Apache-2.0). Virtual/intrinsic templates,
multiple inheritance, mixins, parent keys/arrays, `$parent`, relative keys,
script inheritance, scroll children and button texture/text regions are
simulator input mechanisms; none introduces untyped Lua into Selu programs.
`inherit="append"` appends the inherited handler after the local handler,
as required by FloatingChatFrame.xml's chat-frame initialization. Intrinsic
postcall OnLoad handlers run after the instance's normal OnLoad and regions.

The client atlas index decodes UiTextureAtlas (897470), UiTextureAtlasMember
(897532) and UiTextureAtlasElement (1989276), joining member ElementID to
element names. The decoded index is cached on the immutable Storage build
snapshot. Supported WDC5 layout hashes are checked; unknown compression or
layouts fail closed. Table layouts follow the primary
[WoWDBDefs definitions](https://github.com/wowdev/WoWDBDefs/tree/master/definitions).
GlobalColor (4328468) supplies the native color palette. GlobalStrings
(1394440) was absent locally; selected HUD labels and fixed chat tags use
visible synthetic text. These are not client-localization fidelity claims.
Numeric GameMode/GameRule enums and primitive Constants are sourced from the
Forever client export at BlizzardInterfaceResources commit
`d1ee569878094270c0ad8d8b00de0762158fd578` (Resources/LuaEnum.lua).

Simulator Lua helpers capture private builtin/library snapshots when the VM
is created. Addons retain the real mutable global environment. Blizzard
startup errors are captured and later chunks continue. Missing native
implementations retain manifest-shaped defaults and once-only notes; explicit
empty-world models respect no-result APIs such as UnitPowerBarTimerInfo,
whose documented MayReturnNothing behavior terminates timer enumeration.

`SELU_SIM_PROFILE=1` enables exclusive phase timings in development builds.
Source construction batches layout; login handlers query live geometry. Trace
event records remain available, while partial startup frame snapshots are omitted.
Normal event snapshot history resumes after startup. Blizzard source execution
uses bounded count-hook quanta; user addon instruction accounting remains
precise, including coroutine resumes. Four bounded simulated frames settle
Blizzard's initial dirty layout propagation before Studio's first preview.
The worker reports loading progress, captured errors, top missing natives and
elapsed time. The current visual/performance receipt is maintained separately
under `E:/AgentData/reports/default-ui/`; passing focused tests does not imply
complete HUD or native rendering fidelity.

Blizzard Edit Mode performs more than 10,000 protected callbacks while searching
registered systems for raid-frame settings. Default UI sessions allow 100,000
callbacks per dispatch, with the existing instruction/depth limits; addon-only
sessions retain the 10,000 callback limit. UPDATE_CHAT_WINDOWS initializes the
synthetic profile after VARIABLES_LOADED. FontString line height uses the client
font metrics. Model view insets retain their values but do not render a 3D model.

The installed 70790 atlas metadata resolves micro-menu sprites to FileDataID
4708813, but CASC reports that texture sheet is not installed locally. Such art
remains a labelled placeholder. This differs from an unavailable atlas name and
is not evidence that atlas metadata lookup failed. Full HUD acceptance remains
open: private/forbidden partitions, missing native/world data, unwanted panels
and unpositioned render regions still require stage-1 work. Later interaction
and richer world scenarios do not replace those acceptance requirements.

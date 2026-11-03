# Selu — design

Selu is a language that compiles to World of Warcraft addon Lua. The compiler
takes the target game version as an argument and checks the program against
the API surface of that exact client.

- **Name:** Selu (Selene, the moon, + Lua). CLI `selu`, source files `.selu`.
- **First rule: easy to use, understand and write.** If a JavaScript or
  TypeScript developer can't read a Selu file without a tutorial, the design is
  wrong. Safety comes from the compiler doing the work, not from extra syntax.
  When a feature needs a new keyword or type to be safe, first look for a way
  the compiler can handle it automatically and explain itself in a clear error.
- **Second rule:** the default output is readable Lua that a human can debug
  and that is allowed on every addon platform.

## 1. Why

WoW addon Lua fails late. When you call an API the client lacks, misspell an
event, do arithmetic on a secret value, or touch a protected frame in combat,
the error only shows up at runtime, often in front of a player. Each client
flavour (retail, Classic Era, Classic progression, Forever) has a different API
set, so the same code can work on one and break on another. Selu moves these
failures to compile time.

## 2. Targets

```
selu build --target retail
selu build --target classic-era
selu build --target forever@1.60.1.69913
```

- A target is a **manifest**: the native functions with their signatures, the
  events and their payloads, FrameXML globals, templates and mixins, CVars,
  `.toc` fields and the interface number, plus protected/secure and secret-value
  flags.
- Manifests are generated from the client's own sources (Blizzard UI source and
  the generated API documentation), not written by hand. Each entry carries an
  evidence level, so "the documentation says so" and "verified in the game" stay
  distinguishable.
- Calling a symbol that the target lacks is a compile error. Optional APIs need
  an explicit guarded form, and the compiler emits the existence check.
- `--target` accepts several targets. One source tree can then build for each
  flavour, with branches the compiler can resolve at compile time and an error
  when an API is missing on any requested target.

## 3. What the type system guards

Ordered by how many real bugs each one would have prevented:

1. **Missing APIs per target** (see section 2).
2. **Secret values.** APIs whose result the client may mark secret (unit
   health, aura data, some cast booleans) simply return an optional value
   (`number?`). The compiler emits the guarded read; the author handles "no
   value" with ordinary `??` or `if`, exactly like `undefined` in TypeScript.
3. **Combat lockdown.** Protected calls the compiler can't prove are out of
   combat are queued automatically and run when combat ends. Debug builds log
   each deferral, so the delay is never a mystery.
4. **Events.** Event names come from the manifest, and handler parameters get
   the payload types.
5. **SavedVariables schema.** Persisted shapes are declared with a version. When
   a shape changes, the build fails unless a migration exists for it.
6. **Localisation.** User-facing strings are keys; a missing locale entry is a
   build warning (an error when configured).

## 4. Output

- One `.lua` file per `.selu` module plus a generated `.toc`. Local names and
  structure follow the source closely.
- Source maps: a runtime error at `Foo.lua:120` maps back to `Foo.selu:48`. A
  small optional helper can rewrite stack traces in error handlers.
- No runtime library is required. Helpers are emitted only when used.

## 5. Optimizer

| Flag | Does |
|---|---|
| `-O0` | Direct translation, for debugging the compiler |
| `-O1` (default) | Constant folding, dead-code removal, dropping target branches that can't run on the chosen target |
| `-O2` | Caching hot globals in locals, inlining small functions, merging event dispatch, removing table allocations in handlers that run often when they are provably unnecessary |
| `--strip-comments` | Smaller output that stays readable |

Every optimization must keep the output readable and keep the source maps
correct.

## 6. Experimental minifier

```
selu build -O2 --experimental-minify
```

- Off by default, never enabled by a profile or config file silently.
- Every build that uses it prints: *"Minified output may violate Blizzard's
  AddOn policy and the CurseForge/Wago rules. You are responsible for where you
  publish it."*
- It strips whitespace, shortens local and upvalue names, pools repeated
  constants and can merge modules into one chunk.
- Globals, frame names, SavedVariables keys, event names and anything the
  manifest marks as externally referenced keep their exact spelling.
- Source maps still work.
- **Out of scope, permanently:** string encryption, anti-tamper, packed or
  self-decoding loaders, and anything else designed to stop a reader from
  understanding the code.

## 7. Simulator and debugger

Most addon bugs today are found by logging in, reloading and guessing. Selu
should let you find them on your desktop first.

```
selu debug --target forever       # build, then start the virtual WoW simulator
selu test                         # run test scenarios headless, same simulator
```

**Simulator.** The compiled Lua runs in a Lua 5.1 VM with a fake game client
generated from the target manifest: every API, event and frame the target has
exists, with sensible default behaviour. You control the world from the test
or an interactive console:

```selu
test("badge hides at full health", () => {
  sim.unit("player", { health: 100, maxHealth: 100 });
  sim.fire("UNIT_HEALTH", "player");
  expect(badge.isShown()).toBe(false);
});

test("unreadable health shows nothing", () => {
  sim.secret("UnitHealth");          // the client hides the value
  sim.fire("UNIT_HEALTH", "player");
  expect(badge.isShown()).toBe(false);
});

test("bar moves after combat", () => {
  const original = bar.point();
  sim.combat(true);
  sim.fire("PLAYER_LEVEL_UP", 20);
  expect(bar.point()).toEqual(original);
  sim.combat(false);
  expect(bar.point()).toEqual(["TOP", UIParent, "TOP", 0, -40]);
});
```

- Time is simulated, so timers and tickers run instantly and the same way
  every time.
- `sim.reload()` saves and reloads SavedVariables, so migrations and
  "survives a /reload" can be tested.
- Protected calls in combat, secret values and missing APIs behave like the
  target client: the simulator fails the same way the game would.
- Tests use Jest-style `test` and `expect`, since that's what most JavaScript
  developers already know.

### Selu Studio: the standalone simulator app (priority)

The most important tool is a desktop app with its own WoW UI engine, not an
editor plugin. **`selu debug` is how you start it:** it builds the addon, opens
a virtual WoW client in a window, loads the addon as the game would (TOC order,
`ADDON_LOADED`, `PLAYER_LOGIN`, `PLAYER_ENTERING_WORLD`) and attaches the
debugger. Rebuilding on save reloads the addon in place, like a `/reload`.

```
selu debug                        # current folder, default target
selu debug ./MyAddon --target retail
selu debug --break-on-load        # pause before the first line runs
```

Plain Lua addons work the same way (`selu debug ./SomeLuaAddon`), without a
build step.

What it has:

- **Game screen.** Our own engine draws the addon's frames: anchors, sizes,
  strata and levels, textures, font strings, alpha, scale, show/hide, mouse
  hover and clicks, drag. Click a frame to select it.
- **Frame inspector.** The selected frame's tree, its anchors drawn on screen,
  its size, strata, scripts, and which line of source created it.
- **World panel.** Set up the game state without scripts: player and target
  (class, level, health, power, auras), combat on/off, group members, bags,
  money, quest log, zone and position. Changes fire the matching events.
- **Event timeline.** Every event, timer, hook and script call in order, with
  its arguments. Scrub back to see the frame tree at an earlier moment.
- **Debugger.** Breakpoints in `.selu` files (and plain `.lua`), step,
  variables, call stack, a console that runs code inside the addon, and "fire
  this event now" while paused.
- **SavedVariables view.** See and edit saved data live; one button for
  "/reload", which saves, reloads and runs migrations.
- **Target switch.** Rerun the same addon against another target; missing
  APIs, protected-call errors and secret values show up as clearly marked
  errors, the way the real client would fail.
- **Error panel.** The client-style error with a source-mapped stack and a link
  to the line.

Design decisions:

- **It runs any addon, not only Selu output.** Plain Lua addons load too. That
  makes it useful from the first version, and real addons (Renner's Toolkit
  first) become the engine's test suite.
- **Same engine for `selu test`.** The app and the headless tests share one
  simulator core, so a failing test can be opened in the app at the failing
  moment.
- **No Blizzard art is shipped.** By default frames using Blizzard textures
  draw as labelled placeholders. A later option may read textures and fonts
  from the user's own installed client, locally, never bundled or uploaded.
- **Built in Rust:** the Lua 5.1 VM, the frame and layout engine and the API
  model in one core; the app's panels and screen drawn with a native GPU UI.

### VS Code extension (second)

Syntax highlighting, errors while typing (language server), and debugging over
the Debug Adapter Protocol against the same simulator core: breakpoints in
`.selu` files, stepping, variables, firing events while paused. Useful, but it
comes after Studio.

### Honest limits

The simulator is our own model of the client, only as good as the manifest and
its default behaviours. It catches logic, API, combat, layout and save bugs; it
does not prove that something looks pixel-identical in the game or that the
real client behaves exactly as modelled. The final check is still the game,
where source-mapped errors point back to `.selu` lines.

## 8. Non-goals

- Not a general-purpose Lua replacement outside WoW.
- No runtime, VM or bytecode: the client only loads Lua source.
- No way around Blizzard's protections. The compiler models secure execution
  and refuses code that would violate it; it never emits code meant to evade
  it.

## 9. Syntax (decided 2026-11-04)

Selu has its own syntax and is **not** a Lua superset. **If you know
TypeScript, you already know most of Selu.**

- TypeScript-style code: braces, `let`/`const`, `function` and arrow
  functions, `class`, `interface`, `import`/`export`, template strings.
- Types are inferred. You only write them on function parameters, and even
  there they are optional when the compiler can work them out (event handlers
  get theirs from the game).
- Semicolons are required: every statement ends with `;`, like C#, Java and
  Rust. A missing `;` is a compile error with a quick fix, never a guess.
- One "no value": `T?`, handled with `??`, `?.` and `if`. No `null` plus
  `undefined`.
- Arrays start at 0, like every language you know. The compiler handles Lua's
  1-based tables.
- Only one new keyword: `on`, for event handlers. SavedVariables are a `saved`
  declaration. Everything else is a normal function or object.
- Nice-to-haves from C# and Rust (`enum`, `switch` with patterns) come later and
  only if they stay easy to read.

```selu
saved settings = {
  enabled: true,
  threshold: 0.35,
};

function missingHealth(unit: Unit) {
  const hp = UnitHealth(unit) ?? return;     // may be unreadable: that's the ??
  const max = UnitHealthMax(unit) ?? return;
  return max - hp;
}

on UNIT_HEALTH(unit) {
  if (!settings.enabled) return;

  const missing = missingHealth(unit) ?? 0;
  if (missing > 0) {
    badge.setText(L`${missing} missing`);
  } else {
    badge.hide();
  }
}

on PLAYER_LEVEL_UP(level) {
  bar.setPoint("TOP", UIParent, "TOP", 0, -40);  // waits for combat to end if needed
}
```

Changing the shape of `saved` data later is a compile error until a migration
is added; the error message shows the migration to write.

### Prior art

TypeScriptToLua (with community WoW declarations) already compiles TypeScript
to WoW Lua. Selu differs on purpose: per-target manifests generated from the
client, secret values and combat lockdown in the type system, SavedVariables
migrations checked by the compiler, and output designed to read like
hand-written addon Lua. Studying TypeScriptToLua's output and its edge cases is
an early task.

## 10. Open questions

- Implementation language for the compiler (Rust is the working assumption,
  which is why the `selu` crate name matters).
- Manifest pipeline: which sources each flavour's manifest is generated from,
  and how it is updated per client build.
- Editor support: a language server is likely needed early, because most of the
  value is errors shown while typing.
- Licence.

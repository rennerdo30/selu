# Selu version 1 — specification

`DESIGN.md` explains what Selu is and why. This file pins down what version 1
contains and how each part behaves. Where the two disagree, this file wins for
v1 and `DESIGN.md` gets corrected.

Three rules decide every open question:

1. **Easy to use, understand and write.** A TypeScript developer reads Selu
   without a tutorial.
2. **Fast and lean in the game.** Idle code costs nothing, hot code creates no
   garbage, and both are checked by tests, not promised.
3. **Readable output.** The generated Lua looks like a careful person wrote it.

## 1. What v1 contains

| Area | In v1 | Later |
|---|---|---|
| Targets | `retail`, `forever`, `classic-era`, `classic-anniversary`, `classic-titan`, `classic` (owner, 2026-11-04: every client, generated from Blizzard's UI source); `-ptr`/`-beta` variants when a manifest exists | `selu targets update` downloads |
| Language | everything in section 2 | `enum`, pattern `switch`, getters/setters, decorators |
| GUI | the framework in section 4 | animations, drag-and-drop between lists, Blizzard frame skinning |
| Optimizer | `-O0`, `-O1`, `--strip-comments`, `--experimental-minify` | `-O2` |
| Tools | `selu build`, `selu test`, `selu debug`, `selu targets`, `selu probe`, `selu export`, `selu doctor` | `selu fmt`, `selu init` templates beyond one |
| Simulator | section 8 | client art and fonts, BLP textures, Blizzard's own UI |
| Editors | VS Code: syntax colours and errors while typing | VS Code debugging (DAP), other editors |

## 2. The language

### 2.1 Values and types

- `number` (Lua 5.1 has only doubles; there is no separate integer type),
  `string`, `boolean`, `null` (the one "no value", compiled to `nil`).
- `T?` is "a `T` or `null`". Only `T?` may be `null`; the compiler tracks this
  through `if (x != null)`, `??`, `?.` and early returns.
- Arrays `T[]`, `Map<K, V>`, `Set<T>`, object types and `interface`, function
  types `(a: number) => string`, tuples `[string, number]`.
- `unknown` for values from untyped Lua; it must be narrowed with `typeof`
  before use. There is no `any`.
- Simple generics on functions, interfaces and classes (`function first<T>(xs:
  T[]): T?`). Constraints only with `extends`.
- Types are inferred everywhere except function parameters, and parameters of
  event handlers, callbacks and components get theirs from context.

### 2.2 Rules that remove Lua's and JavaScript's traps

| Rule | Why |
|---|---|
| Semicolons are required. | One rule, no guessing. |
| `if (x)` needs a `boolean` or a `T?`. `if (count)` on a number is an error: write `count != 0`. | Lua treats `0` and `""` as true, JavaScript as false. Neither guess is safe. |
| `==` and `!=` compare strictly (no coercion). `===` is an error with the fix "use `==`". | One equality, no surprises. |
| `+` adds numbers or joins two strings. `number + string` is an error: use a template string. | No accidental `"5" + 1`. |
| Arrays start at 0 and can't contain `null`. Use a `Map` for gaps. | Lua's `#` is undefined on arrays with holes. |
| `%` is floored modulo (`-1 % 3 == 2`), as in Lua and Python. Documented, and the error message for `%` on a possibly negative value mentions it once per project. | It's what the client computes. |
| No `for...in`. Use `for (const x of xs)`, `for (const [k, v] of map)`, or a counted `for`. | One way to loop. |
| Writing a global needs `global name = ...;`. Reading an undeclared name is an error. | Accidental globals are a classic addon bug and a taint risk. |

### 2.3 Strings, numbers, collections

- Template strings `` `${a} of ${b}` `` compile to `string.format` with the
  right specifiers, not chains of `..`.
- `s.length` is the length in bytes (what Lua sees). `s.split`, `s.trim`,
  `s.startsWith`, `s.endsWith`, `s.includes`, `s.upper`, `s.lower`,
  `s.slice(from, to)` with 0-based indices.
- `xs.length`, `push`, `pop`, `insert(i, x)`, `removeAt(i)`, `indexOf`,
  `includes`, `sort(compare?)`, `slice`, `forEach`, `map`, `filter`, `find`,
  `some`, `every`, `reduce`, `join`. `map`/`filter` create a new array; in hot
  code the compiler warns and suggests a loop.
- `Map`: `get` (returns `V?`), `set`, `has`, `delete`, `clear`, `count()` (it
  walks the table; documented as O(n)), iteration. `Set`: `add`, `has`,
  `delete`, `clear`, `count()`, iteration. Both compile to plain Lua tables.
- `Math.floor`, `ceil`, `round`, `min`, `max`, `abs`, `clamp`, `random`.

### 2.4 Functions, objects, classes, modules

- `function` declarations, arrow functions, default and rest parameters
  (`...args: number[]`), destructuring of objects, arrays and tuples.
- **Multiple returns** from WoW APIs are tuples:
  `const [name, , , level] = GetItemInfo(id);` compiles to
  `local name, _, _, level = GetItemInfo(id)` — no table is created.
- Object literals compile to Lua tables; `interface` adds only types.
- `class` with fields, constructor, methods, `static`, single `extends` and
  `super`. It compiles to one metatable per class with `__index`; methods are
  called with `:`. `this` becomes `self`. Arrow functions capture `this`.
- **Modules:** one file is one module. `import { a } from "./other";` and
  `export`. Modules share the addon's private table (`local ADDON, ns = ...`),
  never globals. The `.toc` file order comes from the import graph; an import
  cycle is an error that shows the cycle.
- **Blizzard objects keep Blizzard's names:** `frame.SetPoint(...)`,
  `GameTooltip.AddLine(...)`. The compiler emits `:` for methods. Your own
  functions and Selu's library use `camelCase`.

### 2.5 Control flow and errors

- `if`/`else`, `while`, `do...while`, `for`, `for...of`, `break`, `continue`
  (Lua 5.1 has no `continue` or `goto`; the compiler restructures the loop).
- `try { } catch (e) { } finally { }` compiles to `pcall` or `xpcall`; `return`,
  `break` and `continue` inside `try` work as expected. `throw new
  Error("...")` compiles to `error(...)` with the source position.
- An error that escapes an event handler or script reaches the game's normal
  error handler, with a source-mapped `.selu` position.

### 2.6 Async

`await` replaces nested callbacks for things that happen later:

```selu
async function greet() {
  await after(2);                        // seconds
  print("two seconds later");
  const [bag] = await event("BAG_UPDATE", { timeout: 5 });
  await nextFrame();
}

const job = run(greet);
job.cancel();
```

- Compiles to Lua coroutines resumed by `C_Timer.After` or the shared event
  frame. Nothing polls.
- After any `await`, the compiler treats the code as "combat possible" and
  applies the combat rules of section 3.
- `event(...)` with a timeout returns `null` on timeout instead of throwing.

## 3. WoW features in the language

### 3.1 Events

```selu
on PLAYER_LOGIN() { ... }
on UNIT_HEALTH(unit) { ... }               // unit: UnitToken, from the manifest
on UNIT_AURA("player", (unit, info) => { ... });   // unit filter
```

- All handlers of an addon share one event frame. An event is registered only
  while at least one handler for it is active and unregistered when none is,
  automatically.
- Handlers inside a component (section 4) are active only while the component
  is shown, unless declared `on always EVENT(...)`.
- Unit events with a unit filter use `RegisterUnitEvent` where the target has it.
- Event names and parameter types come from the manifest; a misspelled event is
  a compile error.

### 3.2 Saved data

```selu
saved settings = {
  enabled: true,
  thresholdPercent: 35,
};

saved perCharacter progress = {
  questsDone: new Map<number, boolean>(),
};
```

- `saved` declares one SavedVariables global; `perCharacter` declares a
  SavedVariablesPerCharacter one. The `.toc` lines are generated.
- The values in the declaration are defaults. Missing keys are filled in on
  load; existing values are never overwritten.
- **Migrations are checked:** every build writes the saved shapes to
  `selu.saved.toml` (commit it). If a shape changed, the build fails until the
  version is raised and a migration exists:

  ```selu
  saved settings version 2 = { enabled: true, thresholdPercent: 35 };

  migrate settings 1 -> 2 (old) {
    old.thresholdPercent = old.threshold * 100;
    delete old.threshold;
  }
  ```

  Migrations run in order before defaults are applied, and a migration that
  throws stops the chain so it retries at the next login. Saved data from a
  newer version (the user went back to an older addon) is left untouched and a
  warning is logged.
- Saved values can be read and written anywhere after `ADDON_LOADED`; reading
  them earlier is a compile error.

### 3.3 Slash commands, hooks, logging

```selu
command("/myaddon", "/ma", (args) => { ... });

hook(GameTooltip, "SetUnit", (tip) => { ... });     // hooksecurefunc
hookScript(GameTooltip, "OnShow", (tip) => { ... });
log.warn`Spell ${id} has no texture`;               // debug log, English only
```

- Hooks never replace Blizzard functions; only additive hooks exist.
- `log.debug/info/warn/error` writes to the addon's debug log (visible in the
  simulator and through an in-game `/selulog` window). `log.warnOnce` is the
  standard way to say "this feature is off because X is missing".

### 3.4 Localisation

```selu
badge.SetText(L`${missing} missing`);
```

- `` L`...` `` marks a user-facing sentence. The English text is the key.
- The build writes `locales/enUS.toml` from every `L` string. Other languages
  are files like `locales/deDE.toml` with the same keys, `{0}`, `{1}` for the
  arguments in any order.
- A missing translation is a warning (an error with `--strict-locales`); a
  translation whose English text changed is marked stale; a wrong argument
  count is an error.
- `L` strings must be whole sentences: joining two `L` results with `+` is a
  warning.
- At runtime the client's locale picks the table, falling back to English.
  `L` strings compile to `SetFormattedText`/`string.format` with a format string,
  so updating a number in a label creates no new string in Lua.

### 3.5 Version differences

`has(...)` and the strictest-target rule are in `DESIGN.md` section 3.

## 4. The GUI framework

The most important part of v1 for most addons. It must be as easy as writing
a small web page and as cheap as hand-tuned WoW code.

### 4.1 How it looks

```selu
component HealthBadge(props: { unit: UnitToken }) {
  let missing = state(0);

  on UNIT_HEALTH(props.unit, () => {
    missing = missingHealth(props.unit) ?? 0;
  });

  return (
    <Panel size={[90, 22]} visible={missing > 0}>
      <Text text={L`${missing} missing`} color="textPrimary" />
    </Panel>
  );
}

component Settings() {
  return (
    <Window title={L`My Addon`} width={320}>
      <Stack gap="sm" padding="md">
        <Checkbox label={L`Enabled`} bind={settings.enabled} />
        <Slider label={L`Threshold`} min={0} max={100} step={5}
                bind={settings.thresholdPercent} />
        <Button text={L`Reset`} onClick={() => resetSettings()} />
      </Stack>
    </Window>
  );
}

mount(<HealthBadge unit="player" />, { point: ["CENTER", UIParent, "CENTER", 0, -120] });
```

- **JSX-style markup**, which TypeScript and React developers already know.
- **`state(...)` makes a value reactive.** You read and assign it like a normal
  variable. Whatever on screen uses it updates; nothing else does.
- **`bind={...}`** connects a control to a `state` or a `saved` value in both
  directions.
- Components are functions. `props` are typed; `children` work as in JSX.

### 4.2 How it runs (why it's fast)

There is **no virtual DOM, no diffing and no re-rendering** at runtime. The
compiler reads the markup once and generates direct WoW calls:

1. **Mount:** all frames, textures and font strings of a component are created
   once (or taken from a pool) and laid out. This is the only time
   `CreateFrame` runs.
2. **Change:** assigning `missing = 5` compiles to storing the value and
   marking exactly the bindings that read `missing` as dirty. No closure, table
   or string is created.
3. **Flush:** at most once per rendered frame, a single hidden updater frame
   shows itself, applies the dirty bindings with the cheapest calls available
   (`SetFormattedText` for text with numbers, `SetShown`, `SetValue`,
   `SetVertexColor`...), runs layout for dirty containers only, and hides
   itself again. If nothing changed, nothing runs: **idle cost is zero**, no
   `OnUpdate` stays active.
4. **Unmount/hide:** event handlers of the component are unregistered, timers
   stopped, and the frames go back to their pool (WoW can't destroy frames).

The generated Lua for `HealthBadge` reads like a person wrote it: a `create`
function, an `update` function with one `if dirty & BIT then ... end` per
binding, and nothing else.

### 4.3 Building blocks

Layout and containers:

| Component | Use |
|---|---|
| `Window` | movable top-level window with title bar, close button, saved position |
| `Panel` | a plain box; the base for HUD elements |
| `Stack` | vertical or horizontal (`direction="row"`), with `gap`, `padding`, `align`, `justify` |
| `Grid` | fixed columns with gaps |
| `Scroll` | scrolling area with a scrollbar pinned to both ends |
| `List` | **virtual** list: `each={items}`, `key`, `rowHeight`; only visible rows exist, rows come from a pool |
| `Tabs` | tab strip with pages; only the visible page's handlers are active |
| `Divider`, `Spacer` | separators and flexible space |

Content and controls:

| Component | Use |
|---|---|
| `Text` | a label; `size="body|small|title"`, `color`, `wrap`, `maxLines` |
| `Icon` | a texture by file id, path or spell/item icon; optional cooldown and count |
| `Bar` | status bar with value, min, max, text |
| `Button` | `variant="primary|secondary|danger"`, text or icon |
| `Checkbox`, `Toggle` | booleans |
| `Slider` | numbers with min, max, step, value label |
| `Input` | single-line text; `onEnter`, `onEscape`; focus never steals game keys after `Escape` |
| `Dropdown` | choose one of a list |
| `Tooltip` | `tooltip={L`...`}` prop on any component, or rich content as a child |

Low level, when the building blocks aren't enough:

- `<Frame>`, `<Texture>`, `<FontString>`, `<StatusBar>`, `<EditBox>` and every
  other widget type the target has, with props named like Blizzard's setters
  without `Set` (`point`, `size`, `alpha`, `strata`, `level`, `scale`).
- `ref={myRef}` gives the real frame object for any direct call.

### 4.4 Layout

- **Anchors are the native layout:** `point={["TOPLEFT", parent, "TOPLEFT", 8,
  -8]}`, `points="fill"` (all points) and `size`.
- **`Stack` and `Grid` measure their children** (text width, fixed sizes) and
  place them with anchors. Sizes come from content; nothing reserves blank
  space. Re-layout runs only when a child's size or visibility changed.
- Every container gets an explicit size (the client renders nothing inside a
  frame without one); the compiler and layout pass make sure of that.

### 4.5 Theme

- One theme file with tokens: colours (`textPrimary`, `accent`, `surface`...),
  font sizes, spacing (`xs=4`, `sm=8`, `md=12`, `lg=16`), border widths.
- Components take token names (`color="accent"`, `gap="sm"`); literal colours
  are allowed only through `theme.register(...)`, so a theme switch (dark/light)
  repaints everything.
- Selu ships one plain, neutral default theme. Addons replace it.

### 4.6 Combat and secure frames

- `<SecureButton>` wraps secure action templates: `action={{ type: "spell",
  spell: "Flash Heal" }}`, `macro`, `item`. Its props are secure attributes.
- Creating or changing secure frames in combat is impossible in WoW. A secure
  prop bound to `state` is applied immediately out of combat and queued until
  combat ends otherwise; the debug log records each deferral.
- Showing, hiding or moving any protected frame follows the same rule.
- Components can declare `hideInCombat` or `showInCombat`; the compiler uses a
  state driver only where it is safe and never on frames that must stay
  non-interactive.

### 4.7 Performance and memory guarantees

These are tested in the simulator (section 8) for every built-in component and
can be tested for your own:

| Guarantee | Test |
|---|---|
| Nothing runs while idle | `expect(perf.idle(5)).toHaveNoUpdates()` |
| A state change creates no garbage for number, boolean and string bindings | `expect(perf.allocations(() => { hp = 50; })).toBe(0)` |
| A flush runs at most once per frame, however many states change | `expect(perf.flushes(() => sim.fire(...many))).toBe(1)` |
| Frames are created only on mount; remounting reuses pooled frames | `expect(perf.framesCreated(() => remount())).toBe(0)` |
| Events are registered only while a component is shown | `expect(sim.registered("UNIT_HEALTH")).toBe(false)` after hide |
| A `List` of 10,000 items creates only as many rows as are visible | `expect(perf.framesCreated(...)).toBeLessThan(30)` |

Plus the compiler warnings from `DESIGN.md` section 10 for your own hot code.
"Perfect" isn't a promise Selu can make about your code, but every one of
these is a regression test, and the simulator's memory panel shows where
garbage comes from.

## 5. Using existing Lua

- Plain `.lua` files can live in a Selu project. List them in `selu.toml`
  (`lua = ["libs/LibStub/LibStub.lua"]`); they go into the `.toc` before the
  generated files, in the listed order.
- Their types come from declaration files `*.d.selu`:

  ```selu
  declare global function LibStub(name: string, silent?: boolean): unknown;
  declare module "LibDataBroker-1.1" { ... }
  ```

- v1 ships declarations for LibStub and CallbackHandler-1.0. Others are
  written per project or contributed later.
- Selu code is reachable from Lua through `export global` only.

## 6. Projects, building, packaging

Every file a person edits is TOML: the project file `selu.toml`, translations
and manifest overlays. JSON appears only in files the tools write for
themselves (generated manifests, source maps, `--json` output).

```toml
# selu.toml
[addon]
name = "MyAddon"
title = "My Addon"
version = "1.0.0"
author = "Name"
notes = "One line shown in the AddOns list."
targets = ["retail@12.0", "forever"]
lua = []                 # plain Lua files, in load order (section 5)
```

Unknown keys are an error, so a typo never silently does nothing. `assets/`
and `locales/` are found by name; asset settings, tools and converters are
further tables in the same file (6.1).

- Source is `src/`; the entry is `src/main.selu`, and everything reachable from
  its imports is compiled.
- `selu build` writes `dist/<name>/` with the `.toc`, Lua files, converted `assets/` and
  `locales`. Source maps go to `dist/.selu/`, not into the addon folder.
- With several targets, one `.toc` per flavour uses the client's file-name
  suffix (`MyAddon_Mainline.toc`, ...) where the target supports it (the
  manifest says which), otherwise one `dist/<target>/<name>/` folder each.
- `selu export` builds a release: the addon folder **and** a zip (section 6.2).
- `selu install --wow <path>` copies a build into a local `Interface/AddOns`
  folder for testing in the game. It refuses while the game is running.

### 6.1 Assets: images, fonts and sounds

You keep assets in the formats you work with; the build converts them to what
WoW loads.

| You put in `assets/` | The addon gets |
|---|---|
| SVG, PNG, JPG, WebP | TGA (32-bit with alpha, the default) or BLP (`"format": "blp"`, smaller files) |
| TTF, OTF | copied as is |
| Any audio ffmpeg can read (WAV, FLAC, AIFF, M4A, Opus, WMA, video soundtracks...) | OGG Vorbis, through ffmpeg |
| OGG, MP3 | copied as is, or re-encoded when settings ask for it |
| Anything else | a converter you configure (6.1.2) |

- **Assets are imported like code**, so a wrong name is a compile error and the
  path is never typed by hand:

  ```selu
  import logo from "../assets/logo.svg";
  import ding from "../assets/ding.wav";

  <Icon texture={logo} size={[32, 32]} />
  PlaySoundFile(ding, "Master");
  ```

  `logo` becomes the in-game path (`Interface\AddOns\MyAddon\assets\logo`,
  without extension, as WoW expects) together with its size, so the compiler
  can warn when an image is drawn far larger than it was rendered.
- **Texture sizes follow the client's rules:** width and height are powers of
  two, within the target's maximum (both come from the manifest). An SVG is
  rendered straight at a valid size; a bitmap that isn't valid is padded
  (never stretched) with a warning, and the stored texture coordinates hide
  the padding.
- **Per-asset settings** go in `[assets."<file>"]` tables in `selu.toml` when the
  defaults aren't right: render size, `@2x` variants, format, a solid background, mipmaps.
- **Atlases:** `atlas = "icons"` on a group of small images packs them into one
  texture and generates the texture coordinates. `import { gear } from
  "../assets/icons.atlas";` then gives `<Icon atlas={gear} />`. Fewer texture
  files load faster and use less memory.
- **SVG rendering is reproducible:** it uses a built-in renderer and no system
  fonts (text in an SVG must be converted to paths, otherwise the build says
  so). The same input always gives the same bytes; `selu build --check`
  confirms committed outputs are up to date.
- Conversions are cached by content hash, settings and converter version, so
  only changed assets are converted again.
- `selu debug` draws the converted files, so what you see in the simulator is
  what the game gets.

#### 6.1.1 Audio through ffmpeg

- Audio conversion uses **ffmpeg**, so any format ffmpeg reads works. Selu
  finds it in this order: `[tools] ffmpeg = "<path>"` in `selu.toml`, the
  `SELU_FFMPEG` environment variable, then `PATH`. Selu does not ship ffmpeg
  (its licence and size are the user's choice); if it's missing and a sound
  needs it, the build stops with a message saying which file needs it and how
  to install ffmpeg.
- WAV and FLAC also convert without ffmpeg (a small built-in encoder), so a
  project that only uses those needs nothing installed.
- Output is OGG Vorbis, which every target plays. Defaults: 44.1 kHz, quality
  5, stereo kept. Per-sound settings in `selu.toml`:

  ```toml
  [assets."sounds/ding.wav"]
  channels = 1
  quality = 4

  [assets."sounds/alert.m4a"]
  normalize = -16       # loudness in LUFS
  trimSilence = true
  maxSeconds = 3
  ```

  `normalize` sets the loudness in LUFS (ffmpeg's `loudnorm`), `trimSilence`
  cuts silence at both ends, `maxSeconds` warns when a sound is longer.
- Builds are reproducible for a given ffmpeg version: metadata is stripped and
  bit-exact flags are used. `selu build --check` reports a different ffmpeg
  version instead of claiming the files changed.
- `selu doctor` lists the tools found (ffmpeg and any custom converters) with
  their versions.

#### 6.1.2 Custom converters

Any other format can be handled by a command you configure, for example a PSD
exporter or a different audio encoder:

```toml
[[converters]]
from = [".psd"]
to = ".png"
run = ["magick", "{in}", "{out}"]

[[converters]]
from = [".mid"]
to = ".wav"
run = ["fluidsynth", "-ni", "font.sf2", "{in}", "-F", "{out}"]
```

- A converter turns a file into a format Selu already handles; the result then
  goes through the normal pipeline (`.psd` -> `.png` -> TGA/BLP).
- Converters run with a timeout, without a shell, and only on files inside the
  project. Their version (an optional `version = [...]` command) is part of the
  cache key.

### 6.2 Export

```
selu export                 # every target in selu.toml
selu export --target forever
```

1. **Builds** the addon in release mode (`-O1`, source maps kept outside the
   addon folder).
2. **Converts** every asset (6.1).
3. **Checks** the result before anything is zipped:
   - the `.toc` has a title, version, interface number and only files that
     exist;
   - every file the code references exists, and no file is left unreferenced
     (a warning);
   - textures have valid sizes and formats for the target;
   - file and folder names are ones the client loads reliably (ASCII, no
     reserved characters, not too long);
   - nothing outside the addon folder, no source files, no source maps, no
     editor or OS files.
4. **Writes** `export/<name>/` (the folder you could copy straight into
   `Interface/AddOns`) and `export/<name>-<version>.zip` with the addon folder
   at the top level of the zip, as CurseForge and Wago expect, plus a
   `.sha256` next to the zip.

An addon that consists of several folders (for example a separate data addon)
lists them in `selu.toml` under `addons`; the zip then contains all of them.

## 7. Manifests

- One JSON file per build: `manifests/<flavour>/<build>.json`, with a
  versioned schema. These are generated and only read by tools, so they stay
  JSON; nobody edits them by hand.
- **Generated, not written:** from the client's own API documentation in
  Blizzard's UI source for that branch (functions, events, structures,
  documented restriction and secret flags), plus FrameXML globals and widget
  methods found in that source. Only names, signatures and flags go into the
  manifest; no Blizzard source is copied.
- **Overlays** (`manifests/<flavour>/overlay.toml`, edited by hand) add what the
  documentation lacks, each entry with a note saying where the knowledge comes
  from.
- **Probe results** (`selu probe`) mark entries as verified in the game.
- v1 bundles manifests for the newest known `retail` and `forever` builds.

## 8. Simulator in v1

Both `selu debug` and `selu test` use it.

**Modelled:**

- Lua 5.1 with the WoW library additions the target's dialect lists (`bit`,
  `strsplit`, `strjoin`, `wipe`, `tinsert`, ...).
- Addon loading: `.toc` order, `...` addon name and table, `ADDON_LOADED`,
  `PLAYER_LOGIN`, `PLAYER_ENTERING_WORLD`, SavedVariables saved on logout and
  loaded on login, `/reload`.
- Frames: Frame, Button, CheckButton, EditBox, StatusBar, Slider, ScrollFrame,
  Cooldown, Texture, FontString, MaskTexture. Anchors, sizes, scale, effective
  scale, strata, frame levels, draw layers and sublevels, alpha, shown/visible
  propagation, mouse enable, hover, click, drag, scripts and `HookScript`.
- `C_Timer`, `GetTime`, `debugprofilestop`, `OnUpdate` with simulated time.
- Events, `RegisterUnitEvent`, `hooksecurefunc`.
- Units `player`, `target`, `focus`, `party1-4`, `raid1-40`, `pet`: name,
  class, level, health, power, auras, in-combat; the world panel edits them.
- Combat lockdown and protected calls (`ADDON_ACTION_BLOCKED` the way the client
  reports it), secret values per target.
- A minimal `UIParent`, `WorldFrame`, `GameTooltip`, chat output and
  `SlashCmdList`.

**Not modelled in v1:** Blizzard's own UI, the 3D world, the map, spells
actually casting, items beyond what the world panel sets, real textures and
fonts (Selu ships one open font and draws Blizzard textures as labelled
placeholders; the addon's own TGA/PNG files are drawn). A call into something
not modelled returns the manifest's documented empty value and is listed once
in the "not simulated" panel.

**`selu test`:** files named `*.test.selu`. `test`, `describe`, `beforeEach`,
`expect` with `toBe`, `toEqual`, `toBeNull`, `toThrow`, `toContain`,
`toBeLessThan`, `toBeGreaterThan`, and `sim.spy("Name")` with
`toHaveBeenCalled`/`toHaveBeenCalledWith`. Plus the `sim` and `perf` helpers
shown above. Output for people by default, `--json` for tools; the exit code
says whether everything passed.

## 9. Error messages

- Every diagnostic has a code (`SELU0042`), the source line with the exact span
  underlined, one plain sentence saying what is wrong, and a "try" line with
  the fix when there is one. `selu explain SELU0042` prints a longer
  explanation with an example.
- Errors name the target when only one target is affected.
- The compiler reports all errors in a file at once, not just the first.

## 10. How it's built

- **Rust workspace**, built through the shared sccache. Crates:
  `selu-diagnostics`, `selu-syntax`, `selu-manifest`, `selu-check`,
  `selu-lower`, `selu-sim`, `selu-cli`, then `selu-ui` (the GUI framework's
  runtime and codegen), `selu-debug` (the simulator app) and `selu-lsp`.
- The simulator embeds Lua 5.1 through `mlua`; frames, layout and the API model
  are Rust.
- The `selu debug` app is a native window: the game screen is drawn with the
  GPU, the tool panels with an immediate-mode Rust UI library.
- The VS Code extension is a thin TypeScript client of `selu-lsp`.

## 11. Order of work

1. **Base** (in progress): workspace, syntax for a small subset, manifest
   types, Lua output, simulator boot, `selu build` for a hello-world addon.
2. **Language core:** all of section 2 and 3 except async.
3. **Simulator core:** frames, layout, timers, units, combat, saved data;
   `selu test` with `sim` and `expect`.
4. **GUI framework:** section 4 with its performance tests.
5. **`selu debug` app:** game screen, frame inspector, event timeline, world
   panel, debugger, saved data view, memory panel.
6. **Manifests:** generators for `retail` and `forever`, overlays, `selu probe`.
7. **Async, `-O1`, minifier, asset conversion and `selu export`, VS Code extension.**
8. **Proof:** rebuild one real feature of Renner's Toolkit in Selu, compare its
   output, memory and CPU against the hand-written version, and fix what the
   comparison shows.

## 12. Owner decisions on publishing

- **Licence:** MIT, like Renner's Toolkit (owner, 2026-11-04).
- **crates.io:** the name `selu` is claimed with a placeholder `0.0.1`
  (`placeholder/selu/`, outside the workspace); the real compiler is
  published as `0.1.0` later (owner, 2026-11-04). Every publication still
  needs the owner's yes.
- **npm:** not needed by the plan (the VS Code extension goes to the VS Code
  Marketplace); claiming the name there is optional and still open.
- **GitHub:** `rennerdo30/selu`, public (owner, 2026-11-04).

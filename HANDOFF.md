# Handoff

## State (2026-11-05, after round 9)

- `master`, public at https://github.com/rennerdo30/selu. Push only
  `master`, by explicit ref. The gate (fmt, clippy `-D warnings`,
  `cargo test --workspace`, both proof suites with `--locked`,
  `git diff --check`, clean status) passes with about 790 tests. A full
  workspace run takes about 10-15 minutes; run it in the background.
- Rounds 1-6: see the git history and `docs/WORK-PACKAGES.md`.
- Round 7-8 (2026-11-04/05): frame templates (F), restrictions (G), lean
  output (K), the simulator core S0-S3 (sessions, frames, source maps), and
  Selu Studio (`selu debug`, `crates/selu-debug`): editor with highlighting,
  completion and formatting, Explorer, Project Settings, background builds,
  Run in Game through Battle.net with a version picker.
- `selu fmt` (`crates/selu-fmt`), opinionated, width 100.
- Round 9 and follow-ups (2026-11-05):
  - B: `keybinding(...)` writes `Bindings.xml` (`docs/lang/bindings.md`).
  - S4: `selu test` with `*.test.selu`, `expect`, `sim.*`, spies
    (`docs/lang/testing.md`).
  - Pilot port of the toolkit's XPBarETA (`proof/xpeta/`,
    `docs/proof/xpeta.md`) with a gap list for the full port.
  - F2: `frameFamily("MerchantItem", i, "NameFrame")`, typed numbered
    Blizzard frames; widget members are closed (only declared members
    compile); `_G` is a reserved name.
  - W/W2/W3: typed methods from Blizzard's Lua mixins and templates, a
    sourced engine overlay (GameTooltip NumLines/ClearLines), typed frame
    pools behind reviewed source digests (an unknown Pools.lua digest
    withholds the pool contracts with an `update` warning; see
    `docs/manifests.md`), and the `update` command
    (`scripts/update-manifests.bat`).
  - Z + H: the manifest store (`manifests/store/*.selus`): content
    addressed, append-only, delta encoded, 3.3 MB for 14 builds; extraction
    revisions for generator improvements; a derived local cache under
    `target/selu-manifest-cache/`. It holds the newest build of every
    `wow-ui-source` branch.
  - Targets are Blizzard TACT product codes (`wow`, `wow_classic`,
    `wow_classic_era`, `wowf`, ...) or exact pins; `selu.lock` records the
    exact builds and moves only with `selu targets --update`. Old flavour
    names still work with a warning.
- **Owner rules for Selu** are in `CLAUDE.md`/`AGENTS.md` of this repository:
  no Lua interop of any kind (package N was thrown away), and only build
  what the owner asked for.
- Owner goal: Renner's Toolkit gets written in Selu
  (`docs/TOOLKIT-SURVEY.md`, `docs/proof/xpeta.md` for the remaining gaps).

## Decisions taken while the owner was away (2026-11-04 night)

Each is recorded in the named document; all can be reversed.

- **Initialisation order of class fields:** standard (current TypeScript and
  JavaScript), verified with node (`docs/lang/classes.md`).
- **Classes versus object shapes:** an instance fits a fields-only object
  type; an object literal never becomes a class instance
  (`docs/lang/classes.md`).
- **Import paths are case-exact;** module files starting with `__selu` are
  rejected (`docs/lang/modules.md`).
- **SavedVariables names:** readable `<Addon>_<name>` by default; `saved
  global Name` adopts an existing global, and data without a Selu version
  counts as version 1, which is the porting recipe for `RennerToolkitDB` /
  `RennersAddonsDB` (`docs/lang/saved.md`). Class instances can't be stored
  in saved data.
- **Undocumented game globals** come from Blizzard's own UI Lua in the same
  branch, with evidence `source` (`docs/manifests.md`); nothing is marked
  `verified` without a probe in the client. `SetOverrideBindingClick` on
  retail has no honest evidence yet and stays out until `selu probe`.
- **Multiple results of game API calls:** first result as a single value,
  all by destructuring, never implicit expansion; Selu's own tuple functions
  stay strict. Second opinion from Sol agreed (`docs/WORK-PACKAGES.md`,
  round 4).
- **Game API checks warn, they don't block** (`docs/lang/api.md`): the
  manifests can't capture templates, mixins, overloads and undocumented
  optional arguments, so signature mismatches on game APIs and unknown
  widget members are warnings (`[build] strict_api = true` makes them
  errors); type names the documentation doesn't define are untyped. A
  missing API on a target always stays an error. Selu's own code stays
  strict. Reason: a false error blocks porting real code, a missing check
  doesn't.
- **Namespaces are not values** (`const ns = C_Foo` is an error), which
  closes an availability bypass (`docs/lang/targets.md`).
- **Every client:** manifests for `classic-era`, `classic-anniversary`,
  `classic-titan` and `classic` next to `retail` and `forever` (owner's
  wish; SPEC-V1 section 1 updated).
- **Manifests stay plain JSON in git** (about 39 MB together after M3, text that
  diffs well), not LFS.
- **`typeof` gives Lua type names** (`"nil"`, `"table"`, `"function"`,
  never `"object"` or `"undefined"`), since the result is Lua's `type()`
  (`docs/lang/types.md`).
- **Manifest constants keep values only for numbers and short
  identifier-like strings (ASCII, at most 64 characters);** other strings (markup, long text) keep the name
  only (`docs/manifests.md`).
- **Saved schemas record resolved shapes,** so renaming a `type` alias
  doesn't count as a schema change; existing schemas stayed byte-identical.
- **String patterns:** a `luaPattern` tag rather than JavaScript regexes
  (Sol's second opinion; `docs/WORK-PACKAGES.md`, round 6).
- **Errors:** an `Error` records its Selu position when created; `e.message`
  is the raw message; the position is shown once, when the error escapes to
  the game; a user name `Error` is reserved (`docs/lang/errors.md`).
- **`%f` frontier patterns are allowed** (Lua 5.1's `lstrlib.c` implements
  them on every client); `%g` is rejected (`docs/lang/patterns.md`).
- **Simulator timers follow `1.13.2:SharedXML/C_TimerAugment.lua`**, the only
  client evidence: an erroring ticker stops (`crates/selu-sim/src/timers.rs`).

## Next

0. The full toolkit port. Before migrating modules, per the pilot
   (`docs/proof/xpeta.md`): redesign the toolkit's frame scanning (`.bars`,
   one `pairs(_G)`), plan adopting the existing `RennerToolkitDB` saved
   data, and port the shared `RA.UI`/`RA.HUD` layers. Ask the owner before
   starting any of these.
1. Small items from the pilot: `array.clear()`, an empty typed array as a
   class field, the `C_Timer.NewTicker` callback type.
2. 75 retail engine methods without typed declarations (mostly
   GameTooltip setters), listed per flavour in the W report.
3. Not yet planned: GUI framework (SPEC section 4), async, asset pipeline
   and `selu export`, VS Code extension.

## Traps

- **Gate the push.** Run every check (fmt, clippy, `cargo test`, `selu build
  proof/junksell` followed by `cargo test --manifest-path
  proof/junksell/tests/Cargo.toml`, `git diff --check`) joined with `&&`, and
  put `git push` last in the same chain. Twice a failing proof test was
  pushed because the commands were chained with `;`.

- `.cargo/config.toml` is local only (gitignored since the repo went
  public): it points builds at the shared sccache (`rustc-wrapper` =
  `E:/Development/_tools/sccache/0.19.0/.../sccache.exe`, `jobs = 8`,
  `incremental = false`, `[env] SCCACHE_DIR = "E:/sccache"`). A fresh
  checkout on this PC needs it recreated; never commit it.
- Push only `master`, by explicit ref (`git push origin
  HEAD:refs/heads/master`): the local `refs/t3/...` checkpoints are tool
  commits with a non-approved identity.

- Python on Windows writes CRLF: open files with `newline='\n'` when
  patching. Bash heredocs in this tool can eat backslashes; write scripts
  with the Write tool when they contain `\`.
- `selu-lower` doesn't run the checker; every test that lowers must check
  first (see `crates/selu-cli/tests/semantics.rs`).
- Tests must be able to fail: prove a new semantics test red on the old
  code (break the fix temporarily) before keeping it.
- Delegated jobs committed CRLF files three times; `.gitattributes` now
  has `* text=auto eol=lf`, and `git diff --check` in the gate catches the
  rest.
- The store is append-only: never rewrite a published segment. Generator
  changes bump the extraction version (a test enforces it) and append a
  new revision.

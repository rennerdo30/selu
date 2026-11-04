# Handoff

## State (2026-11-04, after round 6)

- `master`, public at https://github.com/rennerdo30/selu. Push only
  `master`, by explicit ref. `cargo test` (480 tests, including every
  example built with the real binary and run in the simulator), the proof
  test (4), clippy and `cargo fmt --check` are clean.
- Built and merged, each by a Codex job with an independent review (every
  review found real bugs, all fixed before merge):
  - Round 1: the language core of SPEC-V1 section 2 (packages A-C).
  - Round 2: modules with `.toc` order from imports (D1), classes (D3),
    `command`/`hook`/`hookScript`/`log`/`L` (D4).
  - Round 3: `saved` with checked migrations and `selu.saved.toml` (D2),
    and generated manifests for retail 12.1.5 and Forever 1.60.1 (M1).
  - Round 4: readability (R), every client (M2), typed game API (T1),
    `has(...)` and several targets (T2).
  - Round 5: named types, `interface`, generics, `unknown`/`typeof` (L);
    XML frames, fonts, constants and templates in the manifests (M3); the
    `JunkSell` proof port (P, `proof/junksell/`, `docs/proof/junksell.md`).
  - Round 6: simulator timers and `OnUpdate` (Q), `try`/`catch`/`finally`/
    `throw` with `Error` (E), Lua string patterns via `luaPattern` (S). Codex
    ran out of usage mid-round (until 2026-11-08); the last fixes and
    re-checks were done by Claude agents.
- Examples: 13 (`examples/README.md`); each is a test.
- `selu` 0.0.1 placeholder is on crates.io; licence MIT.
- Owner goal: Renner's Toolkit gets written in Selu
  (`docs/TOOLKIT-SURVEY.md`). The proof port's open gaps are the plan.

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

0. Round 7 (F, G, K) is running. Then round 8, the simulator core
   (`docs/SIMULATOR.md` is the architecture, reviewed by Sol;
   `docs/WORK-PACKAGES.md` has the S0-S3 briefs and the outline of
   `selu test`, Selu Studio and the debugger in rounds 9 and 10).
1. Less runtime scaffolding (proof gap 8); the error runtime is about 45
   lines per file that uses it and could live once in the shared namespace; restriction and combat
   checks and secret values (DESIGN.md section 3); `selu probe` for
   verified evidence; a second, larger proof module that uses `try`,
   patterns and timers instead of the JunkSell workarounds.
2. Small follow-ups from the round-6 re-checks: hoist capture-free `try`
   bodies outside loops too; the flaky `every_example_builds_for_every_client`
   failure seen once on `timers --target retail` (empty stderr, not
   reproduced).
3. Not yet planned: GUI framework (SPEC section 4), `selu debug` app, async,
   asset pipeline and `selu export`, VS Code extension.

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

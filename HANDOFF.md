# Handoff

## State (2026-11-04, night)

- `master`, public at https://github.com/rennerdo30/selu. Push only
  `master`, by explicit ref. `cargo test` (290 tests, including every
  example built with the real binary and run in the simulator), clippy and
  `cargo fmt --check` are clean.
- Built and merged, each by a Codex job with an independent review (every
  review found real bugs, all fixed before merge):
  - Round 1: the language core of SPEC-V1 section 2 (packages A-C).
  - Round 2: modules with `.toc` order from imports (D1), classes (D3),
    `command`/`hook`/`hookScript`/`log`/`L` (D4).
  - Round 3: `saved` with checked migrations and `selu.saved.toml` (D2),
    and generated manifests for retail 12.1.5 and Forever 1.60.1 (M1).
- Round 4: R readability, M2 every client and T1 typed game API are merged;
  T2 `has(...)` and several targets is in its final integration.
- Examples: `hello`, `loot-summary`, `cooldowns`, `modules`, `classes`,
  `slash-and-hooks`, `saved-settings`; each is a test (`expected.txt`).
- `selu` 0.0.1 placeholder is on crates.io; licence MIT.
- Owner goal: Renner's Toolkit gets written in Selu
  (`docs/TOOLKIT-SURVEY.md`). Estimate after round 3: about 60% of what the
  full port needs; a first small module is within reach once T1 lands.

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
- **Manifests stay plain JSON in git** (about 9.9 MB together, text that
  diffs well), not LFS.

## Next

1. Merge round 4 (R, then T1, then T2), with the usual review per branch.
2. Then: restriction and combat checks and secret values (DESIGN.md
   section 3), using the manifest flags M1 records; `selu probe` for
   verified evidence; unify the two file-prelude mechanisms if R didn't.
3. Proof port (SPEC step 8): one small Toolkit module (`XPBarETA` or
   `JunkSell`) in Selu, compared with the hand-written Lua.
4. Not yet planned: GUI framework (SPEC section 4), `selu debug` app, async,
   asset pipeline and `selu export`, classic flavours, VS Code extension.

## Traps

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

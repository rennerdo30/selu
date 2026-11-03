# Handoff

## State (2026-11-04)

- `master`, public at https://github.com/rennerdo30/selu. Push only
  `master`, by explicit ref. `cargo test` (153 tests), clippy and
  `cargo fmt --check` are clean.
- The language core of SPEC-V1 section 2 is in, except classes, modules,
  async and the WoW features: scopes, types, loops, `continue`, arrays,
  objects, `Map`/`Set`/`WeakMap`, template strings, closures, defaults,
  rest parameters, destructuring, tuple returns. Built as packages A-C by
  Codex jobs, each independently reviewed (all three reviews found real
  bugs, fixed before merge). Decisions per area are in `docs/lang/`.
- `selu` 0.0.1 placeholder is on crates.io; licence MIT.
- Owner goal: Renner's Toolkit gets written in Selu
  (`docs/TOOLKIT-SURVEY.md`).

## Next

1. Package D (WoW features): `saved` with migrations and
   `selu.saved.toml`, `command`, `hook`/`hookScript`, `log`, `L` strings,
   modules with `.toc` order from the import graph, then classes. Write its
   brief in `docs/WORK-PACKAGES.md` first, in the style of A-C, with the
   shared-file hooks named up front (A-C each needed some that weren't
   foreseen).
2. Readability package (found by the examples, 2026-11-04): long table
   literals print on one line (the printer should break them); `filter`/`map`
   with an inline arrow copy receiver and callback into extra temporaries
   instead of inlining the body; `m.set(k, (m.get(k) ?? 0) + n)` produces
   eight temporaries; `x += a.b * c` wraps a field read in temporaries;
   `return (f()), x` has brackets Lua doesn't need.
   From round 2: the per-addon runtimes sit in the addon table under the
   keys `[true]` (events) and `[false]` (log/L/commands), which is
   collision-free but cryptic (use named reserved keys); log and `L` text
   without placeholders still goes through `string.format`, and `tostring`
   wraps values that are already strings. Each fix gets a test on
   the exact output.
3. Owner wish (2026-11-04): more examples. Every package adds one with an
   `expected.txt` (`examples/README.md`); add `examples/modules` (from D1),
   classes and WoW built-ins examples when those packages merge.
4. Typed manifests (SPEC step 6) so game API values stop being untyped.

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

# Handoff

## State (2026-11-04)

- `master`, local only. `cargo test`, `cargo clippy --all-targets -- -D
  warnings` and `cargo fmt --check` are clean.
- SPEC-V1 section 11 step 2 (language core) is in progress. Done: real block
  scopes, shadowing, use-before-declaration, `const` reassignment, undeclared
  reads, `global name = value;`, and forward declarations in the lowering
  (rules in `docs/ARCHITECTURE.md`, "Scopes and names"), and the basic
  type checker ("Types").
- Owner goal: Renner's Toolkit gets written in Selu. `docs/TOOLKIT-SURVEY.md`
  lists what it uses and the resulting order of work.

## Next

- The foundation for parallel work is done (owner asked, 2026-11-04): the
  parser handles all of SPEC-V1 section 2 except classes and modules, the
  lowering builds a Lua tree with temporaries, and each language area has
  its own files. `docs/WORK-PACKAGES.md` holds the three briefs (A control
  flow, B data, C functions) with acceptance items.
- Owner's plan: three agents, effort medium, each in its own worktree on a
  branch from current `master`; merge A first, then B and C; an independent
  review per branch, then the final check here.
- Published: `selu` 0.0.1 placeholder on crates.io; public repo
  https://github.com/rennerdo30/selu (push `master` by explicit ref only).

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

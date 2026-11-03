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

1. Loops, then arrays/objects/templates, hoisted temporaries for `??`.
2. Functions/arrow functions and module tables early (TOOLKIT-SURVEY).

## Traps

- Python on Windows writes CRLF: open files with `newline='\n'` when
  patching. Bash heredocs in this tool can eat backslashes; write scripts
  with the Write tool when they contain `\`.
- `selu-lower` doesn't run the checker; every test that lowers must check
  first (see `crates/selu-cli/tests/semantics.rs`).
- Tests must be able to fail: prove a new semantics test red on the old
  code (break the fix temporarily) before keeping it.

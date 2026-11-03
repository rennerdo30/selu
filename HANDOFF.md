# Handoff

## State (2026-11-04)

- `master`, local only. `cargo test`, `cargo clippy --all-targets -- -D
  warnings` and `cargo fmt --check` are clean.
- SPEC-V1 section 11 step 2 (language core) is in progress. Done: real block
  scopes, shadowing, use-before-declaration, `const` reassignment, undeclared
  reads, `global name = value;`, and forward declarations in the lowering
  (rules in `docs/ARCHITECTURE.md`, "Scopes and names").
- Owner goal: Renner's Toolkit gets written in Selu. `docs/TOOLKIT-SURVEY.md`
  lists what it uses and the resulting order of work.

## Next

1. Basic type checker: `number`, `string`, `boolean`, `null`, `T?`,
   inference; truthiness rule, `+`, `==`/`===`. Plan: the parser gives every
   `Expr` an id, `selu-check` returns a type table, `selu-lower` reads it
   (needed for `+` on strings -> `..`, `.length`, array indexing). Values
   from the manifest stay an internal "untyped" type until manifests carry
   types; record that as a known simplification.
2. Loops, then arrays/objects/templates, hoisted temporaries for `??`.
3. Functions/arrow functions and module tables early (TOOLKIT-SURVEY).

## Traps

- Python on Windows writes CRLF: open files with `newline='\n'` when
  patching. Bash heredocs in this tool can eat backslashes; write scripts
  with the Write tool when they contain `\`.
- `selu-lower` doesn't run the checker; every test that lowers must check
  first (see `crates/selu-cli/tests/semantics.rs`).
- Tests must be able to fail: prove a new semantics test red on the old
  code (break the fix temporarily) before keeping it.

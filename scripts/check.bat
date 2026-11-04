@echo off
rem The repository gate, as run before every push: format check, clippy,
rem the full workspace tests and the proof port's tests. Stops at the
rem first failure. This builds and tests everything, so it takes a while.
setlocal
set "ROOT=%~dp0.."
cd /d "%ROOT%" || exit /b 1
echo == cargo fmt --check
cargo fmt --all -- --check || exit /b 1
echo == cargo clippy
cargo clippy --workspace --all-targets -- -D warnings || exit /b 1
echo == cargo test
cargo test --workspace || exit /b 1
echo == proof test
cargo test --manifest-path proof\junksell\tests\Cargo.toml || exit /b 1
echo All checks passed.
exit /b 0

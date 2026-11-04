@echo off
rem Builds the release `selu` and `selu-studio` binaries into target\release.
rem Called by the other scripts; sets SELU_EXE for the caller.
rem `selu debug` finds selu-studio beside selu, so both are built together.
for %%I in ("%~dp0..") do set "SELU_ROOT=%%~fI"
set "SELU_EXE=%SELU_ROOT%\target\release\selu.exe"
cargo build --release --manifest-path "%SELU_ROOT%\Cargo.toml" -p selu-cli -p selu-debug
if errorlevel 1 (
  echo selu scripts: building selu and selu-studio failed.
  exit /b 1
)
exit /b 0

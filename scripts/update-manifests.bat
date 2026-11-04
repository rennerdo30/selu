@echo off
setlocal
pushd "%~dp0.."
cargo run -p selu-manifest-gen -- update %*
set "result=%errorlevel%"
popd
exit /b %result%

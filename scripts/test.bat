@echo off
rem Run a Selu project's *.test.selu files (same as `selu test`).
rem   scripts\test.bat [project folder] [--filter text]
setlocal
call "%~dp0_tools.bat" || exit /b 1
set "PROJECT=%~1"
if "%PROJECT%"=="" set "PROJECT=."
if not "%~1"=="" shift
"%SELU_EXE%" test "%PROJECT%" %1 %2 %3 %4
exit /b %errorlevel%

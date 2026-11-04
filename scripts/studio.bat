@echo off
rem Open Selu Studio for a Selu project: builds the tools, builds the addon,
rem then starts selu-studio (same as `selu debug`).
rem   scripts\studio.bat [project folder] [--target flavour@version]
rem Default project: examples\hello.
setlocal
call "%~dp0_tools.bat" || exit /b 1
set "PROJECT=%~1"
if "%PROJECT%"=="" set "PROJECT=%SELU_ROOT%\examples\hello"
if not "%~1"=="" shift
"%SELU_EXE%" debug "%PROJECT%" %1 %2 %3 %4
exit /b %errorlevel%

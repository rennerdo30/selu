@echo off
rem Open Selu Studio for a Selu project: builds the tools, builds the addon,
rem then opens the Studio window (same as `selu debug`). Studio rebuilds and
rem reloads the addon when a file under the project's src folder changes.
rem   scripts\studio.bat [project folder] [--target flavour@version]
rem Default project: examples\frames.
setlocal
call "%~dp0_tools.bat" || exit /b 1
set "PROJECT=%~1"
if "%PROJECT%"=="" set "PROJECT=%SELU_ROOT%\examples\frames"
if not "%~1"=="" shift
"%SELU_EXE%" debug "%PROJECT%" %1 %2 %3 %4
exit /b %errorlevel%

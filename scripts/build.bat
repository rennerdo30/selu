@echo off
rem Build a Selu project into its dist folder.
rem   scripts\build.bat [project folder] [--target flavour@version]
rem Default project: the current folder.
setlocal
call "%~dp0_tools.bat" || exit /b 1
set "PROJECT=%~1"
if "%PROJECT%"=="" set "PROJECT=."
if not "%~1"=="" shift
"%SELU_EXE%" build "%PROJECT%" %1 %2 %3 %4
exit /b %errorlevel%

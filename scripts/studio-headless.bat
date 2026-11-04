@echo off
rem Run the Studio engine without a window, on a built addon folder (one with
rem a .toc). Type one JSON command per line, for example:
rem   {"command":"fire","event":"PLAYER_LOGIN"}
rem   {"command":"slash","line":"/hello"}
rem   {"command":"advance_time","seconds":5}
rem   {"command":"snapshot"}
rem Ctrl+Z then Enter ends the session.
rem   scripts\studio-headless.bat [addon folder]
rem Default addon: the simulator's Hello test addon.
setlocal
call "%~dp0_tools.bat" || exit /b 1
set "ADDON=%~1"
if "%ADDON%"=="" set "ADDON=%SELU_ROOT%\crates\selu-sim\tests\addons\Hello"
"%SELU_ROOT%\target\release\selu-studio.exe" --headless "%ADDON%"
exit /b %errorlevel%

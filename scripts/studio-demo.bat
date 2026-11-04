@echo off
rem Open Selu Studio on a hand-written demo addon (no Selu project needed):
rem press Play to watch the bar grow; type /demo hide or /demo in the console.
setlocal
call "%~dp0_tools.bat" || exit /b 1
"%SELU_ROOT%\target\release\selu-studio.exe" "%SELU_ROOT%\crates\selu-sim\tests\addons\StudioDemo"
exit /b %errorlevel%

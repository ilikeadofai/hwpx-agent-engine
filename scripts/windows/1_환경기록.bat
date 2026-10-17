@echo off
pushd "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\step1_environment.ps1"
echo.
echo Exit code: %ERRORLEVEL%
popd
pause

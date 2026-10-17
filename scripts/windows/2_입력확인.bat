@echo off
pushd "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\step2_verify_inputs.ps1"
echo.
echo Exit code: %ERRORLEVEL%
popd
pause

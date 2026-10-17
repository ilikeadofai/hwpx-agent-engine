@echo off
pushd "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\step3_pack_results.ps1"
echo.
echo Exit code: %ERRORLEVEL%
popd
pause

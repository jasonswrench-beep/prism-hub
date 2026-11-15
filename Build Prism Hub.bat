@echo off
rem Builds the signed Prism Hub installer into the dist folder. Nothing is published.
rem Needs: Rust, Node.js, Python, the GitHub command line, and the signing setup the other
rem apps use (the OPENSTAGE_SIGN_* variables and `az login`). See README.md.
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\build-release.ps1" -Sign
echo.
pause

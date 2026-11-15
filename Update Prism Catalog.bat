@echo off
rem Reads each Prism app's newest stable and preview releases, checks every setup file's
rem SHA-256 and signature, writes the catalog, asks for the hub key's password, signs it and
rem uploads it to the "catalog" release (it asks before uploading). Run it after any app release.
cd /d "%~dp0"
if not exist node_modules (call npm install --no-audit --no-fund)
python -I "%~dp0tools\update_catalog.py"
echo.
pause

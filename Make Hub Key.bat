@echo off
rem Makes the hub's own signing key, once. It signs the catalog and nothing else, and is
rem separate from PrismQDA's and Prism Reference Manager's update keys.
rem
rem You choose the password. Keep the key file and its password off GitHub and backed up:
rem without them no new catalog can be published, and every hub keeps the last one it has.
cd /d "%~dp0"
if not exist node_modules (call npm install --no-audit --no-fund)
node "%~dp0tools\make-hub-key.mjs"
echo.
pause

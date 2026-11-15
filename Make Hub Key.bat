@echo off
title Prism Hub: make the hub key
chcp 65001 >nul
cd /d "%~dp0"
echo This makes the hub key, which signs the catalog. Run it once, before the first hub release.
echo You choose its password. The private half is saved in "Prism Hub key" in your user folder (never in this
echo repository, never on GitHub); keep a backup of that folder. The public half goes into brand\brand.json.
echo.
where node >nul 2>nul || (echo Node.js is not installed. Get it from nodejs.org, then run this again. & goto :end)
node tools\make-hub-key.mjs
:end
echo.
pause

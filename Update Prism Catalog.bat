@echo off
title Prism Hub: update the catalog
chcp 65001 >nul
cd /d "%~dp0"
echo This reads each Prism app's newest releases on GitHub, checks every setup file (its SHA-256 and that it is signed by
echo Jason Wrench), writes the catalog, asks for the hub key's password, signs the catalog and uploads it.
echo Run it after any app release. Until it runs, the hub keeps offering the previous version, which is safe.
echo.
where node >nul 2>nul || (echo Node.js is not installed. Get it from nodejs.org, then run this again. & goto :end)
where gh >nul 2>nul || (echo The GitHub command line is not installed. Get it from cli.github.com, then run this again. & goto :end)
node tools\update-catalog.mjs %*
:end
echo.
pause

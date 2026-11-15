@echo off
title Prism Hub: run the development copy
chcp 65001 >nul
cd /d "%~dp0"
echo This starts the hub from this folder (a development copy, with a console window behind it).
echo The first start takes a few minutes while the program is built.
echo.
where node >nul 2>nul || (echo Node.js is not installed. Get it from nodejs.org, then run this again. & goto :end)
where cargo >nul 2>nul || (echo Rust is not installed. Get it from rustup.rs, then run this again. & goto :end)
if not exist node_modules (call npm.cmd install || goto :end)
rem To try a catalog before it is published: run "Update Prism Catalog.bat --local", then start this with
rem   set HUB_CATALOG_DIR=%~dp0catalog\out
rem A development copy also accepts HUB_TEST_KEY (a test key's public half); a release build ignores both.
call npm.cmd run tauri dev
:end
echo.
pause

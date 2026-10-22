@echo off
rem ============================================================
rem  StageCue one-click Windows packager — DOUBLE-CLICK THIS FILE
rem  Installs dependencies (if needed), builds the installer,
rem  and opens the folder containing it. No typing required.
rem ============================================================
setlocal
title StageCue Windows Builder

rem --- Go to the folder this .bat lives in (the project root) ---
cd /d "%~dp0.."

echo Checking prerequisites...

where node >nul 2>nul
if errorlevel 1 (
    echo Node.js was not found. Opening the download page...
    start https://nodejs.org/en/download
    echo Install the LTS version, then double-click this file again.
    pause
    exit /b 1
)

where rustc >nul 2>nul
if errorlevel 1 (
    echo Rust was not found. Opening the rustup download page...
    start https://rustup.rs
    echo Install it with the defaults, then double-click this file again.
    pause
    exit /b 1
)

if not exist node_modules (
    echo Installing project dependencies ^(first run only, few minutes^)...
    call npm install || goto :fail
)

echo.
echo Building StageCue... this can take 5-15 minutes. A window may pop up to
echo install the Windows build tools on first run — approve it.
call npm run tauri build || goto :fail

echo Opening the folder with your installer...
start "" explorer "src-tauri\target\release\bundle\nsis"
echo.
echo Done! The .exe installer in that folder installs StageCue with a
echo Start Menu shortcut. You can now launch it from the Start Menu.
pause
exit /b 0

:fail
echo.
echo Build failed. See the messages above ^(or send them to the developer^).
pause
exit /b 1

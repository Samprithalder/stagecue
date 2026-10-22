#!/usr/bin/env bash
# StageCue launcher: opens the app (dev mode) from anywhere, e.g. a desktop shortcut.
set -e
PROJECT_DIR="/home/samprit/Code/MOV"
LOG_FILE="${XDG_CACHE_HOME:-$HOME/.cache}/stagecue-dev.log"
mkdir -p "$(dirname "$LOG_FILE")"
cd "$PROJECT_DIR"
exec npm run tauri dev >>"$LOG_FILE" 2>&1

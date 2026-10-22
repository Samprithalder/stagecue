#!/usr/bin/env bash
# One-click StageCue packager: builds the release app and installs a real
# menu/desktop shortcut pointing at the self-contained binary. Run from the
# app menu ("StageCue – Build App" entry) — no terminal needed.
set -e
PROJECT_DIR="/home/samprit/Code/MOV"
LOG_FILE="${XDG_CACHE_HOME:-$HOME/.cache}/stagecue-build.log"
APPS_DIR="$HOME/apps"
mkdir -p "$(dirname "$LOG_FILE")" "$APPS_DIR"

cd "$PROJECT_DIR"
echo "Building StageCue (release)…" | tee "$LOG_FILE"
npm run tauri build >>"$LOG_FILE" 2>&1

# Install the built binary + icon as a normal desktop app
BIN="src-tauri/target/release/stagecue"
if [ -f "$BIN" ]; then
    install -Dm755 "$BIN" "$APPS_DIR/stagecue/stagecue"
    install -Dm644 "src-tauri/icons/icon.png" "$APPS_DIR/stagecue/icon.png"
    cat > "${XDG_DATA_HOME:-$HOME/.local/share}/applications/stagecue-app.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=StageCue
GenericName=Cue-based show control
Comment=Cue-based theatre playback and backdrop control
Exec=$APPS_DIR/stagecue/stagecue
Icon=$APPS_DIR/stagecue/icon.png
Terminal=false
MimeType=
Categories=AudioVideo;Audio;Video;
StartupWMClass=app.stagecue.desktop
EOF
    update-desktop-database "${XDG_DATA_HOME:-$HOME/.local/share}/applications" 2>/dev/null || true
    notify-send "StageCue" "Build complete — 'StageCue' is now in your app menu." 2>/dev/null || true
else
    echo "Build failed — see $LOG_FILE" >>"$LOG_FILE"
    notify-send -u critical "StageCue build failed" "Details in $LOG_FILE" 2>/dev/null || true
    exit 1
fi

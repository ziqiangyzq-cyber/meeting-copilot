#!/usr/bin/env bash
# Build Swift AudioHelper (release) and copy into src-tauri/resources/
# so Tauri bundler includes it in the .app.
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC_TAURI_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$SRC_TAURI_DIR")"
AUDIO_HELPER_DIR="$PROJECT_ROOT/audio-helper"
RESOURCES_DIR="$SRC_TAURI_DIR/resources"

echo "[bundle-audio-helper] building Swift AudioHelper..."
cd "$AUDIO_HELPER_DIR"
swift build -c release

BUILT_BIN="$AUDIO_HELPER_DIR/.build/release/AudioHelper"
if [ ! -f "$BUILT_BIN" ]; then
    echo "[bundle-audio-helper] ERROR: built binary not found at $BUILT_BIN" >&2
    exit 1
fi

mkdir -p "$RESOURCES_DIR"
cp "$BUILT_BIN" "$RESOURCES_DIR/AudioHelper"
chmod +x "$RESOURCES_DIR/AudioHelper"
echo "[bundle-audio-helper] copied to $RESOURCES_DIR/AudioHelper"

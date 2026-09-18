#!/bin/bash
# Build and install the RELEASE app to /Applications, then launch it.
#
# This is what the pinned "Luno Desktop" Dock icon runs. Use it after changes
# you want in the release app. For fast iteration, use the DEV launcher
# (scripts/luno-tray.sh launch / ~/Applications/Luno Dev.app) instead.
#
# `--bundles app` is deliberate: the DMG bundler deletes the intermediate .app
# after packaging it, so a plain `npx tauri build` leaves no .app to copy.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$REPO/node_modules/.bin:$PATH"

DEST="/Applications/Luno Desktop.app"
SRC="$REPO/src-tauri/target/release/bundle/macos/Luno Desktop.app"

cd "$REPO"
echo "Building release app…"
npx tauri build --bundles app

echo "Quitting the running release copy (if any)…"
osascript -e 'quit app "Luno Desktop"' >/dev/null 2>&1 || true
sleep 1
pkill -f "$DEST/Contents/MacOS/luno-desktop" 2>/dev/null || true

echo "Installing to $DEST…"
rm -rf "$DEST"
ditto "$SRC" "$DEST"

"$REPO/scripts/verify-desktop-bundle.sh" "$DEST"
open "$DEST"
echo "Installed and launched $DEST"

#!/bin/bash
# Luno Dev — build the double-clickable Dock icon (~/Applications/Luno Dev.app).
# The .app is a generated local artifact, never committed: it embeds this
# machine's repository path and simply fires scripts/luno-tray.sh in the
# background. Re-run after moving the repository. Idempotent.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
APP="$HOME/Applications/Luno Dev.app"
TMP_SCRIPT="$(mktemp -t luno-applescript).applescript"
trap 'rm -f "$TMP_SCRIPT"' EXIT

mkdir -p "$HOME/Applications"
sed "s#__LUNO_REPO__#$REPO#" "$REPO/scripts/Luno-Dev.applescript" > "$TMP_SCRIPT"

rm -rf "$APP"
osacompile -o "$APP" "$TMP_SCRIPT"

# Best-effort icon: stamp the applet with the app artwork when sips can
# convert it, otherwise keep the default script icon (cosmetic only). Prefer
# the 1024 master; `icon.png` is a 512 generated derivative and yields a
# lower-resolution applet.icns.
ICON_SRC="$REPO/src-tauri/icons/app-icon.png"
[ -f "$ICON_SRC" ] || ICON_SRC="$REPO/src-tauri/icons/icon.png"
if [ -f "$ICON_SRC" ]; then
  TMP_ICON="$(mktemp -t luno-applet).icns"
  trap 'rm -f "$TMP_SCRIPT" "$TMP_ICON"' EXIT
  if sips -s format icns "$ICON_SRC" --out "$TMP_ICON" >/dev/null 2>&1; then
    cp "$TMP_ICON" "$APP/Contents/Resources/applet.icns"
    touch "$APP"
  fi
fi

# Reveal the app so it can be dragged to the Dock. ~/Applications is hidden
# in Finder by default, which is why "nothing appeared" after install.
open -R "$APP" >/dev/null 2>&1 || open "$HOME/Applications" >/dev/null 2>&1 || true

echo "Installed $APP"
echo
echo "This NEVER adds anything to your Dock. To use it:"
echo "  1. Finder is opening with the app selected (or: open ~/Applications)."
echo "  2. Drag 'Luno Dev.app' to the Dock yourself."
echo
echo "Each click runs a fresh DEBUG rebuild (frontend HMR + incremental Rust)."
echo "Manage the server with: scripts/luno-tray.sh status|logs|stop"

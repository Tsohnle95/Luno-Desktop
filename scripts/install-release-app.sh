#!/bin/bash
# Build, verify, and install the RELEASE app in /Applications.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="$REPO/scripts/install-release-app.sh"
DEST="/Applications/Luno Desktop.app"
SRC="$REPO/src-tauri/target/release/bundle/macos/Luno Desktop.app"
VERIFY="$REPO/scripts/verify-desktop-bundle.sh"

source "$REPO/scripts/dev-env.sh"
luno_configure_dev_path "$REPO"

install_staged_bundle() {
  local staged="$1"
  local backup="$2"
  local had_original=0

  if [[ "$backup" != "$DEST".previous.* ]]; then
    echo "Refusing unexpected app backup path: $backup" >&2
    return 1
  fi
  if [[ ! -d "$staged" ]]; then
    echo "Staged app bundle is missing: $staged" >&2
    return 1
  fi
  if [[ -e "$backup" ]]; then
    echo "App backup path already exists: $backup" >&2
    return 1
  fi

  if [[ -e "$DEST" ]]; then
    /bin/mv "$DEST" "$backup"
    had_original=1
  fi

  if ! /usr/bin/ditto "$staged" "$DEST"; then
    /bin/rm -rf "$DEST" || true
    if (( had_original )); then /bin/mv "$backup" "$DEST"; fi
    echo "Could not copy the release app; the previous app was restored." >&2
    return 1
  fi

  if ! "$VERIFY" "$DEST"; then
    /bin/rm -rf "$DEST" || true
    if (( had_original )); then /bin/mv "$backup" "$DEST"; fi
    echo "Installed bundle verification failed; the previous app was restored." >&2
    return 1
  fi

  if (( had_original )); then /bin/rm -rf "$backup"; fi
}

if [[ "${1:-}" == "--install-staged" ]]; then
  if [[ "$#" -ne 3 ]]; then
    echo "usage: $0 --install-staged '/path/to/Luno Desktop.app' '/Applications/Luno Desktop.app.previous.ID'" >&2
    exit 2
  fi
  install_staged_bundle "$2" "$3"
  exit 0
fi

luno_require_node "$REPO"
cd "$REPO"

echo "Building release app with Node $(node --version)…"
npx tauri build --bundles app

if [[ ! -d "$SRC" ]]; then
  echo "Tauri reported a successful build but the app bundle is missing: $SRC" >&2
  exit 1
fi

STAGE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/luno-release-install.XXXXXX")"
trap '/bin/rm -rf "$STAGE_ROOT"' EXIT
STAGED="$STAGE_ROOT/Luno Desktop.app"
BACKUP="$DEST.previous.$$"

echo "Staging release app…"
/usr/bin/ditto "$SRC" "$STAGED"
echo "Verifying staged app before touching the installed copy…"
"$VERIFY" "$STAGED"

echo "Quitting the running release copy (if any)…"
osascript -e 'quit app "Luno Desktop"' >/dev/null 2>&1 || true
sleep 1
pkill -TERM -f "$DEST/Contents/MacOS/luno-desktop" 2>/dev/null || true

if [[ -w /Applications ]]; then
  "$SCRIPT" --install-staged "$STAGED" "$BACKUP"
else
  echo "Administrator access is required to replace $DEST."
  sudo /usr/bin/env "PATH=$PATH" "$SCRIPT" --install-staged "$STAGED" "$BACKUP"
fi

open "$DEST"
echo "Installed and launched $DEST"

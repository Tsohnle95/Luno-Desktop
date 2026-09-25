#!/bin/bash
# Luno — development mode (Tauri dev server).
# Double-click to open a terminal that starts Vite's dev server, compiles a
# debug build, and launches the app. Frontend edits hot-reload instantly via
# Vite HMR; Rust edits trigger an automatic incremental debug rebuild.
# Use this for day-to-day testing instead of a full `npx tauri build`.
set -euo pipefail

cd "$(dirname "$0")"

print_step() { printf '\n\033[1;36m=== %s ===\033[0m\n' "$1"; }
print_ok() { printf '\n\033[1;32m=== %s ===\033[0m\n' "$1"; }

# A double-clicked .command file gets a minimal PATH from Terminal. Resolve
# the repository-pinned Node runtime as well as the Rust and local CLI tools.
source "$PWD/scripts/dev-env.sh"
luno_configure_dev_path "$PWD"
luno_require_node "$PWD"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust stable is required. Install rustup and ensure ~/.cargo/bin is available." >&2
  exit 1
fi
if [ ! -x "$PWD/node_modules/.bin/tauri" ]; then
  echo "Dependencies are not installed. Run npm ci in $PWD first." >&2
  exit 1
fi

print_step "Luno development mode (frontend hot-reload + debug app)"
print_ok "Close this terminal (or press Ctrl+C) to stop the dev server and app."
exec npm run tauri dev

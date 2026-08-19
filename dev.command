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

# A double-clicked .command file gets a minimal PATH from Terminal:
# ~/.cargo/bin (rustup) and /opt/homebrew/bin (Homebrew rust on Apple
# Silicon) are not included, so make sure the Rust toolchain is findable.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PWD/node_modules/.bin:$PATH"

print_step "Luno development mode (frontend hot-reload + debug app)"
print_ok "Close this terminal (or press Ctrl+C) to stop the dev server and app."
exec npm run tauri dev
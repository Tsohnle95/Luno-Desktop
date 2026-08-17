#!/bin/bash
# Luno — build & test (Tauri desktop app).
# Double-click to open a terminal that runs the Rust test suite, the
# frontend typecheck + tests, then builds the production bundle and
# opens the installable package.
set -euo pipefail

cd "$(dirname "$0")"

print_step() { printf '\n\033[1;36m=== %s ===\033[0m\n' "$1"; }
print_ok() { printf '\n\033[1;32m=== %s ===\033[0m\n' "$1"; }
print_err() { printf '\n\033[1;31m=== %s ===\033[0m\n' "$1"; }

export PATH="$PWD/node_modules/.bin:$PATH"

print_step "Rust test suite (src-tauri)"
( cd src-tauri && cargo test ) || { print_err "RUST TESTS FAILED"; exit 1; }

print_step "Frontend typecheck + tests"
npm run typecheck || { print_err "TYPECHECK FAILED"; exit 1; }
npm test || { print_err "FRONTEND TESTS FAILED"; exit 1; }

print_step "Frontend production build"
npm run build || { print_err "FRONTEND BUILD FAILED"; exit 1; }

print_step "Tauri release build"
npx tauri build || { print_err "TAURI BUILD FAILED"; exit 1; }

print_ok "ALL TESTS PASSED AND BUILD COMPLETE"
dmg=$(ls src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1)
if [ -n "$dmg" ]; then
  echo "Package: $dmg"
  echo "Opening the package to install Luno…"
  open "$dmg"
else
  echo "No DMG found under src-tauri/target/release/bundle/dmg/."
  echo "Manual build: npx tauri build"
fi
exit 0
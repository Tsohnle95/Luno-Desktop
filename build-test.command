#!/bin/bash
# Luno — build & test (Tauri desktop app).
# Double-click to open a terminal that runs the Rust test suite, the
# frontend typecheck + tests, then builds the production bundle and
# launches the packaged app.
set -euo pipefail

cd "$(dirname "$0")"

print_step() { printf '\n\033[1;36m=== %s ===\033[0m\n' "$1"; }
print_ok() { printf '\n\033[1;32m=== %s ===\033[0m\n' "$1"; }
print_err() { printf '\n\033[1;31m=== %s ===\033[0m\n' "$1"; }

# A double-clicked .command file gets a minimal PATH from Terminal:
# ~/.cargo/bin (rustup) and /opt/homebrew/bin (Homebrew rust on Apple
# Silicon) are not included, so make sure the Rust toolchain is findable.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PWD/node_modules/.bin:$PATH"

print_step "Rust test suite (src-tauri)"
( cd src-tauri && cargo test ) || { print_err "RUST TESTS FAILED"; exit 1; }

print_step "Frontend typecheck + tests"
npm run typecheck || { print_err "TYPECHECK FAILED"; exit 1; }
npm test || { print_err "FRONTEND TESTS FAILED"; exit 1; }

print_step "Frontend production build"
npm run build || { print_err "FRONTEND BUILD FAILED"; exit 1; }

print_step "Tauri release build (app bundle)"
npx tauri build --bundles app || { print_err "TAURI BUILD FAILED"; exit 1; }

print_ok "ALL TESTS PASSED AND BUILD COMPLETE"
app_bundle=src-tauri/target/release/bundle/macos/Luno.app
if [ -d "$app_bundle" ]; then
  echo "App bundle: $app_bundle"
  echo "Launching Luno…"
  open "$app_bundle" || print_err "Could not launch Luno with 'open' — run: open \"$app_bundle\""
else
  echo "App bundle not found under src-tauri/target/release/bundle/macos/."
  echo "Manual build: npx tauri build"
fi

print_step "Tauri DMG package (best-effort)"
npx tauri build --bundles dmg \
  || print_err "DMG PACKAGING FAILED — the app above is already built and launched"

dmg=$(ls src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1 || true)
if [ -n "$dmg" ]; then
  echo "Installable DMG: $dmg"
fi
exit 0
#!/bin/bash
# Luno — detached development launcher (no Terminal window required).
#
# Clicking the Dock icon (~/Applications/Luno Dev.app, built by
# scripts/install-dock-icon.sh) fires `luno-tray.sh launch` in the
# background. The first launch starts `npm run tauri dev` detached with
# nohup; every later click only focuses the already-running app.
# Rebuilds stay automatic through the dev server itself: frontend edits
# hot-reload via Vite HMR and Rust edits trigger an incremental debug
# rebuild, exactly as with ./dev.command — minus the terminal.
#
# NOTE: this launches the DEV build at
# src-tauri/target/debug/luno-desktop. The Dock icon at
# /Applications/Luno Desktop.app is a RELEASE bundle and never rebuilds;
# reinstall it with `npx tauri build` + copy after release changes.
#
# Usage: scripts/luno-tray.sh launch|status|stop|logs [-f]
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
# A double-clicked launcher gets a minimal PATH: make sure the Rust
# toolchain and local node binaries are findable (same as dev.command).
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$REPO/node_modules/.bin:$PATH"

RUNDIR="${TMPDIR:-/tmp}/luno-dev"
PIDFILE="$RUNDIR/server.pid"
LOGFILE="$RUNDIR/dev.log"
LOCKDIR="$RUNDIR/launch.lock"
VITE_URL="http://127.0.0.1:1420/"
# The DEV binary `tauri dev` runs. Match on the full path so a running
# release bundle (/Applications/Luno Desktop.app, same process name) is
# never mistaken for the dev app.
DEV_BINARY="$REPO/src-tauri/target/debug/luno-desktop"
# First debug compile can take several minutes; the launcher runs
# detached so this never blocks a click.
READY_TIMEOUT_SECS=600

mkdir -p "$RUNDIR"

alert() {
  # Failure surface without a terminal: a macOS dialog naming the log.
  osascript -e "display dialog \"${1//$\"/\\\"}\nLog: $LOGFILE\" with title \"Luno\" buttons {\"OK\"} default button \"OK\"" >/dev/null 2>&1 || true
}

vite_up() { curl -sf -o /dev/null --max-time 2 "$VITE_URL" 2>/dev/null; }
daemon_alive() { [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE" 2>/dev/null)" 2>/dev/null; }
app_running() { pgrep -f "$DEV_BINARY" >/dev/null 2>&1; }

focus_app() {
  # Name-based activation is ambiguous: the release bundle shares the
  # `luno-desktop` name. Target the dev PID directly.
  local pid
  pid="$(pgrep -f "$DEV_BINARY" | head -n1)"
  [ -n "$pid" ] || return 0
  osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $pid) to true" >/dev/null 2>&1 || true
}

cmd_status() {
  if vite_up && daemon_alive; then
    echo "Luno dev server running (pid $(cat "$PIDFILE"), vite up)."
    app_running && echo "App process running." || echo "App process not started yet (still compiling?)."
    return 0
  fi
  if daemon_alive; then
    echo "Luno dev server starting (pid $(cat "$PIDFILE"), vite not up yet)."
    return 0
  fi
  echo "Luno dev server not running."
  return 0
}

cmd_stop() {
  if [ ! -f "$PIDFILE" ]; then echo "Luno dev server not running."; return 0; fi
  pid="$(cat "$PIDFILE")"
  rm -f "$PIDFILE"
  # The daemon was started with setsid, so killing its process group stops
  # npm + vite + cargo together. Refuse to signal a recycled PID.
  if kill -0 "$pid" 2>/dev/null && ps -o comm= -p "$pid" 2>/dev/null | grep -qE '^(npm|node|tauri|cargo)$'; then
    kill -- "-$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
    echo "Stopped Luno dev server (was pid $pid)."
  else
    echo "Luno dev server not running (stale pid file cleared)."
  fi
}

cmd_logs() {
  [ -f "$LOGFILE" ] || { echo "No log yet at $LOGFILE."; return 0; }
  if [ "${1:-}" = "-f" ]; then exec tail -f "$LOGFILE"; else tail -n 100 "$LOGFILE"; fi
}

cmd_launch() {
  # Single-flight: a second click while one launch is working is a no-op.
  if ! mkdir "$LOCKDIR" 2>/dev/null; then return 0; fi
  trap 'rmdir "$LOCKDIR" 2>/dev/null || true' EXIT

  if vite_up && daemon_alive; then focus_app; return 0; fi
  if daemon_alive && ! vite_up; then return 0; fi  # boot in progress
  rm -f "$PIDFILE"  # clear stale pid file, if any

  if [ ! -x "$REPO/node_modules/.bin/tauri" ]; then
    alert "Dependencies are not installed. Run npm ci in $REPO once, then click again."
    echo "Missing node_modules/.bin/tauri; run npm ci first." >&2
    return 1
  fi
  command -v cargo >/dev/null 2>&1 || {
    alert "The Rust toolchain was not found on PATH. Install rustup stable, then click again."
    echo "cargo not found." >&2
    return 1
  }

  echo "Starting Luno dev server (log: $LOGFILE)…"
  cd "$REPO"
  # nohup detaches so closing callers (and Ctrl+C) cannot take the server
  # down. `setsid` is Linux-only and is NOT available on macOS, so the old
  # `setsid nohup …` line failed instantly and the dev build never started.
  nohup npm run tauri dev >>"$LOGFILE" 2>&1 < /dev/null &
  DAEMON_PID=$!
  disown "$DAEMON_PID" 2>/dev/null || true
  echo "$DAEMON_PID" > "$PIDFILE"

  # Wait in the background-friendly foreground: AppleScript callers already
  # detached us, terminal callers can Ctrl+C (the daemon survives).
  trap 'echo "Continuing in background; check status with scripts/luno-tray.sh status."; rmdir "$LOCKDIR" 2>/dev/null || true; exit 0' INT
  waited=0
  while [ "$waited" -lt "$READY_TIMEOUT_SECS" ]; do
    vite_up && break
    daemon_alive || { alert "The dev server exited during startup. See the log."; return 1; }
    sleep 2
    waited=$((waited + 2))
  done
  vite_up || { alert "The dev server did not come up in time. See the log."; return 1; }

  # Vite is up; the debug app follows once Rust finishes compiling.
  waited=0
  while [ "$waited" -lt "$READY_TIMEOUT_SECS" ]; do
    app_running && break
    daemon_alive || break  # crashed after vite came up; surfaced via log
    sleep 2
    waited=$((waited + 2))
  done
  focus_app
  echo "Luno is up."
}

case "${1:-launch}" in
  launch) cmd_launch ;;
  status) cmd_status ;;
  stop) cmd_stop ;;
  logs) cmd_logs "${2:-}" ;;
  *) echo "Usage: $0 launch|status|stop|logs [-f]" >&2; exit 1 ;;
esac

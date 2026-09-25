#!/bin/bash
# Luno — one-click detached DEV launcher (no Terminal window required).
#
# Clicking ~/Applications/Luno Dev.app runs `luno-tray.sh launch`. The first
# click starts `npm run tauri dev` detached; it compiles the debug app and
# opens the window. While it runs, frontend edits hot-reload (Vite HMR) and
# Rust edits trigger an incremental debug rebuild automatically — no terminal
# and no manual rebuild per change.
#
# Force a clean rebuild: quit the app window, then click the icon again (or run
# `scripts/luno-tray.sh restart`). `status`/`logs` are diagnostic helpers.
#
# This is the DEV build (src-tauri/target/debug/luno-desktop), NOT the release
# bundle at /Applications/Luno Desktop.app. Install a release copy with
# `scripts/install-release-app.sh`.
#
# Usage: scripts/luno-tray.sh launch|restart|status|stop|logs [-f]
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
source "$REPO/scripts/dev-env.sh"
luno_configure_dev_path "$REPO"

RUNDIR="${TMPDIR:-/tmp}/luno-dev"
PIDFILE="$RUNDIR/server.pid"
LOGFILE="$RUNDIR/dev.log"
LOCKDIR="$RUNDIR/launch.lock"
VITE_URL="http://localhost:1420/"
# `tauri dev` runs the DEV binary as the RELATIVE path
# `target/debug/luno-desktop`, so match that suffix. Safe against the release
# bundle (/Applications/Luno Desktop.app/... never contains target/debug).
DEV_MATCH="target/debug/luno-desktop"
# The installed release copy, if present. Dev and release share the
# single-instance guard port (48713) and config, so the release app must be
# quit before the dev app can start.
RELEASE_BINARY="/Applications/Luno Desktop.app/Contents/MacOS/luno-desktop"
# First debug compile can take several minutes; the launcher runs detached so
# this never blocks a click.
READY_TIMEOUT_SECS=600

mkdir -p "$RUNDIR"

alert() {
  # Failure surface without a terminal: a macOS dialog naming the log.
  osascript -e "display dialog \"${1//\"/\\\"}\nLog: $LOGFILE\" with title \"Luno Dev\" buttons {\"OK\"} default button \"OK\"" >/dev/null 2>&1 || true
}

vite_up() { curl -sf -o /dev/null --max-time 2 "$VITE_URL" 2>/dev/null; }
daemon_alive() { [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE" 2>/dev/null)" 2>/dev/null; }
dev_pid() { pgrep -f "$DEV_MATCH" 2>/dev/null | head -n1; }
app_running() { [ -n "$(dev_pid)" ]; }
release_running() { pgrep -f "$RELEASE_BINARY" >/dev/null 2>&1; }

quit_release_app() {
  # Two installs cannot run at once (same instance port + shared config). The
  # dev app would silently exit "another Luno instance is already running", so
  # quit the release copy first when it is the one currently open.
  release_running || return 0
  osascript -e 'quit app "Luno Desktop"' >/dev/null 2>&1 || true
  sleep 1
  release_running && pkill -f "$RELEASE_BINARY" 2>/dev/null || true
}

focus_app() {
  # Name-based activation is ambiguous (release bundle shares the name), so
  # target the dev PID. Needs Accessibility permission for System Events; if
  # that is denied the window simply keeps its current stacking order.
  local pid
  pid="$(dev_pid)"
  [ -n "$pid" ] || return 0
  osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $pid) to true" >/dev/null 2>&1 || true
}

cmd_status() {
  if vite_up && daemon_alive; then
    echo "Luno dev server running (pid $(cat "$PIDFILE"), vite up)."
    app_running && echo "Dev app running." || echo "Dev app not started yet (still compiling?)."
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
  local pid=""
  [ -f "$PIDFILE" ] && pid="$(cat "$PIDFILE")"
  rm -f "$PIDFILE"
  # Best-effort teardown: npm/cargo/vite may have reparented, so stop the dev
  # app by path suffix and the launcher tree rather than assuming a process
  # group. Leave a moment for the Vite/instance ports to free before relaunch.
  pkill -f "$DEV_MATCH" 2>/dev/null || true
  pkill -f "$REPO/node_modules/.bin/tauri" 2>/dev/null || true
  pkill -f "$REPO/node_modules/.bin/vite" 2>/dev/null || true
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    pkill -P "$pid" 2>/dev/null || true
    kill "$pid" 2>/dev/null || true
  fi
  sleep 1
  echo "Stopped Luno dev server."
}

cmd_logs() {
  [ -f "$LOGFILE" ] || { echo "No log yet at $LOGFILE."; return 0; }
  if [ "${1:-}" = "-f" ]; then exec tail -f "$LOGFILE"; else tail -n 120 "$LOGFILE"; fi
}

cmd_launch() {
  # Single-flight: a second click while one launch is working is a no-op.
  if ! mkdir "$LOCKDIR" 2>/dev/null; then return 0; fi
  trap 'rmdir "$LOCKDIR" 2>/dev/null || true' EXIT

  if daemon_alive; then
    if vite_up && app_running; then
      focus_app
      return 0
    fi
    # An existing launch may still be compiling. Wait for both readiness
    # signals below instead of reporting success while the app is absent.
  else
    luno_require_node "$REPO" || {
      alert "Node.js $(cat "$REPO/.node-version") is required. See the launcher log."
      return 1
    }
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
    rm -f "$PIDFILE"
    echo "Starting Luno dev server (log: $LOGFILE)…"
    quit_release_app
    cd "$REPO"
    # `setsid` is Linux-only and absent on macOS; `nohup` alone detaches the
    # server and survives the launcher exiting.
    nohup npm run tauri dev >>"$LOGFILE" 2>&1 < /dev/null &
    daemon_pid=$!
    disown "$daemon_pid" 2>/dev/null || true
    echo "$daemon_pid" > "$PIDFILE"
    trap 'echo "Continuing in background; check with scripts/luno-tray.sh status."; rmdir "$LOCKDIR" 2>/dev/null || true; exit 0' INT
  fi

  # Vite first (fail fast so a config error surfaces), then the debug app.
  waited=0
  while [ "$waited" -lt "$READY_TIMEOUT_SECS" ]; do
    vite_up && break
    daemon_alive || break
    sleep 2
    waited=$((waited + 2))
  done
  if ! vite_up; then
    alert "The dev server did not start. See the log."
    echo "The dev server did not start within ${READY_TIMEOUT_SECS}s; see $LOGFILE." >&2
    return 1
  fi

  # The debug app follows once Rust finishes compiling/restarting.
  waited=0
  while [ "$waited" -lt "$READY_TIMEOUT_SECS" ]; do
    app_running && break
    daemon_alive || break  # crashed after vite came up; surfaced via the log
    sleep 2
    waited=$((waited + 2))
  done
  if ! app_running; then
    alert "The debug app did not launch. See the log."
    echo "The debug app did not launch within ${READY_TIMEOUT_SECS}s; see $LOGFILE." >&2
    return 1
  fi
  focus_app
  echo "Luno dev build is up."
}

cmd_restart() {
  cmd_stop
  sleep 1
  cmd_launch
}

case "${1:-launch}" in
  launch|start) cmd_launch ;;
  restart|rebuild) cmd_restart ;;
  status) cmd_status ;;
  stop) cmd_stop ;;
  logs) cmd_logs "${2:-}" ;;
  *) echo "Usage: $0 launch|restart|status|stop|logs [-f]" >&2; exit 1 ;;
esac

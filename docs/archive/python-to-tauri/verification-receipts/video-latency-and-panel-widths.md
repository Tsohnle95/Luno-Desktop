# Verification Receipt — video-load latency regression + panel widths (2026-08-25 night)

## Symptom

Music videos took over 2 minutes to load.

## Root cause

The same-evening registry-starvation fix over-corrected. Its "defer on busy
store" rule left a deferred CURRENT track with no guaranteed retrier: the
retry paths are the next kick (next skip) and the song-page 2 s poll — and
while the PREVIOUS track's download still held the store, both kept
deferring. Net: previous-remaining-download + own search/download ≈ minutes.

## Fix

Stage workers now take a **cancellable bounded wait**: poll `try_lock` every
100 ms until the store frees, the request is superseded (cancel flag),
shutdown, or `VIDEO_STAGE_WAIT_CAP_SECS` (300). Inflight dedupe collapses any
number of polls/skips into ONE waiter per track, so the original pile-up bug
stays dead while progress is guaranteed. Pressure gate unchanged.

Test updated to pin the final contract: 25 requests behind a held store →
exactly one waiter (≤2 active tasks), then **completes to Ready on its own**
once the store frees (index-cached fixture, zero network).

## Also

- Sidebar/queue panels now OPEN at their minimum widths (240 / 240; user-set
  "they open too large"). Drag clamps already enforced 240–440 / 240–480.
- Brain docs synced (loader contract + widths).

## Battery

| Check | Result |
| --- | --- |
| `cargo test` | **148 passed; 0 failed** |
| `npm run typecheck` + `npx vitest run` | clean, **107 passed** |
| `.venv/bin/python -m pytest -q` | **109 passed** |
| `npm run build` + `npx tauri build --bundles app` | ✓ |
| `dist/Luno.app` refreshed | ✓ |

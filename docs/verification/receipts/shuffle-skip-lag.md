# Verification Receipt — shuffle-skip lag / registry starvation (2026-08-25 evening)

## Symptoms

- With video mode ON, playing a few songs and skipping made the entire app
  lag.
- The running app was observed at ~3 GB memory.

## Root cause

Every `video_stage_track` request spawned a worker that **blocked** on the
staging-store mutex. With video mode on:

- the song-page poll fires every 2 s while a stage is Loading, and
- every song skip re-kicks the pipeline,

so each poll/skip piled one more blocked thread + registry slot while any
download held the store. The 32-slot task registry saturated →
`playback-load` / heartbeat submissions failed Busy → transport dead, UI
unresponsive — the "lag". The old binary's double-kick-per-switch doubled the
pile-up rate; multi-hundred-MB whole-blob video loads (pre-range-streaming)
plus duplicate superseded downloads explain the memory observation on that
build.

## Fixes

- Stage workers are NON-BLOCKING by contract: exactly one `try_lock`; on
  contention they release their inflight marker and defer to the next
  kick/poll (Loading placeholder persists).
- New pressure gate `VIDEO_STAGE_PRESSURE_LIMIT` (24 of 32 slots): staging
  defers while the runtime is saturated so playback-load/heartbeat/scan never
  starve.
- Tests are structurally silent: harness volume initializes to 0 under
  `cfg!(test)` after a shuffle regression test played real audio through
  speakers during verification.

## Tests (red-green verified)

| Test | Neutered | With fix |
| --- | --- | --- |
| `stage_requests_never_pile_up_behind_a_busy_store` (25 requests behind a held store) | FAIL — tasks piled to ~25 | PASS — ≤2, no inflight leak |
| `stage_defers_when_task_registry_is_under_pressure` (registry saturated) | FAIL — spawned anyway | PASS — defers |

## Full battery

| Check | Result |
| --- | --- |
| `cargo test` | **148 passed; 0 failed** |
| `npm run typecheck` + `npx vitest run` | clean, **107 passed** |
| `.venv/bin/python -m pytest -q` | **109 passed** |
| `npx tauri build --bundles app` | ✓ |
| `dist/Luno.app` refreshed from the new bundle | ✓ |

The named `desktop-playback` harness command remains unavailable in this
environment; its configured equivalent (gate3 playback tests) ran inside the
148-pass `cargo test`.

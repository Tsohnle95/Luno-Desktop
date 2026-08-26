# Verification Receipt — paused-song video readiness (2026-08-25)

## Symptom

"With video mode on, if I pause a song the video never loads; resuming much
later finally loads it."

## Diagnosis

Staging itself is pause-independent — proven by a new deterministic test that
drives the REAL pipeline to `Ready` while `playback_status == Paused` using an
index-cached fixture (`video_stages_to_ready_while_playback_is_paused`; zero
network, nonexistent yt-dlp path). The stall came from two other mechanisms:

1. **Loading corpses (primary, old binary).** The pre-2026-08-25 build kicked
   the video pipeline TWICE per song switch; the second kick superseded the
   first (cancelling its worker mid-search) and then SKIPPED the slot as
   "already Loading". Result: a Loading slot with no live worker that no later
   pass would ever touch. Today's single-kick removed the trigger, but any
   corpse could still wedge a track's video until an unrelated trigger ran.
2. **Resume never caught up.** `resume_track` did not re-kick the pipeline,
   so returning from a long pause offered no recovery path either.

## Changes

- `state.rs` — new `current_video_live(track)`: resolved slots are live; a
  Loading slot is live ONLY with an inflight worker marker; corpses fall
  through and restage. `kick_video_pipeline`'s current-slot skip now uses it.
- `state.rs` `resume_track` — catch-up kick: when video mode is ON and the
  current track's video is unresolved, one idempotent pipeline pass runs
  before audio work (off the UI thread), so resuming finds the video ready.

## Tests (red-green verified)

| Test | Neutered | With fix |
| --- | --- | --- |
| `video_stages_to_ready_while_playback_is_paused` | — (new proof) | PASS |
| `kick_restages_a_loading_slot_with_no_live_worker` | FAIL (timeout) | PASS |
| `resume_kicks_the_video_pipeline_when_the_current_video_is_unresolved` | FAIL (timeout) | PASS |

## Full battery

| Check | Result |
| --- | --- |
| `cargo test` | **138 passed; 0 failed** |
| `npm run typecheck` + `npx vitest run` | clean, **107 passed** |
| `.venv/bin/python -m pytest -q` | **109 passed** |
| `npm run build` + `npx tauri build --bundles app` | ✓ |
| `dist/Luno.app` refreshed from the new bundle | ✓ |

The named `desktop-playback` harness command remains unavailable in this
environment; its configured equivalent (gate3 playback tests) ran inside the
138-pass `cargo test`.

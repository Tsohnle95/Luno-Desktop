# Task: Responsive playback transport

Status: ACTIVE

## Goal
Remove decoder shutdown and output-device recreation from ordinary transport transitions.

## Acceptance criteria
- Regression tests fail against synchronous decoder retirement and per-track stream teardown.
- Pause/resume and replacement do not wait for a retired decoder; obsolete samples never enter the replacement.
- One healthy output stream survives track replacement and pause/resume; failed output still rebuilds.
- Existing owner-generation, queue, volume and completion behavior remains intact.

## Relevant context
- docs/modules/playback.md
- src-tauri/src/playback/backend_adapter.rs
- src-tauri/src/state.rs: start_selected_track, pause_track, resume_track

## Invariants / constraints
- Keep authoritative transport generation and sample clock ownership.
- Retired decoders only access their old isolated sample state.
- Shutdown still joins owned workers. No detached untracked decoder workers.

## Affected surfaces
- macOS audio backend and playback documentation.

## Phases
1. Reproduce blocked decoder retirement and unnecessary stream recreation with deterministic tests.
2. Retain healthy device stream; swap isolated track state; retire producers without blocking transport; validate and commit.
3. Canonical gates and direct native playback check; cumulative review.

## Validation plan
Focused Rust backend tests, existing transport/owner tests, Node 22 npm run verify and npm run build, full Rust tests, native stream timing smoke.

## Decisions
- Adversarial review: keeping only a pause flag cannot fix a blocked backend mutex. Output callback must follow the current track slot, and old producers must retain only old state. Preserve failed-device recovery and join retired workers at shutdown; test both sample isolation and paused clock behavior.

## Open risks / blockers
- Cold/cloud file opening and OS/device latency cannot be guaranteed to take zero time. Measure local ready-file transport separately.

## Validation receipts
- Original implementation: both replacement regression tests failed (joined blocked decoder; destroyed healthy output stream).
- Revised backend: five focused tests passed; explicit muted native test passed 20 transitions with maximum load-to-sample 11.8 ms and pause/resume-to-sample 15.8 ms. Simulated device error rebuilt output; shutdown released stream/workers.
- Node 22: npm run verify passed (279 frontend tests); npm run build passed.
- Full Rust gate passed: 276 passed, seven opt-in tests ignored (native timing separately executed).
- Review correction: silence retired output in load, not in generic producer retirement, preserving EOF-seek resume semantics. Admission cap applies to seek restarts too.

# Formal Gate 3 Receipt: Playback And Queue

Date: 2026-08-15
Scope: playback context and queue, clock/seek/completion, shuffle/repeat,
failure recovery, preview isolation/handoff, backend errors, and shutdown.

## Boundary And Safety

This receipt uses only deterministic fixtures and isolated temporary paths. No
real user data, library, credentials, Keychain, network response, audio device,
 or production deployment was used. A native window capture exists for the
 disposable run; no accessibility capture or audible-output observation exists.

The target exposes authoritative queue and playback snapshots through the
versioned event bridge. macOS playback uses the cancellation-guarded heartbeat
and the real backend; non-macOS audio commands remain structured `unavailable`
errors. Decode/fixture checks are not evidence that samples reached a device or
were audible.

## Compiler Reproduction

The two reported compiler failures do not reproduce in this checkout:

- `SettingsInput` already derives `PartialEq`, not `Eq`, because it contains
  `Option<f64>` (`src-tauri/src/contracts/types.rs`).
- `start_download` clones/captures `JobId` at the request, worker, event,
  envelope, coordinator, and returned-JSON ownership boundaries
  (`src-tauri/src/state.rs`).

No speculative compiler fix was added.

## Fixture And Independent Native Evidence

The independently observed disposable run used these canonical paths:

```text
HOME=/private/tmp/luno-gate3-live/home
TMPDIR=/private/tmp/luno-gate3-live/tmp
library_root=/private/tmp/luno-gate3-live/library
stage=/private/tmp/luno-gate3-live/stage/gate3-stage.wav
destination=/private/tmp/luno-gate3-live/library/Unsorted/gate3-promoted.wav
```

The generated WAV was 44.1 kHz stereo PCM. The stage file was consumed and the
destination existed at the end of the run.

The exact disposable native command was run from the repository root:

```text
HOME=/private/tmp/luno-gate3-live/home TMPDIR=/private/tmp/luno-gate3-live/tmp CARGO_HOME=/Users/ty/.cargo CARGO_NET_OFFLINE=true VITE_LUNO_GATE3_SMOKE=1 npm run tauri -- dev --no-watch
```

The run reached and ran `target/debug/luno-desktop`. A Quartz CGWindow capture
of the onscreen native Luno window was 1180x800. The rendered Gate 3 probe text
visibly reported:

```text
Gate 3 smoke observed metadata:gate3promoted:0: subscribe, registerRoot, promoteAudioOutput, scanLibrary, getLibrary, playWithContext, pause, seek, resume, playNext, addToQueue, next, previous, shutdown
```

The native UI visibly showed one library song and three queue entries. After
tracked termination, no `luno-desktop`, Vite, or Tauri process remained.

The development probe reports `observed` only when its real bridge/event
conditions are met: a `playback_snapshot`, a `track_started` event, no errors,
and a complete shutdown event. This is native bridge/playback/queue event
evidence, not a human audible-output measurement or crossfade-envelope parity
measurement.

Direct post-run filesystem checks observed:

```text
stage=/private/tmp/luno-gate3-live/stage/gate3-stage.wav consumed
destination=/private/tmp/luno-gate3-live/library/Unsorted/gate3-promoted.wav exists
```

Artifact: `docs/verification/artifacts/gate3-native-playback-smoke.png`.

## Development Probe

`src/app/gate3Smoke.tsx` is mounted only when `import.meta.env.DEV` is true and
`VITE_LUNO_GATE3_SMOKE=1` plus all three explicit fixture paths are present.
It uses the real typed `Bridge` methods and the real `backend-event` subscription;
the test transport is confined to `src/test/gate3_smoke.test.ts`.

The probe sequence is:

```text
subscribe -> registerRoot -> promoteAudioOutput -> scanLibrary/getLibrary
-> playWithContext(selected track, selected context)
-> wait for playback_snapshot and track_started, or retain a backend error
-> pause -> seek -> resume (the real pause toggle) -> playNext -> addToQueue
-> next -> previous -> shutdown
```

Playback errors, command failures, and event errors are collected as structured
path-sanitized errors. A playback snapshot must have `playing` status and a
`track_started` event; any playback/backend error, missing event, or recovery
command error is rendered as `fail`, never native `PASS`. Recovery commands and
shutdown are attempted even after a backend failure. The timer and backend
listener are cleaned up in `finally`; focused tests cover the successful
lifecycle and an unavailable-device error path with recovery failures. These
tests do not claim native runtime evidence.

## Verification

All commands ran from the repository root.

| Command | Result | Classification |
|---|---|---|
| `cargo test --manifest-path src-tauri/Cargo.toml gate3_tests -- --nocapture` | 10 passed, 0 failed, 74 filtered | PASS deterministic target-boundary tests |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 83 passed, 0 failed, 1 ignored; 0 doc tests | PASS; ignored test requires interactive macOS Keychain |
| `npm test` | 4 files, 25 tests passed | PASS |
| `npm run typecheck` | `tsc --noEmit` completed | PASS |
| `npm run build` | 53 Vite modules transformed; build completed | PASS |
| independent native disposable run | generated 44.1 kHz stereo PCM WAV; stage consumed; destination existed; ran `target/debug/luno-desktop`; Quartz CGWindow capture was 1180x800; native probe visibly reported the observed metadata and the UI showed one library song and three queue entries; tracked termination left no luno-desktop/Vite/Tauri process | native bridge/playback/queue event evidence; no audible-output or crossfade claim |
| `git diff --check` | clean | PASS |

Cargo emitted only the three existing warnings listed above. No formatter or
desktop-playback harness result is promoted here; the configured
`desktop-playback` command is unavailable in this environment.

## Evidence Classification

- Rust policy, queue, clock, recovery, preview-state, and synthetic-fixture
  tests: PASS as deterministic unit/target-boundary evidence.
- Frontend bridge/probe lifecycle and sanitized-error tests: PASS as frontend
  transport evidence.
- PCM format and Symphonia/FakeBackend fixture behavior: PASS as decode/fixture
  evidence only; no device or audible-output claim.
- Native window, real backend event stream, playback snapshot,
  `track_started`, seek/resume, queue transition, and shutdown event: PASS as
  independently observed native bridge/playback/queue evidence, subject to an
  acceptance scope that permits this classification without audible or
  crossfade claims.
- Promotion/filesystem and launch evidence: the generated WAV was 44.1 kHz
  stereo PCM, the stage was consumed, the destination existed, and
  `target/debug/luno-desktop` ran; no separate human audible-output
  measurement was made.
- Preview handoff remains covered by the evidence above. Crossfade has an
  explicit target v1 retirement/no-op-absence decision: legacy preference and
  behavior are not carried forward, and target transitions are immediate.
  Crossfade parity is not claimed, and no human-audible envelope measurement
  was made.

## Bounded Changes

The bounded Gate 3 additions are `src/app/gate3Smoke.tsx` and
`src/test/gate3_smoke.test.ts`, with their existing `src/main.tsx`, Vitest, and
`project_brain.md` entries. The probe was corrected to wait for subscribed
playback evidence, use the real pause toggle for resume, sanitize event errors,
and refuse a success status for failed or unevidenced playback. Unrelated dirty
worktree changes and existing evidence files were preserved.

## Formal Gate 3 Verdict

**INCONCLUSIVE / BLOCKED overall: not a Formal Gate 3 release PASS.**

The deterministic Rust and frontend checks pass, and the independent native
run directly evidenced the live window plus bridge/playback/queue conditions.
Native bridge/playback/queue event parity can therefore be classified PASS only
if the existing acceptance scope permits that classification without audible
output or crossfade claims. This receipt keeps the overall verdict
INCONCLUSIVE/BLOCKED because no human audible-output measurement was made; it
does not claim crossfade parity or a human-audible crossfade envelope.

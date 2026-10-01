# Bridge, Envelopes, and Transfer Contract

Document role: canonical owner for the typed frontend↔backend contract,
envelope shapes, event delivery, and the versioned library-export format.
Nothing here redefines module internals; it defines the wire everything
crosses.

## What this owns

- Command/event envelope shapes, versioning, and validation rules.
- The `backend-event` delivery path and its threading guarantees.
- The TypeScript↔Rust contract duality: what must stay in sync.
- The `luno.library.export` v1 transfer format and its guarantees.
- Cross-boundary change paths (ordered traces for multi-layer work).

What it does not own: library/playback/download semantics (their module
docs), release packaging (`docs/release/desktop.md`).

## Entry points

- `src/shared/contracts.ts` — TS envelope types, guards
  (`isCommandEnvelope`, `isEventEnvelope`), `CONTRACT_VERSION`.
- `src/app/bridge.ts` — `createBridge` transport wrapper, `BACKEND_EVENT`
  (`"backend-event"`), `BACKEND_COMMAND` (`"backend_command"`), response
  validators per command family.
- `src-tauri/src/contracts/events.rs` — `Command`, `CommandEnvelope`,
  `Event`, `EventEnvelope`, `RepeatMode`, `RootStatus`, shutdown types.
- `src-tauri/src/contracts/types.rs` — shared shapes (`Track`, `TrackId`,
  `LibrarySnapshot`, `JobState`, `DownloadPhase`, `DiscoverSnapshot`,
  `PlaybackStatus`, …), `CONTRACT_VERSION`.
- `src-tauri/src/contracts/mod.rs` — module root.
- `src-tauri/src/state.rs` — `AppState::dispatch`, the single command
  entry point all Tauri commands funnel through.
- `src-tauri/src/main.rs` — Tauri command wiring (`backend_command`,
  `downloader_search`, `settings_load`, `transfer_preview`,
  `migration_status`, `track_artwork`, `track_duration`, discovery-key commands),
  `spawn_blocking` dispatch, pending-event drain, `luno-video://` scheme.
- `src-tauri/src/transfer/` — `codec.rs`, `planner.rs`,
  `provider_index.rs`: export/import of the v1 manifest.
- `contracts/library-export/v1/` — `schema.json`, `README.md`, `valid.json`,
  `invalid-unknown-field.json`.

## Invariants

- `CONTRACT_VERSION` is `1` on both sides (`contracts.ts`, `types.rs`).
  A version bump is a migration-grade change: both validators, both test
  suites, and this doc update together.
- Every command carries `{ version, request_id, command }`; `request_id`
  must be non-blank. Every event carries `{ version, event_id,
  generation, job_id, session_id, event }`; `event_id` is `event-<n>`,
  `n >= 1`.
- Unknown fields are rejected on both sides and in transfer manifests.
  `exact()` shape checks in TS mirror `deny_unknown_fields`-style
  strictness in Rust — an approved additive shape such as `phase` or
  `prepared` must be added to both validators together; arbitrary fields
  remain invalid.
- `delete_tracks` and `delete_playlist` accept an optional
  `final_copy_review_id`, preserving the original command shape for callers
  that omit it. Their validated `SafeDeleteResult` either reports deletion or
  returns the affected file labels and a single-use review ID without deleting.
  The confirmed command recomputes the last-copy result before it can remove
  files.
- The optional `SettingsInput.youtube_cookies_browser` field is allowlisted
  on both sides. `"none"` explicitly clears the preference; omitted or null
  values preserve existing config under settings merge semantics.
- Frontend never recreates backend policy: it renders snapshots and sends
  commands. Queue order, scan results, job state, and playback position
  are backend-owned; frontend projections (`src/state/`) are read models.
- Playlist sync review summary and resolution commands remain backend-owned;
  issue responses include candidate local tracks, while the frontend only
  presents choices and sends the selected local track ID or dismissal.
- `dispatch` must leave the UI thread: `main.rs` runs it under
  `spawn_blocking`, and panics are caught and sanitized (never leak
  `PoisonError` internals to the UI).
- Events publish over `backend-event`; a 25 ms delivery thread drains
  pending events. Shutdown publishes a terminal shutdown envelope and
  stops delivery.
- Download jobs may publish an explicit `finalizing` phase after helper
  transfer reaches its handoff and before validation/promotion completes.
  Discover snapshots may include stable keys in `prepared` and `unavailable`;
  these fields are additive for clients that still read older snapshot shapes.
  `discover_select` selects or retries one recommendation inside the active
  Discover projection without transferring ownership to focused preview.
- Playback snapshots include `audible_track_id` when audio is playing or
  paused. It can differ from `queue[current_index]` while a focused preview is
  sounding over a physical queue, including after Discover is disabled.
  Older snapshots without the field remain accepted by the frontend.
- Transfer manifests are metadata-only: no local paths, secrets, audio
  bytes, queue state, favorites, or history. Imports are additive —
  consumers never delete existing library content. A track without
  provider identity requires `ambiguity_confirmation_required: true`.
  The fixture set is intentionally duplicated in Luno Mobile; neither
  repo clones the other at build or runtime.

## Core flows

```text
user action → view/component → bridge.send(command) → backend_command
→ spawn_blocking → AppState::dispatch → state mutation
→ pending events → backend-event → backend_projection → re-render
```

```text
transfer export → transfer/codec → v1 manifest JSON → consumer validates
against schema.json + semantic rules → additive apply → coalesced rescan
```

## Adding or changing a command/event

1. Add the variant in `contracts/events.rs` (+ `types.rs` if new shapes).
2. Mirror it in `src/shared/contracts.ts` with guard/validator coverage.
3. Expose it in `src/app/bridge.ts` with response validation.
4. Handle it in `AppState::dispatch`.
5. Cover producer + consumer: Rust test plus `src/test/bridge.test.ts`
   or the relevant view/smoke test.
6. Update this doc only if envelope rules, delivery guarantees, or the
   transfer format changed — not for routine additions.

## Tests

- `src/test/bridge.test.ts` — envelope guards, migration-status parsing.
- `src-tauri/src/gate2_tests.rs`, `gate3_tests.rs`, `gate4_tests.rs`,
  `gate5_tests.rs` — command Dispatch behavior per domain; gate4 covers
  transfer codec against `contracts/library-export/v1/` fixtures via
  `include_str!`.
- `src/test/gate{2,3,4,5}_smoke.test.ts` — frontend-side smoke coverage.

Note: root `tests/*.test.ts` assert-scripts and root `tests/**/*.rs`
are **not** wired into `vitest.config.ts` or Cargo; they are not
executed by the canonical gate. Do not cite them as proof.

## Related canonical docs

- `project_brain.md` — cross-system flows and ownership.
- Module docs — semantics behind each command family.
- `contracts/library-export/v1/README.md` — wire-shape guarantees.

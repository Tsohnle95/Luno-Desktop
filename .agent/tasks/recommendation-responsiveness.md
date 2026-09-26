# Task: Restore responsive recommendation playback and discovery

Status: ACTIVE

## Goal

Recommendation preparation must not hold shared controls behind a media helper; unavailable YouTube results should advance to another playable source; saved songs without embedded art should show Luno's generated artwork.

## Acceptance criteria

- [x] A running preview helper does not block a new recommendation tap, Discovery toggle, or save lookup.
- [x] An age gated first YouTube result is skipped through a bounded, cancellable fallback; a genuinely unavailable recommendation settles its row without a global error banner.
- [ ] Saved playlist rows without artwork use the same generated design as recommendation, queue, and player surfaces.
- [x] Volume changes remain effective during a preview handoff.

## Relevant context

- `docs/modules/downloads-discovery.md`, `docs/modules/playback.md`, `docs/modules/frontend.md`, `docs/modules/library.md`.
- `src-tauri/src/state.rs` `prepare_one`, preview/Discover drivers, `play_preview_audio`; `src-tauri/src/preview/assets.rs`; `src-tauri/src/runtime/preview_worker.rs`; `src/components/TrackRow.tsx`.

## Invariants / constraints

- Prepared files publish atomically only for the current store generation. Cancellation prevents a stale helper from publishing after a seed change.
- Preview assets remain outside the library until explicit save. Preview helper concurrency stays serialized per presentation mode.
- Discovery errors stay per row; no repeating global banners. Volume is authoritative at `AppState`.
- Do not change the transfer contract, app identity, or managed binary pins.

## Affected surfaces

- Preview store generation and worker access: remove long store lock while retaining publication fencing.
- Source acquisition: bounded retry of alternative YouTube results after an age gate.
- Playback volume: handoff uses the latest intended level.
- Library rows: generated artwork when embedded/provider artwork is missing.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Preview responsiveness, source recovery, and volume | complete | Focused regressions; 225 Rust tests pass, 5 ignored | pending |
| 2 | Playlist artwork and final integration | active | 188 frontend tests, docs check, and build pass; diff review pending | pending |

## Validation plan

- Targeted checks: preview store cancellation/publication and helper fallback; recommendation state; library artwork rendering.
- Integration/platform/manual checks: use the narrowest available live app path for tap, mute, Discovery toggle, and saved artwork; report any path unavailable in this environment.
- Canonical/final gate: `npm run verify`, `cargo test --manifest-path src-tauri/Cargo.toml`, `npm run build` under the pinned toolchain.

## Decisions

- The worker will use a clone of the store with shared generation state; helper work runs without the outer store mutex. Generation changes and publication share a short critical section, so a stale completion cannot publish after invalidation.
- Fallback searches only after an age gated result, avoiding extra network work on the normal path. Limit candidates and honor cancellation between attempts.
- Review: the main risk is a publication racing cleanup. Keep generation check and rename under the same store generation lock; cleanup runs under that lock as well. A second risk is helper duplication after mode handoff; preserve current driver ownership checks.
- The two Rust phases were combined because both change the preview worker and its state transitions; they form one sound recovery checkpoint.

## Open risks / blockers

- Live YouTube availability and macOS audio output can vary; automated fakes must cover failure paths, with live behavior called out separately.

## Completion

- [ ] Acceptance criteria satisfied
- [ ] Cumulative validation passed
- [ ] Canonical durable truth updated
- [ ] Future work transferred to backlog/issues
- [ ] Final Git state reviewed

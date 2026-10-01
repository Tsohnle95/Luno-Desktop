# Task: Remove library UI stalls and favorite row flicker

Status: ACTIVE

## Goal

Keep library interactions responsive by separating favorite updates from scan generations, correcting footer track identity, isolating sidebar filter state, and simplifying single-song scrolling.

## Acceptance criteria

- [ ] Favoriting a local track updates all favorite indicators through a targeted event without changing the library scan generation or resetting visible duration data.
- [ ] Pathless library snapshot tracks are recognized by stable ID for footer artwork and local-track actions.
- [ ] Footer shuffle color reflects the projected playback shuffle value and stays synchronized with the playlist control.
- [ ] Opening, closing, and typing in the Your Library filter do not rerender unrelated route content; outside-pointer dismissal remains intact.
- [ ] The single-song view uses one vertical scroll container and does not measure/write multiple scrollers on every wheel event; scrolling over video remains usable.
- [ ] The scroll diagnosis reports the observed runtime evidence without attributing lag to inactive download helpers.

## Relevant context

- `docs/modules/frontend.md`
- `docs/modules/contracts.md`
- `project_brain.md`
- `src/app/Shell.tsx`, `src/views/LibraryView.tsx`, `src/views/SongPageView.tsx`
- `src/state/backend_projection.ts`, `src/styles/layout.css`
- `src-tauri/src/state.rs` (`toggle_favorite`, library snapshot publication)

## Invariants / constraints

- Library scan generations continue to reject stale scan results; favorite-only changes do not invalidate duration probes.
- Backend remains authoritative for favorite and shuffle state.
- Library artwork remains lazy and bounded; stable library IDs identify local tracks even though snapshot paths are omitted.
- Preserve the virtual library row height and scroll-region clamping contract.
- Keep video-wheel handling functional without doing synchronous dual-scroller layout reads for ordinary scrolling.

## Affected surfaces

- Rust favorite mutation, typed event, and latest-snapshot recovery cache: publish a targeted ordered favorite event at the current library scan generation.
- Frontend library projection and track-map dependencies: update one track while retaining duration results for the same scan generation.
- Shell/footer and transport styling: resolve local tracks by stable ID and distinguish shuffle on/off.
- Your Library sidebar: isolate filter state and animate the filter locally.
- Song page and main-content layout: use a single native scroll region with a narrow video-wheel fallback.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Favorite-only event contract, latest-snapshot recovery, track-map dependencies, and stable-ID local track resolution | complete | `npm run typecheck`, `cargo check --manifest-path src-tauri/Cargo.toml`, `git diff --check` | `abca98a6` |
| 2 | Footer shuffle state styling | complete | `git diff --check`; state source confirmed to be shared with playlist control | `dc001388` |
| 3 | Sidebar filter render isolation and interaction | complete | `npm run typecheck`, `git diff --check` | pending |
| 4 | Single-scroller song-page path and video wheel handling | pending | Type/build validation and runtime process inspection; no active media helpers at initial inspection | — |

## Validation plan

- Inspect the exact producer, projection, and consumer paths for favorite and shuffle state.
- Run the supported frontend type/build checks and Rust compile check if available.
- Do not add or run tests unless requested; report this validation limit explicitly.
- Inspect the final diff and current running dev app behavior without launching a second app instance.

## Decisions

- Use a targeted `track_favorite_changed` event at the current library generation. The reducer already orders same-generation events by event ID, while the generation remains a scan epoch for duration invalidation; the event updates the backend's cached latest snapshot for reconnect recovery.
- Identify local tracks by membership in the current library snapshot, not the optional `Track.path` field, which Rust intentionally omits.
- Keep only a video-targeted, frame-batched wheel fallback if native video handling needs it; ordinary song-page scrolling uses native scrolling.

## Open risks / blockers

- A scan may be in flight when a favorite changes; reapply the latest path-based favorite set at scan commit, and publish the scan's current in-memory snapshot so an older favorite value cannot win afterward.
- Verify that the video-specific wheel fallback does not reintroduce per-event layout work.

## Completion

- [ ] Acceptance criteria satisfied
- [ ] Cumulative validation passed
- [ ] Canonical durable truth updated
- [ ] Future work transferred to backlog/issues
- [ ] Final Git state reviewed

# Task: Desktop usability batch (autocorrect, media keys, downloader naming, metadata edit)

Status: ACTIVE

## Goal

Fix four independently completable user-reported defects, one unit at a
time, each committed as a recovery checkpoint.

## Units

| Unit | Class | Outcome |
|---|---|---|
| A | PATCH | OS text autocorrect/spellcheck disabled in app text fields. |
| B | FEATURE | Hardware media keys control Luno again (now-playing + playback state). |
| C | FEATURE | URL/download naming resolves `Artist - Title`; UI reports the found song. |
| D | FEATURE | Right-click a track to edit artist/title (renames the library file). |

## Acceptance criteria

- [ ] A: text inputs/textarea render `autocorrect="off"` and `spellcheck="false"`.
- [ ] B: Luno publishes now-playing playback state + track metadata so
      `MPRemoteCommandCenter` keeps routing media keys to it; disabled setting
      releases the claim. (OS routing needs manual verification.)
- [ ] C: direct-URL and URL-queue downloads resolve title/artist and name the
      output `Artist - Title.<ext>`; resolved name is surfaced before/while
      downloading; falls back safely when metadata is unavailable.
- [ ] D: track context menu offers "Edit song info…"; submits artist/title;
      backend renames the file inside the library root, preserving the folder,
      refusing collisions/unsafe paths, updating favorites, then rescanning.
- [ ] Existing behavior unchanged: no contract/version changes, no scan or
      playback-policy changes, transfer v1 untouched, 56px rows preserved.

## Relevant context

- `docs/modules/downloads-discovery.md`, `docs/modules/playback.md`,
  `docs/modules/library.md`, `docs/modules/frontend.md`,
  `docs/modules/contracts.md`.
- `src-tauri/src/media_keys.rs`, `src-tauri/src/state.rs` (`publish_playback`,
  `handle_media_key`, `validate_direct_url`), `src-tauri/src/download/search.rs`,
  `src-tauri/resources/bin/luno-download-helper`.
- `src/app/Shell.tsx` (download queue, context menu), `src/app/bridge.ts`,
  `src/shared/contracts.ts`, `src/components/PlaylistDialogs.tsx`.
- Tests: `src-tauri/src/gate3_tests.rs`, `gate4_tests.rs`, `state.rs` unit
  tests, `src/test/views/downloader.test.tsx`, `shell.test.tsx`,
  `src/test/dialogs.test.tsx`.

## Invariants / constraints

- Filename metadata is the only production track-metadata source
  (`provider_metadata` is test-only); downloads must be named correctly.
- Renaming changes the derived track id; favorites keyed by path must move.
- No new event shapes unless required; commands stay in the versioned envelope.
- Blocking yt-dlp work runs on the command worker (consistent with search).

## Affected surfaces

- Frontend: text inputs, download queue/status, context menu + new dialog,
  bridge command.
- Rust: `contracts/events.rs` (`validate_direct_url` response enrichment,
  `edit_track_metadata`), `state.rs`, `download/search.rs`,
  `download/workflows.rs`, `media_keys.rs`.
- Docs: contracts, downloads-discovery, playback, library, frontend.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Unit A autocorrect | pending | tsc + view tests | |
| 2 | Unit C downloader naming | pending | cargo tests + view tests | |
| 3 | Unit D metadata edit | pending | cargo tests + dialog tests | |
| 4 | Unit B media keys | pending | cargo tests + manual note | |
| 5 | Final gate + docs | pending | npm run verify, cargo test, npm run build | |

## Validation plan

- Targeted: `npx vitest run <files>` / `cargo test` filters per unit.
- Manual/unverifiable-by-gate: macOS media-key routing, live UI appearance,
  real yt-dlp download naming. Report explicitly.
- Canonical: `npm run verify`, `cargo test --manifest-path src-tauri/Cargo.toml`,
  `npm run build`.

## Decisions

- Unit B root cause: launch-time `MPNowPlayingInfoCenter` claim without
  `playbackState`/live metadata is insufficient on current macOS; publish
  truthful playback state + track metadata on playback transitions.
- Unit C: enrich the existing `validate_direct_url` response with probed
  metadata instead of adding a new probe command.
- Unit D: filename-derived identity means rename == file rename; keep the
  track in its folder so playlist membership survives.

## Open risks / blockers

- Unit B cannot be verified by automated gates; needs a real media-key press.
- Unit C postprocessing (ffmpeg extract + loudnorm) is inherently slow and
  silent between 95% and 100%; naming/feedback is fixed, latency is reported.

## Completion

- [ ] Acceptance criteria satisfied
- [ ] Cumulative validation passed
- [ ] Canonical durable truth updated
- [ ] Future work transferred to backlog/issues
- [ ] Final Git state reviewed

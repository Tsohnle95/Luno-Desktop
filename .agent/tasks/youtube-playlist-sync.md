# Task: Sync linked YouTube playlists from the playlist menu

Status: REVIEWED

## Goal

Let each physical Luno playlist store a YouTube playlist URL and start a
background sync from its right-click menu that downloads only unseen entries.

## Acceptance criteria

- [ ] A playlist context menu can add, replace, or remove its YouTube playlist URL.
- [ ] Stored URLs survive rescan/restart and appear on the playlist snapshot.
- [ ] A linked playlist exposes an enabled `Sync playlist` action; an unlinked playlist directs the user to add a URL first.
- [ ] Sync resolves the remote playlist, skips media already present anywhere in the library or recorded in that playlist's success markers, and downloads only new entries into the selected playlist folder.
- [ ] Successful item IDs are persisted before sync completion; a later sync does not redownload them.
- [ ] Sync runs off the UI thread, publishes truthful `sync` job states/progress, supports cancellation, and triggers a library rescan after downloads.
- [ ] Normal single downloads and playlist sync cannot run concurrently through the shared managed helper.
- [ ] Invalid/non-YouTube-playlist URLs, unsafe paths, missing helpers, and provider failures surface as bounded user-facing errors.
- [ ] Existing single-download, playlist CRUD, migration readers, transfer v1, and app identity behavior remain unchanged.

## Relevant context

- `docs/modules/contracts.md`; `docs/modules/downloads-discovery.md`; `docs/modules/library.md`; `docs/modules/frontend.md`
- `src-tauri/src/download/{sync,markers,search,workflows}.rs`
- `src-tauri/src/library/mutations.rs`; `src-tauri/src/state.rs`
- `src-tauri/src/contracts/events.rs`; `src/shared/contracts.ts`; `src/app/bridge.ts`
- `src/app/Shell.tsx`; `src/components/PlaylistDialogs.tsx`
- `src-tauri/src/gate{2,4}_tests.rs`; `src/test/{bridge,views/shell}.test.tsx`

## Invariants / constraints

- Rust owns URL validation, remote enumeration, deduplication, marker persistence, path derivation, cancellation, and job truth; React only sends playlist IDs and renders results.
- Playlist IDs, never UI-provided paths, select the destination. Every resolved destination remains contained under the configured library root.
- `.luno_playlist_urls.json`, `.downloaded_vids`, and legacy marker readers remain compatible and are written atomically/append-only through existing owners.
- Production output stays M4A/AAC at 44.1 kHz through the reviewed helper/FFmpeg path.
- The sync admission guard is acquired around both normal-download and sync starts so their check/start transition cannot race.
- A sync task always releases admission through an RAII guard, including provider failure, cancellation, panic unwinding, or shutdown.

## Affected surfaces

- Commands: add `set_playlist_url` and `sync_playlist`; mirror Rust/TS validators and bridge methods.
- Persistence/snapshots: call existing `PlaylistService::set_url`, canonicalize YouTube playlist URLs, then rescan.
- Provider adapter: enumerate flat YouTube playlist entries with a bounded, cancellable managed yt-dlp process.
- Runtime: background sync task, shared media-download admission, job events, markers, final rescan.
- UI: URL dialog and two playlist context-menu actions with status notices.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Contract, YouTube URL normalization, URL persistence command/snapshot projection | complete | Rust parser/persistence tests; TS contract/bridge tests | phase checkpoint |
| 2 | Background sync runtime, admission/cancellation, marker and job-state tests | complete | two command-level Cargo tests: new-only marker behavior and process-killing cancellation/admission release | phase checkpoint |
| 3 | Playlist-menu URL dialog + sync UX | pending | shell/dialog tests, typecheck, focused render check | pending |
| 4 | Durable docs, cumulative review and canonical validation | pending | live narrow sync probe where safe; canonical gate | pending |

## Validation plan

- Targeted checks: playlist URL normalization; flat-entry parsing; marker skip/retry; safe M4A destinations; command serialization; sync success/failure/cancellation events; frontend strict-envelope and menu/dialog rendering.
- Integration/platform/manual checks: managed yt-dlp flat-playlist query and a bounded real playlist sync into a temporary library; verify second run reports no new downloads. If provider/network prevents this, report it explicitly.
- Canonical/final gate: `npm run verify`, `cargo test --manifest-path src-tauri/Cargo.toml`, `npm run build` under the supported toolchain.

## Decisions

- Use one backend-owned `sync_playlist` job rather than returning a plan for React to execute; this keeps deduplication, paths, markers, and recovery out of the UI.
- Reuse `PlaylistService`, `download::sync`, `Markers`, the managed helper, task registry, and existing `DownloadKind::Sync`; no parallel persistence or downloader subsystem.
- Use a process-group-aware/cancellable playlist enumerator or the existing supervised process boundary; no detached yt-dlp children.
- Change sync output names to `.m4a` to match the production format contract; existing MP3/WebM files remain recognized for deduplication.
- Empty/no-new-item sync is a successful terminal job with a clear notice, not an error.
- Context menu offers `Set YouTube playlist URL…` (or edit) and `Sync playlist`; sync without a URL is disabled with the URL action immediately adjacent.

## Open risks / blockers

- No current backend command executes `download::sync`; phase 2 must prove task cancellation kills or bounds the active helper/enumerator rather than merely cancelling publication.
- YouTube may change flat-playlist metadata; parsing accepts stable `id`, `title`, and URL variants and rejects entries without a usable ID/title.

## Completion

- [ ] Acceptance criteria satisfied
- [ ] Cumulative validation passed
- [ ] Canonical durable truth updated
- [ ] Future work transferred to backlog/issues
- [ ] Final Git state reviewed

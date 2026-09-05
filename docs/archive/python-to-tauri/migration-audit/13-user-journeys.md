# Cross-Feature Desktop User-Journey Audit

This dossier traces the implemented Python/Tk desktop application only. Android
is intentionally excluded from the evidence and verification below. The target
migration is not present in this repository, so acceptance requirements are
requirements for a future implementation, not claims about shipped code.

## Scan, Browse, Play, Queue, Favorite
- Status: implemented
- Evidence:
  - `music_player.py:3334-3466` (background scan, generation/coalescing, library projection)
  - `engine.py:76-106,238-277,401-414` (context queue, next/previous, queue mutation)
  - `music_player.py:1705-1809,4662-4684` (favorite state, UI projection, config persistence)
  - `views/library.py:213-248,539-570` (browse, row actions, playlist transfer entry points)
  - `test_engine_playback.py:257-283` (deterministic context/shuffle playback coverage)
- User behavior: A scan populates All Music, physical playlists, Unsorted, and the virtual Favorites playlist. Selecting a row starts playback in its supplied or containing-playlist context. Play Next/Add to Queue preserve manual Up Next ordering; favorites can be toggled from rows, queue/player/Song Page, and survive restart as normalized absolute paths.
- State and data flow: `scan_library` produces playlist-to-path lists off the Tk thread. `_finalize_library_update` replaces `_library`, prunes favorites to known paths, redraws sidebar/list, and broadcasts view updates. `play_with_context` replaces `engine.queue`; queue insertion and playback updates are controller callbacks. `_favorite_paths` is projected over scanned paths and is not a physical folder.
- Persistence and compatibility: Config stores `favorite_songs`; queue and history are session state. The shared export intentionally excludes favorites, queue, and history (`project_brain.md:239-253`).
- Async, lifecycle, and failure behavior: Scan generations ignore stale results and coalesce pending refreshes. Playback load failures log and schedule an asynchronous skip (`engine.py:108-129,180-191`). A scan can prune a favorite while a queue still contains its old path, so missing-file handling remains the queue's responsibility.
- UI states and visual requirements: Preserve 56 px virtual rows, current search/sort/scroll behavior, playing indicator, favorite star, queue panel, and explicit Favorites ordering. UI callbacks must be Tk-thread mutations.
- Migration implementation requirements: Use one authoritative library snapshot and path identity normalization across scan, queue, favorites, history, and deletion/rename. Preserve context replacement versus manual queue insertion and do not serialize runtime queue state into the transfer manifest. Define behavior when a scan result, favorite toggle, or queue edit races another operation.
- Verification requirements: Conformance tests must cover scan-to-row projection, play context, Play Next/Add to Queue ordering, favorite persistence/pruning, missing-file auto-skip, stale scan suppression, and close-time callback suppression. Add an integration test for favorite toggle followed by scan and playback.
- Confidence: high
- Open questions: Whether session history should become durable in the migration is unresolved; desktop currently does not persist it.

## Discover, Preview, Save
- Status: implemented
- Evidence:
  - `discovery.py:48-128` (Last.fm similar/top-track fallback)
  - `music_player.py:1860-2056,2086-2189,2460-2815,2860-3008` (recommendation requests, staging, sessions, save)
  - `previews.py:64-175` (temporary MP3 resolve/download limits)
  - `views/song_page.py:421-1110` (recommendation states, paging, save controls)
  - `test_discover_mode.py:294-377,381-739,750-885` (prefetch, queue handoff, failure, promotion)
- User behavior: Discover recommendations are filtered against the local library. The first five are passively staged outside the library; tapping a row starts a temporary preview session, while Start Discover adopts ready assets behind the seed and naturally autoplays them. Add to Library requires an explicit playlist: staged files are copied without redownloading, already-saved tracks are linked/copied without another download, and unstaged rows use the durable download path.
- State and data flow: Last.fm results are in-memory. Preview rows progress through waiting/resolving/preparing/ready/failed; files live in `PREVIEW_DIR`, and temporary paths can enter `engine.queue` without entering `_library`. Save state observes either a worker copy or controller download completion, then refreshes the library. Session seed stability prevents transient tracks from reseeding recommendations.
- Persistence and compatibility: Preview files are cleaned at session/app end, with currently playing/promotion-protected files deferred. Permanent audio is ordinary library audio; recommendations, save state, and queue-session state are not transfer-contract fields.
- Async, lifecycle, and failure behavior: Discovery uses bounded active/pending requests and generation checks. Preview resolution/download is sequential, deadline/cancellation-aware, and independent of the real Downloader slot. A failed preview is removed from queue/file and marked retryable. Save cleanup must not delete a file while it is being copied or played.
- UI states and visual requirements: Preserve explicit key/no-seed/loading/error/empty states, queued/resolving/preparing/ready/failed/retry states, save progress, 25-item paging through 1,000, scroll preservation, and explicit destination selection. Never imply a preview was saved before copy/download completion.
- Migration implementation requirements: Model preview assets and permanent library assets as separate lifecycles. Carry generation/session ownership through every resolver, download, save, cleanup, and library-refresh event; make save idempotent and promotion-safe. Preserve queue capture/restoration and distinguish standalone durable download from Song Page temporary preview.
- Verification requirements: Port all focused Discover tests and add late completion after cancellation, duplicate save, app shutdown during save, destination failure, and scan-before/after-promotion tests. Verify a staged preview is never discoverable by a library scan before promotion.
- Confidence: high
- Open questions: Desktop queued-save durability across process death is not established (`migration-audit/07-discovery-previews.md:260-262`).

## Download, Scan, Play
- Status: implemented
- Evidence:
  - `views/downloader.py:1710-1775,2022-2049` (search/result destination/download wiring)
  - `downloader.py:73-119,491-537,599-603` (job admission, sequential sync, output validation)
  - `music_player.py:3262-3332` (download callback and rescan handoff)
  - `test_downloader_sync.py:79-219` (dedupe, markers, cancellation, failures)
- User behavior: Search/direct URL/CSV/sync downloads write normalized audio to the chosen playlist or Unsorted. Progress/log callbacks update the UI; completion locates output, requests a fresh scan, and makes the track browseable/playable. Playlist sync compares the whole music root and records successful/permanent-failure video IDs.
- State and data flow: One class-level Downloader job owns yt-dlp/FFmpeg and writes one job at a time. Completion calls `_find_downloaded_audio` in the worker, then `_refresh_library` on Tk. Scan replaces the library snapshot and updates destination options and all views.
- Persistence and compatibility: Downloads are ordinary files. Sync markers are playlist-local, legacy `.downloaded.txt` is read, and playlist URLs live in `.luno_playlist_urls.json`; URL Queue is session-only. Desktop output is 44.1 kHz, 192 kbps MP3 with loudnorm and embedded thumbnail (`downloader.py:123-170`).
- Async, lifecycle, and failure behavior: Busy starts are rejected, cancellation is cooperative, and sync timeouts remain in the owning job. Sync validates that audio output exists before marking success. The direct-download path has a documented/source-backed gap: it reports yt-dlp completion without the same missing-output check (`migration-audit/06-download-sync.md:61-67`). A watchdog skips active downloads/scans (`project_brain.md:118-120`).
- UI states and visual requirements: Preserve destination choice, inline progress, bounded logs, partial-failure truthfulness, Downloaded songs refresh, and no duplicate row until scan completion.
- Migration implementation requirements: Make download completion a validated terminal event, then atomically hand off to scan. Prevent watchdog/manual/download refresh races from publishing an older snapshot. Preserve one active write job unless the target explicitly changes this contract; provide cancellation and shutdown ownership for subprocesses.
- Verification requirements: Run sync tests and add an end-to-end fake download -> output validation -> scan -> play test, plus busy/cancel/timeout, stale completion, direct missing-output, and shutdown tests.
- Confidence: high for sync; medium for direct URL because output validation is incomplete.
- Open questions: Whether session URL Queue remains non-durable and whether marker writes need atomicity in the migration.

## Rename, Reference, Rescan
- Status: implemented
- Evidence:
  - `music_player.py:489-601` (single-file edit and filesystem rename)
  - `music_player.py:636-677` (live path-reference replacement)
  - `music_player.py:3334-3466` (rescan and favorite pruning)
  - `project_brain.md:141-145,233-234` (filename identity/data flow)
- User behavior: Editing one selected local file sanitizes artist/title into `Artist - Title.ext` or `Title.ext`, rejects collisions, renames synchronously, updates status, rewrites live references, saves favorites config, and triggers a generation-tagged rescan so search/order/Unsorted reflect the new identity.
- State and data flow: The rename updates recently played, playlist context, engine queue and original queue, favorite path key, now-playing metadata, queue UI, and Recent view before scanning. Filename parsing remains the identity surface for labels, discovery, duplicate matching, playback lookup, and export fallback.
- Persistence and compatibility: The file path is the durable identity; favorite config is rewritten with the new normalized absolute path. No separate metadata database is updated. A transfer or download operation already holding an old snapshot may still refer to the old path.
- Async, lifecycle, and failure behavior: The filesystem rename and reference replacement run on Tk; rescan is background. A queued scan may be coalesced, and stale generations are discarded. There is no transaction spanning filesystem rename, config write, queue/history replacement, and scan.
- UI states and visual requirements: Preserve one-file selection, collision/error dialogs, immediate now-playing update, status feedback, search continuity where applicable, and list reset requested by rename.
- Migration implementation requirements: Define an atomic/reference-safe rename protocol, including active playback, pending download/import/export snapshots, artwork-cache invalidation, and process crash between rename and config write. Preserve case-insensitive collision behavior and path normalization.
- Verification requirements: Add tests for rename while playing/queued/favorited/recent, collision and invalid title, stale scan result, artwork cache old-path pruning, and transfer/download operation started before rename.
- Confidence: high for current-process reference replacement; medium for crash/race behavior.
- Open questions: Whether migration identity should remain filename-based or introduce a durable track ID is unresolved and affects every journey here.

## Export, Import, Download
- Status: implemented
- Evidence:
  - `music_player.py:3655-3799` (threaded UI export/import and missing-download handoff)
  - `library_transfer.py:73-210,239-369,372-486` (validation, metadata matching, additive copy)
  - `contracts/library-export/v1/schema.json:1-53` and `contracts/library-export/v1/valid.json` (wire contract)
  - `test_library_transfer.py:16-90` and `test_contract_fixtures.py:9-28` (desktop/fixture coverage)
- User behavior: Export writes a metadata-only JSON reference for selected tracks, playlists, or the library. Import validates it, matches local files by optional provider identity or normalized metadata plus compatible duration, additively copies matches into sanitized playlist folders, reports missing/errors, and asks once whether to queue missing tracks through the downloader.
- State and data flow: Export snapshots `_library` and selection before background probing/writing. Import loads/plans/applies on a worker, then Tk presents results and optionally constructs downloader rows with playlist destinations. A subsequent scan exposes copied/downloaded files.
- Persistence and compatibility: v1 rejects unknown/path-bearing fields, forbids audio/paths/secrets/queue/favorites/history, preserves playlist order, and uses collision suffixes without deletion. Normal desktop controller wiring does not provide `source_by_path`, so provider-ID matching is codec-supported but not normally produced/used (`migration-audit/10-transfer-contracts.md:24-25`).
- Async, lifecycle, and failure behavior: `_transfer_running` serializes transfers, but import completion can immediately enqueue downloads and trigger refreshes. Copy errors accumulate without rollback; missing downloads share the single Downloader slot and can be busy. A scan or rename concurrent with a snapshot can produce a valid but stale result.
- UI states and visual requirements: Preserve Preparing/Checking states, explicit “audio and paths excluded” export messaging, missing/ambiguity confirmation, partial-copy reporting, error log, and post-operation refresh.
- Migration implementation requirements: Keep the exact v1 codec and additive/no-deletion semantics. Make snapshot generation, copy, download, and scan ownership explicit; prevent stale transfer callbacks after close; define whether missing-track download is one aggregate job and how partial completion is reported. Do not silently substitute metadata-only recordings.
- Verification requirements: Preserve fixture acceptance/rejection and add integration coverage for export -> import -> copy -> scan -> play, missing YouTube handoff, non-YouTube provider fallback, duration mismatch, duplicate candidates, collision suffixes, copy failure, transfer/download busy, and shutdown.
- Confidence: high for codec/merge behavior; medium for controller provider-ID and end-to-end handoff.
- Open questions: Provider identity persistence and per-track versus global ambiguity confirmation need an explicit migration decision.

## Song Page Video Alongside Audio
- Status: implemented
- Evidence:
  - `video_preview.py:50-147` (probe relevance and temporary MP4 staging)
  - `views/song_page.py:26-210,1218-1258,1364-1461,1491-1680` (audio-clock decoder, adoption/preload/fullscreen lifecycle)
  - `engine.py:152-166,295-399` (track start, synchronized position/pause/seek/completion)
  - `test_video_preview.py:28-53` (source relevance/canonical URL)
- User behavior: Song Page can probe/load current or next video without changing audio queue/library. Video is optional and degrades to artwork when FFmpeg/source/download is unavailable. Pause, resume, seek, track changes, resize, fullscreen, and page destruction keep video subordinate to audio.
- State and data flow: yt-dlp probes up to ten relevant results across query variants, stages merged MP4 in `PREVIEW_DIR`, and the page adopts matching files by artist/title key. FFmpeg decodes timestamped frames; presentation consumes `engine.get_position()`. Video paths and preload states are separate from `_library` and `engine.queue`.
- Persistence and compatibility: MP4s are disposable cache/presentation assets, not exported or scanned. Current/next preload has terminal ready/unavailable state; staged adoption avoids duplicate download.
- Async, lifecycle, and failure behavior: Probe/download/decode are off Tk and callbacks are marshalled to Tk. Track generations reject stale adoption; scrubbing interrupts/restarts decoding; resize restarts at the audio position; fullscreen destroys its window before asynchronous decoder stop. Missing tools/source preserve audio playback.
- UI states and visual requirements: Preserve responsive 16:9 media stage, aspect-fit letterboxing, artwork fallback, compact Song Page layout, honest current/next status, spinner only for active jobs, and F/Escape cleanup.
- Migration implementation requirements: Use one authoritative audio clock and explicit ownership for yt-dlp/FFmpeg subprocesses. Port key derivation, relevance rules, terminal preload states, generation guards, cancellation, decoder EOF/resize/seek/fullscreen cleanup, and cache cleanup. Video must never become library or queue state.
- Verification requirements: Preserve `test_video_preview.py`; add fake-process tests for stale track adoption, pause/seek synchronization, EOF, resize, fullscreen close, missing FFmpeg, cancellation, and app shutdown. Add a journey test proving audio remains playable and scan/export ignore MP4s.
- Confidence: high for probe/staging and lifecycle intent; medium for real Tk/FFmpeg timing because no renderer fixture exists.
- Open questions: FFmpeg distribution, codec/provider policy, and legal availability are deployment decisions.

## Cross-Journey Dependency And Race Matrix
- Scan is the shared projection boundary: download/import/rename all eventually request `_refresh_library`; generation checks prevent stale publication, but snapshots can still be semantically stale when operations overlap (`music_player.py:3334-3389,3655-3749`).
- Path identity is the shared coupling: queue, history, favorites, artwork cache, transfer matching fallback, and filename-derived discovery all depend on paths or names. Rename must update every owner before the next scan; migration should prefer a transaction or durable IDs.
- Download ownership is shared by discovery saves, imports, Downloader tabs, and sync. The one-job coordinator prevents concurrent writes but creates busy/error states and requires explicit handoff (`downloader.py:73-119`; `music_player.py:3262-3332,3784-3799`).
- Temporary media is deliberately isolated: recommendation MP3 and Song Page MP4 can be queued/displayed without library membership, while promotion/download must be the only path to permanence (`project_brain.md:280-284`; `video_preview.py:140-147`).
- Shutdown is a universal race: late Tk callbacks, subprocesses, decoder frames, scans, saves, and download completion must not mutate destroyed views or resurrect files. Existing callbacks commonly check `_closing`, but future migration acceptance must test every operation.

## Acceptance Requirements
- A complete desktop migration must preserve all seven journeys above as observable state transitions, not only individual service APIs.
- Every background completion must carry operation/generation ownership, be delivered on the UI thread, and be ignored after supersession or shutdown.
- Scan, rename, download, import, promotion, and cleanup must have explicit commit/rollback or compensating behavior; no stale path may remain in queue, favorites, history, artwork, or transfer snapshots.
- Temporary recommendation audio and Song Page video must remain invisible to library scans, browse counts, exports, and playlist membership until an explicit save/download succeeds.
- Download/import/promote completion must validate actual output before reporting success and must make partial failures, busy state, cancellation, and retry visible.
- Queue semantics must retain playlist-context replacement, manual Up Next ordering, preview handoff/restoration, and audio-clock video synchronization.
- Verification must include deterministic service tests plus at least one fake-filesystem/UI-thread integration path for each journey; Android tests are outside this unit's gate.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`, `project_brain.md`, `music_player.py`, `engine.py`, `downloader.py`, `library_transfer.py`, `discovery.py`, `previews.py`, `video_preview.py`, `views/library.py`, `views/song_page.py`, `views/downloader.py`, contract schema/fixtures.
- Tests/fixtures inspected: `test_engine_playback.py`, `test_downloader_sync.py`, `test_discover_mode.py`, `test_library_transfer.py`, `test_contract_fixtures.py`, `test_video_preview.py`.
- Claims requiring review: Active cross-journey crash atomicity; direct-download output validation; desktop provider-ID production; queued discovery-save durability; real Tk/FFmpeg timing.
- Coverage gaps: No target migration exists; no full desktop UI integration harness, real yt-dlp/FFmpeg/network fixture, crash/restart test, or end-to-end export/import/download/scan/play test is tracked.
- Verification run: Not run; this dossier did not run Android tests. Focused desktop test execution should be performed by the parent migration gate because the local environment/test dependency state is not established in this unit.
- Status: complete

# Discovery and Preview Audit

This dossier audits the implemented desktop Python/Tk and native Android/Kotlin
Last.fm discovery and temporary recommendation-preview behavior. The two
applications do not share a runtime; parity statements below mean observable
product intent, not shared code.

## Last.fm Discovery
- Status: implemented
- Evidence:
  - `discovery.py:48-128`
  - `music_player.py:1860-1944`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/discovery/LastfmService.kt:62-132,188-287`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/DiscoveryRepository.kt:22-78`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/discover/DiscoverScreen.kt:106-175`
- User behavior: A Last.fm key and artist/title seed produce similar-track
  recommendations. Desktop supports current playback or a selected library row,
  including folder-name artist fallback; Android chooses current, recent, then
  first library track. Missing key, missing seed, errors, retry, and empty
  recommendation states are surfaced in the respective UIs.
- State and data flow: Both call `track.getsimilar`, then
  `artist.gettoptracks` when the first result is empty (fallback match `0.8`).
  Results include match and usable artwork URL. Desktop uses one active daemon
  request plus one latest pending request; Android Compose cancels/restarts its
  effect for seed/key/refresh changes.
- Persistence and compatibility: The API key is desktop config state and an
  Android encrypted key-store value exposed by `StateFlow`; it is not part of
  library transfer. Last.fm results are in-memory only.
- Async, lifecycle, and failure behavior: Desktop request work is off Tk and
  UI delivery is through `after(0)` with generation checks. HTTP calls have an
  8-second request timeout. Android uses OkHttp IO dispatch and reports
  user-safe failures; Compose cancellation is preserved.
- UI states and visual requirements: Desktop Discover is a dedicated tab;
  Android `DiscoverScreen` is a dedicated bottom-nav screen with key setup,
  no-seed, loading, error/retry, empty, row, and download controls.
- Migration implementation requirements: Preserve seed precedence, folder
  fallback where applicable, two-stage API behavior, key handling, bounded
  request delivery, and honest empty/error states. Do not migrate API keys in
  the library manifest.
- Verification requirements: Exercise desktop request-generation tests and
  Android `LastfmServiceTest` plus Discover UI/manual states with mocked HTTP.
- Confidence: high
- Open questions: Android standalone Discover fetch uses `limit = 20`; the
  full-player recommendation flow uses a separate paged/preview path. A future
  migration must decide whether those surfaces remain distinct.

## Recommendation Filtering, Cache, and Artwork
- Status: implemented
- Evidence:
  - `music_player.py:1947-1969,2012`
  - `discovery.py:60-65,121-124`
  - `rec_artwork.py:42-45,116-153,156-178`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/DiscoveryRepository.kt:52-78`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/discovery/RecommendationArtworkService.kt:11-61`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/components/RecommendationArtwork.kt:135-215`
  - `test_discover_mode.py:294-377`
  - `mobile-app/app/src/test/java/com/luno/mobile/data/discovery/LastfmServiceTest.kt:99-145,233-272`
  - `mobile-app/app/src/test/java/com/luno/mobile/data/discovery/RecommendationArtworkServiceTest.kt:18-62`
- User behavior: Recommendations already present in the local library and
  duplicate identities within a response are hidden. Last.fm placeholder or
  absent artwork falls back to a cached Deezer cover, then a deterministic
  abstract design.
- State and data flow: Desktop filtering normalizes lowercase trimmed
  `artist - title` strings and deduplicates the displayed list. Its Last.fm
  cache is keyed by seed and requested limit and clears when over ten entries.
  Android uses Unicode-normalized identity keys and the same seed/limit cache
  policy. Deezer caches both hits and nulls. Desktop artwork uses quoted query
  then plain fallback; Android source currently uses the Deezer field-query
  form only.
- Persistence and compatibility: Recommendation caches and artwork URLs are
  process-memory data; no recommendation is added to the library by fetching
  or previewing it.
- Async, lifecycle, and failure behavior: Artwork lookup is bounded by 8-second
  HTTP calls and failure returns null. Desktop song-page discovery has its own
  active/pending bound independent of Discover (`music_player.py:1981-2036`).
- UI states and visual requirements: Abstract artwork is stable per normalized
  identity and is used for missing/loading/error states; valid remote artwork
  remains authoritative.
- Migration implementation requirements: Preserve filtering before display,
  limit-sensitive cache keys, placeholder rejection, bounded artwork work, and
  deterministic fallback artwork. Verify Android Deezer field-query behavior
  before claiming parity with desktop's quoted fallback.
- Verification requirements: Run `test_discover_mode.py`, desktop artwork/cache
  tests, Last.fm service tests, and recommendation-artwork tests with null,
  placeholder, duplicate, and cache cases.
- Confidence: high
- Open questions: Android `RecommendationArtworkService` does not show the
  desktop quoted-query/plain-query fallback in the inspected source; whether
  API behavior makes this a real parity gap needs confirmation.

## Temporary Preview Resolution and Download
- Status: implemented
- Evidence:
  - `previews.py:1-11,28-40,64-103,106-175`
  - `music_player.py:2038-2056,2240-2425`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/RecommendationPreviewManager.kt:125-174,879-1087`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/RecommendationPreviewManager.kt:1975-1989,2118-2135`
  - `test_discover_mode.py:437-493,750-776`
- User behavior: Tapping a recommendation resolves a playable YouTube source,
  downloads temporary audio, and plays it without placing it in the library.
  Existing staged files are adopted. Preview output is MP3, 44.1 kHz, 192 kbps,
  with thumbnail embedding and no loudness normalization.
- State and data flow: Desktop stages under `PREVIEW_DIR` (normally
  `~/.luno_previews`) and checks safe-name candidates. Android stages under
  `cacheDir/recommendation_previews`, creates generation/index-qualified files,
  and builds transient `MediaTrack` items for Media3.
- Persistence and compatibility: Preview files are temporary and outside the
  library/export contract. Explicit playlist promotion creates the permanent
  library copy; staged files remain until session cleanup so active queue items
  continue to work.
- Async, lifecycle, and failure behavior: Desktop resolution uses a two-thread
  resolver pool and download daemon; the preview chain does not consume the
  library Downloader's single slot. Android uses sequential session preparation
  and separate preview/durable resolver executors.
- UI states and visual requirements: Rows transition waiting/resolving or
  preparing to ready/failed, with retryable timeout/too-large messages and
  queued-for-preview indication.
- Migration implementation requirements: Preserve transient identity, staging
  isolation, adoption, raw-preview encoding, size/duration guards, and no
  accidental library scan or export exposure.
- Verification requirements: Mock resolver/download and assert queue/library
  separation, adoption, file format mapping, size rejection, and failed-row
  recovery.
- Confidence: high
- Open questions: Desktop's `download_preview` size check occurs after yt-dlp
  completes, so a source can consume work/storage before rejection; Android's
  streaming implementation should be checked for equivalent pre-write bounds.

## Deadlines, Cancellation, and Request Coalescing
- Status: implemented
- Evidence:
  - `music_player.py:1981-2036,2315-2403`
  - `previews.py:106-145,159-175`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/RecommendationPreviewManager.kt:612-680,1880-1889,2108-2135`
  - `mobile-app/app/src/test/java/com/luno/mobile/playback/RecommendationPreviewManagerTest.kt:439-537`
- User behavior: A resolver timeout says “Finding a playable source timed out.
  Tap to retry.”; download timeout says “Preview download timed out. Tap to
  retry.”; oversized sources receive an explicit durable-download alternative.
- State and data flow: Desktop generation increments invalidate stale callbacks;
  cancellation is cooperative through yt-dlp progress hooks. Android uses
  coroutine timeouts plus generation checks and deletes stale targets.
- Persistence and compatibility: No cancelled or stale preview result is
  promoted by the preview chain; durable saves intentionally use a separate
  resolver/job path.
- Async, lifecycle, and failure behavior: Desktop has bounded discovery request
  coalescing (active + latest pending) and independent song-page coalescing.
  Android serializes each preview session and cancels prefetch/session jobs on
  seed/session changes. Android preview resolution/download deadlines are 15s
  and 5m.
- UI states and visual requirements: Timeout clears active spinner state and
  leaves a retryable failure instead of an indefinitely busy row.
- Migration implementation requirements: Port generation ownership, latest
  request semantics, cooperative cancellation, deadline messages, stale-file
  cleanup, and cancellation-safe UI delivery.
- Verification requirements: Run the desktop discovery tests and Android timeout
  tests; include late completion after timeout/cancel in migration tests.
- Confidence: high
- Open questions: Desktop uses a `threading.Timer` timeout while the underlying
  resolver worker may continue until yt-dlp returns; the generation prevents
  stale adoption, but does not forcibly stop that worker.

## Prefetch and Tap Sessions
- Status: implemented
- Evidence:
  - `music_player.py:2086-2189,2504-2624,2687-2714`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/RecommendationPreviewManager.kt:540-705,707-1087`
  - `test_discover_mode.py:294-357,437-513`
  - `mobile-app/app/src/test/java/com/luno/mobile/playback/RecommendationPreviewManagerTest.kt:314-347`
- User behavior: A normal recommendation list passively prepares the first five
  without changing playback. Tapping any row starts/restarts a temporary session
  at that row and keeps the tapped row plus up to five successors prepared.
- State and data flow: Prepared assets are retained when still useful and
  adopted instead of downloaded again. Desktop tracks window indices and paths;
  Android tracks keyed assets and transient Media3 queue entries.
- Persistence and compatibility: Prefetch/session state is in memory; files are
  cache/staging data only. Staged files outside the active window are cleaned.
- Async, lifecycle, and failure behavior: Preparation is sequential. A failed
  first tapped item ends the non-Discover session without hijacking the seed;
  later failures skip onward and remain retryable.
- UI states and visual requirements: Active row, ready rows, queued rows, and
  failure/retry states are represented independently of playback queue state.
- Migration implementation requirements: Preserve five-ahead window semantics,
  refresh-safe adoption, first-item failure behavior, restart-at-row behavior,
  and seed stability during a session.
- Verification requirements: Verify passive prefetch does not alter queue,
  tapped cached row does not redownload, window cap, restart cleanup, and first
  failure behavior.
- Confidence: high
- Open questions: None for observed behavior.

## Discover Mode, Queue Adoption, and Restoration
- Status: implemented
- Evidence:
  - `music_player.py:2460-2502,2718-2815`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/RecommendationPreviewManager.kt:327-435,707-874,1090-1272,1374-1473`
  - `test_discover_mode.py:381-435,546-739`
  - `mobile-app/app/src/test/java/com/luno/mobile/playback/RecommendationPreviewManagerTest.kt:250-312,349-381`
- User behavior: Start Discover captures the physical queue, inserts/adopts
  ready recommendations behind the current track, and waits for the seed to
  finish naturally. Next/Previous route through the recommendation block in
  Discover; non-Discover Next leaves to the seed successor and Previous returns
  to the seed.
- State and data flow: The rolling block refills as recommendations are
  consumed. Discover can append another page at the end; a replaced user queue
  ends the session without overwriting that queue.
- Persistence and compatibility: Turning Discover off restores the captured
  physical queue when still valid. A recommendation currently playing may
  finish before the seed successor; playback already past the block is rebuilt
  at the current physical position. Preview files are deleted, except a
  currently playing file deferred until track change.
- Async, lifecycle, and failure behavior: Natural handoff ends the session;
  playback decoder failure removes the preview queue item, deletes its file, and
  leaves a retryable row. Android additionally monitors Media3 playback and
  request revisions.
- UI states and visual requirements: Desktop Song Page exposes Discover mode;
  Android Full Player exposes “Start discover mode” / “Discover mode: On”.
- Migration implementation requirements: Port queue snapshot/adoption,
  deferred handoff, media-key routing, restoration rules, queue-intervention
  detection, infinite-scroll continuation, and deferred cleanup.
- Verification requirements: Run Discover-mode tests for adoption, rolling
  refill, handoff, toggle-off, queue replacement, playback failure, and auto
  extension; run Android manager tests for handoff and queue continuation.
- Confidence: high
- Open questions: The standalone Android Discover tab downloads permanent
  tracks directly, while Full Player Discover mode uses temporary previews. The
  target migration must preserve that surface distinction or explicitly unify it.

## Save to Playlist and Cleanup
- Status: implemented
- Evidence:
  - `music_player.py:2860-3008`
  - `test_discover_mode.py:778-868`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/RecommendationPreviewManager.kt:1274-1355`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/player/FullPlayerScreen.kt:463-496`
  - `mobile-app/app/src/test/java/com/luno/mobile/playback/RecommendationPreviewManagerTest.kt:383-437`
- User behavior: Add to Library requires an explicit playlist. A staged preview
  is copied into that playlist; an unstaged row uses the durable resolver and
  queues a download. A previously saved recommendation is added to another
  playlist without redownloading. Save state reports saving, queued, saved, or
  failure.
- State and data flow: Desktop copies staged audio on a worker and refreshes the
  library; Android promotes transactionally through `DownloadRepository` or
  observes the queued Room download job and marks permanent completion.
- Persistence and compatibility: Permanent audio belongs to the selected
  playlist/library. Preview state and staged audio remain temporary until
  session cleanup; saved-path/job state is process-local on desktop and manager
  state plus Room job/track state on Android.
- Async, lifecycle, and failure behavior: Duplicate save requests are rejected;
  save failures remain retryable. Cleanup avoids deleting a file being promoted
  and defers deletion of the currently playing preview.
- UI states and visual requirements: Desktop status text and row state expose
  save progress/failure; Android uses Toast plus row state and recently-saved
  playlist ordering.
- Migration implementation requirements: Require explicit destination, preserve
  staged-copy/no-redownload semantics, durable fallback, duplicate-save guard,
  completion observation, and promotion-safe cleanup.
- Verification requirements: Run promotion, already-saved, no-staged-download,
  queued completion, duplicate-save, and cleanup tests.
- Confidence: high
- Open questions: Desktop's queued permanent-download completion relies on its
  existing controller download callback; exact persistence across process death
  is not established by the inspected discovery tests.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`, `project_brain.md`, `mobile-app/project_brain.md`, `discovery.py`, `previews.py`, `rec_artwork.py`, relevant `music_player.py` discovery/session sections, `test_discover_mode.py`, Android `LastfmService.kt`, `DiscoveryRepository.kt`, `RecommendationArtworkService.kt`, `DiscoverScreen.kt`, `FullPlayerScreen.kt`, `RecommendationPreviewManager.kt`, and Android discovery/preview/artwork tests.
- Tests/fixtures inspected: `test_discover_mode.py`, `LastfmServiceTest.kt`, `RecommendationArtworkServiceTest.kt`, `RecommendationPreviewManagerTest.kt`.
- Claims requiring review: Android Deezer query fallback parity; desktop resolver worker termination after deadline; process-death durability of desktop queued saves; standalone Android Discover permanent-download behavior versus Full Player temporary-preview behavior.
- Coverage gaps: No real Last.fm/Deezer/YouTube network or manual UI/device run was used; no target migration/Tauri implementation exists to audit; desktop full preview resolver/download tests are indirectly exercised through Discover-mode stubs rather than live subprocesses.
- Status: complete

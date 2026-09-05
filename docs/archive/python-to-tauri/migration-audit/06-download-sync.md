# Download, Search, and Sync Audit

## Desktop Search and Acquisition
- Status: implemented
- Evidence:
  - `views/downloader.py:1710-1775`
  - `downloader.py:123-170`
  - `project_brain.md:274-277`
- User behavior: The Downloader searches YouTube with `ytsearchN` and SoundCloud
  with `scsearchN`, supports Load More, and presents result cards with download
  actions. Search runs through the view's metadata worker and returns to Tk with
  `.after(0, ...)`. Search errors are shown in inline status; SoundCloud search
  errors are silently ignored.
- State and data flow: Search results carry title, uploader, URL, source,
  duration, and thumbnail. A selected result is sent to `Downloader.download_single`.
  The default destination is the selected destination or `MUSIC_ROOT`/Unsorted;
  the view creates it before starting a job (`views/downloader.py:2022-2049`,
  `music_player.py:3262-3277`).
- Persistence and compatibility: Search itself is not persisted. Completed
  files become visible after a background library rescan. Filename metadata is
  the desktop identity surface, not a download database.
- Async, lifecycle, and failure behavior: Search metadata and artwork use bounded
  background work, but the search operation has no explicit cancellation. One
  process-wide download job is admitted at a time (`downloader.py:73-119`);
  a second request receives `DownloadBusyError`. Closing the desktop sets the
  cancellation event (`music_player.py:4730-4736`), but normal UI cancellation
  is not exposed by the Downloader view.
- UI states and visual requirements: Inline status and a coalesced progress bar
  are updated on the Tk thread (`views/downloader.py:1380-1421`). The log is
  bounded to `MAX_LOG_LINES` and pending worker messages are coalesced
  (`views/downloader.py:1327-1375`).
- Migration implementation requirements: Preserve result identity, destination
  selection, bounded search/artwork work, one active real-download/write job,
  and main-thread event delivery. Define cancellation for ordinary user actions,
  not only application shutdown.
- Verification requirements: Retain mocked search-generation and busy/cancel
  tests; add direct assertions for search error presentation and cancellation.
- Confidence: high
- Open questions: Whether SoundCloud search should remain best-effort or expose
  its extraction failure to users.

## Desktop Direct URL and CSV
- Status: implemented
- Evidence:
  - `views/downloader.py:1428-1472`
  - `views/downloader.py:2094-2141`
  - `library_tools.py:46-61`
  - `downloader.py:172-229,232-293`
- User behavior: Direct URLs require an `http`/`https` URL with a network
  location, accept optional title/artist overrides, and queue to the selected
  destination. Exportify CSV files are parsed by `Track Name`, `Artist Name(s)`,
  and `Album Name`; multiple files can be loaded and downloaded sequentially,
  optionally probing SoundCloud before YouTube search.
- State and data flow: CSV rows are converted to `{artist,title,album}` and the
  view adds `playlist_name`; each row resolves a URL, creates its destination,
  and downloads to a sanitized `Artist - Title` template
  (`downloader.py:232-281`).
- Persistence and compatibility: CSV source files and loaded rows are session
  state only. Output is ordinary audio in the selected folder and is discovered
  by the next scan.
- Async, lifecycle, and failure behavior: CSV processing is one owning job with
  cooperative checks before each row and in yt-dlp progress hooks. Per-row
  failures are logged and processing continues; cancellation breaks the loop but
  still calls `on_complete` (`downloader.py:241-293`). Direct download catches
  exceptions but reports success after yt-dlp returns without checking that an
  audio file exists (`downloader.py:178-194`), unlike sync downloads' explicit
  missing-output validation (`downloader.py:599-603`).
- UI states and visual requirements: The desktop shows per-row counts, a thin
  progress bar, bounded log, and failure/success status. Direct URL rejects
  malformed input before creating a folder (`views/downloader.py:1428-1447`).
- Migration implementation requirements: Preserve the desktop CSV column
  contract and per-row continuation semantics, but make direct URL completion
  require validated output and an explicit terminal failure state. Do not treat
  “yt-dlp returned” as “audio exists.”
- Verification requirements: Add tests for malformed CSV quoting, destination
  failure, direct-download missing output, partial CSV failure, and cancellation.
- Confidence: high
- Open questions: None for the desktop parser; Android's separate CSV parser is
  audited below and is not equivalent.

## Desktop Playlist Sync, URL Queue, and Markers
- Status: implemented
- Evidence:
  - `downloader.py:296-548`
  - `views/downloader.py:623-851,1781-1901`
  - `music_player.py:3952-4056,4691-4720`
  - `test_downloader_sync.py:79-155`
- User behavior: A playlist URL can be previewed, synced to a selected playlist,
  synced across stored playlist URLs, or placed in a session URL Queue. URLs
  containing `list=` are normalized to a playlist URL in the preview flow
  (`views/downloader.py:1907-1923`). Stored URLs live in
  `MUSIC_ROOT/.luno_playlist_urls.json`; URL Queue entries are not durable.
- State and data flow: Sync extracts flat playlist entries, splits `artist - title`,
  scans the entire library root, and excludes known normalized artist/title or
  title-only keys. It also checks expected audio extensions. Existing/matched
  video IDs are written to `.downloaded_vids`; permanent failures matching
  unavailable/age/content markers are written with reasons to `.sync_failed_vids`;
  legacy `.downloaded.txt` is read (`downloader.py:334-475`). Each new item is
  downloaded sequentially with a 120-second cooperative progress-hook deadline,
  then the newly created audio output is validated before its ID is marked
  successful (`downloader.py:491-537`).
- Persistence and compatibility: Marker files are playlist-folder-local and
  plain text. They are compatibility inputs, including legacy `.downloaded.txt`.
  The URL map is a separate JSON file. No atomic write or file locking protects
  either persistence path.
- Async, lifecycle, and failure behavior: The class-level coordinator allows one
  download/write job across direct, CSV, and sync operations. Playlist items are
  sequential and timeout cleanup stays in the owning thread; cancellation stops
  before the next item or through the hook. Playlist extraction failure and
  per-item failures are logged, then completion is reported; permanent failures
  are skipped on future syncs, while transient failures are retried next time.
  URL Queue chains playlists sequentially and uses the queue-running flag as its
  cancellation predicate (`views/downloader.py:789-851`).
- UI states and visual requirements: Sync exposes playlist messages, per-track
  logs, batch counts, progress, and completion status. Sync All uses persisted
  URLs for playlists still present in the scanned library (`views/downloader.py:1853-1901`).
- Migration implementation requirements: Port marker compatibility, whole-root
  duplicate avoidance, title normalization, destination mapping, sequential
  ownership, cancellation, timeout, missing-output validation, and truthful
  partial-failure reporting. Define atomic marker updates and a durable URL queue
  only if the target promises crash recovery; the current URL Queue is session
  only.
- Verification requirements: Preserve `test_downloader_sync.py` coverage for
  second sync, legacy archive, sibling playlist dedupe, missing output, permanent
  failure, exclusive jobs, cancellation hook, and cooperative timeout. Add tests
  for marker-write failure, duplicate playlist membership, and cancellation during
  extraction.
- Confidence: high
- Open questions: Marker semantics for a video that later becomes available, and
  whether URL Queue persistence is required by the migration target.

## Desktop Duplicate and Allowed-Duplicate Workflows
- Status: implemented
- Evidence:
  - `library_tools.py:64-105`
  - `views/downloader.py:853-1176`
  - `engine.py:416-430`
  - `project_brain.md:225-247`
- User behavior: Duplicate scan groups files by normalized artist/title, strips
  common release tags and extensions, and offers delete or Allow Selection.
  Allowed keys are excluded from later scans and can be removed, exported, or
  imported as JSON.
- State and data flow: The view scans the controller's current `_library` on a
  metadata worker and progressively renders groups. Allowed keys are normalized
  identity strings held by the engine and persisted at
  `MUSIC_ROOT/.luno_allowed_dups.json`.
- Persistence and compatibility: The allowed-duplicate file and its JSON-list
  backup are desktop-local; they are not part of the shared library-export
  manifest.
- Async, lifecycle, and failure behavior: Scan admission is locked and failures
  become a visible failed-scan status. Deletion is synchronous in the UI callback;
  individual delete errors are logged and the library is rescanned. JSON import
  accepts any list values without type/normalization validation.
- UI states and visual requirements: Groups show playlist and filename columns;
  double-click plays a track; progressive insertion avoids blocking large scans.
- Migration implementation requirements: Preserve the exact normalization and
  exclusion-key behavior, explicit destructive confirmation, and portable backup
  actions. Validate imported key types before using them.
- Verification requirements: Add duplicate normalization, allowed-key persistence,
  malformed backup, bulk selection, and deletion-failure tests.
- Confidence: high
- Open questions: Whether Android should gain an allowed-duplicate backup workflow;
  its source currently has duplicate detection but no corresponding allowed-key
  persistence surface.

## Android Search, Direct URL, and CSV
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/WebSearchService.kt:50-87,155-183`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/search/SearchScreen.kt:200-263,299-347,495-527`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/search/SearchScreen.kt:1323-1362`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/DownloadRepository.kt:62-126`
- User behavior: Android searches YouTube through NewPipe, queues a selected
  result after stream extraction, accepts a direct URL with optional metadata,
  and imports Exportify-style CSV rows. Search and CSV resolution run on IO
  coroutines; jobs are then durable WorkManager records. Direct URLs are only
  checked for nonblank input in the UI, so unlike desktop they are not validated
  as HTTP(S) URLs before enqueue.
- State and data flow: Search extracts an audio stream URL, stores title/artist,
  thumbnail, destination playlist, and source URL in `DownloadJob`; absent
  destination routes to a durable Unsorted playlist. CSV rows search one YouTube
  result per row and queue each successful extraction. The CSV reader uses
  `line.split("\\\",\\\"")`, drops the header, and does not implement quoted
  commas or the desktop column-name parser.
- Persistence and compatibility: Room `DownloadJob` stores queued/downloading/
  completed/failed/cancelled state, progress, local URI, error, playlist IDs,
  and thumbnail. WorkManager uses unique `download_<id>` work. There is no shared
  desktop marker-file or output-codec compatibility; Android stores the original
  stream format.
- Async, lifecycle, and failure behavior: Search has an in-flight guard per
  video, but CSV has no cancellation control and its loop stops on an exception
  rather than recording per-row durable failure. Queueing failures are shown in
  Compose state. Download cancellation/retry is exposed from result rows and
  routed through WorkManager (`SearchScreen.kt:450-455`).
- UI states and visual requirements: Thin active-download progress, row-level
  extraction/queue states, downloaded-song accordion, retry/cancel actions, and
  error messages are implemented. The full queue/status UI is in
  `DownloadsScreen.kt:187-243,255-269`.
- Migration implementation requirements: Decide whether the migration preserves
  desktop's MP3/44.1kHz/stereo/loudnorm output or Android's source-native M4A/
  Opus/MP3 behavior. Preserve explicit destinations, durable job identity,
  thumbnails, and retry/cancel states; add strict URL and standards-compliant
  CSV parsing if desktop parity is required.
- Verification requirements: Existing repository/DAO tests cover enqueue routing,
  thumbnails, states, cancel/delete, and preview promotion
  (`mobile-app/app/src/test/java/com/luno/mobile/data/repository/DownloadRepositoryTest.kt:56-187`).
  Add worker HTTP/output-validation tests, URL validation tests, CSV quoting and
  partial-failure tests, and cancellation tests.
- Confidence: high
- Open questions: Whether direct Android URLs may intentionally accept arbitrary
  WorkManager/HTTP-supported schemes, and whether Android must transcode/normalize.

## Android Playlist Sync and URL Queue
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/WebSearchService.kt:90-133,732-744`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/PlaylistSyncWorker.kt:34-155`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/DownloadRepository.kt:534-563`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/downloads/DownloadsScreen.kt:111-182`
- User behavior: A playlist URL is attached to a Room playlist and can be synced
  individually or through Sync All Playlists. Sync work is durable and network
  constrained. Playlist URL parsing requires `list=` or a raw playlist ID.
- State and data flow: The worker extracts playlist videos, loads all tracks once,
  then for each video either adds a matching existing track to the playlist or
  resolves an audio URL and creates a `DownloadJob` targeting that playlist.
  Each child uses unique WorkManager work with playlist and download tags.
- Persistence and compatibility: Playlist URLs are Room fields (schema migration
  v3); jobs retain playlist IDs and source stream URLs. No `.downloaded_vids`,
  `.sync_failed_vids`, or equivalent per-video permanent-failure ledger exists.
  Child jobs are deduped only while their `sourceUrl` contains the video ID.
- Async, lifecycle, and failure behavior: WorkManager provides network
  constraints, exponential backoff, foreground notifications, and cancellation.
  `isStopped` stops the sync loop, but already-enqueued child work is not rolled
  back. Extraction errors increment an in-memory error count and the sync worker
  still returns success after logging. Sync All launches one unique work per
  playlist from a Compose loop, so it is enqueue-all rather than desktop's
  sequential URL Queue (`DownloadsScreen.kt:161-183`).
- UI states and visual requirements: Playlist URL, Sync, Sync All, Stop All, and
  per-job state/progress are visible. There is no per-playlist result summary in
  the cited screen beyond durable child rows.
- Migration implementation requirements: Preserve playlist URL persistence,
  destination membership, idempotence, network constraints, cancellation tags,
  retry policy, and foreground progress. If desktop parity is required, add a
  durable marker/failure ledger, robust normalized identity matching, explicit
  sequential URL-queue semantics, and a truthful aggregate failure result.
- Verification requirements: Add tests for playlist ID parsing, existing-track
  matching, duplicate child suppression, playlist membership ordering, stop/cancel
  propagation, and aggregate extraction failure. Current Android tests do not
  exercise `PlaylistSyncWorker` or `DownloadWorker` network/finalization behavior.
- Confidence: high
- Open questions: Whether Android's concurrent WorkManager children are an intended
  improvement or a migration regression against desktop's one-job sequential model.

## Android Download Worker, FFmpeg/Output, Destinations, and Failure
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/DownloadWorker.kt:54-169,171-285`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/DownloadWorker.kt:286-347`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/DownloadRepository.kt:188-307`
  - `mobile-app/project_brain.md:307-348`
- User behavior: Android downloads the resolved stream with OkHttp into a unique
  app-private file, determines extension from `Content-Type`, probes duration,
  fetches embedded/thumbnail/Deezer artwork, copies to the selected SAF folder
  when configured, and inserts the track plus playlist memberships transactionally.
  Without a selected folder, the app-private file is the durable fallback.
- State and data flow: Progress is written to Room per read when content length is
  known. Completed jobs retain a `content://` or `file:` URI. SAF output and
  artwork are compensated on finalization failure; the DB transaction inserts
  track, memberships, and completed job together. Preview promotion separately
  copies a cache file into a unique staged filename and uses `NonCancellable` plus
  a Room transaction (`DownloadRepository.kt:193-307`).
- Persistence and compatibility: Android does not run yt-dlp or FFmpeg subprocesses
  in the inspected worker; it consumes NewPipe/Invidious/InnerTube stream URLs and
  retains M4A/Opus/etc. Source extraction strategy and supported formats are in
  `WebSearchService.kt:155-183,335-389,407-537`. This differs materially from
  desktop yt-dlp FFmpeg MP3 192k, 44.1kHz, stereo, loudnorm
  (`downloader.py:133-167`).
- Async, lifecycle, and failure behavior: WorkManager cancellation is checked in
  the copy loop and deletes the staged file; IO errors retry up to three attempts
  with exponential backoff, while finalization errors fail immediately. Non-200/
  206 responses, empty bodies, missing jobs, and invalid input fail explicitly.
  A response can still be accepted as audio without a codec/content validation
  beyond media-duration probing; duration extraction failure becomes `0L` rather
  than failing the job. `DownloadWorker` has no byte-size cap.
- UI states and visual requirements: Foreground notifications identify downloads
  and playlist sync; Room flows render queued, percentage downloading, completed,
  failed error text, and cancelled states (`DownloadsScreen.kt:255-269`).
- Migration implementation requirements: Treat output codec, sample rate,
  normalization, stream extraction, SAF destination, unique naming, atomic DB/file
  finalization, retry limits, cancellation cleanup, and progress as separate
  compatibility decisions. If the target requires desktop output parity, Android
  needs an approved on-device conversion strategy; do not infer FFmpeg parity from
  documentation alone.
- Verification requirements: Add deterministic worker tests for HTTP status/body,
  content-type extension, cancellation cleanup, missing/invalid media output,
  SAF-copy rollback, retry/final failure, progress, and transaction membership.
  Existing tests verify repository-level preview rollback but not the worker itself.
- Confidence: high
- Open questions: Whether zero-duration media is acceptable, whether a content
  length is required for progress UX, and the legally/technically supported source
  matrix for the migration target.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`,
  `project_brain.md`, `mobile-app/project_brain.md`, `downloader.py`, `library_tools.py`,
  `engine.py`, `music_player.py`, `views/downloader.py`, Android `DownloadWorker.kt`,
  `PlaylistSyncWorker.kt`, `WebSearchService.kt`, `DownloadRepository.kt`,
  `DownloadJob.kt`, `DownloadJobDao.kt`, `SearchScreen.kt`, `DownloadsScreen.kt`,
  `Playlist.kt`, `PlaylistRepository.kt`.
- Tests/fixtures inspected: `test_downloader_sync.py`, `test_dl.py`,
  `mobile-app/app/src/test/java/com/luno/mobile/data/repository/DownloadRepositoryTest.kt`,
  `mobile-app/app/src/test/java/com/luno/mobile/data/db/DownloadJobDaoTest.kt`.
- Claims requiring review: Android source-native format versus desktop FFmpeg
  normalization; Android concurrent Sync All versus desktop sequential behavior;
  direct-download output validation; Android CSV parser compatibility; marker-file
  portability and URL Queue durability.
- Coverage gaps: No Android worker/network tests, no Android playlist-sync tests,
  no desktop direct-download output test, no Android CSV parser tests, no real
  yt-dlp/FFmpeg or device/SAF integration fixture, and no crash/restart test for
  marker or WorkManager persistence.
- Status: complete

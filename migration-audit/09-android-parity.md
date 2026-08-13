# Android Parity Audit

This dossier audits the native Android application as an implemented product
and compares it with the implemented Python desktop boundary. Android and
desktop are separate runtimes. The explicit interoperability boundary is the
path-free `luno.library.export` v1 manifest, not shared application code
(`migration-audit/00-repository-census.md:15-34`).

## Application Lifecycle And Runtime Boundary
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/LunoApp.kt:36-89,118-188,211-227`
  - `mobile-app/app/src/main/java/com/luno/mobile/MainActivity.kt:18-74`
  - `mobile-app/app/src/main/AndroidManifest.xml:5-61`
- User behavior: Android starts a manual-singleton `Application`, initializes Room, repositories, process-lifetime import/artwork/preview managers, WorkManager configuration, notification channel, NewPipe, and crash capture. `MainActivity` owns one `MusicController`, attaches it to the process preview manager, renders Compose, handles notification opens, and releases the controller on destruction.
- State and data flow: Application-owned repositories and `appScope` outlive composable navigation; activity-owned controller connects to the manifest-declared `MusicService` and exposes StateFlows to the shell.
- Persistence and compatibility: Playback restore is service-owned SharedPreferences; recently played and settings use app-private preferences. This is Android-local state, not a desktop file/config migration.
- Async, lifecycle, and failure behavior: `MusicController` is single-use and terminal after release, with stale-future guards and pending-play behavior (`MusicController.kt:31-53,125-140,240-280`). `MusicService` persists playback and releases MediaSession before ExoPlayer (`MusicService.kt:146-168`).
- UI states and visual requirements: The Android shell is Compose/Material3 and uses a modal drawer plus bottom navigation; desktop Tk event-loop constraints do not apply.
- Migration implementation requirements: Preserve the target runtime's lifecycle ownership and never assume desktop `pygame` or Tk state can be reused.
- Verification requirements: Run the Android Gradle gate and an instrumented lifecycle/service test on a device or emulator.
- Confidence: high
- Open questions: Instrumented tests document an API 34+ requirement (`MusicControllerInstrumentedTest.kt:34-40`), while the configured minimum is API 29; API 29-33 service behavior is not established by committed device evidence.

## Room Schema And Persistence
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/AppDatabase.kt:17-25,30-119`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/entity/Track.kt:8-22`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/entity/Playlist.kt:6-20`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/entity/DownloadJob.kt:6-30`
  - `mobile-app/app/schemas/com.luno.mobile.data.db.AppDatabase/1.json`
  - `mobile-app/app/schemas/com.luno.mobile.data.db.AppDatabase/9.json`
- User behavior: Android persists tracks, playlists, ordered playlist membership, download jobs, artwork paths, play counts, playlist URLs, multi-playlist destinations, and favorites. Favorites are a derived virtual playlist route (`NavGraph.kt:169-195`), not a physical folder.
- State and data flow: Room v9 is opened as `luno.db`; eight explicit migrations add downloads, playlist URL, artwork, counts, destination IDs, and favorite state.
- Persistence and compatibility: Android schema history is independently migratable. The shared transfer contract excludes local paths, audio, queue, history, and favorites (`contracts/library-export/v1/schema.json:1-53`; `LibraryManifestCodec.kt:16-80`).
- Async, lifecycle, and failure behavior: DAO flows feed the warmed `LibraryData`; repository and worker transactions insert tracks and memberships atomically (`DownloadWorker.kt:264-282`).
- UI states and visual requirements: Room-backed library/download/playlist screens render persisted state; no desktop `_library` dictionary is used.
- Migration implementation requirements: If a future desktop migration consumes Android exports, preserve v1 validation and additive semantics, not Room table layout.
- Verification requirements: Run DAO tests, manifest codec tests, and migration tests against every checked-in schema version. `AppDatabaseTest.kt:14-36` verifies an in-memory database but does not itself enumerate historical migrations.
- Confidence: high
- Open questions: No committed test was found that opens each on-disk schema 1 through 8 and applies every migration; schema files establish snapshots, not complete migration execution coverage.

## SAF Storage And Discovery
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/LibraryRepository.kt:51-107,139-225,251-319`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/MusicFolderRepository.kt:9-88`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MainShell.kt:247-300`
  - `mobile-app/app/src/androidTest/java/com/luno/mobile/storage/LibrarySmokeTest.kt:18-116`
  - `mobile-app/app/src/test/java/com/luno/mobile/data/repository/LibraryRepositoryTest.kt:29-179`
- User behavior: Users select audio files/folders through SAF. Tree imports recurse folders into playlists, put root files in `Unsorted`, reject folders/non-audio documents as tracks, persist read grants, deduplicate URI/document IDs, report errors, and extract metadata off the UI thread. A selected music-folder tree is also the download destination and is persisted in preferences (`MusicFolderRepository.kt:14-24`).
- State and data flow: Content URIs become Room `Track` primary keys; folder imports create or reuse playlist rows and membership rows. Downloaded app-private files can later be copied to SAF and their URIs reconciled (`DownloadRepository.kt:310-360`).
- Persistence and compatibility: SAF grants and the chosen destination are Android-specific. The repository reports revoked/unavailable grants instead of silently treating storage as empty.
- Async, lifecycle, and failure behavior: Import work uses IO dispatchers, bounded parallelism, per-operation timeouts, recursion guards, and per-file error containment (`LibraryRepository.kt:241-319`).
- UI states and visual requirements: The process-lifetime `MusicFolderImportManager` owns progress and completion messaging; the drawer is the launcher. The source does not show a desktop-style periodic filesystem watchdog.
- Migration implementation requirements: Desktop migration must retain its path-based scan semantics separately; Android content URI identity and grant recovery cannot be represented as desktop paths without a policy decision.
- Verification requirements: Keep fake DocumentsProvider coverage for nested folders, Samsung/AOSP URI forms, duplicate document IDs, invalid audio, empty folders, and revoked access.
- Confidence: high
- Open questions: Real provider behavior after reinstall or SD-card ejection is not covered by the committed tests; only injectable/fake provider and FileProvider scenarios are present.

## Media3 Playback And Queue
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/MusicService.kt:19-80,142-168,171-260`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/MusicController.kt:31-53,75-140,159-235,240-280`
  - `mobile-app/app/src/androidTest/java/com/luno/mobile/playback/MusicControllerInstrumentedTest.kt:34-40,61-139`
  - `mobile-app/app/build.gradle.kts:131-134`
- User behavior: Android plays local/content/file URIs through ExoPlayer and MediaSession, supports notification/lock-screen controls, headset-noisy handling, queue state, repeat, shuffle, recent history, queue reordering, pending play, and playback error reporting.
- State and data flow: `MusicController` translates `MediaTrack` values to Media3 items and StateFlows; `MusicService` owns the player/session and persists the current non-transient item and position.
- Persistence and compatibility: Android playback restore is metadata plus URI in SharedPreferences. Desktop uses `LunoEngine` and pygame queue state (`engine.py:26-72`; `project_brain.md:146-157`), so queue persistence/lifecycle is not a shared interface.
- Async, lifecycle, and failure behavior: Media3 connection futures are guarded against release races; decoder failures emit sanitized errors. `onTaskRemoved` stops only when no media item remains (`MusicService.kt:146-152`).
- UI states and visual requirements: Routes include full player, queue/recents, mini-player, playlist details, and recommendation previews (`NavGraph.kt:30-44,140-197`).
- Migration implementation requirements: Port observable queue semantics and failure states, not the Android Media3 service implementation. Desktop's documented 56 px virtual rows remain a desktop-only constraint.
- Verification requirements: Unit tests cover pending-play, queue, lifecycle, and errors; instrumented tests cover real service connection and playback. A real audio-device matrix remains necessary.
- Confidence: high
- Open questions: Android has no source evidence for desktop crossfade, and the product boundary explicitly excludes crossfade/sleep timer/lyrics/visualizer (`mobile-app/project_brain.md:256-262`).

## WorkManager Downloads And Sync
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/DownloadRepository.kt:40-126,188-307,467-565`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/DownloadWorker.kt:30-51,54-107,121-169,245-311`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/PlaylistSyncWorker.kt:20-157`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/entity/DownloadJob.kt:14-30`
- User behavior: Search/direct URL/CSV/playlist flows queue durable jobs. WorkManager applies connected-network constraints, foreground notifications, progress, cancellation, exponential retry up to three attempts, unique job names, collision-safe filenames, SAF destination copying, artwork sidecars, atomic Room completion, retry/failure states, and stop-all tags.
- State and data flow: `DownloadJob` stores source, metadata, stream URL, destination playlist IDs, thumbnail, WorkManager ID, and terminal state. `PlaylistSyncWorker` extracts playlist videos, skips existing/queued candidates, resolves audio, and enqueues individual `DownloadWorker` jobs.
- Persistence and compatibility: Android downloads preserve source URLs and content/file URIs locally. Desktop instead uses yt-dlp plus FFmpeg to produce normalized 192k MP3 at 44.1 kHz with loudnorm (`downloader.py:58-96`; `project_brain.md:307-320`). Android writes extractor-native `.m4a`, `.opus`, `.mp3`, `.ogg`, or `.audio` based on response content type (`DownloadWorker.kt:134-142`) and does not reproduce the desktop transcode/loudnorm pipeline.
- Async, lifecycle, and failure behavior: WorkManager survives activity navigation/process scheduling; worker cleanup compensates staged and SAF outputs on failures (`DownloadWorker.kt:286-323`). Network extraction is Android synchronous worker work, not Tk daemon-thread work.
- UI states and visual requirements: Search is the bottom-nav Download workspace; full queue management is a separate Downloads route in the drawer (`NavGraph.kt:104-112,140-142`; `MainShell.kt:144-149`).
- Migration implementation requirements: Treat Android acquisition as Android-only implementation and preserve the desktop's yt-dlp/FFmpeg output contract independently. Do not claim codec parity.
- Verification requirements: Keep `DownloadRepositoryTest.kt`, `DownloadJobDaoTest.kt`, and playlist sync tests; add worker integration tests for HTTP content types, retries, SAF copy rollback, and process restart.
- Confidence: high
- Open questions: No committed Android worker test exercises a real WorkManager execution or provider destination failure. SoundCloud search is absent; only direct URLs are supported by the documented Android boundary (`mobile-app/project_brain.md:348`).

## Discovery, Artwork, And Transfer
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/DiscoveryRepository.kt:1-120`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/discovery/LastfmService.kt:1-120`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/discover/DiscoverScreen.kt:1-180`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/LibraryTransferRepository.kt:1-180`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/export/ExportImportScreen.kt:1-160`
  - `mobile-app/app/src/test/java/com/luno/mobile/data/discovery/LastfmServiceTest.kt`
  - `mobile-app/app/src/test/java/com/luno/mobile/data/repository/LibraryTransferRepositoryTest.kt`
- User behavior: Android implements Last.fm recommendations with encrypted key storage, library-owned filtering, artwork fallback, temporary recommendation previews, explicit playlist promotion, and additive SAF JSON export/import with preview/ambiguity handling and missing YouTube download queueing.
- State and data flow: Discovery is repository/service/UI; artwork is cached under app-private files. Transfer uses the shared v1 codec and Android Room/WorkManager after validation.
- Persistence and compatibility: The manifest is the actual desktop/mobile parity boundary. Both apps implement the format independently; Android does not import desktop config, history, favorites, queue, or local paths.
- Async, lifecycle, and failure behavior: Recommendation preview work is process-scoped and bounded; transfer/download work is separate from route composition. Tests cover codec rejection, transfer ordering, and repository planning.
- UI states and visual requirements: Discover, full player recommendations, export/import, and artwork states are Android routes/components; desktop has separate Discover and Song Page implementations (`views/discover.py`, `views/song_page.py`).
- Migration implementation requirements: Preserve manifest v1 exactly. Treat recommendation caching, artwork caches, and preview files as platform-local behavior.
- Verification requirements: Run codec, discovery, artwork, preview-manager, and transfer tests plus the shared desktop contract fixture tests.
- Confidence: high
- Open questions: Provider-ID matching breadth and real Last.fm/Deezer/YouTube availability remain external-service risks; source/tests do not establish production network reliability.

## Routes And Feature Boundary
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/navigation/NavGraph.kt:30-44,62-197`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MainShell.kt:138-149,156-180`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/library/DuplicateScreen.kt`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/player/RecentsScreen.kt`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/export/ExportImportScreen.kt`
- User behavior: Implemented Android routes are Home, Download/Search, Library, Discover, Downloads, Recents, Duplicates, full player, Made-for-you, export/import, and playlist detail. The bottom bar currently has four items: Home, Download, Your Library, Discover; Create is opened as a sheet callback, not a navigation route (`MainShell.kt:144-149`; `NavGraph.kt:30-44`).
- State and data flow: `MainShell` supplies controller and app repositories to `LunoNavHost`; drawer-only routes remain reachable without adding bottom-nav destinations.
- Persistence and compatibility: Android route names are not a desktop navigation API.
- Async, lifecycle, and failure behavior: Route changes are covered by a shell transition mask; library/discovery loading gates its reveal (`MainShell.kt:247-300`).
- UI states and visual requirements: Android uses touch-sized Compose controls and lazy lists; desktop uses Tk views and a documented 56 px virtual row contract.
- Migration implementation requirements: The current source, not the brain's stale five-item description (`mobile-app/project_brain.md:395`), is authoritative. Desktop-only Duplicates/Allowed Dups, URL Queue, and richer downloader tree workflows are not evidence of Android parity; Android has duplicate metadata removal and download management but not those desktop backup/tree surfaces.
- Verification requirements: Verify every route manually or with Compose navigation tests, including drawer-only routes and back-stack behavior. Current committed tests are primarily unit/repository tests; no Compose route test was found.
- Confidence: high
- Open questions: Whether Create should become a fifth bottom destination is a documentation/product question; source currently implements only the sheet.

## Explicitly Desktop-Only, Android-Only, Deferred, And Unknown
- Status: implemented
- Evidence:
  - `project_brain.md:112-128,165-190,212-219,257-284`
  - `mobile-app/project_brain.md:238-262,348-360`
  - `views/downloader.py`, `views/settings.py`, `views/error_log.py`, `views/song_page.py`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/update/GitHubReleaseService.kt`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/update/ApkInstaller.kt`
- User behavior: Desktop-only evidence includes Tk/CVE shell, filesystem-root library scanning, yt-dlp/FFmpeg normalization, SoundCloud search, desktop duplicate allow-list export/import, URL Queue/tree download workflows, global media keys, video previews, and desktop settings/config persistence. Android-only evidence includes MediaSession notifications/lock-screen integration, SAF content URI/grant handling, Room/WorkManager persistence, Android update/sideload code, crash-log file capture, and Android's four-tab Compose shell.
- State and data flow: These are independent platform implementations. Only the v1 manifest crosses the boundary. Android's vendor extractor and HTTP stream fallbacks are not shared desktop dependencies.
- Persistence and compatibility: Deferred/excluded product features explicitly include live sync/pairing/cloud accounts/remote playback, podcasts/audiobooks, sleep timer, lyrics, visualizer, crossfade, and equalizer (`mobile-app/project_brain.md:256-262`). They must remain classified deferred/excluded, not inferred from planned roadmap text.
- Async, lifecycle, and failure behavior: Desktop uses Tk `.after()` marshaling and daemon/worker coordination; Android uses coroutines, Media3 service lifecycle, and WorkManager. No cross-runtime lifecycle guarantee exists.
- UI states and visual requirements: Android has no source route for desktop Error Log parity as a user-facing route, although `LunoApp` writes `crash_log.txt` and the drawer exposes an error-log action. Desktop Song Page video preview has no Android equivalent in the inspected source.
- Migration implementation requirements: A future migration must inventory desktop behavior first, then selectively port behavior through a new target architecture. Android implementation is a parity reference, not a replacement or complete feature superset.
- Verification requirements: Maintain separate desktop pytest and Android Gradle gates; test the manifest fixtures in both environments.
- Confidence: high
- Open questions: Manual UI parity, performance on large libraries, storage-provider diversity, audio-device behavior, and production extractor availability are not fully established by source/unit tests.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`, `project_brain.md`, `mobile-app/project_brain.md`, `mobile-app/app/build.gradle.kts`, `mobile-app/settings.gradle.kts`, `mobile-app/gradle/libs.versions.toml`, `mobile-app/app/src/main/AndroidManifest.xml`, `LunoApp.kt`, `MainActivity.kt`, `MusicService.kt`, `MusicController.kt`, `AppDatabase.kt`, Room entities, `LibraryRepository.kt`, `MusicFolderRepository.kt`, `DownloadRepository.kt`, `DownloadWorker.kt`, `PlaylistSyncWorker.kt`, `WebSearchService.kt`, `DiscoveryRepository.kt`, `LastfmService.kt`, `LibraryTransferRepository.kt`, `NavGraph.kt`, `MainShell.kt`, update services, selected desktop brains/views/source files, and checked-in Room schemas.
- Tests/fixtures inspected: Android DAO/repository/discovery/artwork/transfer/codec/controller/preview/UI tests, `LibrarySmokeTest.kt`, `MusicControllerInstrumentedTest.kt`, `contracts/library-export/v1/schema.json`, and desktop downloader/playback/discovery/transfer references.
- Claims requiring review: The mobile brain's five-item navigation description conflicts with current four-item source; Android worker and historical Room migration execution coverage is incomplete; production provider/extractor/network behavior is not proven by local tests.
- Coverage gaps: No device run was performed in this audit; no API 29-33 matrix; no real SD/ejection/reinstall grant test; no end-to-end WorkManager/SAF destination failure test; no Compose route test; no Android SoundCloud search implementation; no Android equivalent of desktop video preview, duplicate allow-list backups, or full desktop downloader auxiliary workflows.
- Status: complete

# Song Page, Artwork, and Video Audit

This dossier audits the implemented Python desktop behavior and the native
Android parity surface. The migration target remains the desktop product;
Android-only details are recorded as parity evidence, not as shared runtime
behavior.

## Desktop Song Page And Recommendations
- Status: implemented
- Evidence:
  - `views/song_page.py:212-395` (`SongPageView`, full-player layout, favorite and recommendation controls)
  - `views/song_page.py:399-419,1120-1240` (page-level scrolling, centering, responsive media stage)
  - `views/song_page.py:421-1110` (Last.fm seed, paging, retry/empty/error states, artwork rows and discover controls)
  - `engine.py:152-166,331-342` (engine-to-Song-Page updates and synchronized scrub display)
- User behavior: Clicking the player track opens a full Song Page with artwork,
  title/artist, favorite control, playback controls, and a `Recommended for
  this song` section. The page is one scrollable column; content is centered
  when it fits and scrollable when it does not. Recommendations are seeded by
  the current track, use the queue playlist/folder as artist fallback, load in
  pages of 25 up to 1,000, and expose refresh, retry, empty, missing-key, and
  discover-mode states.
- State and data flow: The page reads `engine.queue` and `queue_idx`, derives
  metadata with `split_track_name`, requests recommendations through the
  controller, and renders results asynchronously on Tk's event loop. Active
  preview sessions retain their seed rather than re-seeding from transient
  recommendation tracks (`views/song_page.py:423-1110`; controller/session
  behavior is also exercised in `test_discover_mode.py:294-885`).
- Persistence and compatibility: Recommendations and preview media are
  temporary session data. Video files are stored under the controller's
  `PREVIEW_DIR` (`~/.luno_previews` by default), never added to `_library` or
  the audio queue (`video_preview.py:1-4,140-147`).
- Async, lifecycle, and failure behavior: Recommendation/network work uses
  bounded worker paths and UI callbacks; page generations reject stale blur
  results. Page destruction cancels scheduled callbacks, stops video and
  fullscreen playback, cancels artwork work, shuts down executors, and removes
  key bindings (`views/song_page.py:1316-1362`).
- UI states and visual requirements: Responsive artwork/media sizing reserves
  room for recommendations, keeps compact typography on narrow windows, and
  maintains the blurred/dimmed artwork background with a dark fallback
  (`views/song_page.py:1146-1192,1269-1335,1705-1766`).
- Migration implementation requirements: Preserve page-level scroll and
  scroll-position semantics, recommendation seed stability, 25-item paging,
  1,000-item cap, honest loading/error/empty states, and temporary preview
  isolation. Do not treat Android Compose implementation as a shared API.
- Verification requirements: Test recommendation paging/session handoff and
  retry behavior; test UI-thread callback generation guards and responsive
  scroll behavior with a Tk-capable harness.
- Confidence: high
- Open questions: Full desktop UI geometry and accessibility behavior are not
  covered by deterministic tests.

## Desktop Artwork Extraction, Cache, And Fallback
- Status: implemented
- Evidence:
  - `music_player.py:104-310` (`ArtCache`, bounded LRU/pixel/in-flight behavior and Tk callback delivery)
  - `music_player.py:876-1036` (embedded APIC extraction, Deezer lookup, YouTube fallback and logging)
  - `views/song_page.py:1242-1267,1285-1335` (last-good art retention and background blur generation)
  - `test_art_cache.py` (bounded cache regression coverage)
- User behavior: Embedded MP3 artwork is preferred. Missing artwork can be
  fetched from Deezer and then a YouTube thumbnail fallback; invalid/missing
  art displays a music-note placeholder. Song Page and other desktop views use
  a shared bounded cache, while recommendation thumbnails use a separate
  bounded cache/service.
- State and data flow: A cache miss queues filesystem/Mutagen/Pillow work off
  the Tk thread and returns a `PhotoImage` through `controller.after(0, ...)`.
  Cache entries are keyed by path and requested size; stale paths are pruned
  after library refresh (`music_player.py:174-310`). Background blur uses one
  executor and generation/signature checks, preserving the current image while
  a refetch is pending (`views/song_page.py:1242-1335`).
- Persistence and compatibility: Desktop art is embedded in audio files or
  fetched for display; no separate cross-platform artwork contract exists.
  Recommendation art designs are deterministic and temporary (`rec_artwork.py`;
  `views/song_page.py:1031-1059`).
- Async, lifecycle, and failure behavior: Oversized/invalid embedded payloads
  are rejected by the cache; fetch failures are logged and degrade to
  placeholders. Worker completion is guarded against stale paths and destroyed
  Tk widgets.
- UI states and visual requirements: Crop/fill is used without distortion for
  artwork, with a blurred glass background and last-good-image retention to
  avoid flashing during refresh or resize.
- Migration implementation requirements: Preserve bounded memory behavior,
  worker/UI boundary, APIC validation, source priority, deterministic
  recommendation fallback art, and stale-result rejection.
- Verification requirements: Run `test_art_cache.py`; add coverage for APIC
  rejection, Deezer failure, YouTube fallback, and Song Page stale-generation
  behavior if the target UI supports these paths.
- Confidence: high
- Open questions: The desktop fetch sweep's exact network timeout and retry
  policy is not fully covered by focused tests.

## Desktop Video Probe, Staging, And Decode
- Status: implemented
- Evidence:
  - `video_preview.py:50-107` (three query variants, up to ten results each, deduplication, relevance and canonical URL)
  - `video_preview.py:110-147` (yt-dlp MP4 staging and cache lookup)
  - `views/song_page.py:1364-1461,1491-1604` (probe/download/adopt/preload state machine)
  - `views/song_page.py:26-210` (`_FFmpegVideoPlayer`, timestamped decode buffer and cleanup)
  - `test_video_preview.py:28-53` (relevance and canonical URL tests)
- User behavior: The Song Page Video control probes before downloading. It can
  adopt a staged current video, download a merged temporary MP4, preload the
  current/next track, or show unavailable/FFmpeg-missing status. Preload paths
  have terminal `ready`/`unavailable` states and do not retry forever.
- State and data flow: yt-dlp searches `artist - title`, `title artist`, and
  title-only, deduplicates IDs, requires a title match and artist evidence when
  available, then builds `youtube.com/watch?v=ID`. Download output is
  `bestvideo[ext=mp4]+bestaudio[ext=m4a]` with MP4 merge into `PREVIEW_DIR`.
  The preview file is independent of the local audio queue/library.
- Persistence and compatibility: Staged video filenames are derived from the
  same artist/title fallback used for lookup and adoption. They are disposable
  presentation assets, not library media.
- Async, lifecycle, and failure behavior: Probe and download run in daemon
  threads and marshal results with `.after(0, ...)`; transient probe errors
  retry once. Missing FFmpeg, no relevant source, failed download, and stale
  track changes leave normal artwork playback intact. Decoder termination is
  bounded and fullscreen stopping is moved off the Tk thread
  (`views/song_page.py:1666-1680`).
- UI states and visual requirements: Video is aspect-fitted with letterboxing
  in a responsive 16:9 stage. Resize stops/restarts decoding at the new size;
  staged adoption waits for layout realization, avoiding square-first output.
- Migration implementation requirements: Port the relevance rules, staging
  filename/key, terminal preload states, retry policy, temporary-directory
  isolation, FFmpeg process ownership, and fallback to artwork. Make external
  tool availability explicit in UI.
- Verification requirements: Preserve `test_video_preview.py`; add process-level
  tests for cancellation, resize, decode EOF, and stale adoption where the
  target runtime permits.
- Confidence: high for source probing/staging; medium for real FFmpeg/Tk
  rendering because no deterministic renderer fixture exists.
- Open questions: Legal/provider availability and the target migration's video
  codec/tool distribution remain deployment decisions.

## Desktop Audio Clock, Pause, Seek, Resize, And Fullscreen
- Status: implemented
- Evidence:
  - `engine.py:215-236,295-329,344-361` (pause/resume, canonical position, seek)
  - `views/song_page.py:60-106,143-169,183-210` (audio-clock presentation and decoder restart)
  - `views/song_page.py:1146-1185` (resize/restart)
  - `views/song_page.py:1407-1417,1642-1680` (F/Escape fullscreen and cleanup)
- User behavior: Pause halts audio and video decoding; resume continues from the
  same position. Scrubbing seeks audio and explicitly interrupts the active
  decoder. Resizing changes the media stage and restarts decoding at the audio
  position. F opens fullscreen and Escape or window close exits it.
- State and data flow: `pygame.mixer.music.get_pos()` is preferred while busy,
  with monotonic fallback and duration clamping. `_FFmpegVideoPlayer` consumes
  `engine.get_position()` and only presents decoded frames whose timestamps are
  due; a position jump over 0.2 seconds restarts the decoder and clears its
  buffer.
- Persistence and compatibility: Video state is session-only. Audio queue,
  duration, pause state, and playback position remain engine state; video never
  becomes queue state.
- Async, lifecycle, and failure behavior: Decoder and Tk presentation loops are
  separately stopped; subprocesses are terminated/killed if needed. Fullscreen
  destroys its window before asynchronous decoder stop, preventing a black or
  frozen stranded window.
- UI states and visual requirements: Presentation polling is only a 5 ms Tk
  clock check; frame timing comes from measured source FPS and timestamps, not a
  fixed video timer. The media stage is bounded to available content width and
  responsive 16:9 proportions.
- Migration implementation requirements: Use one authoritative audio clock;
  never let a separate video timer drift. Explicitly handle pause, seek, track
  handoff, resize, fullscreen exit, process EOF, and app destruction.
- Verification requirements: Existing playback tests cover deterministic engine
  clock/pause/seek behavior (`test_engine_playback.py:196-630`); renderer tests
  remain a gap and should use fake decoder streams/processes.
- Confidence: high for engine and lifecycle source; medium for visual timing.
- Open questions: Pygame codec-specific clock accuracy and cross-platform
  FFmpeg packaging need acceptance testing.

## Android Full Player, Recommendations, And Artwork
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/player/FullPlayerScreen.kt:127-250,507-779` (full player, seed/fetch state, artwork, scrub and transport)
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/components/ArtworkImage.kt:42-145` (Coil loading, decode cap, placeholder and gradient)
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/components/RecommendationArtwork.kt:120-140` (stable fallback art)
  - `mobile-app/app/src/main/java/com/luno/mobile/data/artwork/ArtworkStorage.kt:12-120` (embedded extraction/cache/validation)
  - `mobile-app/app/src/main/java/com/luno/mobile/data/artwork/ArtworkFetchService.kt:18-121` (Deezer fetch/fallback/cache)
  - `mobile-app/app/src/test/java/com/luno/mobile/data/artwork/ArtworkFetchServiceTest.kt:60-100`
  - `mobile-app/app/src/test/java/com/luno/mobile/data/repository/LibraryRepositoryArtworkFetchTest.kt:81-242`
- User behavior: Android's full player has artwork-driven gradient, metadata,
  favorite, queue/actions, scrubber, transport controls, and a scrollable
  recommendation section. Recommendations support loading, API-key, error,
  empty, refresh, paging, temporary preview, and explicit playlist-save states.
- State and data flow: Room track metadata/artwork paths feed `MediaTrack`; the
  full player fetches Last.fm results through `DiscoveryRepository`, filters
  owned tracks, and uses `RecommendationPreviewManager` for cache-backed
  temporary audio. Artwork is extracted with `MediaMetadataRetriever`, reduced
  to a validated <=512 px JPEG in `filesDir/artwork`, and rendered by Coil.
  Missing artwork searches Deezer with combined/title/artist queries; downloaded
  thumbnails are also cached. Media-session artwork bytes are loaded off the
  service thread for notification/lock-screen surfaces
  (`MusicService.kt:83-138`).
- Persistence and compatibility: Artwork paths are persisted on Room `Track`;
  temporary recommendation audio lives under app `cacheDir` and is promoted
  only through an explicit playlist save. Android uses Room rather than desktop
  JSON artwork state.
- Async, lifecycle, and failure behavior: Artwork/network work uses IO
  coroutines and invalid/deleted cache entries are refilled. `MusicService`
  releases MediaSession before ExoPlayer and shuts down its artwork executor
  (`MusicService.kt:154-168`). Controller seek clamps to duration and updates
  progress immediately (`MusicController.kt:435-451`).
- UI states and visual requirements: Coil shows custom missing/loading/error
  content, thumbnails use decode caps, fallback recommendation designs are
  deterministic, and dominant-color gradients animate over 600 ms.
- Migration implementation requirements: Preserve the observable full-player
  and recommendation behavior, artwork source priority/validation, durable
  cache lifecycle, notification artwork enrichment, and Media3 pause/seek
  semantics. These are Android parity references, not desktop dependencies.
- Verification requirements: Run the Android unit gate, including artwork,
  recommendation, preview-manager, and controller tests.
- Confidence: high for implemented source and focused tests; medium for
  device-specific Coil/Media3 rendering because no device receipt is tracked.
- Open questions: Full-player Compose UI has no dedicated screenshot/golden
  tests in the repository.

## Android Video And Fullscreen Parity Gap
- Status: unknown
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/player/FullPlayerScreen.kt:127-779` contains artwork/audio controls but no video surface or video control.
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/WebSearchService.kt:19-87` implements YouTube search/audio metadata and thumbnail URLs, not video staging or decoding.
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/RecommendationPreviewManager.kt:97-128` stages temporary audio only.
  - `mobile-app/app/build.gradle.kts:8-180` contains Media3/Coil/OkHttp dependencies and no FFmpeg/video-decoder dependency.
  - `mobile-app/app/src/test/java/com/luno/mobile/playback/RecommendationPreviewManagerTest.kt` covers temporary audio recommendation behavior, not video.
- User behavior: Android can search YouTube and download/play audio previews,
  and can show fetched/embedded artwork. No source establishes Android Song
  Page video probing, MP4 staging, FFmpeg decoding, audio-clock frame sync,
  resize/restart, or fullscreen controls.
- State and data flow: Android `thumbnailUrl` is artwork data for downloaded
  audio; it is not a video asset. No Android video path, decoder state, or
  fullscreen lifecycle state was found.
- Persistence and compatibility: No Android video cache contract or cleanup
  policy is implemented. The desktop `~/.luno_previews` and Android
  `cacheDir/recommendation_previews` are separate and must not be conflated.
- Async, lifecycle, and failure behavior: Unknown for Android video. Existing
  Media3 audio lifecycle cannot establish video behavior.
- UI states and visual requirements: Unknown. The Android full player has no
  evidence for a video stage, letterboxing, fullscreen, or FFmpeg-unavailable
  fallback.
- Migration implementation requirements: Treat desktop video as an explicit
  Android parity gap. If Android video is required, define a permitted decoder,
  source/staging policy, cache cleanup, audio-clock synchronization, pause/seek
  restart, resize/fullscreen lifecycle, and tests before implementation. Do not
  claim NewPipe audio extraction or thumbnails provide this feature.
- Verification requirements: Add Android source/tests for the chosen video
  pipeline and manual/device checks for pause, seek, rotation/resize,
  background/foreground, fullscreen exit, decoder failure, and cache cleanup.
- Confidence: high that no Android implementation is present in the inspected
  tree; low only for untracked/external code outside this repository.
- Open questions: Whether Android video is in migration scope, and which
  legally/permitted on-device decoder/source is acceptable, are unresolved.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`, `project_brain.md`, `mobile-app/project_brain.md`, `views/song_page.py`, `video_preview.py`, `music_player.py`, `engine.py`, `rec_artwork.py`, `mobile-app/app/src/main/java/com/luno/mobile/ui/player/FullPlayerScreen.kt`, `ArtworkImage.kt`, `RecommendationArtwork.kt`, `ArtworkStorage.kt`, `ArtworkFetchService.kt`, `MusicService.kt`, `MusicController.kt`, `WebSearchService.kt`, `RecommendationPreviewManager.kt`, Android Gradle configuration.
- Tests/fixtures inspected: `test_video_preview.py`, `test_art_cache.py`, `test_engine_playback.py`, `test_discover_mode.py`, Android `ArtworkFetchServiceTest.kt`, `LibraryRepositoryArtworkFetchTest.kt`, `MusicControllerTest.kt`, `RecommendationPreviewManagerTest.kt`.
- Claims requiring review: Android video is absent based on the tracked source/dependency tree; exact desktop FFmpeg packaging and cross-platform codec behavior remain deployment concerns.
- Coverage gaps: No deterministic Tk/FFmpeg renderer tests, no Android video tests because no Android video implementation exists, and no device/manual fullscreen or resize receipt.
- Status: complete

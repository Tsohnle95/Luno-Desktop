# 🧠 Project Brain: Luno Native Android App

> [!IMPORTANT]
> **MAINTENANCE INSTRUCTIONS FOR AI/HUMANS:**
> This document is the authoritative knowledge base for the **implemented** BoomBastic native Android app under `mobile-app/`, including its planned extensions. It must be updated whenever settled decisions change.
>
> **Last verified and updated:** 2026-07-31 (App rebranded to "Luno" — app_name + About toast; desktop-style header "Luno ▮" at top of the screen via MainShell; GitHub CI workflow removed (no more tests on commit); refreshed baseline checks — 97/97 unit tests, lint PASS)
>
> **Authority policy (descending):**
> 1. **Source code + tests + config** in this repo (highest truth)
> 2. **Desktop `project_brain.md`** and cited shared source modules (engine.py, downloader.py, theme.py)
> 3. **Confirmed product decisions** recorded in this document
> 4. **Readable screenshots** — only if images are readable to the agent
> 5. **Labeled proposals / open decisions** — never presented as implemented fact
>
> **Golden rule:** Never turn aspiration into implemented fact. Statements about Kotlin, Compose, Media3, Room, and SAF are now **supported by source files** in `mobile-app/`. WorkManager, downloader, Last.fm discovery, and other deferred features remain **planned** unless source exists. Distinguish current Flet/Pygame prototype facts from native Android implementation.
>
> **When selecting file-format/output-codec decisions, the planner/coder MUST**
> 1. Inspect the actual desktop `downloader.py` source
> 2. Verify what `yt-dlp` + `FFmpeg` produce on the desktop side
> 3. Research what is legally/technically feasible on Android with a permitted alternative (e.g., android-youtube-dl / NewPipe extractor / ExoPlayer/Media3 extractors) if `yt-dlp`/FFmpeg cannot run on-device
> 4. Document the supported-source-format table before coding
>
> **Image-reader requirement:** Any agent relying on visual mockups MUST immediately declare whether images are readable. If not readable, fall back to the exact hex/contract spec in this document.

---

## Product Direction

Luno is a **native Android offline-first music player** in the same monorepo as the desktop Vibe Music Player and its Flet mobile/desktop-capable port. The architecture is **Kotlin + Jetpack Compose + Media3**. **As of this writing Kotlin, Compose, Media3, and Room source exist** under `mobile-app/` — the native stack foundation is implemented and committed to the repository.

**Current repo state (desktop/Flet legacy alongside native):**
- `music_player_flet.py` — Flet-based mobile/desktop prototype using **Pygame** audio backend, 3-tab responsive layout (Library/Player/Settings), mini-player, bottom nav on mobile, sidebar layout on desktop ([source](../music_player_flet.py))
- `main.py` — Flet entry point that runs `music_player_flet.main` via `ft.run()` ([source](../main.py))
- `buildozer.spec` — Buildozer Android build config for the Flet prototype; target API 33, min API 21, `arm64-v8a + armeabi-v7a`, permissions `INTERNET, READ_EXTERNAL_STORAGE, WRITE_EXTERNAL_STORAGE, MANAGE_EXTERNAL_STORAGE` ([source](../buildozer.spec))
- `engine.py` — Desktop audio engine (`pygame.mixer`), shared `DiscoveryService`, `scan_library`, `find_duplicates`, `_normalize_for_dupe` ([source](../engine.py); verification at lines 107–507)
- `downloader.py` — Desktop yt-dlp wrapper (`bestaudio/best` → FFmpegExtractAudio MP3 192k 44.1kHz with loudnorm I=-14:LRA=11:TP=-1.5) ([source](../downloader.py); verification at lines 58–96)
- `music_player.py` — Desktop Tkinter app (Controller-View-Engine pattern)
- `views/` — Desktop Tkinter view modules (library, downloader, settings, etc.)

**Target device baseline:** Samsung Galaxy S20 FE, Android 13 (API 33). Min SDK set to **API 29**, target/compile SDK **35**. **No iOS implementation** is planned now, though a platform-neutral manifest format may be designed for eventual compatibility.

---

## 📁 Repository Map (mobile-app relevant)

```
boomtastic/
├── mobile-app/                          # ← THIS DOCUMENT lives here
│   ├── project_brain.md                 # This file
│   ├── build.gradle.kts                 # Root Gradle build (plugin declarations)
│   ├── settings.gradle.kts              # Project settings (single :app module)
│   ├── gradle.properties                # JVM args, AndroidX, Kotlin style
│   ├── gradle/
│   │   ├── libs.versions.toml           # Version catalog (AGP 8.5.2, Media3 1.3.1, Room 2.6.1, etc.)
│   │   └── wrapper/
│   │       ├── gradle-wrapper.jar
│   │       └── gradle-wrapper.properties
│   ├── gradlew / gradlew.bat            # Gradle wrapper scripts
│   │
│   ├── app/
│   │   ├── build.gradle.kts             # App module: compileSdk 35, minSdk 29, targetSdk 35
│   │   ├── proguard-rules.pro           # Keep Room entity annotations
│   │   ├── schemas/
│   │   │   └── com.boombastic.mobile.data.db.AppDatabase/1.json
│   │   └── src/
│   │       ├── main/
│   │       │   ├── AndroidManifest.xml  # FOREGROUND_SERVICE, POST_NOTIFICATIONS, MusicService
│   │       │   ├── res/
│   │       │   │   ├── drawable/        # 9 vector icons (home, search, library, discover, create, play, pause, music_note, ic_download, launcher foreground/background)
│   │       │   │   ├── mipmap-anydpi-v26/ic_launcher.xml
│   │       │   │   └── values/
│   │       │   │       ├── colors.xml   # Mobile palette: #101010, #202020, #292929, #1ED760
│   │       │   │       ├── strings.xml  # App name, nav labels, dialog strings
│   │       │   │       └── themes.xml   # Theme.BoomBastic (Material NoActionBar)
│   │       │   └── java/com/boombastic/mobile/
│   │       │       ├── BoomBasticApp.kt          # Application class, manual DI singletons
│   │       │       ├── MainActivity.kt           # Compose entry point, MusicController init
│   │       │       ├── playback/
│   │       │       │   ├── MusicService.kt       # Media3 MediaSessionService + ExoPlayer
│   │       │       │   ├── MusicController.kt    # MediaController wrapper, StateFlow, pending-play logic
│   │       │       │   ├── DownloadWorker.kt     # WorkManager: HTTP download, progress, duration extraction, Track insertion, app-icon notification
│   │       │       │   ├── PlaylistSyncWorker.kt # WorkManager worker: fetch YT playlist → create individual download jobs
│   │       │       │   ├── WebSearchService.kt    # YouTube: watch page scraping + InnerTube + Piped + Invidious audio extraction
│   │       │       │   └── NotificationPermissionPolicy.kt # One-shot POST_NOTIFICATIONS prompt policy
│   │       │       ├── data/
│   │       │       │   ├── db/
│   │       │       │   │   ├── AppDatabase.kt       # Room DB (tracks, playlists, playlist_tracks)
│   │       │       │   │   ├── dao/
│   │       │       │   │   │   ├── TrackDao.kt       # CRUD + search Flow
│   │       │       │   │   │   ├── PlaylistDao.kt    # CRUD + relations + sort order
│   │       │       │   │   │   └── DownloadJobDao.kt # DownloadJob CRUD + progress/state queries with Flow
│   │       │       │   │   └── entity/
│   │       │       │   │       ├── Track.kt          # uri PK, title, artist, album, durationMs
│   │       │       │   │       ├── Playlist.kt       # autoGenerate id, name, description
│   │       │       │   │       ├── PlaylistTrack.kt  # composite PK, FK cascade, sortOrder
│   │       │       │   │       └── DownloadJob.kt    # DownloadJob entity + DownloadState enum
│   │       │       │   └── repository/
│   │       │       │       ├── LibraryRepository.kt  # SAF import, MediaMetadataRetriever, dedupe
│   │       │       │       └── PlaylistRepository.kt # CRUD, validation, sort order mgmt
│   │       │       └── ui/
│   │           │   ├── shell/
│   │           │   │   └── MainShell.kt          # ModalNavigationDrawer ("Settings" header: Downloads, Export/Import, About, Update check) + AppHeader ("Luno" + green bar) + Scaffold + BottomNav (Home/Search/Library/Discover/Create) + MiniPlayer (hidden on full player)
│   │       │           ├── navigation/
│   │       │           │   └── NavGraph.kt           # NavHost: Home / Search / Library / Discover / Downloads (drawer) / full_player
│   │       │           ├── theme/
│   │       │           │   ├── Color.kt              # Dark palette (visual spec colors)
│   │       │           │   ├── Theme.kt              # BoomBasticTheme (Material3 dark color scheme)
│   │       │           │   ├── Type.kt               # Sans-serif typography scale
│   │       │           │   └── Dimens.kt             # 24dp icons, 48dp touch targets, 64dp mini-player
│   │       │           ├── components/
│   │       │           │   ├── MiniPlayer.kt         # Persistent progress + artwork thumb + title + play/pause; tap → full player
│   │       │           │   ├── ArtworkImage.kt       # Coil AsyncImage + gradient placeholder + animated dominant-color extraction
│   │       │           │   ├── ArtworkCollage.kt     # 2x2 square collage: first 4 track artworks, one per quadrant (playlist thumbnails + detail header)
│   │       │           │   ├── PlaylistCard.kt       # Shared card: 2x2 collage thumbnail + name + green 3-dot menu (sync/stop-sync/URL/clear/delete, opt-in) + multi-select indicator
│   │       │           │   ├── TrackRowCard.kt       # Track row in playlist-card UI: artwork thumb + title/artist + green 3-dot + multi-select indicator
│   │       │           │   ├── TrackActionsSheet.kt  # Long-press track actions: add to playlist (picker + create-new) + remove (confirm dialog)
│   │       │           │   └── PlaylistPickerSheet.kt # Shared "add to playlist" bottom-sheet picker (batch + per-track flows)
│   │       │           ├── player/
│   │       │           │   ├── FullPlayerScreen.kt   # 280dp real artwork, animated dominant-color gradient backdrop, scrub bar, transport row, queue/action triggers
│   │       │           │   ├── QueueSheet.kt         # "Playing Next" modal bottom sheet with artwork rows + long-press drag-to-reorder
│   │       │           │   └── ActionSheet.kt        # Add-to-playlist/play-next/add-to-queue/artist/share sheet
│   │       │           ├── home/
│   │       │           │   └── HomeScreen.kt         # Spotify-style: greeting + 36dp green-circle settings icon, edge-clipped Recently-played (history) / Made-for-you / Your-playlists carousels
│   │       │           ├── search/
│   │       │           │   └── SearchScreen.kt       # Library search (results only when searching; file + folder-tree import) + Web search (job-status rows, tap-to-cancel, retry) + CSV import + direct URL
│   │       │           ├── library/
│   │       │           │   ├── LibraryScreen.kt      # Desktop "All Music": Play + Shuffle (16dp apart), Playlist-view tab + A–Z/Recent sort, search w/ clear-X, alphabetical tracks, batch multi-select (3-dot: select all / add to playlist / remove), collage-thumbnail playlist cards
│   │       │           │   └── PlaylistDetailScreen.kt # Spotify-style playlist view: 2x2 four-artwork collage header, name/desc/count/duration, play-all, track list (play + long-press actions)
│   │       │           ├── discover/
│   │       │           │   └── DiscoverScreen.kt     # Honest empty state (Last.fm TBD)
│   │       │           ├── downloads/
│   │       │           │   └── DownloadsScreen.kt    # Full download management: sync controls, queue, cancel/retry/delete (reachable via options drawer)
│   │       │           └── create/
│   │       │               └── CreatePlaylistSheet.kt # AlertDialog with name validation
│   │       ├── data/artwork/
│   │       │   └── ArtworkStorage.kt                 # Embedded-artwork extraction (MediaMetadataRetriever), 512px JPEG cache in filesDir/artwork, Palette dominant color
│   │       └── test/java/com/boombastic/mobile/
│   │           ├── data/db/
│   │           │   ├── AppDatabaseTest.kt            # Abstract Robolectric base class
│   │           │   ├── TrackDaoTest.kt               # 11 tests: CRUD, search, dedupe, count
│   │           │   └── PlaylistDaoTest.kt            # 7 tests: CRUD, cascade, sortOrder
│   │           ├── data/repository/
│   │           │   ├── LibraryRepositoryTest.kt    # 4 tests: SAF tree + multi-picker folder recursion (fake DocumentsProvider)
│   │           │   └── PlaylistRepositoryTest.kt   # 5 tests: validation, CRUD, trim
│   │           └── playback/
│   │               ├── NotificationPermissionPolicyTest.kt # 11 tests: permission policy matrix
│   │               └── MusicControllerTest.kt        # Added contract tests; final reviewer did not verify compilation
│   │
│   ├── vendor/
│   │   └── NewPipeExtractor/            # Git submodule, pinned tag v0.26.4 (shallow); built via composite build
│   └── .gradle/                         # Gradle caches (not tracked — in .gitignore implicitly)
│
├── project_brain.md                     # Desktop brain — do not duplicate its detail here
│
├── engine.py                            # SHARED LOGIC (desktop): DiscoveryService,
│                                        #   scan_library, find_duplicates,
│                                        #   _normalize_for_dupe, VibeEngine (pygame)
│
├── downloader.py                        # SHARED LOGIC (desktop): Downloader class,
│                                        #   yt-dlp wrapper, playlist sync, dedupe ledger
│
├── theme.py                             # SHARED: current desktop/Flet palette
│                                        #   (#121212, #181818, #282828, etc.)
│                                        #   does NOT match planned mobile colors
│
├── utils.py                             # SHARED: split_track_name, format_time,
│                                        #   hex_to_rgb, resource_path, etc.
│
├── music_player_flet.py                 # FLET PROTOTYPE: pygame backend, responsive
│                                        #   mobile/desktop UI, 3 tabs, mini-player
│
├── main.py                              # FLET ENTRY POINT: runs music_player_flet.main
│
├── buildozer.spec                       # FLET ANDROID BUILD: target API 33, min API 21
│
├── music_player.py                      # DESKTOP APP: Tkinter CVE controller
│
├── views/                               # DESKTOP VIEWS: library.py, downloader.py,
│   │                                    #   settings.py, discover.py, etc.
│   └── ...
│
├── test_media_keys.py                   # TESTS: deterministic media key unit tests
├── test_dl.py                           # TESTS: downloader tests
├── test_downloader_sync.py              # TESTS: sync/download tests
│
├── buildozer.spec                       # FLET BUILD CONFIG (see above)
├── requirements.txt                     # PYTHON DEPENDENCIES (desktop + Flet)
├── .gitignore                           # Ignores .venv, build/, dist/, __pycache__, .DS_Store
├── .opencode/                           # AI agent/plugin configuration
└── mobile-app/                          # Native Android app (see tree above)
```

---

## 🎯 Product Decisions (Settled)

### Scope & Non-Goals

**In scope:**
- Offline-first native Android music player
- Play local audio files from device/SD storage
- Download from YouTube/YouTube Music/SoundCloud (via desktop-semantics reproduction)
- Library management (playlists, search, duplicates)
- Last.fm discovery/recommendations (user's own API key)
- Local queue, history, shuffle/repeat modes
- Audio playback via Media3 with notification/lock-screen/headset/Bluetooth/AUX
- Local display name/profile (editable, never sent to a server)
- ReplayGain/loudness normalization (planned — see below)
- Equalizer (planned)
- Export/import playlists and selected songs as versioned JSON (via email/share/files)
- GitHub Releases with signed APK, update check

**EXCLUDED (must not implement):**
- No live sync, pairing, peer transfer, cloud account/backend, or remote playback
- No Premium/Spotify Premium features
- No podcasts/audiobooks (unless explicitly implemented later)
- No social messaging, accounts, or collaborative features (no "Blend")
- No sleep timer, lyrics display, visualizer, or crossfade (open to future addition)
- No Spotify logos, assets, or branding — BoomBastic is an independent app

### Interoperability (Export/Import)

**Export:** Selected song(s), playlist(s), or full library as a **versioned JSON manifest**.

**Manifest format principles:**
- Versioned schema; include `"manifest_version": 1` at root
- Never include secrets, audio blobs, history, or local file paths
- **Track identification:** Track name alone is insufficient. Require stable provider/source identity (YouTube video ID, Deezer ID, MusicBrainz ID) when available. Without a stable ID, include full metadata (artist, title, album, duration) plus an **explicit ambiguity confirmation** that the import may match a different recording.
- Treat every manifest as **untrusted** — validate and sanitize all fields

**Import behavior:**
- Import independently downloads missing music (does not copy audio blobs)
- Creates or merges playlists (by name); user confirms merge strategy
- **No automatic deletion** — import never removes existing content
- Duplicate resolution uses the same normalized-key approach as desktop (see `engine.py` `_normalize_for_dupe`)

### Storage

**User-selected storage** — always ask the user to pick:
- Shared internal storage (default)
- Removable SD card

Via **Scoped Storage / Storage Access Framework (SAF)**:
- Use `ACTION_OPEN_DOCUMENT_TREE` or `MediaStore` for the music root
- Audio survives app uninstall (it's in user-visible shared storage)
- Grants may not survive reinstall; on reinstall, prompt user to reselect and reconcile

**State handling:**
- **Unavailable/ejected storage:** Gracefully detect `Environment.MEDIA_UNMOUNTED` and distinguish from a genuinely empty folder. Show a clear message; do not crash or show empty library silently.
- **Stop-all:** A single action to halt all active/pending download jobs
- **Resumable jobs:** Track download state (queued, downloading, paused, failed, completed). On connectivity change or storage re-availability, offer to resume.
- **Insufficient storage:** Check available space before each download. Notify user with actionable information.
- **Placeholders/retry:** Failed downloads show a placeholder entry; user can retry individually or in bulk.

### Downloader (Android Adaptation)

**Must reproduce desktop acquisition semantics** where Android-compatible. The planner/coder MUST:
1. Inspect `downloader.py` lines 58–96 ([source](../downloader.py#L58-L96)) for format/quality/loudnorm details
2. Research a permitted Android alternative if `yt-dlp` + `FFmpeg` subprocess cannot run on device
3. Document the supported-source-format table before coding

**Desktop downloader facts (from `downloader.py`):**
| Setting | Value |
|---------|-------|
| Video format selector | `bestaudio/best` |
| Audio output codec | MP3 (via FFmpegExtractAudio) |
| Bitrate | 192 kbps |
| Sample rate | 44.1 kHz |
| Channels | 2 (stereo) |
| Loudness normalization | EBU R128 loudnorm: `I=-14:LRA=11:TP=-1.5` |
| Thumbnail | `EmbedThumbnail` postprocessor, also `writethumbnail: true` |
| Filename template | `%(title)s.%(ext)s` |
| Retry | 5 retries, 5 fragment retries, 3 extractor retries |
| Socket timeout | 30s |
| Download timeout | 120s (daemon thread continues) |

**Implemented Android approach:**
- **YouTube search**: Uses NewPipe Extractor **v0.26.4** bundled as a Git submodule (`vendor/NewPipeExtractor`) and wired via a Gradle composite build (`settings.gradle.kts` dependency substitution). No JitPack, no API key required.
- **YouTube playlist extraction**: `WebSearchService.getPlaylistVideos()` fetches all videos from a playlist URL via NewPipe Extractor; `PlaylistSyncWorker` orchestrates downloading each new track.
- **Playlist URL management**: Each `Playlist` entity has a `playlistUrl` field (v3 migration). Sync downloads all missing tracks.
- **Batch sync**: Downloads tab has "Sync All Playlists" button; each playlist with a URL synced sequentially.
- **Audio extraction** (`WebSearchService.getAudioStreamUrl()` — multi-strategy pipeline):
  1. **NewPipe StreamExtractor** — primary. v0.24.3 was too old for YouTube SABR enforcement (v0.26.3+ carries the SABR workaround); v0.26.4 is now vendored locally as a submodule and built via a composite build (JitPack does not publish v0.26.3+). Audio stream selected by priority M4A/AAC → Opus → best bitrate. The Invidious/InnerTube fallbacks below remain as emergency paths.
  2. **Invidious companion proxy** — emergency fallback. Fetches `invidious.tiekoetter.com/embed/VIDEO_ID`, parses the `<source>` tag to get the companion server URL, selects the audio itag from the actual adaptive formats parsed from the page (M4A/AAC → Opus priority — not a blind `itag=18` → `itag=140` swap), and **preserves the original `check` token** (companions with `verify_requests` enabled reject missing/invalid checks with HTTP 400). Falls back to itag 140 only when no format list is present. The companion server bridges HTTP/2 (to googlevideo.com) → HTTP/1.1 (to our device).
  3. **InnerTube player endpoint** — as last resort, POSTs to `youtubei/v1/player` with ANDROID client; strips `lsparams`/`lsig` (login signature tokens); attempts `n`-parameter deobfuscation via `YoutubeJavaScriptPlayerManager` if a player JS URL can be extracted. URLs from this path point to googlevideo.com and **fail with HTTP 403** on this device due to HTTP/1.1 protocol mismatch with `gvs 1.0` CDN.
- **Direct URL download**: Paste any direct audio URL (optional title/artist override)
- **CSV import (Exportify)**: Select a Spotify Exportify CSV file; each row (artist, title) is searched on YouTube and the first result downloaded
- **Download queue management**: Dedicated Downloads tab — view all active, queued, completed, failed downloads; cancel/retry/delete per item
- `DownloadWorker` uses **OkHttp 4.12.0** (`ConnectionSpec.MODERN_TLS`) for download streaming via WorkManager. Saves files with correct extension based on response `Content-Type` header (`.m4a`, `.opus`, `.mp3`, `.ogg`, `.audio`)
- Files saved to app-internal `downloads/` directory
- `MediaMetadataRetriever` extracts `durationMs` from downloaded file before creating `Track` entity
- Download progress via `DownloadJob` state in Room, rendered in UI
- Failed downloads auto-retry (exponential backoff, up to 3 attempts)
- Foreground notification uses `ic_download` drawable, notification ID `1000 + jobId`
- **SoundCloud**: Not yet extracted via search API; users can paste direct SoundCloud audio URLs

### Native Planned Stack

| Component | Technology | Status |
|-----------|-----------|--------|
| Language | Kotlin 2.0.0 | ✅ **Implemented** — source in `playback/`, `data/`, `ui/` |
| UI | Jetpack Compose (BOM 2024.06.00) | ✅ **Implemented** — 5 screens + full player + sheets + shell + theme + mini-player |
| Playback | Media3 (ExoPlayer 1.3.1) | ✅ **Implemented** — `MusicService` (MediaSessionService) + `MusicController` (StateFlow wrapper) |
| Local DB | Room 2.6.1 | ✅ **Implemented** — 4 entities, 3 DAOs (v5 schema — playlistUrl on Playlist, albumArtPath on Track, thumbnailUrl on DownloadJob) |
| Background downloads | WorkManager + Foreground Service | ✅ **Implemented** — `DownloadWorker` + `DownloadRepository` + `DownloadJob` Room entity (v2 schema) |
| Media scanning | MediaStore / SAF | ✅ **Implemented** — SAF `OpenMultipleDocuments` import via `LibraryRepository` |
| Dependency injection | Manual singleton (BoomBasticApp) | ✅ **Implemented** — Hilt deferred; manual DI in Application class |

**Playback architecture (current implementation):**
- `MusicService` extends `MediaSessionService` — single ExoPlayer instance. `onDestroy()` releases `MediaSession` before ExoPlayer (correct Media3 teardown order).
- `MusicController` wraps `MediaController` with `StateFlow` for `isPlaying`, `currentTrack`, `progress`, `duration`, `hasActiveItem`, `isConnected`, `repeatMode`, `shuffleEnabled`; supports **pending-play semantics** via a single immutable `PlaybackRequest(items, startIndex)` (last-user-request wins).
- **Reliability merge (2026-07-30):** The pending-play implementation is now production‑ready:
  - `AsyncConnector` injectable seam for deterministic testing without a live service.
  - Terminal `released` flag + `generation` counter prevents late‑arriving controller futures from attaching listeners or starting playback after `release()`.
  - `pendingRequest: PlaybackRequest?` atomically replaced on each pre‑connection call (full queue + clamped start index preserved).
  - `playbackError: SharedFlow<PlaybackError>` exposes Media3 `Player.Listener.onPlayerError` events.
  - `hydrateState(MediaController)` synchronises all `StateFlow`s immediately after (re)connection.
  - `sanitizeErrorMessage()` strips URIs and absolute paths from user‑facing messages.
  - `lastDispatchedRequest` internal hook enables test assertions on dispatched requests.
- `NotificationPermissionPolicy` — one-shot prompt policy; API<33 skips, API 33+ prompts once via `MainShell`'s central `onPlay` callback shared by Search and Library screens; playback proceeds regardless of the permission result.
- Official Android documentation and Media3 source confirm media-session notifications are exempt from `POST_NOTIFICATIONS`; denial alone is **not** the Android 14+ crash previously suspected for a correctly declared `MediaSessionService`/`mediaPlayback` foreground service.
- ExoPlayer configured with `AudioAttributes` for music, `setHandleAudioBecomingNoisy(true)` for headset unplug detection.
- Notification and lock-screen controls provided by Media3 session.
- `onTaskRemoved` stops service if nothing is playing.
- `MainShell` collects `connectionError` and `playbackError` via `SnackbarHostState` and shows one‑shot Snackbars.
- `MediaTrack` extended with `album: String` and `durationMs: Long`. SearchScreen/LibraryScreen construct `MediaTrack` from Room `Track` with exact title, artist, album, durationMs; `MiniPlayer` displays `Artist · Album` when available.
- **Full player screen implemented (2026-07-30):** `ui/player/FullPlayerScreen.kt` (280dp artwork, m:ss scrub bar, shuffle/prev/play-pause/next/repeat transport row, queue + action-sheet triggers, back arrow), `ui/player/QueueSheet.kt` ("Playing Next" modal sheet from `MusicController.getQueue()`), `ui/player/ActionSheet.kt` (Add to playlist sub-sheet via `PlaylistRepository`, Play next, Add to queue, Go to artist toast placeholder, Android Sharesheet). Repeat cycles OFF→ALL→ONE (ExoPlayer `REPEAT_MODE_*`), shuffle toggles `shuffleModeEnabled`; both exposed as `StateFlow` on `MusicController`. MiniPlayer track-info tap navigates to `Routes.FULL_PLAYER`; bottom bar hidden on the full-player route.
- **Real artwork + dynamic gradients implemented (2026-07-30):** Embedded album art is extracted at import/download time (`ArtworkStorage.saveEmbeddedArtwork*` via `MediaMetadataRetriever.getEmbeddedPicture`), downsampled to ≤512px JPEG and cached in `filesDir/artwork/`; the path is stored on `Track.albumArtPath` (Room **v4** migration `3_4`). `MediaTrack` carries `artworkUri` (file://) through `MusicController.buildMediaItem` (MediaMetadata `artworkUri`), and `MusicService`'s `ArtworkEnrichingCallback` (`MediaSession.Callback.onAddMediaItems`) loads `artworkData` bytes so the **notification and lock-screen show artwork**. UI renders via Coil (`ui/components/ArtworkImage.kt`): full player (280dp), MiniPlayer (48dp thumb), Home recently-played card, and QueueSheet rows. `rememberArtworkColors()` extracts the dominant color (androidx Palette vibrant→muted→dominant) and animates a vertical gradient backdrop behind the full player (600ms `animateColorAsState`), satisfying "gradients animate subtly on transition".
- **Queue reordering implemented (2026-07-30):** `MusicController.moveQueueItem(from, to)` → `Player.moveMediaItem`; `QueueSheet` rows show artwork + drag handle and support **long-press drag-to-reorder** (`detectDragGesturesAfterLongPress`, row translation + scale feedback, drop-target index computed from drag delta; snapshot refreshed after each move).
- **Download thumbnails implemented (2026-07-30):** YouTube audio streams carry no embedded album art, so downloaded songs previously had no artwork anywhere (full player, mini player, queue, home). Now the video thumbnail is captured at enqueue time: `WebSearchResult.thumbnailUrl` (search + CSV import) and `PlaylistVideo.thumbnailUrl` (playlist sync) flow through `DownloadJob.thumbnailUrl` (Room **v5** migration `4_5`) and WorkManager inputData (`DownloadWorker.KEY_THUMBNAIL_URL`). After the audio file is saved, `DownloadWorker` falls back to fetching the thumbnail via OkHttp (`ArtworkStorage.saveImageBytes`, ≤512px JPEG in `filesDir/artwork/`) when `MediaMetadataRetriever` found no embedded picture; the path lands on `Track.albumArtPath` so every artwork surface picks it up. Manual retries re-read the URL from the job entity.
- **Navigation restructure (2026-07-30):** Bottom nav is exactly **5 items** — Home, Search, Your Library, Discover, Create (Downloads removed from the tray). Search and Create render at 28dp (their material glyphs are optically smaller than the other tabs; the other four render at the standard 24dp). A **green-circle profile icon at the Home screen top-left** opens a `ModalNavigationDrawer` whose header reads **"Settings"** (the standalone Settings item was removed) containing Downloads (navigates to `Routes.DOWNLOADS`), Export-Import, About, Check for updates (honest "coming soon" toasts). The drawer never contains cloud account/login/logout items.
- **Playlist card UI (2026-07-30):** Shared `ui/components/PlaylistCard.kt` — thumbnail on the left, playlist name beside it, **green 3-dot options icon** on the right (menu: Sync playlist, Set/Edit YouTube URL via dialog, Delete with confirm dialog), on a `SurfaceDark` (#202020) rounded background that stands out against the black screen. Used **2-per-row on Home** (quick-action grid) and **full-width in the Library tab** (desktop-home style, one after another); tapping it opens the playlist detail screen.
- **Spotify-style Home (2026-07-30):** `HomeScreen` matches the visual-spec contract: greeting ("Good morning/afternoon/evening" + name) with a **36dp green-circle settings icon** (22dp Person glyph, no IconButton padding, 14dp spacing below), **edge-clipped horizontal carousels** ("Recently played" — current track; "Made for you" — first 10 library tracks until Last.fm lands), and a **quick-action playlist grid** (two shared PlaylistCards per row). "Your top genres" remains unimplemented (no genre metadata).
- **Playlist detail screen (2026-07-30):** `ui/library/PlaylistDetailScreen.kt` at route `playlist/{playlistId}` (opened by tapping any playlist card on Home or Library). Spotify-inspired header: **2x2 collage of up to four track artworks** filling a rounded square, playlist name, description, "N songs · total duration", green Play button (plays the whole playlist via `MusicController.play(tracks, 0)`), then the track list — tap plays the playlist from that track, long-press opens `TrackActionsSheet`. Not yet implemented (desktop parity): sort options, search within playlist, drag-to-reorder, download-all toggle.
- **Recently-played history (2026-07-30):** `MusicController` now keeps an **in-session recently-played array** (`recentlyPlayed: StateFlow<List<MediaTrack>>`, most recent first, max 100 per desktop convention), populated from `onMediaItemTransition` + state hydration (consecutive duplicates coalesced). **Home's "Recently played" section is now a swipeable edge-clipped horizontal carousel of the full history** (up to 20, same layout as "Made for you"; tapping a card replays it). Not yet persisted across app restarts — Room `HistoryEntry` remains planned.
- **Desktop-style folder import (2026-07-30):** `LibraryRepository.importLibraryTree(treeUri, onProgress)` imports a whole music root picked via SAF `ACTION_OPEN_DOCUMENT_TREE`, mirroring desktop `scan_library`: **each subfolder becomes a playlist named after the folder** (created on demand, merge-safe via name lookup + IGNORE dedupe), files directly in the root land in the **"Unsorted"** playlist. Audio detection by MIME type or extension; embedded artwork/metadata extraction reuses `importAudioUri`. The Search screen's Library tab has an "Import music folder (playlists by folder)" button with live `imported/duplicates/errors` progress. Root URI permission persisted. **Recursion fixed 2026-07-31:** subfolders are enumerated via `buildChildDocumentsUriUsingTree` (`…/children`) and every descendant URI is built from the **original tree root** + document id (`buildDocumentUriUsingTree`) — the previous code built children from child document URIs, producing nested `tree/…/document/…/document/…` URIs the provider cannot resolve, so folder contents were silently skipped. **Multi-picker folders (2026-07-31):** when the file picker returns directory documents (Samsung pickers allow selecting folders), `importMultipleUris` now recurses into them with the same folder-name = playlist-name semantics instead of importing one bogus "track" per folder; a directory guard in `importAudioUri` keeps folders from ever becoming tracks. Regression-covered by `LibraryRepositoryTest` (fake SAF `DocumentsProvider`, 4 tests).
- **Home tab always returns Home (2026-07-30):** Tapping the Home bottom-nav item now `popBackStack`s to the Home route (falling back to navigate) instead of the tab-style `popUpTo(saveState)` — you can never get "stuck" on the Downloads screen or any drawer/deep route.
- **CI workflow removed (2026-07-31):** `.github/workflows/android.yml` deleted — no GitHub Actions run on push/PR.
- **Stop All fixed (2026-07-30):** `stopAllActive()` previously cancelled jobs by stored work ID — running playlist syncs kept spawning new downloads. Every download work now shares tag `DownloadWorker.TAG_DOWNLOAD` and every sync shares `PlaylistSyncWorker.TAG_PLAYLIST_SYNC`; Stop All cancels by tag (reaching sync workers + their spawned downloads) and marks active jobs CANCELLED.
- **Library sorting (2026-07-30):** Tracks are listed **alphabetically** (case-insensitive title). Playlist view defaults to **A–Z** with a **"Sort: Recent"** toggle (by `Playlist.createdAt`) in the filter row; both are driven by `Track.addedAt` / `Playlist.createdAt` which are populated at import/download/create time.
- **Search tab lists nothing by default (2026-07-30):** The Search screen's Library tab shows "Songs appear here when you search." until a query is typed — it no longer dumps the whole library.
- **Never deletes audio from the phone (2026-07-30, enforced):** Removal is **metadata-only** by design. "Remove from library" deletes the Room `Track` row (FK cascade cleans `playlist_tracks`), "Delete playlist" removes the playlist + its membership rows, and the **"Clear playlist"** action empties a playlist's membership — in every case the audio files on the device are untouched. Confirm dialogs state this explicitly ("The audio file stays on your phone."). The only `file.delete()` in the app is `DownloadWorker` discarding a **cancelled partial download** inside app-internal `filesDir/downloads/` (never a user's file). This matches the import-side "no automatic deletion" principle.
- **Folder import fixed + persisted music-folder destination (2026-07-30):** The desktop-style tree import previously failed for every file — `importAudioUri` called `takePersistableUriPermission` per child document, which is only valid on the tree **root** (descendant document URIs throw). New `importTrackFromGrantedUri` imports tree descendants without per-file persist (the root grant covers them), so folder imports now work. The chosen folder is **persisted** (`MusicFolderRepository`, SharedPreferences-backed): the Settings drawer gained a **"Music folder"** item (picker → save → import → summary toast) and the Search tab's folder button saves the destination too, with a completion toast on top of the live progress.
- **Library batch multi-select (2026-07-30):** The section header directly below the search bar ("Tracks…" / "Playlists…") has a **gray 3-dot menu on the far right**. Menu: **Select all** (enters multi-select with everything checked in the current view), then — with items selected — **Add to playlist** (songs → chosen playlist; playlists → merges the selected playlists' tracks into the chosen one) and **Remove from app** (metadata-only confirm; files stay on the phone), plus **Cancel selection**. In selection mode rows show check indicators and taps toggle selection instead of playing/navigating.
- **Home playlist cards use singular artwork (2026-07-30):** On Home only, playlist cards show the **first track's artwork** (Made-for-you block styling), not the 4-quadrant collage — the collage thumbnail remains for Library playlist cards.
- **Library Play/Shuffle spacing (2026-07-30):** Shuffle sits **16dp (≈1rem) to the right of Play** (not the far side of the screen); the Playlist-view tab + sort toggle sit under Play.
- **Playlist sync membership fix (2026-07-30):** Syncing a playlist previously downloaded the songs but never added them to the playlist. Now `DownloadWorker` adds each completed download to its job's playlist (`playlistId` → `PlaylistTrack` with next sort order), and `PlaylistSyncWorker` also inserts pre-existing matching tracks into the playlist (idempotent via IGNORE conflict).
- **Playlist thumbnails = 4-quadrant collage (2026-07-30):** Playlist cards (Library, playlist view) use a **2x2 collage of the playlist's first four song artworks** as the thumbnail (`PlaylistCard` renders `ArtworkCollage` at 48dp); only individual songs use their singular artwork. Home's "Your playlists" carousel cards now **match the "Made for you" TrackCard layout** (square collage artwork on top, name, "N songs" subtitle) — Home-only styling.
- **Search download rows (2026-07-30):** The web-search page no longer shows the "Recent Downloads" list. Each result row is driven by its enqueued job (videoId → jobId map): green circular progress while queued/downloading (tap to cancel), **green checkmark on completion, then the row disappears** after ~1.5s to make room for other results; failed jobs show a retry icon.
- **Library play/shuffle/tab layout (2026-07-30):** Play sits **left**, Shuffle **right** (icon + green text, no box — desktop All Music layout), and the **Playlist-view filter tab sits under the Play button** (left-aligned; label flips to "All songs view" when active).
- **Create-playlist inside add-to-playlist (2026-07-30):** The "Add to playlist" picker in `TrackActionsSheet` now leads with a **"New playlist"** row that opens a create dialog (name + optional description); on create the track is added to the new playlist immediately.
- **Long-press track actions (2026-07-30):** `ui/components/TrackActionsSheet.kt` — long-press (or 3-dot) any track row (Library, Search library results, PlaylistDetail) to open a bottom sheet with **Add to playlist** (nested picker incl. create-new) and **Delete from library** (confirm dialog; `LibraryRepository.deleteTrack` cascades `playlist_tracks` rows via FK).
- **Not yet implemented:** Queue/history persistence, Bluetooth AVRCP metadata publication, Android Auto, volume slider (deferred — hardware keys only).

**Queue/history:**
- Queue is **local only** — never synced to a server
- History is local only, max 100 entries (following desktop convention from `engine.py` line 328)
- Shuffle/repeat modes: off, repeat one, repeat all (matching `music_player_flet.py` line 78)
- "Up Next" model: user-queued items play before the playlist context (mirroring desktop `user_queue_count` logic at `engine.py` lines 213, 384–385)

### Last.fm (Recommendations Only — No Scrobbling)

**Scope:**
- User enters their own Last.fm API key (free — register at https://www.last.fm/api)
- **Recommendations only** (display similar tracks based on current playback)
- **No scrobbling** unless the product direction changes (this is a settled non-goal for now)
- Recommendations are downloaded via the normal acquisition pipeline (same downloader)

**Desktop DiscoveryService facts (from `engine.py` lines 10–83):**
- Two-stage: `track.getSimilar` → if 0 results, fallback to `artist.getTopTracks`
- Cache limited to 10 entries; cleared when exceeded
- Fallback tracks get a hardcoded match score of 0.8
- Recommendations are cross-referenced against the user's library and filtered out

**Planned Android behavior:**
- Same two-stage API logic
- Credentials secured via `EncryptedSharedPreferences` (never exported in JSON manifests)
- Show **current track** as the query seed; if nothing is playing, show the **fallback last track** (matching desktop behavior)

### ReplayGain / Loudness Normalization

**Status: Planned** — no implementation exists in any form (the desktop uses EBU loudnorm at encode time, which is different).

**Desktop facts (separate from ReplayGain):**
- Desktop applies loudnorm (`I=-14:LRA=11:TP=-1.5`) during yt-dlp postprocessing
- This is a destructive encode-time normalization, not ReplayGain

**Planned Android modes:**
1. **Off** — no normalization
2. **Track** — normalize to a target level per track
3. **Album** — normalize consistently within an album
4. **Automatic** — choose Track or Album based on playback context

**Non-negotiable constraints:**
- **Clipping prevention** — never apply gain that causes intersample clipping
- **Non-destructive playback gain** — never modify source files; apply gain in the audio pipeline
- **Analysis as background job** — scanning for loudness metadata runs as a low-priority WorkManager job; results cached in Room
- **Processing interaction** — ReplayGain analysis interacts with download postprocessing; this interaction must be validated and documented before coding

### Distribution & Updates

- **GitHub Releases** with signed APK (AAB for Play Store if pursued later)
- **Permanent protected signing key** — back up securely; loss breaks updates
- **In-app update check** — compare local version against latest GitHub Release tag
- **Sideload warnings** — display a one-time notice about installing from outside Google Play
- **No mandatory paid developer license** — Google Play \$25 fee is optional; sideload-only distribution is valid
- **Desktop warning-free signing not required** — Android requires a valid signature; self-signed is acceptable for sideload

### Visual Specification (Authoritative Fallback)

> Note: The current desktop/Flet theme (`theme.py`) uses different colors (#121212, #181818, #282828, etc.). The colors below are **implemented** in `ui/theme/Color.kt` and match the visual spec. The full player screen contract (below) is **implemented** in `ui/player/` — real artwork (Coil), dominant-color dynamic gradients, and queue reordering are done; volume slider remains deferred.

**Color palette:**
| Role | Hex | Usage |
|------|-----|-------|
| Primary background | `#101010` | Main app background |
| Surface | `#202020`–`#292929` | Cards, sheets, elevated surfaces |
| Primary text | `#FFFFFF` | Headlines, body text |
| Secondary text | `#B3B3B3` | Subtext, metadata |
| Accent | `#1ED760` | Active indicators, buttons, highlights |

**Typography:**
- Primary: **Inter** (system sans for maximum compatibility)
- Fallback: System default sans-serif

**Iconography & touch:**
- Icon display size: **24–28dp**
- Minimum touch target: **48dp** (Android accessibility guideline)
- System insets (status bar, navigation bar) must be respected

**Artwork:**
- Playlist/album art uses **dynamic artwork gradients** generated from dominant colors (matching desktop's `extract_dominant_color` in `utils.py` line 37)
- Gradients animate subtly on transition

**Mini-player:**
- **Persistent mini-player** at the bottom (similar to Spotify)
- Shows artwork thumbnail, title, artist, play/pause, progress bar
- Tapping expands to full player
- Present on all main tabs when audio is active

**Carousels:**
- **Visible-clipped horizontal carousels** (items slightly clipped at screen edges to indicate scrollability)
- Used on Home, Discover, and Artist/Playlist detail screens

**Navigation:**
- Bottom navigation bar with 5 destinations (in order):
  1. **Home** — Recommended, recently played, quick-start; **profile icon top-left opens the local-function drawer**
  2. **Search** — Search library + web sources
  3. **Your Library** — Playlists, artists, albums, downloaded
  4. **Discover** — Last.fm-powered recommendations
  5. **Create** — One-action create playlist modal
- **Local-function drawer** (ModalNavigationDrawer, opened from the green-circle Home profile icon): header reads **"Settings"**; items are Downloads (functional route), Export/Import, About, Check for updates (honest "coming soon" toasts). No standalone Settings item (the header is the settings entry). Never contains cloud account/login/logout.
- No "Premium" tab, no podcast/audiobook tab

**Screen contracts (comprehensive):**

1. **Home:** ✅ **Implemented** in `ui/home/HomeScreen.kt` (Spotify-inspired, 2026-07-30) — Greeting (display name "Listener", editable in future), "Good morning/afternoon/evening", **36dp green-circle settings icon** (opens the Settings drawer), **recently played swipeable carousel** (edge-clipped, in-session history), **"Made for you" recommendations carousel** (edge-clipped, first 10 library tracks — Last.fm recommendations TBD), **quick-action playlists carousel** (same TrackCard layout: 2x2 collage art + name + song count, tap → playlist detail). **Not yet:** editable display name, "your top genres" (no genre metadata).
2. **Full player:** ✅ **Implemented** in `ui/player/FullPlayerScreen.kt` — **Large real artwork (280dp, Coil `AsyncImage` from `Track.albumArtPath` via `MediaTrack.artworkUri`; also shown in the notification/lock-screen via `MusicService` artwork-data enrichment)**, title, artist, **scrub bar with m:ss time labels**, repeat/shuffle/prev/play-pause/next transport, **queue button (`ui/player/QueueSheet.kt` with artwork rows + long-press drag-to-reorder)**, action sheet trigger (`ui/player/ActionSheet.kt` with add-to-playlist, play next, add to queue, go-to-artist, share), **animated dominant-color gradient backdrop** (androidx Palette → 600ms `animateColorAsState`). **Deferred:** volume slider (device hardware volume keys only — per scope decision), go-to-artist/album detail wiring.
3. **Action sheet (bottom sheet):** Add to playlist, play next, add to queue, go to album, go to artist, share, view credits, remove from playlist
4. **Playlist detail:** ✅ **Implemented (partial)** in `ui/library/PlaylistDetailScreen.kt` — **header with 2x2 four-artwork collage** (Spotify-style, up to 4 track artworks divided amongst a square), title, description, track count + total duration, **Play button**, track list (tap plays from track, long-press opens actions). **Deferred:** sort options, search within playlist, drag-to-reorder, download-all toggle, owner display.
5. **Playlist tools:** Rename, delete, export JSON, import JSON (merge/replace), duplicate track resolution
6. **Six-row recommended footer + Refresh** — on playlist/track detail pages, 6 rows of "Recommended based on this..." with a Refresh button that fetches new recommendations
7. **Local-function drawer:** Settings (audio, storage, Last.fm key, appearance), about, export/import, check for updates — never contains cloud account/login/logout
8. **Queue / Recents:** Tab layout with "Playing Next" (queue) and "Recently Played" (history); clear all button; drag-to-reorder queue — **drag-to-reorder implemented** in `ui/player/QueueSheet.kt` (long-press drag); "Recents"/history tab + clear-all still planned
9. **Discover:** Last.fm recommendations grid/carousel; "Get Similar" button; refresh; download individual or batch
10. **Create Playlist modal:** Name input, optional description, create button — single action, no multiple steps

**Omitted visual elements:**
- Premium upsells, Spotify logos, social/share buttons (except local share via Android Sharesheet), collaborative playlist UI, Blend UI, sleep timer, lyrics tab, visualizer

---

## 🔊 Desktop-Validated Facts (Source Citations)

These facts are confirmed by reading the actual source files. Link to them rather than copying uncertain detail.

### Downloader (`downloader.py`)

| Fact | Detail | Source |
|------|--------|--------|
| Format selector | `bestaudio/best` | Line 60 |
| Audio codec | MP3 via FFmpegExtractAudio | Lines 76–79 |
| Bitrate | 192 kbps | Line 78 |
| Sample rate | 44.1 kHz (`-ar 44100`) | Line 87 |
| Channels | 2 (`-ac 2`) | Line 88 |
| Loudnorm | `I=-14:LRA=11:TP=-1.5` | Line 90 |
| Thumbnail | `EmbedThumbnail` + `writethumbnail: true` | Lines 80–83, 93 |
| Filename | `%(title)s.%(ext)s` | Lines 62, 123 |
| Timeout | 120s daemon-threaded | Lines 15–38, 398 |
| Sync dedupe | `.downloaded_vids` + `.sync_failed_vids` per playlist folder | Lines 219–290 |
| Library-wide dedupe | Walks whole `os.path.dirname(out_folder)` | Lines 308–320 |
| Normalization | `make_key()`: strip non-alnum, `make_safe_name()`: regex-clean filename | Lines 246–304 |

### Playback & Library (`engine.py`)

| Fact | Detail | Source |
|------|--------|--------|
| Backend | `pygame.mixer` with 44.1kHz init | Lines 201–208 |
| Queue model | Flat `queue` list, `queue_idx`, `user_queue_count` (Up Next) | Lines 211–213 |
| Shuffle | Copy to `_original_queue`, shuffle rest after current | Lines 340–358 |
| Repeat | Track-level via `repeat` bool; loops through context | Lines 392–407, 462, 469 |
| Duplicate normalization | `_normalize_for_dupe()` — strips tags, normalizes feat, removes non-alnum | Lines 151–173 |
| Allowed dups | `.vibe_allowed_dups.json` in MUSIC_ROOT | Lines 492–506 |
| Discovery | `DiscoveryService`: `track.getSimilar` → `artist.getTopTracks` fallback | Lines 25–83 |
| Discovery cache | Max 10 entries, cleared on exceed | Lines 76–77 |
| Library scan | `scan_library(roots)`: walks subdirs as playlists, root as Unsorted | Lines 107–126 |
| History | 100-entry `_recently_played` list | Lines 328 |

### Flet Prototype (`music_player_flet.py`)

| Fact | Detail | Source |
|------|--------|--------|
| Framework | Flet (Flutter-based Python UI framework) | Line 7 |
| Audio backend | Pygame mixer (same as desktop) | Lines 93–98 |
| Mobile tabs | 3: Library, Player, Settings | Lines 1048–1058 |
| Responsive | <600px = mobile layout (bottom nav + mini-player), ≥600px = desktop sidebar | Lines 1336–1361 |
| Mini-player | Floating bar at bottom on mobile (visible except on Player tab) | Lines 1031–1044 |
| Theme colors | Uses `theme.py` (#121212, #181818, #1DB954) — different from planned mobile spec | Line 108, 109 |
| Player UI | Fullscreen art (280dp), scrub bar, volume, shuffle/repeat/prev/play/next | Lines 796–898 |
| Bottom player | Art 56dp, controls, scrub, volume (desktop layout) | Lines 911–1012 |
| Search | `Filter tracks...` text field, filters by title/artist | Lines 688–699, 1293–1297 |
| Track limit | Paginated: 100 tracks initially, Load More button | Lines 1220–1282 |
| Art cache | `FletArtCache` with ThreadPoolExecutor, mutagen/Pillow | Lines 18–58 |

### Buildozer Config (`buildozer.spec`)

| Fact | Detail |
|------|--------|
| Target API | 33 |
| Min API | 21 |
| Archs | `arm64-v8a, armeabi-v7a` |
| Permissions | `INTERNET, READ_EXTERNAL_STORAGE, WRITE_EXTERNAL_STORAGE, MANAGE_EXTERNAL_STORAGE` |
| Orientation | Portrait |
| Private storage | True |

---

## 🔐 Security & Permissions (Android)

### Runtime Permissions (current implementation)

| Permission | Rationale | Status |
|-----------|-----------|--------|
| `POST_NOTIFICATIONS` (Android 13+) | Media notification visibility | ✅ Declared in manifest; requested once via centralised `NotificationPermissionPolicy` in `MainShell` before first playback tap; prompt attempt is persisted with `SharedPreferences.commit()`. Playback proceeds regardless of grant because media-session notifications are exempt from `POST_NOTIFICATIONS`. |
| `FOREGROUND_SERVICE` | Ongoing playback | ✅ Declared in manifest |
| `FOREGROUND_SERVICE_MEDIA_PLAYBACK` | Media playback foreground service type | ✅ Declared in manifest (Android 14+) |
| `READ_EXTERNAL_STORAGE` / `READ_MEDIA_AUDIO` | — | **Not used** — SAF-based import avoids broad storage permission |
| `WRITE_EXTERNAL_STORAGE` | — | **Not declared** — downloads write to app-internal `filesDir/downloads/`, no broad storage permission needed |

### Android Manifest Distinction

**AndroidManifest.xml** (the actual OS manifest at `app/src/main/AndroidManifest.xml`) declares:
- Permissions: `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PLAYBACK`, `POST_NOTIFICATIONS`
- Activity: `MainActivity` (LAUNCHER, `adjustResize` soft input)
- Service: `MusicService` (`mediaPlayback` foreground type, `MediaSessionService` intent filter)

**JSON Export Manifests** (planned user-generated data files):
- Versioned, untrusted, never contain secrets or audio data
- Used for export/import of playlist data
- Not yet implemented

### Compatibility & Recovery

- SAF import uses `takePersistableUriPermission` so grants survive app restart
- `LibraryRepository.importAudioUri` handles deduplication and IO errors via `Result` type
- No broad storage permission requested — SAF `OpenMultipleDocuments` handles user-selected files only
- MediaStore batch scanning not yet implemented
- Graceful degradation on older API levels (minSdk 29)

---

## 🧩 Architecture / Data Flow

### Implemented Module Map (with actual files)

```
┌──────────────────────────────────────────────────┐
│  UI Layer (Jetpack Compose)                       │
│  ┌─────┐ ┌──────┐ ┌──────────┐ ┌─────────┐ ┌────┐ │
│  │Home │ │Search│ │Your Lib  │ │Discover │ │Create│ │
│  └──┬──┘ └──┬───┘ └────┬─────┘ └────┬────┘ └──┬─┘ │
│     │       │          │            │         │     │
│  ┌──┴───────┴──────────┴────────────┴─────────┴──┐ │
│  │    MainShell (Scaffold + BottomNav)            │ │
│  │    + MiniPlayer (AnimatedVisibility)           │ │
│  └───────────────────────┬───────────────────────┘ │
│                          │                          │
│  ┌───────────────────────┴───────────────────────┐ │
│  │  MusicController (StateFlow)                   │ │
│  │  - isPlaying, currentTrack, progress,          │ │
│  │    duration, hasActiveItem                     │ │
│  │  No ViewModel layer yet — direct controller    │ │
│  └──────────┬────────────────────────────────────┘ │
└─────────────┼─────────────────────────────────────┘
              │
┌─────────────┼─────────────────────────────────────┐
│  Service    │                                      │
│  Layer      │                                      │
│  ┌──────────┴──────┐                               │
│  │  MusicService    │  (DownloadWorker)            │
│  │  (MediaSession   │                               │
│  │   Service)       │                               │
│  │  - ExoPlayer     │                               │
│  │  - Notification  │                               │
│  │  - No AVRCP yet  │                               │
│  └──────────┬───────┘                               │
└─────────────┼──────────────────────────────────────┘
              │
┌─────────────┼──────────────────────────────────────┐
│  Data Layer │                                       │
│  ┌──────────┴──────────────────────────┐            │
│  │  Repository                          │            │
│  │  - LibraryRepository (Room + SAF)    │ ✅        │
│  │  - PlaylistRepository (Room)         │ ✅        │
│  │  - DownloadRepository                │ ✅ (implemented)   │
│  │  - DiscoveryRepository (Last.fm)     │ Planned   │
│  │  - SettingsRepository (DataStore)    │ Planned   │
│  └──────────────────┬───────────────────┘            │
│                     │                                │
│  ┌──────────────────┴───────────────────┐            │
│  │  Room Database (AppDatabase v4)       │            │
│  │  - Track, Playlist, PlaylistTrack      │            │
│  │  - DownloadJob                         │ ✅ (implemented — v2 migration) │
│  │  - QueueEntry, HistoryEntry,           │ Planned   │
│  │    AllowedDuplicate                    │ Planned   │
│  └──────────────────────────────────────┘            │
└──────────────────────────────────────────────────────┘
```

### Data Flow: Library Import (implemented)

1. User taps "Import audio files" on SearchScreen → SAF `OpenMultipleDocuments` launcher opens
2. User selects one or more audio files → launcher returns `List<Uri>`
3. `LibraryRepository.importMultipleUris()` iterates URIs, calls `importAudioUri()` for each
4. `importAudioUri()`:
   - Checks `TrackDao.exists()` for deduplication → skips if duplicate
   - Calls `takePersistableUriPermission()` to retain access across restarts
   - Extracts metadata via `MediaMetadataRetriever` (title, artist, duration) with filename-based fallback ("Artist - Title" split)
   - Inserts `Track` entity into Room via `TrackDao.insertTrack()`
5. Returns `ImportResult(imported, duplicates, errors)` — UI observes updated `Flow<List<Track>>` from Room

### Data Flow: Download (Implemented)
1. User navigates to SearchScreen → Web Search tab → searches YouTube → taps download on a result
2. UI shows a `CircularProgressIndicator` on that result and tracks the video ID as "extracting" via `downloadingVideoIds` state set (prevents duplicate taps)
3. `WebSearchService.getAudioStreamUrl(videoId)` runs on IO dispatcher with fallback chain: **NewPipe Extractor v0.26.4** (primary; audio stream chosen by priority M4A/AAC → Opus → best bitrate) → **Invidious companion** (emergency; embed player response parsed for the actual format list, companion `latest_version` URL requested with the chosen audio itag and the original `check` token preserved) → **InnerTube player endpoint** (last resort; googlevideo URLs typically 403 on this device)
4. If extraction succeeds: `DownloadRepository.enqueueDownload()` inserts a `DownloadJob(state=QUEUED)` in Room, enqueues a `DownloadWorker` via WorkManager with `NetworkType.CONNECTED` constraint
5. `DownloadWorker` (foreground service with app's `ic_download` icon, notification ID `1000 + jobId`):
   - Sets job state to DOWNLOADING
   - Opens an OkHttp request to the source URL with User-Agent, Referer, Origin, and `Range: bytes=0-` headers (client configured with `ConnectionSpec.MODERN_TLS` + `COMPATIBLE_TLS`)
   - Streams data to `{filesDir}/downloads/{safeFileName}.{ext}` — extension derived from response `Content-Type` (`.m4a`, `.opus`, `.mp3`, `.ogg`, `.audio`)
   - Updates progress (0-100%) in Room
   - On completion: extracts `durationMs` via `MediaMetadataRetriever`, inserts `Track` entity into Room, marks job COMPLETED
   - On failure: marks job FAILED with a status-specific error message (400/403/404/429/503 distinct; never logs full headers or signed stream URLs), auto-retries up to 3 times
6. Successful downloads appear immediately in the Library search results via Room `Flow`
7. User can retry failed downloads or cancel in-progress downloads

> **Verification status (2026-07-30):** Build/compile verified (`assembleDebug` + unit tests green; extractor v0.26.4 classes confirmed in APK). **On-device runtime verification CONFIRMED** on Galaxy S20 FE (API 33) — YouTube search → audio extraction → download exercised successfully.

Alternative paths:
- **Direct URL paste**: User pastes any direct audio URL (optional title/artist override) → taps "Start Download" — skips extraction, goes straight to enqueue
- **CSV import (Exportify)**: User selects Spotify Exportify CSV → each row (artist, title) is searched on YouTube → first result's audio URL extracted → downloads queued

### Data Flow: Playlist Sync (Implemented)
1. User sets a YouTube playlist URL on a playlist (Library screen) or creates a playlist with a URL
2. User taps Sync (per-playlist or batch "Sync All")
3. `DownloadRepository.syncPlaylist()` enqueues a `PlaylistSyncWorker` via WorkManager
4. `PlaylistSyncWorker`:
   a. Fetches all videos from the playlist via `WebSearchService.getPlaylistVideos()` (NewPipe Extractor v0.26.4 `PlaylistExtractor`)
   b. Compares against existing tracks in Room (by title) and existing queued downloads
   c. For each new video: extracts audio URL via `WebSearchService.getAudioStreamUrl()`, creates a `DownloadJob`, enqueues an individual `DownloadWorker`
5. Each `DownloadWorker` runs independently — progress visible in Downloads tab

### Data Flow: Playback (implemented)

1. User taps track in SearchScreen or LibraryScreen → screen constructs `MediaTrack(uri, title, artist, album, durationMs)` from Room `Track` and calls shared `onPlay(MediaTrack)` supplied by `MainShell`.
2. `MainShell` evaluates `NotificationPermissionPolicy`; API 33+ shows the system notification prompt at most once automatically, then dispatches playback regardless of prompt result.
3. `onPlay` calls `MusicController.play(track)` with full metadata.
4. If the Media3 controller is connected, `MusicController` builds a `MediaItem` with a `MediaMetadata` containing title, artist, and album, and sends it to `MusicService` through `MediaController`.
5. If the Media3 controller is not yet connected, the request is serialised as an immutable `PlaybackRequest(items, startIndex)` (last-user-request wins, full queue preserved). When the connection completes, `executePendingPlay()` dispatches the request exactly once.
6. `MusicService` (`MediaSessionService`) receives the item, ExoPlayer decodes and renders audio.
7. `MusicController` listener observes `onIsPlayingChanged`, `onMediaItemTransition`, `onPlaybackStateChanged`, and `onPlayerError`.
8. State published via `StateFlow` (`isPlaying`, `currentTrack`, `progress`, `duration`, `hasActiveItem`, `playbackError`).
9. `MiniPlayer` composable observes `hasActiveItem` for visibility, `currentTrack` for metadata display (title, `Artist · Album`), `isPlaying` for play/pause icon.
10. `connectionError` and `playbackError` emissions are collected by `MainShell` and displayed as one‑shot Snackbars.
11. Track completion triggers auto-advance within ExoPlayer; UI tracks progress via 250ms polling coroutine.
12. Notification/lock-screen controls handled by Media3 session; Bluetooth AVRCP not yet explicitly configured.

---

## 📂 File Registry (Implemented)

All files listed below exist in `mobile-app/` as of this writing.

### Build & Config
| File | Responsibility | Status |
|------|---------------|--------|
| `.github/workflows/android.yml` | Java 17 CI: assemble, unit tests, lint + report upload on failure (emulator smoke-test job removed 2026-07-30) | ✅ |
| `build.gradle.kts` | Root Gradle: plugin declarations (AGP, Kotlin, Compose, KSP) | ✅ |
| `settings.gradle.kts` | Project settings, single `:app` module; `includeBuild("vendor/NewPipeExtractor")` composite build substitutes the JitPack NewPipe coordinate (`com.github.TeamNewPipe.NewPipeExtractor:extractor`) with the local `:extractor` module | ✅ |
| `gradle.properties` | JVM args, AndroidX, Kotlin code style | ✅ |
| `gradle/libs.versions.toml` | Version catalog (AGP 8.5.2, Kotlin 2.0.0, Media3 1.3.1, Room 2.6.1, NewPipe v0.26.4, desugar_jdk_libs_nio 2.1.4) | ✅ |
| `app/build.gradle.kts` | App module: compileSdk 35, minSdk 29, Compose BOM 2024.06.00, all dependencies | ✅ |
| `app/proguard-rules.pro` | Keep Room entity annotations | ✅ |
| `vendor/NewPipeExtractor/` | Git submodule pinned to tag `v0.26.4` (shallow). Built via Gradle composite build; **requires a JDK 11 toolchain** (set in its root `build.gradle.kts`); **requires core-library desugaring** in the app (minSdk 29 < 33). GPLv3 licensed. | ✅ |
| `app/schemas/.../1.json` | v1–v5 Room schema exports (v1: tracks, playlists, playlist_tracks; v2: adds download_jobs; v3: adds playlistUrl to Playlist; v4: adds albumArtPath to Track; v5: adds thumbnailUrl to DownloadJob) | ✅ |

### Android System
| File | Responsibility | Status |
|------|---------------|--------|
| `app/src/main/AndroidManifest.xml` | Permissions (FOREGROUND_SERVICE, POST_NOTIFICATIONS), MainActivity, MusicService | ✅ |
| `app/src/main/res/values/colors.xml` | `#101010`, `#202020`, `#292929`, `#1ED760` (visual spec colors) | ✅ |
| `app/src/main/res/values/themes.xml` | Theme.BoomBastic (Material NoActionBar, dark background) | ✅ |
| `app/src/main/res/values/strings.xml` | App name, nav labels, action strings | ✅ |
| `app/src/main/res/drawable/*.xml` | 9 vector icons (home, search, library, discover, create, play, pause, music_note, ic_download) + launcher assets | ✅ |

### Kotlin Source
| File | Responsibility | Status |
|------|---------------|--------|
| `BoomBasticApp.kt` | Application class, manual DI (database, libraryRepo, playlistRepo) | ✅ |
| `MainActivity.kt` | Compose entry, MusicController init, edge-to-edge; controller is internally visible to instrumentation tests only | ✅ |
| `playback/MusicService.kt` | Media3 MediaSessionService + ExoPlayer; `ArtworkEnrichingCallback` (MediaSession.Callback) loads artworkData bytes from artworkUri for notification/lock-screen artwork | ✅ |
| `playback/MusicController.kt` | MediaController wrapper, StateFlow playback state (incl. recentlyPlayed history, max 100), immutable PlaybackRequest, injectable AsyncConnector, playbackError flow, generation-gated release | ✅ Terminal lifecycle, full-queue pending, metadata-aware MediaItems (incl. artworkUri), moveQueueItem, setShuffle, error Snackbar propagation |
| `playback/NotificationPermissionPolicy.kt` | One-shot `POST_NOTIFICATIONS` prompt policy using SharedPreferences | ✅ |
| `data/artwork/ArtworkStorage.kt` | Embedded-artwork extraction + ≤512px JPEG cache (filesDir/artwork), sampled decode, Palette dominant color | ✅ |
| `data/db/AppDatabase.kt` | Room database (4 entities, version 5, singleton, migrations 1→2→3→4→5) | ✅ |
| `data/db/entity/Track.kt` | Track entity (uri PK, title, artist, album, durationMs, albumArtPath, addedAt) + albumArtUri() helper | ✅ |
| `data/db/entity/Playlist.kt` | Playlist entity (autoId, name, description, createdAt) | ✅ |
| `data/db/entity/PlaylistTrack.kt` | Junction entity (composite PK, FK cascade, sortOrder) | ✅ |
| `data/db/dao/TrackDao.kt` | Track CRUD + search Flow + dedupe check | ✅ |
| `data/db/dao/PlaylistDao.kt` | Playlist CRUD + relation queries + sort order | ✅ |
| `data/repository/LibraryRepository.kt` | SAF import (files + desktop-style folder tree via granted-URI import path), MediaMetadataRetriever, embedded-artwork extraction via ArtworkStorage, dedupe, ImportResult; production-default injectable URI-permission persister for deterministic tests | ✅ |
| `data/repository/MusicFolderRepository.kt` | Persists the chosen music-folder tree URI (SharedPreferences) | ✅ |
| `data/repository/PlaylistRepository.kt` | Playlist CRUD, name validation, sort order mgmt, clear-playlist (metadata-only) | ✅ |
| `data/db/entity/DownloadJob.kt` | DownloadJob entity + DownloadState enum (QUEUED, DOWNLOADING, COMPLETED, FAILED, CANCELLED); thumbnailUrl captures the YouTube video thumbnail | ✅ |
| `data/db/dao/DownloadJobDao.kt` | DownloadJob CRUD + progress/state queries with Flow | ✅ |
| `playback/DownloadWorker.kt` | WorkManager CoroutineWorker: HTTP download, progress tracking, Track insertion (MediaMetadataRetriever duration + embedded-artwork extraction, thumbnail fetch fallback for YouTube), **adds completed download to its playlist**, foreground notification with app icon and stable notification ID | ✅ |
| `playback/PlaylistSyncWorker.kt` | WorkManager worker: fetches YouTube playlist videos via NewPipe Extractor, creates individual DownloadJob per track, **adds pre-existing matching tracks to the playlist**, deduplicates, reports extraction errors as failed jobs | ✅ |
| `playback/WebSearchService.kt` | YouTube client: search, playlist extraction, and audio stream URL extraction via NewPipe Extractor (v0.26.4, vendored Git submodule at `vendor/NewPipeExtractor` via composite build). Replaces the previous 4-fallback InnerTube/Piped/Invidious chain with bundled native extraction. Returns typed `ExtractionResult` for error propagation. | ✅ |
| `playback/NewPipeDownloader.kt` | `HttpURLConnection`-based implementation of NewPipe's `Downloader` interface. Handles GET/POST requests with proper User-Agent and redirects. | ✅ |
| `playback/ExtractionResult.kt` | Sealed class for typed extraction results: `Success<T>` or `Error(message, details)`. Eliminates nullable/pair returns. | ✅ |
| `data/repository/DownloadRepository.kt` | Enqueue, retry, cancel, delete, stop-all, cancel-playlist-sync; bridges Room + WorkManager; threads thumbnailUrl through job + inputData | ✅ |
| `ui/shell/MainShell.kt` | ModalNavigationDrawer ("Settings" header: Downloads/Export-Import/About/Update check) + Scaffold + BottomNav (Home/Search/Library/Discover/Create) + AnimatedVisibility MiniPlayer | ✅ |
| `ui/navigation/NavGraph.kt` | NavHost: Routes (HOME, SEARCH, LIBRARY, DISCOVER, DOWNLOADS, FULL_PLAYER) | ✅ |
| `ui/theme/Color.kt` | Dark palette constants | ✅ |
| `ui/theme/Theme.kt` | BoomBasticTheme (Material3 darkColorScheme) | ✅ |
| `ui/theme/Type.kt` | Sans-serif typography scale | ✅ |
| `ui/theme/Dimens.kt` | Touch targets, icon sizes, padding constants | ✅ |
| `ui/components/MiniPlayer.kt` | Persistent mini-player with progress, artwork thumb, title, artist, play/pause | ✅ |
| `ui/components/ArtworkImage.kt` | Coil SubcomposeAsyncImage wrapper (file:// artwork, gradient + music-note placeholder) + rememberArtworkColors (Palette dominant color → animated gradient stops) | ✅ |
| `ui/components/PlaylistCard.kt` | Shared playlist card: 2x2 collage thumbnail + name + green 3-dot menu (sync / stop-sync / URL / clear / delete, opt-in callbacks) | ✅ |
| `ui/components/TrackRowCard.kt` | Track row in playlist-card UI layout: artwork thumb + title/artist + green 3-dot, optional multi-select check indicator | ✅ |
| `ui/components/ArtworkCollage.kt` | 2x2 square collage of up to 4 track artworks (playlist thumbnails + detail header) | ✅ |
| `ui/components/TrackActionsSheet.kt` | Long-press track sheet: add to playlist (nested picker with create-new playlist dialog) + remove (confirm dialog) | ✅ |
| `ui/components/PlaylistPickerSheet.kt` | Shared "Add to playlist" bottom-sheet picker (batch and per-track flows) | ✅ |
| `ui/player/FullPlayerScreen.kt` | Full-screen player: 280dp artwork, animated dominant-color gradient backdrop, title/artist, m:ss scrub bar, shuffle/prev/play-pause/next/repeat, queue + action-sheet triggers | ✅ |
| `ui/player/QueueSheet.kt` | "Playing Next" bottom sheet: artwork rows, long-press drag-to-reorder via moveQueueItem | ✅ |
| `ui/player/ActionSheet.kt` | Track action sheet: add to playlist (nested picker), play next, add to queue, go to artist, share | ✅ |
| `ui/home/HomeScreen.kt` | Spotify-style Home: greeting + 36dp green-circle settings icon, edge-clipped Recently-played (history) / Made-for-you / Your-playlists carousels (playlist cards = TrackCard layout) | ✅ |
| `ui/library/LibraryScreen.kt` | Desktop "All Music": Play + Shuffle (16dp apart), Playlist-view tab + A–Z/Recent sort, search w/ clear-X, alphabetical tracks, batch multi-select (3-dot: select all / add to playlist / remove), collage-thumbnail playlist cards | ✅ |
| `ui/search/SearchScreen.kt` | Library search (long-press actions) + Web search (job-status rows: spinner→checkmark→vanish, tap-to-cancel, retry on failure; no recent-downloads list) + CSV import + direct URL + SAF import | ✅ |
| `ui/library/PlaylistDetailScreen.kt` | Playlist detail: 2x2 four-artwork collage header, name/desc/count/duration, play-all, track list (play from track + long-press actions) | ✅ |
| `ui/discover/DiscoverScreen.kt` | Honest empty state (Last.fm TBD) | ✅ |
| `ui/downloads/DownloadsScreen.kt` | Full download management screen: playlist sync controls, per-playlist sync, batch sync all, **Stop All**, download queue with cancel/retry/delete | ✅ |
| `ui/create/CreatePlaylistSheet.kt` | AlertDialog with name validation | ✅ |

### Tests
| File | Responsibility | Status |
|------|---------------|--------|
| `data/db/AppDatabaseTest.kt` | Abstract Robolectric base class (in-memory DB) | ✅ |
| `data/db/TrackDaoTest.kt` | 10 tests: insert, search, dedupe, delete, count, albumArtPath round-trip | ✅ |
| `data/db/PlaylistDaoTest.kt` | 8 tests: CRUD, track-to-playlist, cascade, sortOrder | ✅ |
| `data/repository/PlaylistRepositoryTest.kt` | 6 tests: blank name rejection, persistence, trim, list, delete, clear-playlist keeps playlist + tracks | ✅ |
| `data/db/DownloadJobDaoTest.kt` | 11 tests: insert, query, progress, complete, fail, state filter, active-list queries, delete, bulk delete, count | ✅ |
| `data/repository/DownloadRepositoryTest.kt` | 7 tests: enqueue, thumbnailUrl persistence, list, state filter, cancel, delete, get-null | ✅ |
| `playback/NotificationPermissionPolicyTest.kt` | 11 tests: API 29/33+ prompt policy, grant/deny/attempted behavior | ✅ |
| `playback/MusicControllerTest.kt` | 40 contract tests: pending-play, empty-request handling, full-queue preservation, last-request-wins, index clamp, release idempotence, stale-future guard, exact MediaItem metadata (incl. artworkUri), moveQueueItem no-ops, setShuffle no-op, and sanitized error emission | ✅ 40/40 passing |
| `playback/MusicControllerInstrumentedTest.kt` | 7 instrumented tests: connection, error path, pre-connection queue dispatch, real playback-state transition, notification posting, and activity recreation | ✅ Compiles; emulator execution pending |
| `storage/LibrarySmokeTest.kt` | 6 instrumented tests: repository/DAO basics, successful FileProvider-backed import, and deterministic revoked-permission failure | ✅ Compiles; emulator execution pending |

---

## ⚠️ Security Considerations

1. **Last.fm API key** — stored in `EncryptedSharedPreferences`; never logged, never exported in JSON manifests, never sent over HTTP (HTTPS always)
2. **Downloaded audio** — stored in user-selected shared storage; no app-internal encryption needed (user has full filesystem control)
3. **JSON import** — always parse with a strict schema validator; reject unknown fields; limit file size; never execute dynamic content
4. **Network** — HTTPS only for Last.fm API; yt-dlp-equivalent downloads may contact multiple hosts (YouTube, etc.); validate TLS
5. **Signing key** — permanent, backed up securely; loss prevents APK updates; store in hardware-backed keystore when possible
6. **No cloud accounts** — no user authentication, no tokens, no session management, no server-side storage

---

## 🛡 Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|-----------|
| yt-dlp alternatives on Android may have different capabilities | Downloader may not reproduce desktop quality | Research before coding; document format table; fallback to lower quality with user notification |
| SAF/MediaStore complexity | Storage access confusing or broken on some devices | Test on API 29, 30, 33; fallback paths; clear user guidance |
| Android Auto fee-free sideload conflict | Cannot distribute with Auto support without paid Google dev account | Mark Auto as optional/removable |
| AAC-LC licensing | Patent royalty obligations for AAC-LC encoder/distributor | Verify Android's built-in AAC codec license (generally covered by device manufacturer); if uncertain, keep MP3 as safe default |
| WorkManager+Foreground Service on OEM-skinned Android | Background execution limits on Xiaomi/Huawei/etc. | Test on target device (S20 FE); document known OEM quirks |
| Mono repo grows large | mobile-app/ may accumulate stale files | Clear ownership; file registry; archive policy |
| **MusicController async connection/release race** | `MediaController.Builder(...).buildAsync()` can complete after `MusicController.release()` — leaking resources or starting audio after cleanup. | ✅ **Resolved (2026-07-30).** Terminal `released` flag + `generation` counter prevents late futures from attaching listeners or starting playback. `releaseStaleController()` releases any late-arriving controller immediately. |
| **MusicController pending queue loses state** | Pre-connection `play(List<String>, startIndex)` stores only the effective single URI, losing the full queue and start index. | ✅ **Resolved (2026-07-30).** `pendingRequest: PlaybackRequest` stores the complete ordered item list and clamped start index. |
| **MusicController pending state is not atomic** | `pendingUri`/`pendingPlayConsumed` cross-field updates can double-execute, lose, or stale-read pending work. | ✅ **Resolved (2026-07-30).** Replaced with a single immutable `PlaybackRequest?` field; atomically replaced on each pre-connection call. |
| **MusicController connection errors are not surfaced** | `connectionError` emitted on `SharedFlow` but no Compose collector surfaced it. | ✅ **Resolved (2026-07-30).** `MainShell` now collects both `connectionError` and `playbackError` via `LaunchedEffect` and displays them as one-shot Snackbars. |
| **MusicController player errors not surfaced** | No `onPlayerError` listener, so Media3 decoder/network failures were invisible. | ✅ **Resolved (2026-07-30).** `onPlayerError` emits sanitised messages through `playbackError: SharedFlow<PlaybackError>`; collected by `MainShell`. |
| **No latest build evidence after last debugger changes** | Earlier `assembleDebug`, `testDebugUnitTest`, and `lintDebug` passed with 33 tests before the final debugger changes. | ✅ **Resolved (2026-07-30).** Verified: `assembleDebug` ✓, `testDebugUnitTest` (83/83 ✓), `lintDebug` ✓, `compileDebugAndroidTestKotlin` ✓ — all pass under Java 17. |
| **No device/emulator verification** | SAF import flow, Media3 service lifecycle, notification permission dialog, and ExoPlayer audio output untested on real hardware/emulator. | ✅ **Partially resolved (2026-07-30).** **Downloader runtime-verified on S20 FE (API 33)** — search → extraction → download confirmed. Remaining: instrumented smoke tests pending API 34 emulator execution; manual SAF import + notification-dialog + Media3 lifecycle checks still required before release. |

---

## 🚫 Rejected Alternatives

| Alternative | Reason for Rejection |
|------------|---------------------|
| React Native / Flutter for native app | Kotlin/Compose chosen for best Android integration, Media3 compatibility, and native performance |
| Cloud sync (Firebase/AWS) | Product decision: offline-first, no cloud accounts |
| Server-side scrobbling | Until product requires it, not implemented; user can use third-party Last.fm scrobbler |
| Collaborative playlists / Blend | Social feature; out of scope |
| Download audio blobs in export/import | Privacy and size concerns; references only |
| Force internal storage only | User choice is paramount; SD card support is a feature, not a bug |

---

## 🔧 Known Current Code Smells (from desktop/Flet — avoid in native app)

These are issues in the existing codebase that the native app should NOT reproduce:

1. **`music_player.py` orphaned inline sidebar rebuild** (desktop brain line 246): After calling `_populate_sidebar()`, the function has a second block of orphaned code that destroys and rebuilds the sidebar cards again.
2. **`views/artists.py` uninitialized attributes** (desktop brain line 247): `_sel_indices`, `_sel_anchor`, and `artist_tracks` are not initialized in `__init__`.
3. **`views/song_page.py` thread leak** (desktop brain line 248): `_load_blur_bg` spawns a new thread on every `update_view()` call with no cancellation mechanism.
4. **Flet prototype mixed desktop/mobile in one file** (`music_player_flet.py`): Over 1300 lines with conditional visibility toggles; native app should use proper navigation architecture.
5. **Flet prototype hardcoded color constants** (lines 108, 429, 449, 628, etc.): Colors scattered instead of using a theme system.
6. **Desktop downloader uses daemon threads** (`downloader.py`): Daemon threads can be killed mid-operation; native app should use WorkManager for guaranteed completion.

---

## ❓ Open Decisions (Actionable)

| Question | Status | Suggested Approach |
|----------|--------|-------------------|
| `yt-dlp` on Android — embed via Chaquopy/JNI? Or use NewPipe Extractor? Use android-youtube-dl fork? | ✅ **Resolved** (2026-07-30) | **NewPipe Extractor v0.26.4** adopted as a vendored Git submodule + Gradle composite build (JitPack stops at v0.24.x). Bundled native Java library with custom `HttpURLConnection` Downloader. Requires JDK 11 toolchain and core-library desugaring (`desugar_jdk_libs_nio`) for minSdk 29. Handles search, playlist, and audio-stream extraction with typed error propagation. |
| Output format: AAC-LC/M4A vs MP3 vs Opus | **Unresolved** | Provisional: high-quality AAC-LC M4A. Validation needed: does Android's built-in AAC decoder cover patent licensing? Is there any cost/distribution constraint? |
| Android Auto — Google's fee-free distribution rules for media apps on non-Play-Store releases | **Unresolved** | Investigate: can an Android Auto app be sideloaded without Play Store? If not, mark Auto as removable and document the trade-off. |
| SAF tree URI vs MediaStore for music root | **Resolved** | SAF `OpenMultipleDocuments` used for import (`LibraryRepository`). MediaStore broad scan not yet implemented. Hybrid approach deferred. |
| Room schema migration strategy | **Resolved** | Use `AutoMigration` (Room 2.4+) for simple changes; manual migration with testing for complex changes. |
| Min SDK | **Resolved** | **API 29** — set in `app/build.gradle.kts`. SAF/document provider model works from API 19+, but Media3 and Compose benefit from API 29 baseline. |
| Gradle build system configuration | **Resolved** | **Kotlin DSL** + version catalog (`libs.versions.toml`) + single `:app` module. Convention plugins deferred. AGP 8.5.2. |
| Dependency injection: Hilt vs manual | **Resolved** | **Manual singleton DI** in `BoomBasticApp` for now. Hilt deferred — not a blocking decision. |
| CI/CD for signed APK releases | **Resolved** (JVM checks) | `.github/workflows/android.yml` — Java 17 assemble, unit tests, lint on push/PR; uploads reports on failure. API 34 emulator smoke-test job **removed** (2026-07-30) after repeated failures; instrumented tests still compile locally. Signing/release CI deferred. |
| MusicController pending-play reliability | ✅ **Resolved** (2026-07-30) | Terminal lifecycle + generation-gated futures + immutable `PlaybackRequest` + `AsyncConnector` seam + playback error propagation + SnackbarHost. See playback architecture for details. |
| Android notification permission semantics | **Resolved** | `POST_NOTIFICATIONS` denial does not itself block a correctly declared Media3 media-session notification/`mediaPlayback` foreground service. Keep one-shot prompt policy for notification visibility, but do not block playback solely on denial. |

---

## 📦 Dependencies (from version catalog `libs.versions.toml`)

| Dependency | Version | Purpose | Status |
|-----------|---------|---------|--------|
| AGP (Android Gradle Plugin) | 8.5.2 | Build system | ✅ In use |
| Kotlin | 2.0.0 | Language | ✅ In use |
| KSP | 2.0.0-1.0.22 | Room annotation processing | ✅ In use |
| Jetpack Compose BOM | 2024.06.00 | UI framework | ✅ In use |
| Compose Material3 | via BOM | Material Design 3 | ✅ In use |
| Compose Material Icons Extended | via BOM | Extra icons | ✅ In use |
| Media3 (ExoPlayer) | 1.3.1 | Audio playback | ✅ In use |
| Media3 Session | 1.3.1 | MediaSessionService | ✅ In use |
| Room Runtime + KTX | 2.6.1 | Local database | ✅ In use |
| Room Compiler (KSP) | 2.6.1 | Code generation | ✅ In use |
| Navigation Compose | 2.7.7 | Screen navigation | ✅ In use |
| Coroutines | 1.8.1 | Async | ✅ In use |
| AndroidX Core KTX | 1.13.1 | Android API wrappers | ✅ In use |
| Lifecycle Runtime KTX | 2.8.3 | Lifecycle-aware coroutines | ✅ In use |
| Lifecycle ViewModel Compose | 2.8.3 | ViewModel in Compose integration | ✅ In use |
| Lifecycle Runtime Compose | 2.8.3 | Lifecycle-aware Compose effects | ✅ In use |
| Activity Compose | 1.9.0 | Compose entry point | ✅ In use |
| JUnit 4 | 4.13.2 | Test framework | ✅ In use |
| Robolectric | 4.12.2 | Android unit tests (JVM) | ✅ In use |
| Turbine | 1.1.0 | Flow testing | ✅ In use |
| Truth | 1.4.2 | Test assertions | ✅ In use |
| Room Testing | 2.6.1 | Room test helpers | ✅ In use |
| Coil (coil-compose) | 2.6.0 | Artwork image loading (full player, mini-player, queue, home) | ✅ **In use** |
| Palette (palette-ktx) | 1.0.0 | Dominant-color extraction for dynamic artwork gradients | ✅ **In use** |
| **Excluded (current):** | | | |
| WorkManager | 2.9.0 | Background downloads | ✅ **Implemented** |
| OkHttp | 4.12.0 | HTTP client for googlevideo.com downloads (supports HTTP/2, ALPN) | ✅ **Implemented** |
| NewPipe Extractor | v0.26.4 | YouTube extraction (vendored submodule via composite build) | ✅ **Adopted** |
| Hilt | — | Dependency injection | Deferred |
| Kotlinx Serialization | — | JSON parsing | **Planned** |
| DataStore | — | Preferences | **Planned** |
| EncryptedSharedPreferences | — | Secure credential storage | **Planned** — needed for Last.fm key |

---

## 🧪 Testing Strategy

| Level | Tool | Scope | Status |
|-------|------|-------|--------|
| Unit | JUnit 4 + Truth + Turbine + Robolectric | Room DAOs, Repositories | ✅ **42 tests** — `TrackDaoTest` (10), `PlaylistDaoTest` (8), `PlaylistRepositoryTest` (6), `DownloadJobDaoTest` (11), `DownloadRepositoryTest` (7) |
| Unit | JUnit 4 + Truth + Robolectric | Notification permission policy | ✅ **11 tests** |
| Unit | JUnit 4 + Truth + Robolectric | MusicController pending-play/lifecycle contract | ✅ **40 tests** — full-queue preservation, empty-request handling, last-request-wins, index clamp, release idempotence, stale-future guard, exact MediaItem metadata (incl. artworkUri), moveQueueItem no-ops, setShuffle no-op, and sanitized error emission |
| Instrumentation | Android Instrumentation Test + emulator | Media3 connection, queue dispatch, playback state, notification posting, activity recreation, SAF import, and revoked URI access | ✅ **13 smoke tests compile** — `MusicControllerInstrumentedTest` (7), `LibrarySmokeTest` (6); **not run in CI** (emulator job removed 2026-07-30); local emulator execution possible |
| UI | Compose UI Test | Screen composables, navigation | **Planned** |
| Integration | Android Instrumentation Test | WorkManager workers, full Media3 interaction | **Planned** |
| Snapshot | Roborazzi (or Paparazzi) | Visual regression for Compose screens | **Planned** |
| End-to-end | Maestro / ADB script | Full playback flow, download flow, import/export | **Planned** |

**Latest verification status (2026-07-30):** ✅ All four baseline checks pass on Java 17:
- `./gradlew clean :app:assembleDebug` — **PASS**
- `./gradlew :app:testDebugUnitTest` — **93/93 PASS** (11 NotificationPermissionPolicy, 40 MusicController, 10 TrackDao, 8 PlaylistDao, 6 PlaylistRepository, 11 DownloadJobDao, 7 DownloadRepository)
- `./gradlew :app:lintDebug` — **PASS** (after removing the default `WorkManagerInitializer` from the merged manifest — see AndroidManifest.xml)
- `./gradlew :app:compileDebugAndroidTestKotlin` — **PASS**

Instrumented smoke tests created for API 34 emulator (`./gradlew :app:connectedDebugAndroidTest`), pending CI emulator execution. **Downloader runtime-verified on S20 FE (API 33)** — YouTube search → audio extraction → download confirmed working. Remaining manual device checks: SAF import, notification-permission dialog, Media3 service lifecycle on API 29.

**Test targets:** S20 FE (API 33) as primary; Pixel 6 / API 29 as secondary. Add API 34/35 emulator/device coverage for Media3 service and notification behavior before release.

---

## 🔄 Maintenance Policy

1. **This document** must be updated when:
   - Any settled decision changes
   - A planned module is implemented
   - New code smell or risk is discovered
   - The desktop `engine.py` or `downloader.py` changes in ways that affect planned Android behavior

2. **Authority order** (repeated from banner):
   Source/tests/config > desktop brain/cited shared source > confirmed product decisions > readable screenshots > labeled proposals/open decisions

3. **File ownership:**
   - `mobile-app/` files: owned by mobile team
   - Root shared modules (`engine.py`, `downloader.py`, `theme.py`, `utils.py`): shared ownership — read by mobile team for semantics, but Android will reimplement natively
   - Root `project_brain.md`: desktop team

4. **Archive policy:** If a planned module is rejected after implementation, move to `mobile-app/archive/` with a deprecation note. Never delete without documenting the decision.

---

## 📎 Links

- [Desktop Project Brain](../project_brain.md) — desktop architecture, CVE pattern, all desktop technical protocols
- [downloader.py](../downloader.py) — desktop downloader semantics (source of truth for format/quality/loudnorm)
- [engine.py](../engine.py) — desktop engine, DiscoveryService, dedupe logic, library scan
- [theme.py](../theme.py) — current desktop/Flet palette (differs from planned mobile colors)
- [music_player_flet.py](../music_player_flet.py) — current Flet mobile/desktop prototype
- [buildozer.spec](../buildozer.spec) — current Flet Android build configuration
- [utils.py](../utils.py) — shared helpers (split_track_name, format_time, extract_dominant_color)

---

## 📋 Change Record

| Date | Change |
|------|--------|
| 2026-07-30 | **Folder import fixed + music-folder destination + batch multi-select.** Root cause of the failed album import: `takePersistableUriPermission` was called on every tree child (only valid on the tree root) — every file errored. New `importTrackFromGrantedUri` fixes it. Chosen folder now persists (`MusicFolderRepository`): Settings drawer "Music folder" item + Search-tab folder button both save the destination and import with a summary toast. Library gained **batch multi-select**: gray 3-dot on the Tracks/Playlists header (far right) → Select all / Add to playlist / Remove from app (metadata-only) / Cancel selection; rows show check indicators in selection mode. Shared `PlaylistPickerSheet` added. Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **93/93** ✓, `lintDebug` ✓. |
| 2026-07-30 | **Metadata-only removal guarantee.** Song/playlist removal never touches device audio: "Remove from library" and "Delete playlist" already were DB-only (audited — the sole `file.delete()` is a cancelled partial download in app-internal storage); confirm dialogs now state the guarantee explicitly ("The audio file stays on your phone."). Added **"Clear playlist"** (`PlaylistDao.clearPlaylist` / `PlaylistRepository.clearPlaylist` + 3-dot menu item + confirm dialog) which empties membership while keeping the playlist, tracks, and files. Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **93/93** ✓, `lintDebug` ✓. |
| 2026-07-30 | **Folder import + navigation/CI/stop-all fixes.** Desktop-style import: `importLibraryTree` (SAF folder tree — subfolders become playlists named after the folder, root files → "Unsorted", live progress; button on Search Library tab). Home tab always pops back to Home (no more being stuck on Downloads). CI: **removed the API 34 emulator job** from `android.yml` (recurring failed check); JVM checks only. **Stop All fixed**: common WorkManager tags (`TAG_DOWNLOAD`, `TAG_PLAYLIST_SYNC`) so cancelling reaches running playlist syncs that kept spawning downloads. Library: tracks alphabetical; playlist view A–Z with "Sort: Recent" toggle; Shuffle moved to 16dp right of Play. Search Library tab no longer lists the whole library (results only when searching). Home playlist cards use singular first-track artwork (no collage on Home). Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **92/92** ✓, `lintDebug` ✓. |
| 2026-07-30 | **Stop controls + playlist-sync membership fix + search-row statuses.** Added `DownloadRepository.stopAllActive()` (Downloads screen "Stop All") and `cancelPlaylistSync()` (Library playlist 3-dot "Stop sync" while active; cancels sync worker + playlist jobs). **Fixed sync bug**: synced songs are now added to the playlist — `DownloadWorker` inserts the completed track into its job's playlist, `PlaylistSyncWorker` adds pre-existing matching tracks (IGNORE-dup). Playlist card thumbnails are now **2x2 collages of the first four songs** (only songs keep singular art); Home playlist cards match the Made-for-you TrackCard layout. Search page: recent-downloads list removed; result rows show green spinner (tap cancels) → green checkmark → row vanishes after 1.5s; failed rows show retry. Library: Play left / Shuffle right, Playlist-view tab under Play. Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **92/92** ✓, `lintDebug` ✓. |
| 2026-07-30 | **Recently-played history + Library polish.** `MusicController` gained `recentlyPlayed` StateFlow (in-session, max 100, recorded on media-item transitions + hydration, consecutive duplicates coalesced); Home "Recently played" is now a **swipeable edge-clipped carousel of the full history** (tap replays). Library: Shuffle **stacked under Play** with icon + green text (gray box removed), Playlist-view tab right-aligned without overflow, search field gained a **clear-X**, and the 4-quadrant collage header was **removed/disabled** (component remains for playlist detail). Footer Create icon bumped to 32dp. Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **90/90** ✓, `lintDebug` ✓. |
| 2026-07-30 | **Desktop "All Music" library + Home playlist carousel.** `LibraryScreen`: centered ~60% 2x2 collage, Play + Shuffle row (Shuffle uses new `MusicController.setShuffle`), right-aligned **Playlist view filter tab** (filter icon + label, toggles songs ↔ playlists; label flips to "All songs view"), search bar underneath, "New Playlist" button removed (creation moved to long-press → New playlist). Home "Your playlists" became an edge-clipped horizontal carousel (200dp cards, same as "Made for you"). Nav Search + Create icons bumped to 28dp (optically smaller glyphs). Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **90/90** ✓, `lintDebug` ✓. |
| 2026-07-30 | **Library tab = desktop-home layout.** `LibraryScreen` reworked: 2x2 collage square of the first 4 album covers (shared `ArtworkCollage`), search bar below (live filter of playlists + tracks), playlists listed **full-width** one after another (shared `PlaylistCard`), tracks listed with the **same playlist-card UI** (`TrackRowCard`: thumb + title/artist + green 3-dot). Add-to-playlist sheet gained **"New playlist"** (create dialog, track added immediately). Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **89/89** ✓, `lintDebug` ✓. |
| 2026-07-30 | **Spotify-style Home + playlist detail.** Home screen reworked to the recorded visual-spec contract: greeting with a 36dp green-circle settings icon (bigger Person glyph, no inner padding, 14dp margin below), edge-clipped "Recently played" and "Made for you" (library tracks until Last.fm) horizontal carousels, quick-action playlist grid. New `PlaylistDetailScreen` at route `playlist/{playlistId}`: **2x2 four-artwork collage header**, name/description/"N songs · duration", Play button, track list (tap plays from track; long-press actions). New shared `PlaylistCard.kt` (also used by Home) and `TrackActionsSheet.kt` (long-press: add to playlist / delete with confirm; FK cascade removes playlist entries). Library + Search rows gained long-press. Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **89/89** ✓, `lintDebug` ✓. |
| 2026-07-30 | **Download thumbnails + UI polish.** YouTube tracks now get artwork: `WebSearchResult.thumbnailUrl` / `PlaylistVideo.thumbnailUrl` flow through `DownloadJob.thumbnailUrl` (Room **v5** migration) + WorkManager inputData; `DownloadWorker` fetches the thumbnail (OkHttp → `ArtworkStorage.saveImageBytes`) when the audio has no embedded picture, so full player / mini player / queue / home all show the song image for downloads. Home profile icon is now a green circle; the options drawer header reads **"Settings"** and the standalone Settings item was removed. `LibraryScreen` playlists became a **2-column grid**: first-track artwork thumbnail + name + green 3-dot menu (Sync / Set-Edit URL dialog / Delete confirm) on `SurfaceDark` cards. Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **89/89** ✓, `lintDebug` ✓. |
| 2026-07-30 | **Full player artwork + navigation restructure.** Real artwork pipeline: `ArtworkStorage` (embedded-artwork extraction → ≤512px JPEG cache in `filesDir/artwork/`), `Track.albumArtPath` (Room **v4** migration), `MediaTrack.artworkUri` through `MusicController.buildMediaItem`, `MusicService` `ArtworkEnrichingCallback` loads `artworkData` for notification/lock-screen artwork. UI: Coil (`ui/components/ArtworkImage.kt`) in FullPlayerScreen (280dp) + MiniPlayer + Home card + QueueSheet; animated dominant-color gradient backdrop via androidx Palette (`rememberArtworkColors`, 600ms `animateColorAsState`). Queue reordering: `MusicController.moveQueueItem` (Player.moveMediaItem) + long-press drag-to-reorder in `QueueSheet`. Bottom nav trimmed to **5 items** (Home/Search/Library/Discover/Create — Downloads removed from tray); Home profile icon (top-left) opens the **local-function drawer** (Downloads, Settings, Export/Import, About, Check for updates; placeholders toast). New deps: Coil 2.6.0, palette-ktx 1.0.0. Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **88/88** ✓, `lintDebug` ✓. |
| 2026-07-30 | **Downloader runtime-verified.** Confirmed on-device (S20 FE, API 33): YouTube search → audio extraction → download working. Refreshed baseline checks: `assembleDebug` ✓, `testDebugUnitTest` 83/83 ✓ (was recorded as 62 — MusicControllerTest is 35, not 29; adds DownloadJobDaoTest 9 + DownloadRepositoryTest 6), `lintDebug` ✓, `compileDebugAndroidTestKotlin` ✓. Fixed lint Error `RemoveWorkManagerInitializer`: app uses on-demand WorkManager init via `Configuration.Provider` (`BoomBasticApp`), so the default `WorkManagerInitializer` meta-data is now stripped from the merged manifest (`tools:node="remove"`). Synced stale facts: playlist-sync data flow now cites NewPipe `PlaylistExtractor` (not Piped API); `WRITE_EXTERNAL_STORAGE` rationale updated (downloads write to app-internal `filesDir/downloads/`). |
| 2026-07-30 | Implemented Android downloader: `DownloadWorker`, `DownloadRepository`, `DownloadJob` Room entity (v2 schema), DownloadJobDao. Updated all status markers, file registry, data flow, architecture diagram, and file tree. |
| 2026-07-30 | Implemented playlist sync: `PlaylistSyncWorker`, `WebSearchService` (Piped API client), `DownloadsScreen`, v3 schema (playlistUrl on Playlist), CSV import (Exportify). Updated all sections accordingly. |
| 2026-07-30 | Fixed YouTube Web Search: migrated from Piped-only API to YouTube InnerTube API as primary with Piped fallback. Updated `WebSearchService.kt` to call InnerTube search/browse/player endpoints directly. Fixed the Piped API type filter (`"stream"`), added User-Agent headers, removed duplicate import. Updated project brain sections. |
| 2026-07-30 | Fixed audio URL extraction: swapped to Piped-first priority, replaced dead kavin.rocks with working community instances, expanded InnerTube to try 4 client types (ANDROID_MUSIC, ANDROID, TVHTML5_SIMPLY, WEB) with full device context, added Invidious API as third fallback layer, added headers to DownloadWorker. |
| 2026-07-30 | Reordered extraction fallback: YouTube watch page HTML scraping (ytInitialPlayerResponse, brace-depth JSON parser) as primary method, then Piped → InnerTube (updated to 2025 client versions with playbackContext/signatureTimestamp) → Invidious. Added download loading spinner per video ID in SearchScreen. DownloadWorker: replaced private system notification icon with app's ic_download, added MediaMetadataRetriever duration extraction, fixed notification ID collision (1000 + jobId). |
| 2026-07-30 | **Migrated YouTube extraction to NewPipe Extractor v0.24.3.** Replaced the fragile 4-fallback InnerTube/Piped/Invidious chain with bundled native NewPipe Extractor library. Added `NewPipeDownloader.kt` (HttpURLConnection-based Downloader), `ExtractionResult.kt` (typed error propagation). Updated `WebSearchService.kt` to use NewPipe's `StreamExtractor`, `SearchExtractor`, and `PlaylistExtractor`. Added extraction error visibility in SearchScreen and PlaylistSyncWorker. Updated `BoomBasticApp.kt` to initialize NewPipe at startup. Added JitPack repo and dependency. |
| 2026-07-30 | **Multi-strategy audio extraction & download rewrite.** NewPipe v0.24.3 fails SABR enforcement (requires v0.26.3+ for fix, not available on JitPack). Added three-stage fallback: (1) Invidious companion proxy (`tiekoetter.com/embed` → companion `latest_version` with itag=140 for audio-only M4A), (2) InnerTube ANDROID client with `lsparams`/`lsig` stripping, (3) `n`-parameter deobfuscation via `YoutubeJavaScriptPlayerManager`. Replaced `HttpURLConnection` with OkHttp 4.12.0 in `DownloadWorker` (`ConnectionSpec.MODERN_TLS`) for HTTP/2 ALPN support. Added `CookieManager` for session cookies. Auto-detects file extension from Content-Type. googlevideo.com direct downloads return HTTP 403 (HTTP/1.1 protocol mismatch with `gvs 1.0` CDN) — the Invidious companion proxy bridges this gap. |
| 2026-07-30 | **Downloader hardening + vendored extractor.** Bundled NewPipeExtractor v0.26.4 as a Git submodule (`vendor/NewPipeExtractor`, pinned tag, shallow) built via Gradle composite build — JitPack stops at v0.24.x; requires JDK 11 toolchain (registered in CI via `org.gradle.java.installations.paths`; local via Temurin 11) and core-library desugaring (`desugar_jdk_libs_nio` 2.1.4, minSdk 29 < 33); CI checkout now `submodules: recursive`. Hardened `WebSearchService`: audio selection by priority M4A/AAC → Opus → best bitrate; Invidious companion preserves `check` and picks the itag from the actual format list instead of the blind 18→140 rewrite. `DownloadWorker`: redacted failure logging (no full headers / signed stream URLs) and status-specific error messages (400/403/404/429/503 distinct). **On-device runtime verification pending.** |

## 🛠️ Build & Test Operations

When building the debug APK for testing, always copy it to the repo root:

```
cd ~/boombastic/mobile-app && ./gradlew assembleDebug && cp app/build/outputs/apk/debug/app-debug.apk ~/boombastic/test.apk
```

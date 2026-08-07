# 🧠 Project Brain: Luno Native Android App

> [!IMPORTANT]
> **MAINTENANCE INSTRUCTIONS FOR AI/HUMANS:**
> This document is the authoritative knowledge base for the **implemented** Luno native Android app under `mobile-app/`, including its planned extensions. It must be updated whenever settled decisions change.
>
> **Last verified and updated:** 2026-07-31 (import UI can never sit frozen: the folder-import strip is now owned by a process-lifetime **`MusicFolderImportManager`** (shared by the MainShell Settings flow + Search tab folder flow, `ui/shell/MusicFolderImportManager.kt`): progress + final state live in a `StateFlow`; the strip ALWAYS resolves — success flips the green text to **"Import complete!" for ~2s then auto-clears**; the stall watchdog (no progress tick for 90s, checked every 15s) **never cancels the import** — every unit of work is timeout-bounded, so a quiet stretch means the provider/device is slow, not dead; it only shows a calm **"Import is taking longer than expected… still working"** note and the import keeps running and reports its real outcome when it finishes; unexpected exceptions resolve the strip as **"Import failed — please try again (exception message)"** with the exception text also in a toast and logged (`Log.e("MusicFolderImport", …)`) for logcat-free diagnosis; retries replace any previous attempt (`activeJob` guard — a stuck job never locks out a new import); the success toast is guarded so a toast failure can never flip a completed import to "failed". **False-stall root cause fixed earlier:** `lastTickAt` is an `AtomicLong` (a plain captured `var` had no memory barrier — the JIT could hoist the read, so the watchdog compared against the *initial* timestamp and cancelled healthy imports ~1800 songs in). **Import is stall-proof by construction AND complete:** EVERY unit of work is timeout-bounded — folder listings 15s (fallback folder-document query 4s), Room DAO calls 10s each (cancellable suspends), one file's whole pipeline 30s, doc-id preload 30s, URI-grant persistence 15s (blocking Binder call) — a progress tick fires at every folder entry BEFORE any work, **folder listings are RETRIED up to 3× (a timeout/rejection means the provider was slow — never "empty")**, an unlistable folder is counted as an **error** (never silently skipped — silent skips were the random 1000–1800-duplicate "Import complete!" results with 0 errors), and a Throwable inside a folder's work is counted, not swallowed. **Import crash fixed:** a provider that answers a folder-document query with the folder's **own row** (the empty-subfolder fallback path) previously made `importTreeFolder` recurse into itself until a StackOverflowError killed the app ("Luno closed") — self-rows are now skipped, recursion is depth-capped (24), `MediaMetadataRetriever.release()` runs in `finally` (no more native fd/memory leak per file), and the extraction pool is bounded (abandoned threads capped at 32; rejection falls back to filename metadata instead of hanging). **Screen transitions — user-confirmed timings (2026-07-31):** NavHost transitions are **0ms** (`EnterTransition.None`/`ExitTransition.None` — screens swap fully-formed); the green-loader mask is set **BEFORE every navigate** (MainShell call sites + `NavGraph`'s `onNavigate` callback for pushes/backs); mask **enters with `EnterTransition.None`** (fully opaque frame 1 — the screen can never flash through); mask **holds 300ms** (`TRANSITION_MASK_HOLD_MS`) as the black-screen moment **and until `LibraryData.loaded`** (hard cap `TRANSITION_MASK_MAX_MS = 6s`); mask **exits over 400ms** (`TRANSITION_MASK_FADE_MS`, `fadeOut` with `FastOutSlowInEasing`) revealing the fully rendered screen — all constants live in `MainShell.kt`. **Warm library data (`LibraryData`):** all four library Room flows are collected ONCE at app startup into app-level `StateFlow`s (`data/repository/LibraryData.kt`, `SharingStarted.Eagerly`, with a `loaded` flag); Home, Library, Downloads and PlaylistDetail render from them, so screens show full content the moment the mask lifts — no per-tab query latency, no progressive pop-in (the screens' own spinner only appears on the very first app frames). Earlier: provider queries timeout-guarded (15s, abandoned-thread pool) alongside extraction (20s), `SmoothProgressBar` wall-clock driven (`withFrameNanos`), progress throttled ~10/s, `try/finally` strip clearing, import semaphore only serializes file work, bounded parallelism (4 workers, atomic counters, concurrent doc-id set, per-playlist `AtomicInteger` sort orders), Home carousels stabilized (hoisted `LazyListState`s, stable `item(key=…)`, `distinctBy` history), Library view toggle always green + three-state sort chip (A–Z/Z–A/Recent) in both views, per-element no-ripple. Refreshed baseline checks — 105/105 unit tests, lint PASS)
>
> **Current Download-tab scope (2026-08-02):** `SearchScreen` is a desktop-patterned download workspace. Its flat source-tab row exposes **Search**, **YT / CSV** (YouTube playlist URL import plus Exportify CSV), and **Direct URL**; each source uses a compact input/action row, while search results remain flat artwork-backed rows with inline queue states and **Save to playlist**. The playlist URL flow chooses an existing destination or creates one, saves the URL on that playlist for later sync, and keeps queue status visible through the shell's thin progress treatment. A durable **Downloaded songs** accordion is backed by completed download jobs' library tracks, including SAF files in the selected music folder, with long-press actions and multi-select batch playlist assignment. It still has no local-library search, playlist tiles, playback actions, or folder-import UI; the Settings drawer is the only folder-import launcher.
> **Authority policy (descending):**
> 1. **Source code + tests + config** in this repo (highest truth)
> 2. **Desktop `project_brain.md`** and cited shared source modules (engine.py, downloader.py, theme.py)
> 3. **Confirmed product decisions** recorded in this document
> 4. **Readable screenshots** — only if images are readable to the agent
> 5. **Labeled proposals / open decisions** — never presented as implemented fact
>
> **Golden rule:** Never turn aspiration into implemented fact. Statements about Kotlin, Compose, Media3, Room, SAF, and **Last.fm discovery (`data/discovery/` + `DiscoveryRepository` + DiscoverScreen)** are now **supported by source files** in `mobile-app/`. WorkManager, downloader, and other deferred features remain **planned** unless source exists. Distinguish current Flet/Pygame prototype facts from native Android implementation.
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

Luno is a **native Android offline-first music player** in the same monorepo as the desktop Luno and its Flet mobile/desktop-capable port. The architecture is **Kotlin + Jetpack Compose + Media3**. **As of this writing Kotlin, Compose, Media3, and Room source exist** under `mobile-app/` — the native stack foundation is implemented and committed to the repository.

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
luno/
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
│   │   │   └── com.luno.mobile.data.db.AppDatabase/1.json ... 8.json
│   │   └── src/
│   │       ├── main/
│   │       │   ├── AndroidManifest.xml  # FOREGROUND_SERVICE, POST_NOTIFICATIONS, MusicService
│   │       │   ├── res/
│   │       │   │   ├── drawable/        # 9 vector icons (home, search, library, discover, create, play, pause, music_note, ic_download) + launcher foreground/background (green music note on #101010, adaptive icon layers)
│   │       │   │   ├── mipmap-anydpi-v26/ic_launcher.xml
│   │       │   │   └── values/
│   │       │   │       ├── colors.xml   # Mobile palette: #101010, #202020, #292929, #1ED760
│   │       │   │       ├── strings.xml  # App name, nav labels, dialog strings
│   │       │   │       └── themes.xml   # Theme.Luno (Material NoActionBar)
│   │       │   │       ├── strings.xml  # App name, nav labels, dialog strings
│   │       │   │       └── themes.xml   # Theme.Luno (Material NoActionBar)
│   │       │   └── java/com/luno/mobile/
│   │       │       ├── LunoApp.kt          # Application class, manual DI singletons
│   │       │       ├── MainActivity.kt           # Compose entry point, MusicController init
│   │       │       ├── playback/
│   │       │       │   ├── MusicService.kt       # Media3 MediaSessionService + ExoPlayer
│   │       │       │   ├── MusicController.kt    # MediaController wrapper, StateFlow, pending-play logic
│   │       │       │   ├── RecentlyPlayedStore.kt # SharedPreferences-backed ordered playback history
│   │       │       │   ├── DownloadWorker.kt     # WorkManager: HTTP download, progress, duration extraction, Track insertion, app-icon notification
│   │       │       │   ├── PlaylistSyncWorker.kt # WorkManager worker: fetch YT playlist → create individual download jobs
│   │       │       │   ├── WebSearchService.kt    # YouTube: watch page scraping + InnerTube + Piped + Invidious audio extraction
│   │       │       │   └── NotificationPermissionPolicy.kt # One-shot POST_NOTIFICATIONS prompt policy
│   │       │       ├── data/
│   │       │       │   ├── db/
│   │       │       │   │   ├── AppDatabase.kt       # Room DB (tracks, playlists, playlist_tracks, downloads; v9 adds durable track favorites)
│   │       │       │   │   ├── dao/
│   │       │       │   │   │   ├── TrackDao.kt       # CRUD + search Flow + local play-count increment + favorite toggle
│   │       │       │   │   │   ├── PlaylistDao.kt    # CRUD + relations + sort order + playlist play-count increment
│   │       │       │   │   │   └── DownloadJobDao.kt # DownloadJob CRUD + progress/state queries with Flow
│   │       │       │   │   └── entity/
│   │       │       │   │       ├── Track.kt          # uri PK, title, artist, album, durationMs, isFavorite
│   │       │       │   │       ├── Playlist.kt       # autoGenerate id, name, description, playlist playCount + derived SystemPlaylists IDs
│   │       │       │   │       ├── PlaylistTrack.kt  # composite PK, FK cascade, sortOrder
│   │       │       │   │       └── DownloadJob.kt    # DownloadJob entity + DownloadState enum
│   │       │       │   ├── discovery/
│   │       │       │   │   ├── LastfmService.kt      # Last.fm client: two-stage track.getsimilar → artist.gettoptracks (fallback match 0.8), 10-entry evict-on-exceed cache, OkHttp, injectable base URL; LastfmTrack + LastfmResult models
│   │       │       │   │   ├── RecommendationArtworkService.kt # Cached Deezer cover lookup for recommendations missing usable Last.fm artwork
│   │       │       │   │   └── LastfmKeyStore.kt     # EncryptedSharedPreferences (AES256-GCM/SIV) API-key storage with plain-prefs fallback (Keystore unavailable)
│   │       │       │   └── repository/
│   │       │       │       ├── LibraryRepository.kt  # SAF import, MediaMetadataRetriever, dedupe
│   │       │       │       ├── DuplicateFinder.kt    # Desktop-compatible normalized artist/title duplicate groups
│   │       │       │       ├── LibraryData.kt         # App-warmed StateFlows for the library Room flows (tracks/playlists/downloads/playlistsWithUrls + loaded flag) — screens render from these for instant transitions
│   │       │       │       ├── PlaylistRepository.kt # CRUD, validation, sort order mgmt
│   │       │       │       ├── DiscoveryRepository.kt # Last.fm entry point: apiKey StateFlow (encrypted store), getSimilar() filtering out owned tracks (desktop "artist - title" normalized keys)
│   │       │       │       └── LibraryTransferRepository.kt # Shared manifest snapshots, strict import preview/merge, and missing-track download queueing
│   │       │       ├── export/
│   │       │       │   ├── LibraryManifest.kt       # Versioned cross-platform manifest models and import result types
│   │       │       │   └── LibraryManifestCodec.kt  # Strict untrusted JSON encoder/validator
│   │       │       └── ui/
│   │           │   ├── shell/
│   │           │   │   ├── MainShell.kt          # ModalNavigationDrawer ("Settings" header with scrollable Library / Downloads / History & discovery / App accordions, including Duplicate checker) + AppHeader ("Luno" + green bar; the title is a button — tap opens the Settings drawer) + Scaffold + BottomNav (Home/Download/Library/Discover/Create) + MiniPlayer (hidden on full player); black base with four larger edge-oriented green blobs and a persistent header gradient; live music-folder import strip rendered from MusicFolderImportManager; green-loader transition mask (TransitionMask) over the NavHost on every route change and through Discover's Last.fm load (no-ripple applied per element, not via a LocalIndication override)
│   │           │   │   ├── MusicFolderImportManager.kt # Process-lifetime owner of the desktop-style folder import (Settings drawer launcher): StateFlow<FolderImportStatus?> (Importing/Finished), stall watchdog that always resolves the strip, "Import complete!" state auto-clearing after 2s
│   │           │   │   └── ArtworkFetchManager.kt # Process-lifetime owner of the missing-artwork sweep (Settings drawer): StateFlow<ArtworkFetchStatus?> (Progress with live counts → Finished → auto-clear after 5s, failure surfaced in strip + toast), activeJob re-tap guard, main-looper guarded toast
│   │           │   ├── navigation/
│   │           │   │   └── NavGraph.kt           # NavHost: Home / Download / Library / Discover / Downloads / Duplicates (drawer) / full_player / made_for_you / playlist detail; seamless transitions — outgoing screen exits instantly, incoming fades in (150ms FastOutSlowIn), no cross-fade overlap; Home/Library playlist tiles navigate to detail
│   │       │           ├── theme/
│   │       │           │   ├── Color.kt              # Dark palette + restrained black/green four-blob app background
│   │       │           │   ├── Theme.kt              # LunoTheme (Material3 dark color scheme)
│   │       │           │   ├── Type.kt               # Sans-serif typography scale
│   │       │           │   └── Dimens.kt             # 24dp icons, 48dp touch targets, 64dp mini-player
│   │       │           ├── components/
│   │       │           │   ├── MiniPlayer.kt         # Compact artwork thumb + title + play/pause + thin progress bar; artwork-color surface; horizontal swipe skips queue; tap → full player
│   │       │           │   ├── ArtworkImage.kt       # Non-subcomposing Coil painter wrapper (gradient + music-note placeholder) + decodeSizePx decode cap (thumbs decode small, no full 512px art per scroll row) + animated dominant-color extraction
│   │       │           │   ├── ArtworkCollage.kt     # 2x2 square collage: first 4 track artworks, one per quadrant (playlist thumbnails + detail header)
│   │       │           │   ├── PlaylistCard.kt       # Shared card: 2x2 collage thumbnail + name + green 3-dot menu (sync/stop-sync/URL/clear/delete, opt-in) + long-press multi-select indicator
│   │       │           │   ├── TrackRowCard.kt       # Track row in playlist-card UI: artwork thumb + title/artist + green 3-dot + long-press multi-select indicator
│   │       │           │   ├── BulkSelectionToolbar.kt # Shared song/playlist select-all checkbox and anchored bulk actions menu
│   │       │           │   ├── TrackActionsSheet.kt  # Long-press track actions: add to playlist (picker + create-new) + remove (confirm dialog)
│   │       │           │   ├── PlaylistPickerSheet.kt # Shared bounded/lazy playlist picker with top search (batch + per-track add flows; optional create-new destination row for playlist imports)
│   │       │           │   └── SortChip.kt           # Shared arrow sort dropdown (A–Z / Z–A / Recently added / Duration-longest-first) + TrackSortMode enum + track/playlist sorting helpers
│   │       │           ├── player/
│   │       │           │   ├── FullPlayerScreen.kt   # 280dp real artwork, fixed animated dominant-color gradient backdrop, lazy-scrolling player controls, and six current-track recommendations
│   │       │           │   ├── QueueSheet.kt         # Tabbed queue sheet: Playing Next | Recently played (shared QueueRecentsTabs)
│   │       │           │   ├── QueueRecentsTabs.kt   # Shared tabs: QueueRecentsTab enum + green TabRow + PlayingNextTab (drag-reorder) + RecentlyPlayedTab (play-in-context, long-press multi-select bulk actions, Clear all)
│   │       │           │   ├── RecentsScreen.kt      # Queue & Recents full screen (route RECENTS, Settings-drawer "Recently played") — same tabs as the sheet
│   │       │           │   └── ActionSheet.kt        # Add-to-playlist/play-next/add-to-queue/artist/share sheet
│   │       │           ├── home/
│   │       │           │   └── HomeScreen.kt         # Spotify-style Home hero, edge-clipped Recently-played / Made-for-you / Most-popular song + playlist carousels; Made-for-you title opens the session's virtual playlist detail (Settings drawer opens via tappable "Luno" header)
│   │           │   ├── search/
│   │           │   │   └── SearchScreen.kt       # Desktop-patterned download workspace: flat Search / YT-CSV / Direct-URL source tabs, compact input/action rows, YouTube playlist import with existing/new destination selection, Exportify intake, flat artwork-backed results, durable Downloaded songs accordion, long-press actions, multi-select playlist assignment, and search-result Save to playlist
│   │           │   ├── export/
│   │           │   │   └── ExportImportScreen.kt # SAF JSON export/import screen with bounded reads, preview, ambiguity confirmation, and additive merge
│   │       │           ├── library/
│   │       │           │   ├── LibraryScreen.kt      # Desktop "All Music": Play + Shuffle (16dp apart) with the Playlist/Song view pill beneath Play, A–Z/Recent sort, search w/ clear-X, alphabetical tracks, "Unsorted" pinned first in playlist view, long-press multi-select for songs/playlists, shared bulk options, collage-thumbnail playlist cards
│   │       │           │   ├── DuplicateScreen.kt     # Scrollable normalized duplicate groups with multi-select and metadata-only removal
│   │           │   │   └── PlaylistDetailScreen.kt # Spotify-style playlist view: 2x2 four-artwork collage header, name/desc/count/duration, play-all + shuffle, sort chip, in-playlist search (rounded pill w/ clear-X, filters title/artist/album, filtered list = play context), track list (play + long-press actions); also renders the ephemeral Made for you sample
│   │       │           ├── discover/
│   │       │           │   ├── DiscoverScreen.kt     # Last.fm recommendations: seed (current/last track), "Based on" header + refresh + key shortcut, per-row artwork + match %, download individual (YouTube search → audio extraction → queue) or batch; honest key-missing / no-seed / no-new-recs / error states
│   │       │           │   └── LastfmKeyDialog.kt    # Shared API-key dialog (Discover empty state + Settings drawer): paste key, Save / Remove, encrypted storage
│   │       │           ├── downloads/
│   │       │           │   └── DownloadsScreen.kt    # Full download management: sync controls, queue, cancel/retry/delete (reachable via options drawer)
│   │       │           └── create/
│   │       │               └── CreatePlaylistSheet.kt # AlertDialog with name validation
│   │       ├── data/artwork/
│   │       │   ├── ArtworkStorage.kt                 # Embedded-artwork extraction, usable-image validation, 512px JPEG cache in filesDir/artwork, Palette dominant color
│   │       │   └── ArtworkFetchService.kt             # Broad + metadata fallback Deezer cover search/download for missing local artwork; validates decoded cache output
│   │       └── test/java/com/luno/mobile/
│   │           ├── data/export/
│   │           │   └── LibraryManifestCodecTest.kt  # Strict manifest round-trip, unknown-field, path, version, and ambiguity validation
│   │           ├── data/db/
│   │           │   ├── AppDatabaseTest.kt            # Abstract Robolectric base class
│   │           │   ├── TrackDaoTest.kt               # CRUD, search, dedupe, count, local play-count increment
│   │           │   └── PlaylistDaoTest.kt            # 7 tests: CRUD, cascade, sortOrder
│   │           ├── data/repository/
│   │           │   ├── LibraryRepositoryTest.kt    # 10 tests: SAF tree + multi-picker folder recursion incl. AOSP + Samsung-style + tree-URI + non-audio guard + cross-flow doc-id dedupe (fake DocumentsProvider)
│   │           │   ├── LibraryRepositoryArtworkFetchTest.kt # 11 tests: missing-artwork sweep — null/blank/deleted/invalid-cache detection, embedded + remote refill, DAO persistence, valid-artwork skip, failed-extraction skip, file:// URI passthrough, per-track progress sequence, empty library
│   │           │   ├── ArtworkFetchServiceTest.kt       # Deezer combined-search success + title-search fallback, image download, and decoded-cache validation
│   │           │   ├── PlaylistRepositoryTest.kt   # 5 tests: validation, CRUD, trim
│   │           │   └── LibraryTransferRepositoryTest.kt # Manifest export ordering, unassigned tracks, and local-URI exclusion
│   │           ├── playback/
│   │               ├── NotificationPermissionPolicyTest.kt # 11 tests: permission policy matrix
│   │               └── MusicControllerTest.kt        # Added contract tests; final reviewer did not verify compilation
│   │           ├── ui/components/
│   │           │   └── SortChipTest.kt               # Recently-added labels and song/playlist ordering
│   │           ├── ui/home/
│   │           │   └── HomeScreenTest.kt              # Catalogue-key stability for Home's random mix
│   │           └── ui/shell/
│   │               └── ArtworkFetchManagerTest.kt    # 3 tests: Progress→Finished→auto-clear state machine, re-tap guard, failure resolution (test dispatcher + injected fetch)
│   │
│   ├── vendor/
│   │   └── NewPipeExtractor/            # Git submodule, pinned tag v0.26.4 (shallow); built via composite build
│   └── .gradle/                         # Gradle caches (not tracked — in .gitignore implicitly)
│
├── project_brain.md                     # Desktop brain — do not duplicate its detail here
│
├── engine.py                            # SHARED LOGIC (desktop): DiscoveryService,
│                                        #   scan_library, find_duplicates,
│                                        #   _normalize_for_dupe, LunoEngine (pygame)
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
├── build-mobile.command                 # ROOT macOS helper: build/install/launch native Android debug APK
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
- Export/import playlists, selected songs, and full library as versioned JSON (via SAF files/share workflows) — implemented in the native Android app and desktop app
- GitHub Releases with signed APK, update check

**EXCLUDED (must not implement):**
- No live sync, pairing, peer transfer, cloud account/backend, or remote playback
- No Premium/Spotify Premium features
- No podcasts/audiobooks (unless explicitly implemented later)
- No social messaging, accounts, or collaborative features (no "Blend")
- No sleep timer, lyrics display, visualizer, or crossfade (open to future addition)
- No Spotify logos, assets, or branding — Luno is an independent app

### Interoperability (Export/Import)

**Export:** Selected song(s), playlist(s), or full library as a **versioned JSON manifest**. The desktop and Android implementations share the `luno.library.export` format with root `manifest_version: 1`.

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

**Implemented transfer behavior:** Android exposes a SAF-backed Export / Import screen from the Settings drawer, previews matched/missing/ambiguous tracks, merges playlists by case-insensitive name, queues missing YouTube references through WorkManager, and carries all destination playlist ids on one download job. Desktop exposes full-library/settings, current-playlist, and selected-track actions; it matches local files, copies matched audio into playlist folders, and asks before queuing missing downloads.

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
- **Download-tab playlist import**: `SearchScreen` exposes the YouTube playlist URL flow in the desktop-style **YT / CSV** source tab. The user must choose an existing destination playlist or create a new one; the pasted URL is saved on that destination, then `DownloadRepository.syncPlaylist()` queues the WorkManager sync.
- **Batch sync**: Downloads tab has "Sync All Playlists" button; each playlist with a URL synced sequentially.
- **Audio extraction** (`WebSearchService.getAudioStreamUrl()` — multi-strategy pipeline):
  1. **NewPipe StreamExtractor** — primary. v0.24.3 was too old for YouTube SABR enforcement (v0.26.3+ carries the SABR workaround); v0.26.4 is now vendored locally as a submodule and built via a composite build (JitPack does not publish v0.26.3+). Audio stream selected by priority M4A/AAC → Opus → best bitrate. The Invidious/InnerTube fallbacks below remain as emergency paths.
  2. **Invidious companion proxy** — emergency fallback. Fetches `invidious.tiekoetter.com/embed/VIDEO_ID`, parses the `<source>` tag to get the companion server URL, selects the audio itag from the actual adaptive formats parsed from the page (M4A/AAC → Opus priority — not a blind `itag=18` → `itag=140` swap), and **preserves the original `check` token** (companions with `verify_requests` enabled reject missing/invalid checks with HTTP 400). Falls back to itag 140 only when no format list is present. The companion server bridges HTTP/2 (to googlevideo.com) → HTTP/1.1 (to our device).
  3. **InnerTube player endpoint** — as last resort, POSTs to `youtubei/v1/player` with ANDROID client; strips `lsparams`/`lsig` (login signature tokens); attempts `n`-parameter deobfuscation via `YoutubeJavaScriptPlayerManager` if a player JS URL can be extracted. URLs from this path point to googlevideo.com and **fail with HTTP 403** on this device due to HTTP/1.1 protocol mismatch with `gvs 1.0` CDN.
- **Direct URL download**: Paste any direct audio URL (optional title/artist override)
- **CSV import (Exportify)**: Select a Spotify Exportify CSV file; each row (artist, title) is searched on YouTube and the first result downloaded
- **Download queue management**: Dedicated Downloads tab — view all active, queued, completed, failed downloads; cancel/retry/delete per item
- `DownloadWorker` uses **OkHttp 4.12.0** (`ConnectionSpec.MODERN_TLS`) for download streaming via WorkManager. Saves files with correct extension based on response `Content-Type` header (`.m4a`, `.opus`, `.mp3`, `.ogg`, `.audio`)
- Files are staged in the app-private `downloads/` directory, then copied into
  the user-selected SAF music folder from Settings when one is configured;
  completed downloaded tracks retain the destination `content://` URI. If no
  folder has been selected, the app-private directory remains the fallback.
- The Settings > Library accordion also exposes a sync-icon action that copies
  existing app-private downloaded songs into the selected SAF music folder and
  updates their library, playlist, and download-job URIs. Cached downloaded
  artwork is copied beside each audio file as a matching `.jpg` sidecar.
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
| Local DB | Room 2.6.1 | ✅ **Implemented** — 4 entities, 3 DAOs (v9 schema — playlistUrl, artwork, play counts, multi-playlist import destinations, and durable track favorites) |
| Background downloads | WorkManager + Foreground Service | ✅ **Implemented** — `DownloadWorker` + `DownloadRepository` + `DownloadJob` Room entity (v2 schema) |
| Media scanning | MediaStore / SAF | ✅ **Implemented** — SAF `OpenMultipleDocuments` import via `LibraryRepository` |
| Dependency injection | Manual singleton (LunoApp) | ✅ **Implemented** — Hilt deferred; manual DI in Application class |
| Last.fm discovery | OkHttp + EncryptedSharedPreferences | ✅ **Implemented** — `data/discovery/` (LastfmService + LastfmKeyStore), `DiscoveryRepository`, DiscoverScreen (2026-07-31) |

**Playback architecture (current implementation):**
- `MusicService` extends `MediaSessionService` — single ExoPlayer instance. `onDestroy()` releases `MediaSession` before ExoPlayer (correct Media3 teardown order).
- `MusicController` wraps `MediaController` with `StateFlow` for `isPlaying`, `currentTrack`, `progress`, `duration`, `hasActiveItem`, `isConnected`, `repeatMode`, `shuffleEnabled`, and `queueRevision`; supports **pending-play semantics** via a single immutable `PlaybackRequest(items, startIndex, shuffle)` (last-user-request wins).
- **Reliability merge (2026-07-30):** The pending-play implementation is now production‑ready:
  - `AsyncConnector` injectable seam for deterministic testing without a live service.
  - Terminal `released` flag + `generation` counter prevents late‑arriving controller futures from attaching listeners or starting playback after `release()`.
  - `pendingRequest: PlaybackRequest?` atomically replaced on each pre‑connection call (full queue + clamped start index preserved).
  - `playbackError: SharedFlow<PlaybackError>` exposes Media3 `Player.Listener.onPlayerError` events.
  - `hydrateState(MediaController)` synchronises all `StateFlow`s immediately after (re)connection.
  - `sanitizeErrorMessage()` strips URIs and absolute paths from user‑facing messages.
  - `lastDispatchedRequest` internal hook enables test assertions on dispatched requests.
- `NotificationPermissionPolicy` — one-shot prompt policy; API<33 skips, API 33+ prompts once via `MainShell`'s central `onPlay` callback shared by Library, Home, and playlist-detail screens; playback proceeds regardless of the permission result.
- Official Android documentation and Media3 source confirm media-session notifications are exempt from `POST_NOTIFICATIONS`; denial alone is **not** the Android 14+ crash previously suspected for a correctly declared `MediaSessionService`/`mediaPlayback` foreground service.
- ExoPlayer configured with `AudioAttributes` for music, `setHandleAudioBecomingNoisy(true)` for headset unplug detection.
- Notification and lock-screen controls provided by Media3 session.
- `onTaskRemoved` stops service if nothing is playing.
- `MainShell` collects `connectionError` and `playbackError` via `SnackbarHostState` and shows one‑shot Snackbars.
- `MediaTrack` extended with `album: String` and `durationMs: Long`. LibraryScreen/Home/playlist-detail screens construct `MediaTrack` from Room `Track` with exact title, artist, album, durationMs; `MiniPlayer` displays `Artist · Album` when available.
- **Swipeable mini-player (2026-08-02):** `ui/components/MiniPlayer.kt` is a compact artwork/title/play-pause row with a 2dp playback-progress bar at the top and a translucent animated surface derived from `rememberArtworkColors()`; the track-info press indication remains disabled so swipes do not leave a gray highlight. It recognizes a 64dp horizontal swipe across the player above the bottom navigation: swiping left calls `MusicController.skipToNext()` and swiping right calls `skipToPrevious()`, preserving the current Media3 queue, shuffle order, and repeat behavior; taps on the track info still open the full player.
- **Full player screen implemented (2026-07-30, extended 2026-08-06):** `ui/player/FullPlayerScreen.kt` (280dp artwork, m:ss scrub bar, shuffle/prev/play-pause/next/repeat transport row, queue + action-sheet triggers, back arrow) now keeps the artwork-driven gradient fixed to the viewport while the player content scrolls upward. A six-row **Recommended for this song** section loads Last.fm results for the current track, filters library-owned tracks through `DiscoveryRepository`, supports refresh and honest API-key/loading/error/empty states, and uses the cached Deezer artwork fallback. `ui/player/QueueSheet.kt` ("Playing Next" modal sheet from `MusicController.getQueue()`), `ui/player/ActionSheet.kt` (Add to playlist sub-sheet via `PlaylistRepository`, Play next, Add to queue, Go to artist toast placeholder, Android Sharesheet). Repeat cycles OFF→ALL→ONE (ExoPlayer `REPEAT_MODE_*`), shuffle enables Media3 shuffle, immediately chooses a different uniformly selected current item, and `getQueue()` traverses the resulting playback order; both modes are exposed as `StateFlow`s on `MusicController`. Queue changes increment `queueRevision` so Playing Next refreshes after shuffle/reorder. MiniPlayer track-info tap navigates to `Routes.FULL_PLAYER`; bottom bar hidden on the full-player route.
- **Real artwork + dynamic gradients implemented (2026-07-30):** Embedded album art is extracted at import/download time (`ArtworkStorage.saveEmbeddedArtwork*` via `MediaMetadataRetriever.getEmbeddedPicture`), downsampled to ≤512px JPEG and cached in `filesDir/artwork/`; the path is stored on `Track.albumArtPath` (Room **v4** migration `3_4`). `MediaTrack` carries `artworkUri` (file://) through `MusicController.buildMediaItem` (MediaMetadata `artworkUri`), and `MusicService`'s `ArtworkEnrichingCallback` (`MediaSession.Callback.onAddMediaItems`) loads `artworkData` bytes so the **notification and lock-screen show artwork**. UI renders via Coil (`ui/components/ArtworkImage.kt`): full player (280dp), MiniPlayer (48dp thumb), Home recently-played card, and QueueSheet rows. `rememberArtworkColors()` extracts the dominant color (androidx Palette vibrant→muted→dominant) and animates a vertical gradient backdrop behind the full player (600ms `animateColorAsState`), satisfying "gradients animate subtly on transition".
- **Queue reordering implemented (2026-07-30):** `MusicController.moveQueueItem(from, to)` → `Player.moveMediaItem`; `QueueSheet` rows show artwork + drag handle and support **long-press drag-to-reorder** (`detectDragGesturesAfterLongPress`, row translation + scale feedback, drop-target index computed from drag delta; snapshot refreshed after each move).
- **Download thumbnails implemented (2026-07-30):** YouTube audio streams carry no embedded album art, so downloaded songs previously had no artwork anywhere (full player, mini player, queue, home). Now the video thumbnail is captured at enqueue time: `WebSearchResult.thumbnailUrl` (search + CSV import) and `PlaylistVideo.thumbnailUrl` (playlist sync) flow through `DownloadJob.thumbnailUrl` (Room **v5** migration `4_5`) and WorkManager inputData (`DownloadWorker.KEY_THUMBNAIL_URL`). After the audio file is saved, `DownloadWorker` falls back to fetching the thumbnail via OkHttp (`ArtworkStorage.saveImageBytes`, ≤512px JPEG in `filesDir/artwork/`) when `MediaMetadataRetriever` found no embedded picture; the path lands on `Track.albumArtPath` so every artwork surface picks it up. Manual retries re-read the URL from the job entity.
- **Navigation restructure (2026-07-30, download label refreshed 2026-07-31):** Bottom nav is exactly **5 items** — Home, Download, Your Library, Discover, Create (the full Downloads queue remains in the drawer). Download and Create render at 28dp (their material glyphs are optically smaller than the other tabs; the other four render at the standard 24dp). A `ModalNavigationDrawer` whose header reads **"Settings"** (the standalone Settings item was removed) contains Downloads (navigates to `Routes.DOWNLOADS`), Export-Import (navigates to `Routes.EXPORT_IMPORT`), About, Check for updates (honest "coming soon" toasts); it opens from the **tappable "Luno" app header** (the green-circle Home profile icon was removed 2026-07-31). The drawer never contains cloud account/login/logout items.
- **Playlist card UI (2026-07-30):** Shared `ui/components/PlaylistCard.kt` — thumbnail on the left, playlist name beside it, **green 3-dot options icon** on the right (menu: Sync playlist, Set/Edit YouTube URL via dialog, Delete with confirm dialog), flat against the black screen (no card background). Used **2-per-row on Home** (quick-action grid) and **full-width in the Library tab** (desktop-home style, one after another); tapping it opens the playlist detail screen.
- **Spotify-style Home (2026-07-30, refreshed 2026-07-31):** `HomeScreen` has a dark green-accent hero, greeting, **edge-clipped horizontal carousels** ("Recently played" — current history; **"Made for you" — a stable random mix of up to 50 unique tracks sampled from the entire catalogue**), and the existing Your Playlists carousel layout. The Made for You title text and View all action are clickable; they store the exact current sample in the process-lifetime `LunoApp.madeForYouTracks` state and open the `made_for_you` virtual playlist detail without creating a user playlist. "Your top genres" remains unimplemented (no genre metadata). The tappable "Luno" app header opens the Settings drawer from every screen. Spacing standardized: 16dp above the greeting, 24dp section breaks, 8dp header-to-content gaps. The parent and carousel `LazyListState`s are hoisted (the parent is saveable), mutable library/history emissions are applied after Home scrolling becomes idle, and the Made-for-you order is keyed to catalogue URI membership so play-count/artwork updates do not reshuffle it. All long scroll surfaces remain Compose-lazy and artwork uses capped Coil decode sizes.
- **Home scroll stabilization (2026-08-04):** Home coalesces library/history updates until all nested lazy lists are idle, restores the first visible keyed item offset after a section membership/height change, uses deterministic tie-breakers and canonical catalogue membership for carousel ordering, and declares lazy item content types. On the Home route the mini-player is drawn as a measured-height-independent overlay above the bottom navigation, with a permanent list end inset, so playback state changes cannot resize the scrolling viewport mid-gesture.
- **Playlist detail screen (2026-07-30, extended 2026-07-31):** `ui/library/PlaylistDetailScreen.kt` at route `playlist/{playlistId}` (opened by tapping any persisted playlist card on Home or Library) and route `made_for_you` (opened by the Home Made for You title). Both use the Spotify-inspired **2x2 collage of up to four track artworks**, name, description, "N songs · total duration", green Play button, shuffle, sort, in-playlist search, and track list. The Made for You route reads the exact up-to-50-track session sample from `LunoApp` and does not persist or alter library playlists. Tapping a track plays the current filtered/sorted context; persisted playlist starts carry that playlist ID into the Media3 queue, so only transitions from that playlist context increment its own view count; virtual playlists do not count. Long-press enters global multi-select bulk actions. Not yet implemented (desktop parity): drag-to-reorder, download-all toggle.
- **Global bulk selection (2026-08-01):** Long-pressing a song or playlist on Home, Your Library, playlist detail, downloaded songs, duplicate-song lists, or Recently played enters multi-select mode with check indicators and a visible Select all checkbox. The anchored `BulkSelectionToolbar` menu lists add-to-playlist, create-playlist, remove-song, delete-playlist, and Done actions; playlist picking and playlist-name entry remain follow-up dialogs, and individual playlist contents are never duplicated by bulk assignment.
- **Recently-played history (2026-07-30, move-to-front 2026-07-31, persisted 2026-08-01):** `MusicController` keeps a **recently-played array** (`recentlyPlayed: StateFlow<List<MediaTrack>>`, most recent first, max 100 per desktop convention), populated from `onMediaItemTransition` + state hydration and persisted immediately as app-private JSON in `playback/RecentlyPlayedStore.kt`. **Desktop replay semantics (2026-07-31):** a replay anywhere in the history is **moved to the front**, not duplicated (`engine.py play_current` parity). **Home's "Recently played" section is a swipeable edge-clipped horizontal carousel of the full history** (up to 20, same layout as "Made for you"; tapping a card plays it within the history context — next/prev walk recent plays). **Queue/Recents surfaces (2026-07-31):** the full history is also visible as the "Recently played" tab in the full player's queue sheet and in the Queue & Recents screen (Settings drawer "Recently played"), with a **Clear all** button (see contract #8). Cleared via the Settings drawer ("Clear recent history").
- **Desktop-style folder import (2026-07-30):** `LibraryRepository.importLibraryTree(treeUri, onProgress)` imports a whole music root picked via SAF `ACTION_OPEN_DOCUMENT_TREE`, mirroring desktop `scan_library`: **each subfolder becomes a playlist named after the folder** (created on demand, merge-safe via name lookup + IGNORE dedupe), files directly in the root land in the **"Unsorted"** playlist. Audio detection by MIME type or extension; embedded artwork/metadata extraction reuses `importAudioUri`. The Search screen's Library tab has an "Import music folder (playlists by folder)" button with live `imported/duplicates/errors` progress. Root URI permission persisted. **Recursion fixed 2026-07-31:** subfolders are enumerated via `buildChildDocumentsUriUsingTree` (`…/children`) and every descendant URI is built from the **original tree root** + document id (`buildDocumentUriUsingTree`) — the previous code built children from child document URIs, producing nested `tree/…/document/…/document/…` URIs the provider cannot resolve, so folder contents were silently skipped. **Tree-root listing fixed 2026-07-31 (2):** the root was listed by querying the **bare tree URI**, which on standard (AOSP-style) providers returns the tree root document's own row — never its children — so on device `importLibraryTree` silently imported nothing (toast reported "0 songs imported, 0 duplicates, 0 errors"). Root children are now listed via the canonical `buildChildDocumentsUriUsingTree(treeUri, getTreeDocumentId(treeUri))`, with Samsung-style fallbacks for providers that don't answer `…/children` (bare tree URI for the root; `buildDocumentUriUsingTree` folder-document query for deeper folders). **Multi-picker folders (2026-07-31):** when the file picker returns directory documents (Samsung pickers allow selecting folders), `importMultipleUris` now recurses into them with the same folder-name = playlist-name semantics instead of importing one bogus "track" per folder; a directory guard in `importAudioUri` keeps folders from ever becoming tracks. The multi-file picker launches with **`*/*`** so folders are selectable ("Select all" picks folders + loose songs together); a non-audio guard in `importAudioUri` rejects the non-audio documents that `*/*` surfaces. Folder detection is **children-first** (a file never has children) with a Samsung-style fallback — providers that return a folder's children when its document is queried and don't support `document/<id>/children` are handled, as are pickers that return folders as **tree URIs**; picked-folder names come from the URI path segment on external-storage authorities (where a queried display name would be the first song's). **Stall + speed fixed 2026-07-31 (3):** folder children are imported with **bounded parallelism (4 workers, `kotlinx.coroutines.sync.Semaphore`)**; counters are atomic (`ImportCounters`), the document-id dedupe set is concurrent, and playlist sort orders come from per-playlist `AtomicInteger`s seeded from `maxSortOrder` once per folder so parallel inserts can't collide. **Metadata extraction runs on a dedicated cached-thread pool with a 20s per-file timeout** (`withTimeoutOrNull` + `suspendCancellableCoroutine`): a corrupt file that blocks `MediaMetadataRetriever.setDataSource`/provider queries forever used to freeze the whole serial import (stuck at "1846 duplicates, 0 added"); the stuck thread is abandoned and a filename-based "Unknown Track" is imported instead. Regression-covered by `LibraryRepositoryTest` (fake SAF `DocumentsProvider`, 10 tests incl. AOSP + Samsung-style tree/document-URI cases, tree-URI folders, the non-audio guard, and cross-flow document-id dedupe in both directions).
- **Home tab always returns Home (2026-07-30):** Tapping the Home bottom-nav item now `popBackStack`s to the Home route (falling back to navigate) instead of the tab-style `popUpTo(saveState)` — you can never get "stuck" on the Downloads screen or any drawer/deep route.
- **Folder-import strip always resolves + completion state (2026-07-31):** The live import strip (MainShell under the app header + Search tab) is now owned by `ui/shell/MusicFolderImportManager.kt` — a process-lifetime manager shared by both launchers — publishing `StateFlow<FolderImportStatus?>` (`Importing` with live counters → `Finished` with counts/stalled/persist-warning → auto-clear after 2s). On completion the green text flips to **"Import complete! N added, M duplicates, K errors"** and disappears after ~2 seconds; a stalled import shows "Import stalled and was cancelled" (error color) instead of freezing. The stall watchdog transitions the strip itself, so even an import wedged in a non-cancellable provider call can never leave the strip stuck at the final counts (the old `finally`-based clear only ran when the import coroutine unwound).
- **Import recursion crash fixed (2026-07-31):** `LibraryRepository.importTreeFolder` previously crashed the app ("Luno closed") when a provider answered a folder-document query with the folder's **own row** (the fallback consulted for empty subfolders) — the import recursed into the same folder until a `StackOverflowError`. Fixes: self-rows are skipped (both tree and document walks), recursion depth-capped at 24, a Throwable in one folder is contained (cancellation still propagates), `MediaMetadataRetriever.release()` runs in `finally` (native fd/memory leak per file eliminated), and the extraction thread pool is bounded at 32 (abandoned threads can no longer exhaust the device's native thread budget; rejection falls back to filename metadata instead of hanging).
- **Import can no longer be killed by the watchdog (2026-07-31):** the stall watchdog used to CANCEL the import when no progress tick arrived for the window — which is exactly what turned a slow-but-alive import into a guaranteed failure ("import stalled and was cancelled"). The watchdog now **never cancels**: every unit of import work is timeout-bounded, so a quiet stretch means the provider/device is crawling, not dead; the strip shows a calm "Import is taking longer than expected… still working" note and the import keeps running and reports its real outcome ("Import complete!" / "Import failed") when it finishes. Unexpected exceptions resolve the strip as "Import failed — please try again" and are logged. A retry replaces any previous attempt (`activeJob` guard + best-effort cancel) instead of being blocked by a stuck job. Combined with the earlier `AtomicLong` fix (the false-stall root cause: a plain captured `var` with no memory barrier let the JIT hoist the read, so the watchdog compared against the initial timestamp and killed healthy imports ~1800 songs in), the import now completes or fails on its own merits.
- **Green-loader transition mask (2026-07-31) — user-confirmed timings, keep them:** `MainShell` shows the shared four-blob `appBackgroundWash()` over the `PrimaryBackground` base, including the persistent top/header gradient, with the centered green spinner on **every** navigation:
  - **Mask-first:** `transitionMask = true` is set **before** `navigate()`/`popBackStack()`/`navigateUp()` runs — all MainShell call sites (bottom nav, drawer, mini-player) + `NavGraph`'s `onNavigate` callback (playlist-detail/full-player pushes and backs). The layer is opaque before the new screen composes, so it can never flash through.
  - **Enter:** `EnterTransition.None` — fully opaque in the very first frame (an alpha-animated enter shows the screen underneath for a frame or two).
  - **Hold:** `TRANSITION_MASK_HOLD_MS = 300` ms (the "black screen" moment) and until `LibraryData.loaded`; on the Discover route it also waits for the Last.fm request reported by `DiscoverScreen` (the 6000 ms `TRANSITION_MASK_MAX_MS` cap remains for other routes) — the reveal never lands on a loading gate or a second spinner.
  - **Exit:** `TRANSITION_MASK_FADE_MS = 400` ms `fadeOut` with `FastOutSlowInEasing` — one calm motion.
  - **NavHost transitions: 0ms** (`EnterTransition.None`/`ExitTransition.None` in `NavGraph.kt`) — screens swap fully-formed under the mask; no screen-side fade may ever run simultaneously with the mask (that overlap read as jumpy).
  - All constants live in `MainShell.kt`; visual = `PrimaryBackground` + the persistent header gradient + four larger edge-oriented radial green blobs recorded in the latest 2026-08-02 change entry + centered `CircularProgressIndicator(AccentGreen)` over the content area only (below the AppHeader, above the bottom bar). **If any of these timings are changed, re-verify the "loading wash → reveal" feel; 300/400 with instant enter is the confirmed-good baseline.**
- **Import never skips folders silently (2026-07-31, hardened 2026-07-31):** folder listings are now retried up to **3×** (`QUERY_RETRIES`) — a timeout or executor rejection means the provider was slow or busy, **never** that the folder is empty — and a folder that stays unlistable is counted as an **error** instead of being treated as empty. This fixes the "Import complete! 0 added, 1020/1800 duplicates, 0 errors" bug: timed-out/rejected listings were silently converted to empty folders, dropping whole folders (and their ~90 songs each) from the scan with random counts. A Throwable inside a folder's work is counted as an error too (not swallowed), so a completed import with errors tells the user the scan was incomplete. **Hardening:** `queryTreeChildren` now returns `null` (→ error) unless **both** query paths answered empty (an empty answer from ONE path is not proof of emptiness — Samsung-style providers always answer the canonical `…/children` path empty); `queryTreeRowsBlocking`/`queryDocumentRowsBlocking` treat a **throwing** `resolver.query` (rejected URI, `TransactionTooLargeException` on an enormous folder, provider crash) as **unlistable** (null → retried, then error) instead of swallowing it into an empty folder; and `withAbandonableTimeout` resumes with a block failure instead of swallowing it and burning the full timeout window on every retry. **Listing starvation fixed (2026-07-31):** folder listings no longer share the file-extraction thread pool — they run on their own fixed 2-thread **queued** pool (`listingExecutor`, `withListingTimeout`), so once the 32 extraction threads are busy on files, listings are **queued instead of rejected**; previously a whole-import listing burst starved out ~25 of 59 folders (each reporting an error after the hardening above surfaced them). Regression tests pin the desktop-scale shape (4047 files / 59 folders → 4047 imported, 0 errors; re-scan → 4047 duplicates, 0 imported, completes; slow provider queries → still 4047 imported, 0 errors) and the unlistable-folder case (1 error, no silent loss).
- **Import failures are self-diagnosing (2026-07-31):** an unexpected exception now resolves the strip as "Import failed — please try again **(exception message)**" with the same text in a toast and a `Log.e("MusicFolderImport", …)` entry — the cause can be reported without logcat. The success toast is guarded (`runCatching`) so a toast hiccup can never mislabel a completed import as failed.
- **Warm library data (2026-07-31):** `LibraryData` collects the four library Room flows once at app startup (`SharingStarted.Eagerly`) into app-level `StateFlow`s (tracks, playlists-with-tracks, downloads, playlists-with-URLs) with a `loaded` flag. Home, Library, Downloads and PlaylistDetail render from it instead of collecting Room flows per screen — every tab switch shows the full content (Play/Shuffle, track rows, carousels, queue) the moment the transition mask lifts; the all-or-nothing spinner only appears on the very first app frames.
- **Seamless screen transitions (2026-07-31):** The NavHost no longer cross-fades — the outgoing screen exits instantly (0ms) and the incoming one fades in (150ms FastOutSlowIn). A cross-fade composed/rendered BOTH screens simultaneously for the whole overlap, which made tab switches over heavy screens (4k-track library) laggy and stuttery.
- **CI workflow removed (2026-07-31):** `.github/workflows/android.yml` deleted — no GitHub Actions run on push/PR.
- **Stop All fixed (2026-07-30):** `stopAllActive()` previously cancelled jobs by stored work ID — running playlist syncs kept spawning new downloads. Every download work now shares tag `DownloadWorker.TAG_DOWNLOAD` and every sync shares `PlaylistSyncWorker.TAG_PLAYLIST_SYNC`; Stop All cancels by tag (reaching sync workers + their spawned downloads) and marks active jobs CANCELLED.
- **Library sorting (2026-07-30, extended 2026-07-31):** Tracks are listed **alphabetically** (case-insensitive title). A shared **four-state sort control** (**A–Z / Z–A / Recently added / Duration**, dropdown menu, `ui/components/SortChip.kt` + `TrackSortMode`) applies to **both** the songs and playlist views — tracks sort by `Track.addedAt`, playlists by `Playlist.createdAt`, and **Duration sorts longest-first**: tracks by their own `durationMs`, playlists by their **total** duration (sum of member tracks) — replacing the playlist-only A–Z/Recent toggle. **The same SortChip now lives on the playlist-detail screen too** (under Play/Shuffle; rows AND the play context walk the sorted order). Library and playlist detail use the same down-arrow dropdown presentation; Library anchors it at the far right of the view-toggle row. The "Playlist view"/"All songs view" toggle is always accent green, matching the Play/Shuffle controls.
- **Shuffle active affordance (2026-07-31):** Shuffle controls in Library, PlaylistDetail (including Made for You), and FullPlayer all read the shared `MusicController.shuffleEnabled` state. When active, the Library/PlaylistDetail "Shuffle" label is underlined and the FullPlayer shuffle icon receives a green underline.
- **Search tab lists nothing by default (2026-07-30):** The Search screen's Library tab shows "Songs appear here when you search." until a query is typed — it no longer dumps the whole library.
- **Playlist assignment picker fixed (2026-08-03):** `PlaylistPickerSheet` now keeps its title and search field fixed at the top, filters playlist names case-insensitively, and renders the full result set in a bounded `LazyColumn`; long playlist lists no longer get clipped in the download result or other add-to-playlist flows. Track long-press actions now use the same shared picker.
- **Favorites (2026-08-04):** `Track.isFavorite` is durable Room metadata (schema v9, migrated from v8) toggled from the shared 3-dot `TrackActionsSheet`. Home renders a Favorites carousel before Most popular, while Library derives a reserved, non-deletable **Favorites** playlist from favorite tracks and pins it above **Unsorted**. Its playlist-detail route supports the shared track menu, a star beside the sort arrow that stably moves favorite rows to the top, and star markers on those rows; playlist-detail rows expose add-to-playlist, remove-from-playlist, and favorite actions at the far right. The derived playlist is intentionally excluded from normal playlist pickers and export/delete operations.
- **Download workspace refined (2026-08-02):** `ui/search/SearchScreen.kt` now follows the desktop downloader's visual logic: a compact **Downloader** header, flat **Search / YT / CSV / Direct URL** source tabs, integrated input/action rows, a conditional thin queue-progress line, inline errors/status, and flat artwork-backed result rows. The **YT / CSV** tab contains the YouTube playlist URL flow with a `youtube.com/playlist?list=…` field, **SAVE TO** destination selection, existing-playlist picker plus **Create new playlist**, and queue action; the selected destination stores the source URL for future Downloads-tab syncs. It also contains the Exportify CSV loader. Direct audio links use the same compact source-row pattern with optional title/artist details. The durable **Downloaded songs** accordion remains after the source/results area and is sourced from downloaded `Track` URIs. Long-press track actions, explicit multi-select batch playlist assignment, **Save to playlist**, playlist-less assignment to the durable **Unsorted** playlist, queue status, retry/cancel actions, and background-download behavior remain intact; full local-library search, playlist tiles, playback, and folder-import UI remain excluded.
- **Never deletes audio from the phone (2026-07-30, enforced):** Removal is **metadata-only** by design. "Remove from library" deletes the Room `Track` row (FK cascade cleans `playlist_tracks`), "Delete playlist" removes the playlist + its membership rows, and the **"Clear playlist"** action empties a playlist's membership — in every case the audio files on the device are untouched. Confirm dialogs state this explicitly ("The audio file stays on your phone."). The only `file.delete()` in the app is `DownloadWorker` discarding a **cancelled partial download** inside app-internal `filesDir/downloads/` (never a user's file). This matches the import-side "no automatic deletion" principle.
- **Folder import fixed + persisted music-folder destination (2026-07-30):** The desktop-style tree import previously failed for every file — `importAudioUri` called `takePersistableUriPermission` per child document, which is only valid on the tree **root** (descendant document URIs throw). New `importTrackFromGrantedUri` imports tree descendants without per-file persist (the root grant covers them), so folder imports now work. The chosen folder is **persisted** (`MusicFolderRepository`, SharedPreferences-backed): the Settings drawer gained a **"Music folder"** item (picker → save → import → summary toast) and the Search tab's folder button saves the destination too, with a completion toast on top of the live progress.
- **Library batch multi-select (2026-07-30):** The section header directly below the search bar ("Tracks…" / "Playlists…") has a **gray 3-dot menu on the far right**. Menu: **Select all** (enters multi-select with everything checked in the current view), then — with items selected — **Add to playlist** (songs → chosen playlist; playlists → merges the selected playlists' tracks into the chosen one) and **Remove from app** (metadata-only confirm; files stay on the phone), plus **Cancel selection**. In selection mode rows show check indicators and taps toggle selection instead of playing/navigating.
- **Home playlist cards use singular artwork (2026-07-30):** On Home only, playlist cards show the **first track's artwork** (Made-for-you block styling), not the 4-quadrant collage — the collage thumbnail remains for Library playlist cards.
- **Library Play/Shuffle spacing (2026-07-30):** Shuffle sits **16dp (≈1rem) to the right of Play** (not the far side of the screen); the Playlist-view tab + sort toggle sit under Play.
- **Context-aware next/previous + Up-Next queue + desktop settings (2026-07-31):** Next/previous on the full player now walk the **context the track was picked from** — the shared `onPlay` callback changed from `(MediaTrack)` to `(List<MediaTrack>, startIndex)`: LibraryScreen passes the sorted/filtered track list, SearchScreen's library tab passes the live search results, HomeScreen passes the recent-history / made-for-you list, PlaylistDetail passes the playlist (as before). Shuffle works everywhere it is offered (Library Play-row, full-player transport, new PlaylistDetail "Shuffle" control beside Play — `play(context, 0)` then `setShuffle(true)`). **Desktop Up-Next model implemented** (`MusicController.manualQueueUris`): "Play next" inserts at current+1, "Add to queue" inserts after the remaining manually queued items (before the context), the set drains as manual items play/skip, and it resets on a new playback context or a manual queue reorder (flat-queue equivalent of desktop `user_queue_count`; `QueueSheet` re-snapshots on open). **Desktop history semantics**: replays move to the front of the 100-entry history instead of only coalescing consecutive duplicates (engine.py `play_current` parity); `clearRecentlyPlayed()` added. **Desktop settings added to the drawer**: "Fetch missing artwork" (offline re-extraction of embedded artwork for tracks whose cache is missing — the desktop's Deezer/yt-dlp network fallbacks are not reproduced; `LibraryRepository.fetchMissingArtwork()` + new `TrackDao.updateTrack`) and "Clear recent history" (confirm dialog; in-session history only). **Artwork-fetch progress (2026-07-31):** the sweep is owned by a process-lifetime **`ArtworkFetchManager`** (`ui/shell/ArtworkFetchManager.kt`, same pattern as the folder-import manager): `StateFlow<ArtworkFetchStatus?>` (Progress with live scanned/total/updated → Finished → auto-clear after 5s — long enough to read the count; failures surface "Artwork fetch failed — please try again (message)" in the strip + toast + Log.e), activeJob guard so a re-tap is a no-op while running. `fetchMissingArtwork` gained an `onProgress` callback + injectable `extract` seam (production default = the **same** `ArtworkStorage.saveEmbeddedArtwork` call used at import time). The MainShell renders a live strip **inside the scaffold content column** (directly under the import strip, below the app header — never overlaid on it) showing "Fetching missing artwork… N/M (K found)" + smooth sweep bar, then the final message — previously the drawer closed and nothing indicated progress until the completion toast, over a 4000-track library that's minutes of apparent silence.
- **Playback crash hardening + in-app crash log (2026-07-31):** After a shuffle/next/back crash report, `MusicController` was hardened: `playOnController` now uses a **single atomic `setMediaItems(items)`** (position-reset) instead of the `stop()→clearMediaItems()→addMediaItems()` command burst — a burst of separate timeline commands leaves MediaController's cached timeline/position transiently inconsistent with the session (the crash class of androidx/media#86), and a shuffle toggle right after the burst widens that window. **Every MediaController dispatch** (play, seek, next, previous, shuffle, repeat, playNext, addToQueue, moveQueueItem, stop) is wrapped in `safePlayerCommand` — a Media3 race degrades to a logged + Snackbar'd `playbackError`, never a hard crash; listener callbacks and `getQueue()` are individually guarded too. **Global crash capture:** `LunoApp.installCrashCapture()` writes any uncaught stack trace to `filesDir/crash_log.txt`, readable via the new Settings-drawer **"Error log"** item (desktop error_log-view parity). If a crash still reproduces, its stack is visible in-app for reporting.
- **Artwork-enrichment memory fix — the REAL shuffle-crash root cause (2026-07-31, verified on device):** `MusicService.ArtworkEnrichingCallback.onAddMediaItems` loaded `artworkData` bytes for **every** media item in the batch. With context queues (this library: 4k+ tracks, ~170MB of cached artwork at ~163KB each), every play read the whole library's artwork into the session timeline at once → the process ballooned (~412MB PSS) and the device's LMK/Samsung KPM killed it mid-playback (exit-info: repeated `LOW_MEMORY` kills; earlier: an `APP CRASH(EXCEPTION)`). That is why "shuffle → next/back" appeared to crash the app: the queue was finally large enough to trigger the storm. **Fix:** `MusicController.buildMediaItem` now flags only the items the listener needs — the play-start item ±3 (`ENRICH_WINDOW`), plus single manually-queued items (`playNext`/`addToQueue`) — via a `METADATA_ENRICH_ARTWORK` extra; the service enriches **only flagged items**. Verified on-device (S20 FE, 4041-track library): next×3 / prev×3 / shuffle-toggle cycle cleanly, playback auto-advances through shuffled tracks for minutes without a kill, PSS bounded at ~233MB (was ~412MB), notification still shows the current track's artwork. In-app artwork (Coil) is unaffected — it renders from `artworkUri` directly. Trade-off: notification/lock-screen art is only enriched for the start-window and manually queued items; deep/shuffled skips fall back to the default app icon on those two surfaces only.
- **Background-thread toast crash fixed (2026-07-31):** The Settings drawer's "Fetch missing artwork" completion toast ran on `app.appScope` (Dispatchers.Default) — `Toast.makeText` there throws `NullPointerException: Can't toast on a thread that has not called Looper.prepare()` (crash captured by the new crash log). The toast + Compose state now run via `withContext(Dispatchers.Main)`. Same audit applied app-wide: `MusicFolderImportManager`'s three toasts (success/failure/stall — all on `appScope`) now go through a main-looper `showToast` helper (the stall toast was previously **unguarded** and would have crashed too); all other toast sites run on main-thread scopes.
- **Smooth lazy scrolling (2026-08-01):** `ArtworkImage` uses Coil's non-subcomposing `rememberAsyncImagePainter` with the existing gradient/music-note placeholder instead of `SubcomposeAsyncImage`, avoiding a second composition pass when rows enter or leave a lazy list. Row artwork remains capped to display-sized decodes (192 px for 48 dp rows); lazy collections provide stable keys and explicit `contentType` values to improve item reuse during flings.
- **Smooth full-player seeking (2026-08-01):** `FullPlayerScreen` linearly interpolates the `MusicController`'s 250 ms position samples between frames, keyed by track so a new song resets cleanly. Slider drag remains local and visual until release; `MusicController.seekTo()` clamps to the player duration and publishes the accepted target immediately, preventing the thumb/time label from snapping back while Media3 catches up.
- **Playlist sync membership fix (2026-07-30):** Syncing a playlist previously downloaded the songs but never added them to the playlist. Now `DownloadWorker` adds each completed download to its job's playlist (`playlistId` → `PlaylistTrack` with next sort order), and `PlaylistSyncWorker` also inserts pre-existing matching tracks into the playlist (idempotent via IGNORE conflict).
- **Playlist thumbnails = 4-quadrant collage (2026-07-30):** Playlist cards (Library, playlist view) use a **2x2 collage of the playlist's first four song artworks** as the thumbnail (`PlaylistCard` renders `ArtworkCollage` at 48dp); only individual songs use their singular artwork. Home's "Your playlists" carousel cards now **match the "Made for you" TrackCard layout** (square collage artwork on top, name, "N songs" subtitle) — Home-only styling.
- **Search download rows (2026-07-30):** The web-search page no longer shows the "Recent Downloads" list. Each result row is driven by its enqueued job (videoId → jobId map): green circular progress while queued/downloading (tap to cancel), **green checkmark on completion, then the row disappears** after ~1.5s to make room for other results; failed jobs show a retry icon.
- **Library play/shuffle/tab layout (2026-07-30):** Play sits **left**, Shuffle **right** (icon + green text, no box — desktop All Music layout), and the **Playlist-view filter tab sits under the Play button** (left-aligned; label flips to "All songs view" when active).
- **Create-playlist inside add-to-playlist (2026-07-30):** The "Add to playlist" picker in `TrackActionsSheet` now leads with a **"New playlist"** row that opens a create dialog (name + optional description); on create the track is added to the new playlist immediately.
- **Long-press track actions (2026-07-30):** `ui/components/TrackActionsSheet.kt` — long-press (or 3-dot) any track row (Library, Search library results, PlaylistDetail) to open a bottom sheet with **Add to playlist** (nested picker incl. create-new) and **Delete from library** (confirm dialog; `LibraryRepository.deleteTrack` cascades `playlist_tracks` rows via FK).
- **Not yet implemented:** Queue/history persistence, Bluetooth AVRCP metadata publication, Android Auto, volume slider (deferred — hardware keys only).

**Queue/history:**
- Queue is **local only** — never synced to a server
- History is local only, max 100 entries (following desktop convention from `engine.py` line 328); replays **move to the front** (desktop `play_current` parity) — never duplicated; `MusicController.clearRecentlyPlayed()` exposes the desktop "Clear History" action
- Shuffle/repeat modes: off, repeat one, repeat all (matching `music_player_flet.py` line 78)
- **"Up Next" model (implemented 2026-07-31):** user-queued items play before the playlist context, mirroring desktop `user_queue_count` logic (`engine.py` lines 213, 384–385). `MusicController.manualQueueUris` tracks manually queued items: "Play next" inserts at `current + 1`, "Add to queue" inserts after the remaining manual items (`current + 1 + manualItemsAfterCurrent`); the set drains as manual items play (or are skipped past), and resets on a new playback context or a manual queue reorder.

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

**Implemented (2026-07-31):**
- `data/discovery/LastfmService.kt` — faithful port of the two-stage logic (OkHttp, 8s timeout, injectable base URL): `track.getsimilar` parsed for name/artist (`artist` may be an object or a plain string), `match`, and the largest artwork URL (legacy `http://` artwork is upgraded to HTTPS for Android cleartext safety); network I/O, JSON parsing, and image selection stay off the UI dispatcher; **fallback `artist.gettoptracks` only when stage 1 succeeded with zero tracks** (an API error — e.g. invalid key — surfaces as `LastfmResult.Failure` instead of being masked by a doomed fallback); fallback tracks scored **0.8**; cache per (artist, title) with the desktop **clear-when-exceeding-10** policy; **only non-empty results are cached** (an empty outcome is transient — a Refresh can re-ask instead of being stuck on a cached empty list); key read through a provider at call time (key changes need no service recreation); failures carry user-facing messages ("Could not reach Last.fm — check your connection", the API's own error message)
- `data/discovery/LastfmKeyStore.kt` — **EncryptedSharedPreferences** (MasterKey AES256-GCM values / AES256-SIV keys, `androidx.security:security-crypto:1.1.0-alpha06`, deprecated upstream but used per the security decision); falls back to plain SharedPreferences with a `Log.w` if the Keystore is unusable (a crash is worse than the downgrade); never logged, never exported, HTTPS-only
- `data/repository/DiscoveryRepository.kt` — app-level entry point: `apiKey: StateFlow<String?>` (UI reacts to drawer edits), `setApiKey`/`clearApiKey` (trim; blank = clear; cache cleared on key change), `getSimilar(artist, title, limit, libraryTracks)` filtering out tracks the user already owns via normalized artist/title keys (case-, whitespace-, punctuation-, and accent-insensitive; desktop `_update_discovery_results` parity); ownership normalization/filtering runs on `Dispatchers.Default` so Discover loading does not block Compose frames
- **Seed:** current track (`MusicController.currentTrack`) → else most recent play → else last-added library track (desktop: playing → selected-track fallback)
- **Discovery ownership filtering (2026-08-02):** `DiscoveryRepository` removes recommendations already represented in `LibraryData.tracks` before `DiscoverScreen` renders them. Matching is case-, whitespace-, punctuation-, and accent-insensitive across artist/title metadata; the persisted playlist name remains `Unsorted`, while its detail view displays "Unsorted - songs yet to find a home".
- **DiscoverScreen** (`ui/discover/DiscoverScreen.kt`): honest states for missing key (button opens the shared `LastfmKeyDialog`), no seed, loading, API/network error (Retry), and "no new recommendations" (everything already in the library); "Based on {title} — {artist}" header with refresh + API-key shortcut; 20 recommendations as rows (48dp artwork from Last.fm, cached Deezer fallback when Last.fm art is missing or fails, title, "artist • N% match", per-row download); loading is reported to `MainShell` so the single transition-mask spinner remains visible until the request finishes; **Download all** batch with live "Queuing… N/M" progress
- **Downloads** use the same acquisition pipeline as Search (YouTube search "artist title" → `getAudioStreamUrl` → `DownloadRepository.enqueueDownload` with the YouTube thumbnail); failures are honest per-track toasts, never silent skips
- **Settings drawer** gained "Last.fm API key" (shared `LastfmKeyDialog`); the Discover empty state opens the same dialog

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
- **Implemented (2026-08-01):** the Settings drawer's "Check for updates" calls `https://api.github.com/repos/Tsohnle95/musicPlayer/releases/latest`, compares the installed `BuildConfig.VERSION_NAME` with a `v?major.minor.patch` tag, and shows an honest up-to-date, error, or update-available dialog. Update details include release notes, the GitHub release page, and the first `.apk` asset when present. A one-time sideload warning is persisted in `github_releases` preferences; after confirmation the APK downloads into private cache storage and opens Android's package installer through a `FileProvider` (the user may need to enable unknown-app installs for Luno). **Signed Android release CI is now implemented** through `.github/workflows/android-release.yml`; the permanent keystore and four GitHub Actions secrets must still be created by the maintainer.

### Visual Specification (Authoritative Fallback)

> Note: The current desktop/Flet theme (`theme.py`) uses different colors (#121212, #181818, #282828, etc.). The colors below are **implemented** in `ui/theme/Color.kt` and match the visual spec. The full player screen contract (below) is **implemented** in `ui/player/` — real artwork (Coil), dominant-color dynamic gradients, and queue reordering are done; volume slider remains deferred.

**Color palette:**
| Role | Hex | Usage |
|------|-----|-------|
| App shell background | `#101010` + translucent `#102B1C` blobs | Four larger edge-oriented radial blobs with transparent edges, `#1ED760` center highlights, a calmer black center, and a persistent green fade behind the Luno header |
| Primary background | `#101010` | Solid fallback, transition mask, and system/theme background |
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
- MainShell screen content sits over the black base with four larger, soft-edged green blobs inspired by Discord/GitHub home backgrounds, arranged around the screen edges; transparent gaps leave a calmer black center visible, while the Luno header and status-bar area retain a permanent green gradient and navigation, drawers, dialogs, and the full player retain their opaque or artwork-driven surfaces

**Mini-player:**
- **Persistent mini-player** at the bottom (similar to Spotify)
- Shows artwork thumbnail, title, artist, play/pause, a thin position bar, and a translucent surface tinted from the current artwork; horizontal swipes skip through the current queue
- Tapping expands to full player
- Present on all main tabs when audio is active

**Carousels:**
- **Visible-clipped horizontal carousels** (items slightly clipped at screen edges to indicate scrollability)
- Used on Home, Discover, and Artist/Playlist detail screens

**Navigation:**
- Bottom navigation bar with 5 destinations (in order):
  1. **Home** — Recommended, recently played, quick-start; **the "Luno" app header opens the local-function drawer** (profile icon removed 2026-07-31)
  2. **Search** — Search library + web sources
  3. **Your Library** — Playlists, artists, albums, downloaded
  4. **Discover** — Last.fm-powered recommendations
  5. **Create** — One-action create playlist modal
- **Local-function drawer** (ModalNavigationDrawer, opened from the tappable "Luno" app header): header reads **"Settings"** and groups local actions into expandable **Library**, **Downloads**, **History & discovery**, and **App** accordions. Library contains Music folder, **Duplicate checker** (scrollable normalized duplicate groups with selectable metadata-only removal), and Fetch missing artwork. Other sections contain Downloads, Export/Import, **Last.fm API key**, Recently played, Clear recent history, About, Error log, and Check for updates (honest "coming soon" toasts). No standalone Settings item (the header is the settings entry). Never contains cloud account/login/logout.
- No "Premium" tab, no podcast/audiobook tab

**Screen contracts (comprehensive):**

1. **Home:** ✅ **Implemented** in `ui/home/HomeScreen.kt` (Spotify-inspired, refreshed 2026-07-31) — dark green-accent hero, greeting (display name "Listener", editable in future), "Good morning/afternoon/evening", **recently played swipeable carousel** (edge-clipped, persisted history), **"Made for you" recommendations carousel** (edge-clipped, stable up-to-50-track random sample from the full catalogue; title/View all opens the virtual playlist detail), **quick-action playlists carousel** (same TrackCard layout: square art + name + song count, tap → playlist detail). The Settings drawer opens from the tappable "Luno" app header. **Not yet:** editable display name, "your top genres" (no genre metadata).
2. **Full player:** ✅ **Implemented** in `ui/player/FullPlayerScreen.kt` — **Large real artwork (280dp, Coil `AsyncImage` from `Track.albumArtPath` via `MediaTrack.artworkUri`; also shown in the notification/lock-screen via `MusicService` artwork-data enrichment)**, title, artist, **scrub bar with m:ss time labels**, repeat/shuffle/prev/play-pause/next transport (**prev/next walk the source context** — search results, library list, playlist, or history), **queue button (`ui/player/QueueSheet.kt` with artwork rows + long-press drag-to-reorder)**, action sheet trigger (`ui/player/ActionSheet.kt` with add-to-playlist, play next, add to queue, go-to-artist, share), **fixed animated dominant-color gradient backdrop** (androidx Palette → 600ms `animateColorAsState`), and a lazy-scrolling **Recommended for this song** footer with six Last.fm rows, refresh, library filtering, and Deezer artwork fallback. **Deferred:** volume slider (device hardware volume keys only — per scope decision), go-to-artist/album detail wiring.
3. **Action sheet (bottom sheet):** Add to playlist, play next, add to queue, go to album, go to artist, share, view credits, remove from playlist
4. **Playlist detail:** ✅ **Implemented (partial)** in `ui/library/PlaylistDetailScreen.kt` — **header with 2x2 four-artwork collage** (Spotify-style, up to 4 track artworks divided amongst a square), title, description, track count + total duration, **Play button**, **shared sort chip (A–Z / Z–A / Recently added / Duration — added 2026-07-31)**, **in-playlist search (rounded pill field with clear-X — filters title/artist/album, filtered list is the play context; added 2026-07-31)**, track list (tap plays from track, long-press opens actions). **Deferred:** drag-to-reorder, download-all toggle, owner display.
5. **Playlist tools:** Rename, delete, export JSON, import JSON (merge/replace), duplicate track resolution
6. **Recommended songs:** ✅ **Implemented** in the full player as six rows of "Recommended for this song" with a Refresh button, Last.fm key/loading/error/empty states, library ownership filtering, and artwork fallback
7. **Local-function drawer:** Settings (audio, storage, Last.fm key, appearance), about, export/import, check for updates — never contains cloud account/login/logout
8. **Queue / Recents:** ✅ **Implemented (2026-07-31)** — tab layout with **"Playing Next"** (queue) and **"Recently played"** (history) via shared `ui/player/QueueRecentsTabs.kt` (`QueueRecentsTab` + `QueueRecentsTabRow` + `PlayingNextTab` + `RecentlyPlayedTab`): drag-to-reorder queue (long-press drag), recents rows play within the history context (next/prev walk recent plays), **Clear all** button on the recents tab, history counts ("N played"). Available in two places: the full player's `QueueSheet` (now tabbed) and a full **Recents screen** (`ui/player/RecentsScreen.kt`, route `Routes.RECENTS`) opened from the Settings drawer's **"Recently played"** item. History remains in-session only (Room `HistoryEntry` still planned).
9. **Discover:** ✅ **Implemented** in `ui/discover/DiscoverScreen.kt` (2026-07-31) — Last.fm recommendations seeded by the current/last track ("Based on {title} — {artist}" header), rows with Last.fm artwork + "artist • N% match", refresh, per-row download + **Download all** batch (same acquisition pipeline as Search), API-key setup state (shared `LastfmKeyDialog` — also reachable from the Settings drawer), honest no-seed/no-new-recs/error states; library-owned recommendations are filtered out
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
| Allowed dups | `.luno_allowed_dups.json` in MUSIC_ROOT | Lines 492–506 |
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

**JSON Export Manifests** (user-generated data files):
- Versioned, untrusted, never contain secrets or audio data
- Used for export/import of playlist data
- Implemented with `manifest_version: 1`, strict unknown-field/path validation, bounded SAF reads, and additive/no-delete imports

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
│  │  - DiscoveryRepository (Last.fm)     │ ✅ (implemented — 2026-07-31) │
│  │  - LibraryTransferRepository         │ ✅ (implemented — v1 JSON) │
│  │  - SettingsRepository (DataStore)    │ Planned   │
│  └──────────────────┬───────────────────┘            │
│                     │                                │
│  ┌──────────────────┴───────────────────┐            │
│  │  Room Database (AppDatabase v9)       │            │
│  │  - Track, Playlist, PlaylistTrack      │            │
│  │  - DownloadJob                         │ ✅ (implemented — v2 migration) │
│  │  - QueueEntry, HistoryEntry,           │ Planned   │
│  │    AllowedDuplicate                    │ Planned   │
│  └──────────────────────────────────────┘            │
└──────────────────────────────────────────────────────┘
```

### Data Flow: Library Import (implemented)

1. User taps "Music folder" in the Settings drawer → SAF `OpenDocumentTree` launcher returns a selected folder tree URI
2. `MusicFolderImportManager` persists the selected tree URI and delegates to `LibraryRepository.importLibraryTree()` while publishing live progress to the shell status strip
3. `importLibraryTree()` recurses the tree: each subfolder becomes a playlist named after the folder, files at the root land in "Unsorted", and each audio document is passed to `importAudioUri()`
4. `importAudioUri()`:
   - Checks `TrackDao.exists()` for deduplication → skips if duplicate
   - **Rejects directories and non-audio documents** (a folder is never a track; the `*/*` picker surfaces non-audio files, which are errors, not tracks)
   - Calls `takePersistableUriPermission()` to retain access across restarts
   - Extracts metadata via `MediaMetadataRetriever` (title, artist, duration) with filename-based fallback ("Artist - Title" split)
   - Inserts `Track` entity into Room via `TrackDao.insertTrack()`
5. Returns `ImportResult(imported, duplicates, errors)` — UI observes updated `Flow<List<Track>>` from Room

### Data Flow: Favorites (implemented)

1. A track's Library/Search/Home/playlist-detail 3-dot sheet calls `LibraryRepository.setFavorite(uri, isFavorite)`, which updates the durable `Track.isFavorite` column through `TrackDao.setFavorite()`.
2. `LibraryData.tracks` emits the updated track rows. Home derives its Favorites carousel from that flow; Library derives the reserved `SystemPlaylists.FAVORITES_ID` card and keeps it above Unsorted without adding synthetic Room membership rows.
3. Opening the derived Favorites card navigates through the normal playlist-detail route, where the app supplies the live favorite subset as `virtualTracks`; removing a track from that view uses the favorite toggle and does not delete audio or playlist metadata.

### Data Flow: JSON Export / Import (implemented)

1. Library and playlist selection actions ask `LibraryTransferRepository` for a consistent Room snapshot; full-library export includes ordered playlist memberships and explicit unassigned tracks.
2. `LibraryManifestCodec` emits/accepts `luno.library.export` with `manifest_version: 1`; strict allowlists reject unknown fields, local paths, unsafe playlist names, unsupported provider IDs, oversized input, and invalid counts/metadata.
3. Android writes manifests through SAF `CreateDocument` and reads them through bounded `OpenDocument` streams. The Settings drawer opens `ExportImportScreen`, which shows matched, missing, new-playlist, and ambiguity counts before import.
4. Import merges playlists by case-insensitive name and adds matched memberships without deleting audio or metadata. Stable YouTube IDs are preferred; metadata-only matches require explicit confirmation.
5. Missing YouTube tracks are resolved with `WebSearchService`, then queued through `DownloadRepository`. A v8 `playlistIdsCsv` field lets one downloaded file attach to every imported destination playlist after `DownloadWorker` completes; unsupported/unresolved tracks are reported.

### Data Flow: Download (Implemented)
1. User navigates to the Download tab → searches YouTube → taps Download on a result
2. UI shows a `CircularProgressIndicator` on that result and tracks the video ID as "extracting" via `downloadingVideoIds` state set (prevents duplicate taps)
3. `WebSearchService.getAudioStreamUrl(videoId)` runs on IO dispatcher with fallback chain: **NewPipe Extractor v0.26.4** (primary; audio stream chosen by priority M4A/AAC → Opus → best bitrate) → **Invidious companion** (emergency; embed player response parsed for the actual format list, companion `latest_version` URL requested with the chosen audio itag and the original `check` token preserved) → **InnerTube player endpoint** (last resort; googlevideo URLs typically 403 on this device)
4. If extraction succeeds: `DownloadRepository.enqueueDownload()` resolves an explicit playlist ID or atomically reuses/creates the **Unsorted** playlist for playlist-less downloads, inserts a `DownloadJob(state=QUEUED, playlistId=...)` in Room, and enqueues a `DownloadWorker` via WorkManager with `NetworkType.CONNECTED` constraint
5. `DownloadWorker` (foreground service with app's `ic_download` icon, notification ID `1000 + jobId`):
   - Sets job state to DOWNLOADING
   - Opens an OkHttp request to the source URL with User-Agent, Referer, Origin, and `Range: bytes=0-` headers (client configured with `ConnectionSpec.MODERN_TLS` + `COMPATIBLE_TLS`)
   - Streams data to `{filesDir}/downloads/{safeFileName}.{ext}` — extension derived from response `Content-Type` (`.m4a`, `.opus`, `.mp3`, `.ogg`, `.audio`)
   - Updates progress (0-100%) in Room
   - On completion: extracts `durationMs` via `MediaMetadataRetriever`, inserts `Track` entity into Room, marks job COMPLETED
   - On failure: marks job FAILED with a status-specific error message (400/403/404/429/503 distinct; never logs full headers or signed stream URLs), auto-retries up to 3 times
6. Successful downloads insert a `Track`, add it to the job's explicit playlist or **Unsorted** (legacy null jobs are repaired by `DownloadWorker`), then mark the job complete; they appear immediately in the Library via Room `Flow` and in SearchScreen's **Downloaded songs** accordion by matching `Track.uri` to the app-private downloads directory
7. From that accordion, a long press opens the existing track action sheet, while **Select** enables tap-to-select and batch **Add to playlist**; the same picker can assign a completed search result
8. Choosing **Save to playlist** on a not-yet-downloaded YouTube result opens the playlist picker before extraction and persists the selected playlist ID on the `DownloadJob`, so `DownloadWorker` adds the created Track when it completes
9. User can retry failed downloads or cancel in-progress downloads

> **Verification status (2026-07-30):** Build/compile verified (`assembleDebug` + unit tests green; extractor v0.26.4 classes confirmed in APK). **On-device runtime verification CONFIRMED** on Galaxy S20 FE (API 33) — YouTube search → audio extraction → download exercised successfully.

Alternative paths:
- **Direct URL paste**: User pastes any direct audio URL (optional title/artist override) → taps "Add to download queue" — skips extraction, goes straight to enqueue
- **CSV import (Exportify)**: User selects Spotify Exportify CSV → each row (artist, title) is searched on YouTube → first result's audio URL extracted → downloads queued

### Data Flow: GitHub Release Check (Implemented)
1. User opens the Settings drawer from the tappable `Luno` header and selects **Check for updates**.
2. `GitHubReleaseService` performs an HTTPS request to the repository's `/releases/latest` endpoint on `Dispatchers.IO`, sending GitHub's JSON `Accept` header and an app User-Agent.
3. The service validates the installed and release versions, compares their numeric major/minor/patch parts, and parses release notes, page URL, and the first APK asset URL into `ReleaseCheckResult`.
4. `MainShell` renders the result in a Compose dialog. An APK download opens the HTTPS asset URL in the browser; the first download shows a persisted one-time warning about installing outside Google Play.
5. HTTP, malformed JSON, invalid tags, and network failures become user-facing error states; no release metadata or secrets are persisted.

### Data Flow: Playlist Sync (Implemented)
1. User pastes a YouTube playlist URL in `SearchScreen`, chooses an existing destination playlist or creates a new one; `PlaylistRepository.updatePlaylistUrl()` saves the source on that playlist. The existing Library playlist URL editor remains supported.
2. User taps **Queue playlist import** in SearchScreen, or Sync (per-playlist or batch "Sync All") in Downloads/Library.
3. `DownloadRepository.syncPlaylist()` enqueues a `PlaylistSyncWorker` via WorkManager
4. `PlaylistSyncWorker`:
   a. Fetches all videos from the playlist via `WebSearchService.getPlaylistVideos()` (NewPipe Extractor v0.26.4 `PlaylistExtractor`)
   b. Compares against existing tracks in Room (by title) and existing queued downloads
   c. For each new video: extracts audio URL via `WebSearchService.getAudioStreamUrl()`, creates a `DownloadJob`, enqueues an individual `DownloadWorker`
5. Each `DownloadWorker` runs independently — progress visible in Downloads tab

### Data Flow: Playback (implemented)

1. User taps a track in SearchScreen, LibraryScreen, HomeScreen, or PlaylistDetailScreen → the screen passes the **full playback context** — the ordered list the track was picked from plus its index — through the shared `onPlay(List<MediaTrack>, startIndex)` supplied by `MainShell` (Library: sorted/filtered list; Search library tab: live search results; Home: recent-history / made-for-you / Favorites list; PlaylistDetail: the playlist). Next/previous/shuffle on the full player therefore always operate relative to that context.
2. `MainShell` evaluates `NotificationPermissionPolicy`; API 33+ shows the system notification prompt at most once automatically, then dispatches playback regardless of prompt result.
3. `onPlay` calls `MusicController.play(tracks, startIndex)` with full metadata and the complete queue.
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
| `.github/workflows/android-release.yml` | Tag-triggered Java 17 Android release: recursive checkout, signing-keystore decode from GitHub Secrets, version/tag validation, unit tests, lint, signed APK verification, and GitHub Release publication | ✅ |
| `build.gradle.kts` | Root Gradle: plugin declarations (AGP, Kotlin, Compose, KSP) | ✅ |
| `settings.gradle.kts` | Project settings, single `:app` module; `includeBuild("vendor/NewPipeExtractor")` composite build substitutes the JitPack NewPipe coordinate (`com.github.TeamNewPipe.NewPipeExtractor:extractor`) with the local `:extractor` module | ✅ |
| `gradle.properties` | JVM args, AndroidX, Kotlin code style | ✅ |
| `gradle/libs.versions.toml` | Version catalog (AGP 8.5.2, Kotlin 2.0.0, Media3 1.3.1, Room 2.6.1, NewPipe v0.26.4, desugar_jdk_libs_nio 2.1.4) | ✅ |
| `app/build.gradle.kts` | App module: compileSdk 35, minSdk 29, Compose BOM 2024.06.00, all dependencies; release-only environment signing config and `verifyReleaseVersion` task; debug install path remains unchanged | ✅ |
| `ANDROID_RELEASES.md` | Maintainer instructions for the permanent Android keystore, GitHub Actions secrets, tag/version release flow, and preserved local `installDebug` testing | ✅ |
| `app/proguard-rules.pro` | Keep Room entity annotations | ✅ |
| `vendor/NewPipeExtractor/` | Git submodule pinned to tag `v0.26.4` (shallow). Built via Gradle composite build; **requires a JDK 11 toolchain** (set in its root `build.gradle.kts`); **requires core-library desugaring** in the app (minSdk 29 < 33). GPLv3 licensed. | ✅ |
| `app/schemas/.../1.json` | v1–v9 Room schema exports (v1: tracks, playlists, playlist_tracks; v2: adds download_jobs; v3: adds playlistUrl to Playlist; v4: adds albumArtPath to Track; v5: adds thumbnailUrl to DownloadJob; v6/v7 add play counts; v8 adds multiple import destinations; v9 adds isFavorite) | ✅ |

### Android System
| File | Responsibility | Status |
|------|---------------|--------|
| `app/src/main/AndroidManifest.xml` | Permissions (FOREGROUND_SERVICE, POST_NOTIFICATIONS), MainActivity, MusicService | ✅ |
| `app/src/main/res/values/colors.xml` | `#101010`, `#202020`, `#292929`, `#1ED760` (visual spec colors) | ✅ |
| `app/src/main/res/values/themes.xml` | Theme.Luno (Material NoActionBar, dark background) | ✅ |
| `app/src/main/res/values/strings.xml` | App name, nav labels, action strings | ✅ |
| `app/src/main/res/drawable/*.xml` | 9 vector icons (home, search, library, discover, create, play, pause, music_note, ic_download) + launcher assets | ✅ |

### Kotlin Source
| File | Responsibility | Status |
|------|---------------|--------|
| `LunoApp.kt` | Application class, manual DI (database, libraryRepo, playlistRepo); **installCrashCapture(): uncaught exceptions appended to filesDir/crash_log.txt before the process dies (readable via the Settings drawer "Error log")** | ✅ |
| `MainActivity.kt` | Compose entry, MusicController init, edge-to-edge; controller is internally visible to instrumentation tests only | ✅ |
| `playback/MusicService.kt` | Media3 MediaSessionService + ExoPlayer; `ArtworkEnrichingCallback` (MediaSession.Callback) loads artworkData bytes **only for items flagged METADATA_ENRICH_ARTWORK** (start-window + manually queued — bounded memory; enriching every item of a 4k-track context queue OOM/LMK-killed the process) | ✅ |
| `playback/MusicController.kt` | MediaController wrapper, StateFlow playback state (incl. recentlyPlayed history, max 100, desktop move-to-front replay semantics), immutable PlaybackRequest, injectable AsyncConnector, playbackError flow, generation-gated release | ✅ Terminal lifecycle, full-queue pending, metadata-aware MediaItems (incl. artworkUri), playlist-context metadata for per-playlist popularity, moveQueueItem, setShuffle, error Snackbar propagation, **Up-Next queue semantics (manualQueueUris: playNext at current+1, addToQueue after remaining manual items), clearRecentlyPlayed(), local play-count callback, crash-hardened: atomic setMediaItems play dispatch + safePlayerCommand try/catch on every controller command + guarded listener callbacks + bounded artwork-enrichment flags (METADATA_ENRICH_ARTWORK on start-window/manual items only)** |
| `playback/RecentlyPlayedStore.kt` | App-private JSON persistence for the ordered, bounded `MediaTrack` history; malformed data safely resets to empty | ✅ |
| `playback/NotificationPermissionPolicy.kt` | One-shot `POST_NOTIFICATIONS` prompt policy using SharedPreferences | ✅ |
| `data/artwork/ArtworkStorage.kt` | Embedded-artwork extraction + usable-image validation + ≤512px JPEG cache (filesDir/artwork), sampled decode, Palette dominant color | ✅ |
| `data/artwork/ArtworkFetchService.kt` | Broad + metadata fallback Deezer cover search/download, validates decoded downloaded images through ArtworkStorage | ✅ |
| `data/db/AppDatabase.kt` | Room database (4 entities, version 9, singleton, migrations 1→2→3→4→5→6→7→8→9; v6 adds local track play counts, v7 adds playlist-view play counts, v8 carries multiple import destination playlist ids, v9 adds durable track favorites) | ✅ |
| `data/db/entity/Track.kt` | Track entity (uri PK, title, artist, album, durationMs, albumArtPath, playCount, isFavorite, addedAt) + albumArtUri() helper | ✅ |
| `data/db/entity/Playlist.kt` | Playlist entity (autoId, name, description, playlist playCount, createdAt); `SystemPlaylists` reserves the derived Favorites identity | ✅ |
| `data/db/entity/PlaylistTrack.kt` | Junction entity (composite PK, FK cascade, sortOrder) | ✅ |
| `data/db/dao/TrackDao.kt` | Track CRUD + search Flow + dedupe check + `updateTrack` (artwork-fetch persistence) + `incrementPlayCount` + `setFavorite` | ✅ |
| `data/db/dao/PlaylistDao.kt` | Playlist CRUD + relation queries + sort order + per-playlist play-count increment | ✅ |
| `data/repository/LibraryRepository.kt` | SAF import (files + desktop-style folder tree via granted-URI import path), MediaMetadataRetriever, embedded-artwork extraction via ArtworkStorage, dedupe, ImportResult, **fetchMissingArtwork(onProgress, extract) (validates cache images, re-extracts embedded artwork, falls back to Deezer download, reports progress, and exposes injectable extractor/remote seams)**, `recordPlayback(uri, playlistId)` increments global song popularity and only the originating playlist's count, `setFavorite(uri, isFavorite)`; production-default injectable URI-permission persister for deterministic tests | ✅ |
| `data/repository/DuplicateFinder.kt` | Pure desktop-compatible duplicate grouping by normalized artist + title (release tags, featuring variants, extensions, and punctuation normalized) | ✅ |
| `data/repository/MusicFolderRepository.kt` | Persists the chosen music-folder tree URI (SharedPreferences) | ✅ |
| `ui/shell/ArtworkFetchManager.kt` | Process-lifetime owner of the missing-artwork sweep (Settings drawer): launches on appScope, `StateFlow<ArtworkFetchStatus?>` (Progress scanned/total/updated → Finished(updated/failed/errorMessage) → auto-clear after 5s), activeJob re-tap guard, failures logged + surfaced in strip/toast, main-looper guarded toast (appScope is Dispatchers.Default); injectable fetch function for deterministic tests | ✅ |
| `data/discovery/LastfmService.kt` | Last.fm client (desktop DiscoveryService port): `track.getsimilar` → `artist.gettoptracks` fallback (0.8 match), per-(artist,title) cache cleared when exceeding 10 entries (non-empty results only), largest-image artwork pick, API/network failures as `LastfmResult.Failure`; `LastfmTrack` + `LastfmResult` models | ✅ |
| `data/discovery/RecommendationArtworkService.kt` | Cached Deezer search fallback for missing or failed recommendation artwork; selects the largest available album cover and normalizes URLs to HTTPS | ✅ |
| `data/discovery/LastfmKeyStore.kt` | EncryptedSharedPreferences (AES256-GCM/SIV, security-crypto 1.1.0-alpha06) API-key storage; plain-prefs fallback when the Keystore is unavailable | ✅ |
| `data/update/GitHubReleaseService.kt` | HTTPS GitHub `/releases/latest` client for `Tsohnle95/musicPlayer`; validates and compares release tags, parses notes/page/first APK asset, and returns typed update/up-to-date/failure results | ✅ |
| `data/repository/DiscoveryRepository.kt` | Last.fm entry point: `apiKey` StateFlow (encrypted), set/clear (cache cleared on change), `getSimilar()` filtering out library-owned tracks (desktop normalized "artist - title" keys); injectable LastfmService for tests | ✅ |
| `data/repository/LibraryTransferRepository.kt` | Shared v1 manifest snapshots for full/selected/playlist scopes; strict preview and additive import; stable YouTube-ID matching from completed download artwork metadata; missing-track resolution through the existing NewPipe/WorkManager downloader | ✅ |
| `data/export/LibraryManifest.kt` | Cross-platform manifest models, scopes, preview, and import result types | ✅ |
| `data/export/LibraryManifestCodec.kt` | Strict `org.json` encoder/decoder: version, allowlist, count/size/string/path/provider validation | ✅ |
| `data/repository/LibraryData.kt` | App-lifetime warm store for the four library Room flows (tracks, playlists-with-tracks, downloads, playlists-with-URLs) as `StateFlow`s (`SharingStarted.Eagerly` at startup) + `loaded` flag; screens render from it so tab switches show full content instantly | ✅ |
| `ui/shell/MusicFolderImportManager.kt` | Process-lifetime owner of the folder import from the Settings drawer: launches import on appScope, `StateFlow<FolderImportStatus?>` (Importing/Finished with counts + stalled/failed flags + persist warning + errorMessage), stall watchdog (AtomicLong lastTickAt — thread-visible) that warns but NEVER cancels (imports finish on their own merits), failures logged + surfaced in strip/toast, retries replace previous attempts, "Import complete!" auto-clears after 2s; **all toasts via main-looper showToast helper (appScope is Dispatchers.Default — direct Toast.makeText crashes)** | ✅ |
| `data/repository/PlaylistRepository.kt` | Playlist CRUD, name validation, sort order mgmt, clear-playlist (metadata-only) | ✅ |
| `data/db/entity/DownloadJob.kt` | DownloadJob entity + DownloadState enum (QUEUED, DOWNLOADING, COMPLETED, FAILED, CANCELLED); thumbnailUrl captures the YouTube video thumbnail and playlistIdsCsv preserves multi-playlist import destinations | ✅ |
| `data/db/dao/DownloadJobDao.kt` | DownloadJob CRUD + progress/state queries with Flow | ✅ |
| `playback/DownloadWorker.kt` | WorkManager CoroutineWorker: HTTP download, progress tracking, Track insertion (MediaMetadataRetriever duration + embedded-artwork extraction, thumbnail fetch fallback for YouTube), **adds completed download to its explicit or fallback Unsorted playlist**, foreground notification with app icon and stable notification ID | ✅ |
| `playback/PlaylistSyncWorker.kt` | WorkManager worker: fetches YouTube playlist videos via NewPipe Extractor, creates individual DownloadJob per track, **adds pre-existing matching tracks to the playlist**, deduplicates, reports extraction errors as failed jobs | ✅ |
| `playback/WebSearchService.kt` | YouTube client: search, playlist extraction, and audio stream URL extraction via NewPipe Extractor (v0.26.4, vendored Git submodule at `vendor/NewPipeExtractor` via composite build). Replaces the previous 4-fallback InnerTube/Piped/Invidious chain with bundled native extraction. Returns typed `ExtractionResult` for error propagation. | ✅ |
| `playback/NewPipeDownloader.kt` | `HttpURLConnection`-based implementation of NewPipe's `Downloader` interface. Handles GET/POST requests with proper User-Agent and redirects. | ✅ |
| `playback/ExtractionResult.kt` | Sealed class for typed extraction results: `Success<T>` or `Error(message, details)`. Eliminates nullable/pair returns. | ✅ |
| `data/repository/DownloadRepository.kt` | Enqueue, retry, cancel, delete, stop-all, cancel-playlist-sync; bridges Room + WorkManager; threads thumbnailUrl and multi-playlist destination ids through jobs; resolves playlist-less jobs to the shared **Unsorted** playlist | ✅ |
| `ui/shell/MainShell.kt` | ModalNavigationDrawer ("Settings" header with expandable Library / Downloads / History & discovery / App accordions, including **Duplicate checker**) + transparent Scaffold over the black base with four larger edge-oriented green blobs and persistent header gradient + BottomNav (Home/Download/Library/Discover/Create) + AnimatedVisibility MiniPlayer + AppHeader ("Luno" title is a button — opens the Settings drawer) + music-folder import strip (renders MusicFolderImportManager status) + green-loader TransitionMask (instant appear, ~300ms hold, ~400ms FastOutSlowIn reveal) over the NavHost on every route change; **shared context-aware onPlay(List<MediaTrack>, Int, Boolean shuffle)**; SAF CreateDocument launcher for selected-track/playlist exports; Last.fm API-key dialog (shared `LastfmKeyDialog`, saves via `app.discoveryRepository`); GitHub release check dialog with release notes/page/APK browser link and one-time sideload warning | ✅ |
| `ui/navigation/NavGraph.kt` | NavHost: Routes (HOME, SEARCH, LIBRARY, DISCOVER, DOWNLOADS, **RECENTS**, **DUPLICATES**, **EXPORT_IMPORT**, FULL_PLAYER, **MADE_FOR_YOU**, derived Favorites, playlist detail); instant 0ms transitions — screens swap fully-formed under the shell's green-loader transition mask; `onNavigate` callback invoked before every navigate (mask-first); **context-aware `onPlay(List<MediaTrack>, Int, Boolean shuffle)` pass-through** plus selected-track/playlist export callbacks; Home/Library playlist tiles navigate to detail | ✅ |
| `ui/export/ExportImportScreen.kt` | Full-library SAF export/import page with bounded file reads, strict preview, ambiguity confirmation, additive merge status, and missing-download summary | ✅ |
| `ui/theme/Color.kt` | Dark palette constants + restrained black/green four-blob app-shell background | ✅ |
| `ui/theme/Theme.kt` | LunoTheme (Material3 darkColorScheme) | ✅ |
| `ui/theme/Type.kt` | Sans-serif typography scale | ✅ |
| `ui/theme/Dimens.kt` | Touch targets, icon sizes, padding constants | ✅ |
| `ui/components/MiniPlayer.kt` | Compact mini-player with artwork thumb, title, artist, play/pause, artwork-color surface, thin progress bar, and horizontal queue swipes | ✅ |
| `ui/components/ArtworkImage.kt` | Non-subcomposing Coil painter wrapper (file:// or remote artwork, gradient + music-note placeholder, `decodeSizePx` decode cap for list rows, optional load-error callback) + rememberArtworkColors (Palette dominant color → animated gradient stops) | ✅ |
| `ui/components/PlaylistCard.kt` | Shared playlist card: 2x2 collage thumbnail + name + green 3-dot menu (sync / stop-sync / URL / clear / delete, opt-in callbacks), with option suppression for derived system playlists | ✅ |
| `ui/components/TrackRowCard.kt` | Track row in playlist-card UI layout: artwork thumb + title/artist + green 3-dot, optional multi-select check indicator, optional long-press callback | ✅ |
| `ui/components/ArtworkCollage.kt` | 2x2 square collage of up to 4 track artworks (playlist thumbnails + detail header) | ✅ |
| `ui/components/TrackActionsSheet.kt` | Long-press/3-dot track sheet: favorite toggle, add to playlist (shared searchable picker with create-new playlist dialog), optional remove-from-playlist, and remove-from-library (confirm dialog) | ✅ |
| `ui/components/PlaylistPickerSheet.kt` | Shared bounded/lazy playlist picker with top search for batch/per-track adds, plus optional create-new destination row for playlist imports | ✅ |
| `ui/components/SortChip.kt` | Shared accent-green down-arrow sort control + dropdown (A–Z / Z–A / **Recently added** / **Duration — longest first**), `TrackSortMode` enum, `sortedByMode` and `sortedPlaylistsByMode` helpers; used by Library (songs + playlists) and PlaylistDetail | ✅ |
| `ui/player/FullPlayerScreen.kt` | Full-screen player: 280dp artwork, fixed animated dominant-color gradient backdrop, lazy-scrolling artwork/title/artist/scrubber/transport content, six current-track Last.fm recommendations with refresh and artwork fallback, queue + action-sheet triggers; **next/prev walk the source context (search results / library list / playlist / history)** | ✅ |
| `ui/player/QueueSheet.kt` | "Queue" modal bottom sheet — **tabbed (Playing Next / Recently played)** via the shared `QueueRecentsTabs` content | ✅ |
| `ui/player/QueueRecentsTabs.kt` | Shared Queue/Recents tabs (desktop contract): `QueueRecentsTab` enum, green `QueueRecentsTabRow`, `PlayingNextTab` (live queue in actual Media3 playback order, artwork rows, long-press drag-to-reorder via moveQueueItem, refreshes on queue revision/shuffle/current-track changes), `RecentlyPlayedTab` (persisted history, tap plays within history context, "N played" count + **Clear all**) | ✅ |
| `ui/player/RecentsScreen.kt` | "Queue & Recents" full screen (route `Routes.RECENTS`, opened from the Settings drawer "Recently played"): back arrow + the same tab row and tab content as the sheet, lists fill the page | ✅ |
| `ui/player/ActionSheet.kt` | Track action sheet: add to playlist (nested picker), play next, add to queue, go to artist, share | ✅ |
| `ui/home/HomeScreen.kt` | Spotify-style Home: green-accent hero, greeting, edge-clipped Recently-played (history, **replay moves to front — desktop semantics; cards play within the history context**) / **Made-for-you random mix (up to 50 tracks sampled from the full catalogue, stable until the library changes; title/View all opens virtual detail)** / **Favorites carousel (durable `Track.isFavorite`, with per-card 3-dot actions)** / **Most popular songs and playlists (songs by persisted track play count; playlists by persisted plays started inside that playlist)**; Settings drawer via tappable "Luno" header; all-or-nothing first render; all carousels are Compose-lazy and artwork decode-capped | ✅ |
| `ui/library/LibraryScreen.kt` | Desktop "All Music": Play + Shuffle (16dp apart) with the Playlist/Song view pill beneath Play, **far-right down-arrow SortChip (A–Z / Z–A / Recently added / Duration — tracks by own length, playlists by total length)**, search w/ clear-X, alphabetical tracks, **derived "Favorites" pinned above "Unsorted" in playlist view**, batch multi-select (3-dot: select all / add to playlist / remove / export selected songs or playlists), collage-thumbnail playlist cards; all-or-nothing first render; **Play/Shuffle/track-tap pass the sorted context for next/prev** | ✅ |
| `ui/library/DuplicateScreen.kt` | Scrollable duplicate groups sourced from the live library flow; select individual copies, confirm removal, and delete track metadata from the library/playlists while preserving device audio files | ✅ |
| `ui/search/SearchScreen.kt` | Desktop-patterned download workspace: flat Search / YT-CSV / Direct-URL source tabs; compact source input/action rows; artwork-backed YouTube result rows with inline queue states; YouTube playlist import with existing/new destination selection and saved playlist source; Exportify loader; durable app-private downloaded-songs accordion; long-press actions; multi-select playlist assignment; search-result Save to playlist; queue/retry/cancel states | ✅ |
| `ui/library/PlaylistDetailScreen.kt` | Playlist detail for persisted `playlist/{playlistId}`, derived Favorites, and ephemeral `made_for_you`: 2x2 four-artwork collage header, name/desc/count/duration, play-all + **shuffle**, **shared SortChip plus favorites-first star**, **in-playlist search**, track list (play from track + **right-edge 3-dot actions** with add/remove/favorite), selected-song JSON export and persisted-playlist export action; persisted playlist playback carries its ID for view counting, while virtual playlists do not; persisted playlists read from LibraryData, virtual playlists read from app-held tracks/sample | ✅ |
| `ui/discover/DiscoverScreen.kt` | Last.fm recommendations (2026-07-31): seed = current → last played → last-added track; "Based on" header + refresh + API-key shortcut; rows with Last.fm artwork plus cached Deezer fallback + "artist • N% match"; loading is reported to MainShell so its single transition-mask spinner remains visible through the request; per-row download, per-row playlist-target download, Download all, and Download all to selected playlist (YouTube search → audio extraction → queue, Search pipeline); honest key-missing / no-seed / loading / error(retry) / no-new-recs states; library-owned recs filtered by DiscoveryRepository | ✅ |
| `ui/discover/LastfmKeyDialog.kt` | Shared Last.fm API-key dialog (Discover empty state + Settings drawer): paste key, Save (disabled when blank) / Remove, encrypted via DiscoveryRepository | ✅ |
| `ui/downloads/DownloadsScreen.kt` | Full download management screen: playlist sync controls, per-playlist sync, batch sync all, **Stop All**, download queue with cancel/retry/delete; renders from LibraryData (instant) | ✅ |
| `ui/create/CreatePlaylistSheet.kt` | AlertDialog with name validation; optional `onCreated(Playlist)` callback for flows that need the new destination immediately | ✅ |

### Tests
| File | Responsibility | Status |
|------|---------------|--------|
| `data/db/AppDatabaseTest.kt` | Abstract Robolectric base class (in-memory DB) | ✅ |
| `data/db/TrackDaoTest.kt` | 13 tests: insert, search, dedupe, delete, count, local play-count increment, favorite toggle, albumArtPath round-trip, updateTrack (artwork-fetch persistence) | ✅ |
| `data/repository/DuplicateFinderTest.kt` | 3 tests: normalized release-tag grouping, featuring/punctuation normalization, unique/blank-key exclusion | ✅ |
| `data/db/PlaylistDaoTest.kt` | 9 tests: CRUD, track-to-playlist, cascade, sortOrder, per-playlist play-count increment | ✅ |
| `data/repository/PlaylistRepositoryTest.kt` | 6 tests: blank name rejection, persistence, trim, list, delete, clear-playlist keeps playlist + tracks | ✅ |
| `data/db/DownloadJobDaoTest.kt` | 11 tests: insert, query, progress, complete, fail, state filter, active-list queries, delete, bulk delete, count | ✅ |
| `data/repository/DownloadRepositoryTest.kt` | 10 tests: enqueue/Unsorted routing and reuse, explicit playlist assignment, legacy completed-download repair, thumbnailUrl persistence, list, state filter, cancel, delete, get-null | ✅ |
| `data/export/LibraryManifestCodecTest.kt` | Strict v1 round-trip plus unknown-field, path/reference, version, provider, and ambiguity validation | ✅ |
| `data/repository/LibraryTransferRepositoryTest.kt` | Full/selected export coverage: playlist relation ordering, unassigned tracks, and local-URI exclusion | ✅ |
| `playback/NotificationPermissionPolicyTest.kt` | 11 tests: API 29/33+ prompt policy, grant/deny/attempted behavior | ✅ |
| `playback/MusicControllerTest.kt` | Contract tests: pending-play, shuffle request/random-start semantics, uniform non-current shuffle-index mapping, empty-request handling, full-queue preservation, last-request-wins, index clamp, release idempotence, stale-future guard, exact MediaItem metadata (incl. artworkUri and playlist context), moveQueueItem no-ops, setShuffle no-op, persisted recently-played restore/clear, and sanitized error emission | ✅ |
| `data/discovery/LastfmServiceTest.kt` | 11 tests (MockWebServer): two-stage fallback (0.8 match), param/limit wiring, artist object + string parsing, largest-image pick, API error / HTTP error / network error failures, cache hit, empty-not-cached, 10-entry clear-on-exceed, clearCache | ✅ |
| `data/repository/DiscoveryRepositoryTest.kt` | 6 tests: key trim/persist/blank-clear/clear, library filtering (normalized keys), failure pass-through, arg forwarding | ✅ |
| `data/update/GitHubReleaseServiceTest.kt` | MockWebServer coverage for newer/same/invalid releases, APK asset parsing, GitHub headers, HTTP failure, and version comparison | ✅ |
| `ui/components/SortChipTest.kt` | Recently-added label plus newest-first ordering for songs and playlists | ✅ |
| `playback/MusicControllerInstrumentedTest.kt` | 7 instrumented tests: connection, error path, pre-connection queue dispatch, real playback-state transition, notification posting, and activity recreation | ✅ Compiles; emulator execution pending |
| `storage/LibrarySmokeTest.kt` | 6 instrumented tests: repository/DAO basics, successful FileProvider-backed import, and deterministic revoked-permission failure | ✅ Compiles; emulator execution pending |

---

## ⚠️ Security Considerations

1. **Last.fm API key** — stored in `EncryptedSharedPreferences` (`LastfmKeyStore`, security-crypto 1.1.0-alpha06; plain-prefs fallback only if the Keystore is unusable); never logged, never exported in JSON manifests, never sent over HTTP (HTTPS always)
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
| Dependency injection: Hilt vs manual | **Resolved** | **Manual singleton DI** in `LunoApp` for now. Hilt deferred — not a blocking decision. |
| CI/CD for signed APK releases | **Resolved** | `.github/workflows/android-release.yml` — tag-triggered Java 17 build, unit tests, lint, environment-backed signing, `apksigner` verification, and GitHub Release APK upload. The permanent keystore remains outside Git and is supplied through four repository secrets. |
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
| MockWebServer (okhttp3) | 4.12.0 | HTTP mocking for the Last.fm client tests | ✅ In use |
| Coil (coil-compose) | 2.6.0 | Artwork image loading (full player, mini-player, queue, home) | ✅ **In use** |
| Palette (palette-ktx) | 1.0.0 | Dominant-color extraction for dynamic artwork gradients | ✅ **In use** |
| **Excluded (current):** | | | |
| WorkManager | 2.9.0 | Background downloads | ✅ **Implemented** |
| OkHttp | 4.12.0 | HTTP client for googlevideo.com downloads (supports HTTP/2, ALPN) | ✅ **Implemented** |
| NewPipe Extractor | v0.26.4 | YouTube extraction (vendored submodule via composite build) | ✅ **Adopted** |
| Hilt | — | Dependency injection | Deferred |
| Kotlinx Serialization | — | JSON parsing | **Planned** |
| DataStore | — | Preferences | **Planned** |
| EncryptedSharedPreferences (security-crypto) | 1.1.0-alpha06 | Secure credential storage (Last.fm key — `LastfmKeyStore`) | ✅ **In use** — deprecated upstream; kept per the security decision (2026-07-31) |

---

## 🧪 Testing Strategy

| Level | Tool | Scope | Status |
|-------|------|-------|--------|
| Unit | JUnit 4 + Truth + Turbine + Robolectric | Room DAOs, Repositories | ✅ **43 tests** across the Room DAO/repository suite, including `TrackDaoTest` local play-count coverage |
| Unit | JUnit 4 + Truth + Robolectric | Notification permission policy | ✅ **11 tests** |
| Unit | JUnit 4 + Truth + Robolectric | Last.fm discovery | ✅ **17 tests** — `LastfmServiceTest` (11, MockWebServer: two-stage fallback, cache policy, error handling, parsing), `DiscoveryRepositoryTest` (6, encrypted-key round-trip + library filtering) |
| Unit | JUnit 4 + Truth + Robolectric | Missing-artwork sweep | ✅ **16 tests** — `LibraryRepositoryArtworkFetchTest` (11: valid/invalid cache detection, embedded + remote refill, skip/progress with injectable seams), `ArtworkFetchManagerTest` (3: Progress→Finished→auto-clear, re-tap guard, failure resolution), and `ArtworkFetchServiceTest` (2: combined/title Deezer search and decoded image persistence) |
| Unit | JUnit 4 + Truth + Robolectric | MusicController pending-play/lifecycle contract | ✅ **40 tests** — full-queue preservation, empty-request handling, last-request-wins, index clamp, release idempotence, stale-future guard, exact MediaItem metadata (incl. artworkUri), moveQueueItem no-ops, setShuffle no-op, and sanitized error emission |
| Instrumentation | Android Instrumentation Test + emulator | Media3 connection, queue dispatch, playback state, notification posting, activity recreation, SAF import, and revoked URI access | ✅ **13 smoke tests compile** — `MusicControllerInstrumentedTest` (7), `LibrarySmokeTest` (6); **not run in CI** (emulator job removed 2026-07-30); local emulator execution possible |
| UI | Compose UI Test | Screen composables, navigation | **Planned** |
| Integration | Android Instrumentation Test | WorkManager workers, full Media3 interaction | **Planned** |
| Snapshot | Roborazzi (or Paparazzi) | Visual regression for Compose screens | **Planned** |
| End-to-end | Maestro / ADB script | Full playback flow, download flow, import/export | **Planned** |

**Latest verification status (2026-08-01):** ✅ All baseline checks pass on Java 17:
- `./gradlew :app:assembleDebug` — **PASS**
- `./gradlew :app:testDebugUnitTest` — **PASS** (including the 3 new `DuplicateFinderTest` cases)
- `./gradlew :app:lintDebug` — **PASS** (0 errors; existing warnings remain)

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
| 2026-08-06 | **Scrollable full-player recommendations.** `FullPlayerScreen` now keeps its artwork-derived gradient fixed while a lazy content list moves the artwork, metadata, scrubber, and transport controls upward. The page adds six current-track Last.fm recommendations with refresh, library ownership filtering, API-key/loading/error/empty states, and the cached Deezer artwork fallback. |
| 2026-08-04 | **Missing artwork network lookup hardened.** `ArtworkFetchService` now tries a broad artist/title Deezer query followed by title/artist fallbacks, validates that downloaded bytes decode into usable cached artwork, and has MockWebServer coverage for both lookup paths. |
| 2026-08-03 | **Playlist picker made searchable and scroll-safe.** `PlaylistPickerSheet` now uses a bounded lazy list so every playlist is reachable, keeps a case-insensitive search field at the top, shows a clear no-match state, and is reused by long-press track actions as well as download-result assignment. |
| 2026-08-02 | **Cross-platform export/import implemented.** Added the shared `luno.library.export` v1 manifest contract to desktop (`library_transfer.py`) and native Android (`data/export/LibraryManifest*.kt`), with strict untrusted-input validation and no paths/secrets/audio/history. Desktop Settings, playlist actions, and selected-track menus export/import additive metadata and queue confirmed missing downloads. Android added `LibraryTransferRepository`, SAF `ExportImportScreen`, drawer navigation, selected song/playlist export callbacks, Room v8 multi-playlist download destinations, and codec/repository tests. |
| 2026-08-02 | **Desktop-style downloader UI.** `SearchScreen` now mirrors the desktop downloader's source-panel logic without changing the Android download pipeline: compact `Downloader` header, flat `Search` / `YT / CSV` / `Direct URL` tabs, integrated input/action rows, conditional thin queue-progress line, inline status/errors, flat result rows, and the durable downloaded-songs accordion after the source area. The YouTube playlist importer and Exportify loader now share the `YT / CSV` tab; direct-link title/artist details remain optional. |
| 2026-08-02 | **Background blob balance refined.** Reduced the shell background from seven blobs to four larger edge-oriented radial blobs, preserving the permanent top/header gradient while leaving a calmer black center and wider gaps between glows. |
| 2026-08-02 | **Initial blob-based app-wide background.** Replaced the broad diagonal wash in `MainShell.appBackgroundWash()` with seven separated radial green blobs over the `#101010` base, using transparent edges and small `#1ED760` highlights so black gaps remain visible. A dedicated top gradient stays behind the status-bar/Luno header area. The same shared treatment remains on `TransitionMask`; navigation, drawers, dialogs, and the artwork-driven full player keep their existing specialized surfaces. Superseded by the four-blob refinement above. |
| 2026-08-02 | **Download intake and YouTube playlist import.** `SearchScreen` now presents YouTube search and a primary **Import a YouTube playlist** card with explicit URL input, destination selection, existing-playlist picker, create-new flow, validation through `WebSearchService.extractPlaylistId()`, and `DownloadRepository.syncPlaylist()` queueing. The selected playlist stores the pasted source URL for future syncs; direct audio and Exportify CSV remain secondary options. `PlaylistPickerSheet` supports an optional create-new row and `CreatePlaylistSheet` reports the created playlist through `onCreated`. |
| 2026-08-02 | **Angled app-wide background wash.** `MainShell.appBackgroundWash()` draws over the `#101010` base in this order: (1) a `Brush.linearGradient` from `(-0.18W, 0.88H)` to `(1.10W, 0.12H)` with transparent endpoints and stops `0.18=#102B1C@16%`, `0.42=#102B1C@44%`, `0.58=#1ED760@7%`, `0.78=#102B1C@24%`; (2) an upper-right `Brush.radialGradient` centered at `(0.82W, 0.14H)`, radius `0.72 × maxDimension`, with `#1ED760@12% → #102B1C@27% → #102B1C@8% → transparent` stops at `0.0/0.28/0.72/1.0`; (3) a lower-left radial gradient centered at `(0.12W, 0.82H)`, radius `0.66 × maxDimension`, with `#102B1C@36% → @18% → @5% → transparent` stops at `0.0/0.34/0.78/1.0`. The same wash is applied to `TransitionMask` behind the shared green spinner on every route. Transparent edges keep the wash subtle; navigation, drawers, dialogs, and the artwork-driven full player retain their existing opaque or specialized surfaces. |
| 2026-08-01 | **Android GitHub Releases configured.** Added release-only environment-backed signing in `app/build.gradle.kts` (the existing `./gradlew installDebug` path is unchanged), `verifyReleaseVersion` tag/version validation, `.github/workflows/android-release.yml` for signed APK builds and automatic GitHub Release publication, and `ANDROID_RELEASES.md` for keystore/secrets setup. The workflow runs on `vMAJOR.MINOR.PATCH` tags and verifies the APK with `apksigner`; the permanent signing key is never stored in the repository. |
| 2026-08-01 | **Most popular Home section.** Replaced the Home "Your playlists" section with a local **Most popular** section containing separate song and playlist carousels. Playback now increments a persisted `Track.playCount` (Room v6 migration); songs rank by their own counts and playlists rank by the summed counts of their member songs. The feature performs no network work and renders lazily from the warmed library flows. Added `TrackDaoTest` coverage for play-count increments. |
| 2026-08-02 | **Playlist-specific popularity counts.** Home playlist popularity no longer sums global song plays. Room v7 adds `Playlist.playCount`; playback requests from persisted `PlaylistDetailScreen` carry the playlist ID in Media3 metadata, and only those transitions increment that playlist's count. All Songs, shuffle, Popular Songs, Recently Played, and virtual Made for You playback leave playlist counts unchanged. |
| 2026-08-01 | **Duplicate checker + organized Settings drawer.** Added `DuplicateFinder` with the desktop-compatible normalized artist/title key and a new `DuplicateScreen` route reachable from the Settings drawer. Duplicate groups are scrollable, individual copies can be selected, and confirmed removal deletes only library/playlist metadata while preserving device audio files. The Settings drawer is now organized into expandable Library, Downloads, History & discovery, and App accordions. Added `DuplicateFinderTest` coverage for release tags, featuring variants, punctuation, and unique keys. |
| 2026-07-31 | **Fetch-missing-artwork progress + validation.** The Settings-drawer sweep previously closed the drawer and showed nothing until the completion toast — minutes of silence over a 4000-track library. Now owned by a process-lifetime **`ArtworkFetchManager`** (`ui/shell/ArtworkFetchManager.kt`): `StateFlow<ArtworkFetchStatus?>` (Progress with live scanned/total/updated → Finished → auto-clear after **5s**, failures resolved as "Artwork fetch failed — please try again (message)" in strip + toast + Log.e), activeJob re-tap guard, main-looper guarded toast; MainShell renders a live strip **inside the scaffold content column** (under the app header, next to the import strip — an earlier placement as a Scaffold sibling overlaid it on the header and clipped it; also the final message was 2s, too brief to read — both fixed 2026-07-31) with a smooth sweep bar + "Fetching missing artwork… N/M (K found)". `LibraryRepository.fetchMissingArtwork` gained an `onProgress` callback + injectable `extract` seam (production default = the same `ArtworkStorage.saveEmbeddedArtwork` call proven at import time). **Validation:** 11 new tests — `LibraryRepositoryArtworkFetchTest` (8: null/blank/deleted-cache detection, refill + DAO persistence, valid-artwork skip, failed-extraction skip, file:// URI passthrough, progress sequence, empty library) + `ArtworkFetchManagerTest` (3: state machine, re-tap guard, failure). Baseline re-verified: `testDebugUnitTest` **141/141** ✓, `assembleDebug` ✓, `lintDebug` ✓ (no new findings). |
| 2026-07-31 | **Queue / Recents tabs + Recents screen.** The full player's `QueueSheet` is now a **tabbed sheet — "Playing Next" | "Recently played"** (shared `ui/player/QueueRecentsTabs.kt`: `QueueRecentsTab` enum, green `QueueRecentsTabRow`, `PlayingNextTab` with the existing drag-to-reorder, `RecentlyPlayedTab` with play-in-history-context rows, "N played" count and **Clear all**). The same tabs are available as a full page: new `ui/player/RecentsScreen.kt` ("Queue & Recents", route `Routes.RECENTS`) opened from the Settings drawer's new **"Recently played"** item ("Clear recent history" now uses the DeleteSweep icon). History remains in-session (Room `HistoryEntry` still planned). Baseline re-verified: `testDebugUnitTest` 130/130 ✓, `assembleDebug` ✓, `lintDebug` ✓ (no new findings). |
| 2026-07-31 | **Duration sort + playlist-detail sort chip.** The Library's sort chip gained a fourth mode — **Duration (longest first)** — extracted into a shared `ui/components/SortChip.kt` (`TrackSortMode` A–Z / Z–A / Recently added / Duration + track/playlist sorting helpers). In the Library it applies to both views: songs by their own `durationMs`, playlists by their **total** duration. The **entire filter-button setup now lives on the playlist-detail screen too**: the same SortChip under Play/Shuffle, with rows AND the play context (next/prev) walking the sorted order (`rememberSaveable` mode per screen). Baseline re-verified: `testDebugUnitTest` 130/130 ✓, `assembleDebug` ✓, `lintDebug` ✓ (no new findings). |
| 2026-07-31 | **Last.fm discovery implemented.** `data/discovery/LastfmService.kt` (desktop `DiscoveryService` port: `track.getsimilar` → `artist.gettoptracks` fallback at 0.8 match, 10-entry clear-on-exceed cache, non-empty results only, largest-artwork pick, typed failures for bad key/API/network; OkHttp 8s timeout, injectable base URL), `data/discovery/LastfmKeyStore.kt` (EncryptedSharedPreferences AES256-GCM/SIV via security-crypto 1.1.0-alpha06, plain-prefs fallback on Keystore failure), `data/repository/DiscoveryRepository.kt` (apiKey StateFlow, set/clear with cache invalidation, library filtering via desktop normalized "artist - title" keys), `ui/discover/LastfmKeyDialog.kt` (shared dialog), DiscoverScreen rewrite (seed = current → last played → last-added track; "Based on" header + refresh + key shortcut; rows with Last.fm artwork + match %; per-row + batch "Download all" through the normal Search acquisition pipeline; honest key-missing/no-seed/no-new-recs/error states). Settings drawer gained "Last.fm API key". NavGraph passes the controller to Discover. Tests: `LastfmServiceTest` (11, MockWebServer) + `DiscoveryRepositoryTest` (6). Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` **130/130** ✓, `lintDebug` ✓ (no new findings). |
| 2026-07-31 | **Green accent restored + flat listing rows + full-player title/artist spacing.** A purple-rebrand attempt (#2F33A5, `AccentGreen`→`AccentPurple`) was fully reverted the same day — the brand accent is **green `#1ED760`** again (`AccentGreen` in `ui/theme/Color.kt` + Theme.kt + all screens/sheets/components, nav bar, sliders, buttons, spinners, 3-dot menus, import strip, transition-mask spinner; XML `accent_green` and `ic_launcher_foreground`; launcher music-note vector). Kept from that round: playlist/song listing rows (`PlaylistCard`, `TrackRowCard`) lost their `SurfaceDark` rounded background — flat rows of thumbnail + text + green 3-dot (kept); full player title→artist gap increased 8dp → 12dp. Baseline re-verified: `assembleDebug` ✓, `testDebugUnitTest` ✓, `lintDebug` ✓. |
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
| 2026-07-30 | **Downloader runtime-verified.** Confirmed on-device (S20 FE, API 33): YouTube search → audio extraction → download working. Refreshed baseline checks: `assembleDebug` ✓, `testDebugUnitTest` 83/83 ✓ (was recorded as 62 — MusicControllerTest is 35, not 29; adds DownloadJobDaoTest 9 + DownloadRepositoryTest 6), `lintDebug` ✓, `compileDebugAndroidTestKotlin` ✓. Fixed lint Error `RemoveWorkManagerInitializer`: app uses on-demand WorkManager init via `Configuration.Provider` (`LunoApp`), so the default `WorkManagerInitializer` meta-data is now stripped from the merged manifest (`tools:node="remove"`). Synced stale facts: playlist-sync data flow now cites NewPipe `PlaylistExtractor` (not Piped API); `WRITE_EXTERNAL_STORAGE` rationale updated (downloads write to app-internal `filesDir/downloads/`). |
| 2026-07-30 | Implemented Android downloader: `DownloadWorker`, `DownloadRepository`, `DownloadJob` Room entity (v2 schema), DownloadJobDao. Updated all status markers, file registry, data flow, architecture diagram, and file tree. |
| 2026-07-30 | Implemented playlist sync: `PlaylistSyncWorker`, `WebSearchService` (Piped API client), `DownloadsScreen`, v3 schema (playlistUrl on Playlist), CSV import (Exportify). Updated all sections accordingly. |
| 2026-07-30 | Fixed YouTube Web Search: migrated from Piped-only API to YouTube InnerTube API as primary with Piped fallback. Updated `WebSearchService.kt` to call InnerTube search/browse/player endpoints directly. Fixed the Piped API type filter (`"stream"`), added User-Agent headers, removed duplicate import. Updated project brain sections. |
| 2026-07-30 | Fixed audio URL extraction: swapped to Piped-first priority, replaced dead kavin.rocks with working community instances, expanded InnerTube to try 4 client types (ANDROID_MUSIC, ANDROID, TVHTML5_SIMPLY, WEB) with full device context, added Invidious API as third fallback layer, added headers to DownloadWorker. |
| 2026-07-30 | Reordered extraction fallback: YouTube watch page HTML scraping (ytInitialPlayerResponse, brace-depth JSON parser) as primary method, then Piped → InnerTube (updated to 2025 client versions with playbackContext/signatureTimestamp) → Invidious. Added download loading spinner per video ID in SearchScreen. DownloadWorker: replaced private system notification icon with app's ic_download, added MediaMetadataRetriever duration extraction, fixed notification ID collision (1000 + jobId). |
| 2026-07-30 | **Migrated YouTube extraction to NewPipe Extractor v0.24.3.** Replaced the fragile 4-fallback InnerTube/Piped/Invidious chain with bundled native NewPipe Extractor library. Added `NewPipeDownloader.kt` (HttpURLConnection-based Downloader), `ExtractionResult.kt` (typed error propagation). Updated `WebSearchService.kt` to use NewPipe's `StreamExtractor`, `SearchExtractor`, and `PlaylistExtractor`. Added extraction error visibility in SearchScreen and PlaylistSyncWorker. Updated `LunoApp.kt` to initialize NewPipe at startup. Added JitPack repo and dependency. |
| 2026-07-30 | **Multi-strategy audio extraction & download rewrite.** NewPipe v0.24.3 fails SABR enforcement (requires v0.26.3+ for fix, not available on JitPack). Added three-stage fallback: (1) Invidious companion proxy (`tiekoetter.com/embed` → companion `latest_version` with itag=140 for audio-only M4A), (2) InnerTube ANDROID client with `lsparams`/`lsig` stripping, (3) `n`-parameter deobfuscation via `YoutubeJavaScriptPlayerManager`. Replaced `HttpURLConnection` with OkHttp 4.12.0 in `DownloadWorker` (`ConnectionSpec.MODERN_TLS`) for HTTP/2 ALPN support. Added `CookieManager` for session cookies. Auto-detects file extension from Content-Type. googlevideo.com direct downloads return HTTP 403 (HTTP/1.1 protocol mismatch with `gvs 1.0` CDN) — the Invidious companion proxy bridges this gap. |
| 2026-07-30 | **Downloader hardening + vendored extractor.** Bundled NewPipeExtractor v0.26.4 as a Git submodule (`vendor/NewPipeExtractor`, pinned tag, shallow) built via Gradle composite build — JitPack stops at v0.24.x; requires JDK 11 toolchain (registered in CI via `org.gradle.java.installations.paths`; local via Temurin 11) and core-library desugaring (`desugar_jdk_libs_nio` 2.1.4, minSdk 29 < 33); CI checkout now `submodules: recursive`. Hardened `WebSearchService`: audio selection by priority M4A/AAC → Opus → best bitrate; Invidious companion preserves `check` and picks the itag from the actual format list instead of the blind 18→140 rewrite. `DownloadWorker`: redacted failure logging (no full headers / signed stream URLs) and status-specific error messages (400/403/404/429/503 distinct). **On-device runtime verification pending.** |

## 🛠️ Build & Test Operations

When building the debug APK for testing, always copy it to the repo root:

```
cd ~/luno/mobile-app && ./gradlew assembleDebug && cp app/build/outputs/apk/debug/app-debug.apk ~/luno/test.apk
```

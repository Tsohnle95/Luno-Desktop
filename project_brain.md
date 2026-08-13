# 🧠 Project Brain: Luno (Advanced Edition)

> [!IMPORTANT]
> **MAINTENANCE INSTRUCTIONS FOR AI/HUMANS:**
> When updating this document, prioritize **architectural clarity** over code snippets.
> 1. **File Tree**: Keep the File Tree section accurate — every file in the repo must appear there or be explicitly marked as dead/orphaned.
> 2. **Data Flow**: If the pipeline for library scanning, playback, downloading, or artwork changes, update the "Data Flow" section immediately.
> 3. **Add New Views**: When a file is added to `views/`, add it to the "File Registry" with a 1-sentence summary of its unique logic.
> 4. **Update State Patterns**: If the structure of `_library` or `engine.queue` changes, update the "State Management" section immediately.
> 5. **Document "Magic" Logic**: If a new comparison logic (regex), parsing rule (splitting strings), or binary dependency is added, record it in the "Logic" or "Dependencies" sections.
> 6. **Keep it High-Level**: Do not document every method. Only document component interfaces (e.g., how the Controller broadcasts to Views).
> 7. **Verify Constraints**: Always ensure the "Technical Protocols" (44.1kHz, Threading) reflect the current reality of the engine.

This document is a high-level architectural map for the **Luno**. It provides the technical context needed for an AI assistant to understand the codebase, state flows, and logic without ingesting every file.

---

## 📁 File Tree

```
├── music_player.py           # Main entry point — Controller (LunoMusicPlayer)
├── README.md                 # Repository orientation, commands, and boundaries
├── engine.py                 # Audio engine + discovery + playback compatibility surface
├── discovery.py              # Last.fm recommendation service (with artwork URLs)
├── previews.py               # Temporary recommendation preview pipeline (resolve + download)
├── rec_artwork.py            # Deezer row-artwork fallback + 75 deterministic abstract designs
├── source_lookup.py          # yt-dlp source probing helpers
├── video_preview.py           # Temporary YouTube video probe/download helpers
├── library_tools.py          # Library scanning, CSV parsing, and duplicate detection
├── downloader.py             # yt-dlp download wrapper (Downloader class)
├── theme.py                  # Color palette + playlist gradient system
├── utils.py                  # Shared helpers (resource_path, format_time, split_track_name, etc.)
├── library_transfer.py       # Versioned, path-free JSON manifest codec and desktop merge planner
├── widgets.py                # Custom Canvas widgets (RoundButton, SlimSlider, PlaylistArt, ModernScrollbar)
├── icon.icns                 # macOS app icon (Apple Icon Image; regenerated from the current artwork)
├── build.command             # Executable macOS build helper; activates .venv then runs PyInstaller spec
├── build-mobile.command      # Executable macOS helper; builds the Android debug APK and installs/launches it when a device is connected
├── Luno.spec             # PyInstaller spec for macOS build
├── .gitignore                # Ignores local envs, Python caches, build/dist artifacts, and OS metadata
├── .github/workflows/desktop.yml # Desktop pytest verification on pushes and pull requests
├── project_brain.md          # This document
├── contracts/library-export/v1/ # Language-neutral manifest schema and compatibility fixtures
├── docs/                     # Current architecture, decisions, and product roadmap
├── mobile-app/               # Native Android app; see mobile-app/project_brain.md for its full architecture/context
│   └── project_brain.md      # Mobile brain (source of truth for Android work)
│
├── views/                    # View modules (all inherit tk.Frame)
│   ├── __init__.py           # Empty — required for PyInstaller package recognition
│   ├── library.py            # Main library view (virtual scrolling, search, sorting, playlist actions)
│   ├── downloader.py         # Downloader hub view (Search, YT/CSV, Sync, Duplicates, Allowed)
│   ├── recent.py             # Recently played tracks
│   ├── discover.py           # Last.fm discovery hub
│   ├── settings.py           # Settings panel (API keys, audio, storage)
│   ├── song_page.py          # Now Playing full-screen detail view + Recommended-for-this-song section
│   └── error_log.py          # Debug log viewer
│
├── test_engine_playback.py   # Deterministic mocked unit tests for playback engine
├── test_discover_mode.py     # Deterministic mocked unit tests for Discover mode (first-five window, rolling refill, handoff teardown)
├── test_art_cache.py         # Regression tests for bounded artwork caching
├── test_contract_fixtures.py  # Shared library-export compatibility fixture tests
├── test_library_transfer.py   # Manifest validation, export, matching, and additive merge tests
├── test_media_keys.py        # Deterministic mocked unit tests for media key logic
├── test_downloader_sync.py   # Deterministic mocked tests for playlist synchronization
├── test_tkvideo.py            # Ad-hoc Tk/video experiment (not part of the app)
├── pytest.ini                 # Default pytest collection excludes dependency-bound ad-hoc scripts
├── test_dl.py
├── test_focus.py
├── test_label_ph.py
│
└── requirements.txt          # Reproducible Python environment for app/build dependencies
```

Generated/local directories are intentionally omitted from the tracked tree: `.venv/`, `build/`, `dist/`, `__pycache__/`, `.pytest_cache/`, and `.DS_Store` are ignored and should not be committed.

---

## 📦 Build & Packaging (PyInstaller)
To bundle the application as a standalone executable (`.app` on macOS or `.exe` on Windows), follow these protocols:
- **Local Environment Setup**: Create the local env with `python3.12 -m venv .venv`, then install dependencies via `.venv/bin/python -m pip install -r requirements.txt`. The `.venv/` directory is local-only and ignored by Git.
- **Executable Build Helper**: `build.command` is executable (`100755`) and can be run directly/double-clicked on macOS. It activates `.venv`, runs `pyinstaller --clean --noconfirm Luno.spec`, then opens `dist/Luno.app` if the build succeeds.
- **Native Android Build Helper**: `build-mobile.command` is executable (`100755`) and can be run directly/double-clicked on macOS. It runs the Android Gradle wrapper from `mobile-app/`; with an `adb`-connected device it runs `:app:installDebug` and launches `com.luno.mobile`, otherwise it runs `:app:assembleDebug` and reports the APK path.
- **macOS Build Command**: Use `python -m PyInstaller --noconsole --windowed --noconfirm --clean --name "Luno" --icon "icon.icns" --add-data "views:views" music_player.py`.
- **macOS Bundle Identifier**: The `Luno.spec` file sets `bundle_identifier='com.ty.luno'`. The bundle identifier is now Luno's stable macOS application identifier.
- **PyInstaller hiddenimports**: The spec includes `'Foundation', 'objc', 'Quartz', 'AppKit'` to ensure PyObjC bridges resolve `True` at runtime in the bundled app.
- **Windows Build Command**: Use `python -m PyInstaller --noconsole --windowed --noconfirm --clean --name "Luno" --icon "icon.ico" --add-data "views;views" music_player.py`. **Note:** This *must* be run on a Windows OS. Windows uses a semicolon `;` as a path separator and requires an `.ico` icon file.
- **Package Integrity**: The `views/` directory must contain an `__init__.py` to be correctly recognized as a package during bundling.
- **Path Handling**: Always use `utils.resource_path()` for assets to ensure they are found within the `sys._MEIPASS` temporary directory at runtime.
- **Environment**: Always run the build from within `.venv` to ensure all dependencies (`pyinstaller`, `pygame`, `yt-dlp`, `requests`, PyObjC frameworks, etc.) are linked. If you add a new dependency, update `requirements.txt` and reinstall inside `.venv` before building.
- **Artifact Policy**: `build/` and `dist/` are generated PyInstaller outputs. They must remain ignored and should be regenerated locally or published as release artifacts, not committed.

---

## 🛰 Binary Dependencies
The following must be available in `PATH` for full functionality:
-   **`yt-dlp`**: Metadata extraction and downloading; installed by `requirements.txt` into `.venv`.
-   **`ffmpeg`**: Essential for audio conversion, resampling, and embedding metadata.
-   **`pyinstaller`**: macOS desktop app packaging; installed by `requirements.txt` into `.venv`.
-   **`pygame`**: Playback backend (`pygame.mixer`); installed by `requirements.txt` into `.venv` and bundled by PyInstaller.
-   **`mutagen`**: Generic audio metadata/duration probing for playback (`mutagen.File`) plus ID3 artwork/tag handling.
-   **`Pillow`**: Dynamic rendering/resizing of embedded ID3 artwork and YouTube thumbnail conversion; installed by `requirements.txt` into `.venv` and bundled by PyInstaller.
-   **`requests`**: HTTP client for Deezer API, Last.fm API, and YouTube thumbnail fetching; installed by `requirements.txt` into `.venv`.

---

## 🧹 Repository Hygiene
- The repository tracks source/configuration only. Do not commit local environments, generated packages, bytecode, test caches, or macOS metadata.
- Root `.gitignore` intentionally covers `.venv/`, `venv/`, `env/`, `build/`, `dist/`, `__pycache__/`, `*.py[cod]`, `.pytest_cache/`, and `.DS_Store`.
- If generated artifacts are accidentally tracked, remove them from the index with `git rm -r --cached` when the local files should be preserved, or `git rm -r` when they should be removed from disk too.

---

## 🏗 High-Level Architecture: CVE Pattern
The project follows a **Controller-View-Engine (CVE)** pattern. All stateful preferences (API keys, library paths, volume) are persisted in `~/.luno_config.json` for session continuity, independent of the music library's location.

1.  **Controller (`music_player.py` / `LunoMusicPlayer`)**:
    *   **Shared State**: Holds `_library` (Playlists -> Paths), `tk.Variables` (status, search, etc.), and `_frames`.
    *   **Orchestration**: Manages tab switching (`_show_tab`) and broadcasts state changes to views.
     *   **ArtCache**: A `ThreadPoolExecutor`-backed cache for album art thumbnails, shared across all views. Stored pixels, in-flight requests, source image size, and stale path entries are bounded.
     *   **Watchdog**: A 30-second `self.after()` polling loop (`_init_watchdog`) that auto-detects filesystem changes by calling `_refresh_library()` in the background. It skips active downloads, syncs, transfers, and scans; manual and watchdog refreshes coalesce behind one active scan.
2.  **Engine (`engine.py` / `LunoEngine`)**:
    *   **Audio Logic**: Wraps `pygame.mixer`. Manages the `queue` and track transitions.
    *   **Heartbeat**: A 400ms `poll()` loop that updates the scrubber and checks for track completion.
    *   **Duplicate Exclusion**: Loads/saves allowed duplicate keys from `.luno_allowed_dups.json`.
3.  **Views (`views/*.py`)**:
    *   **Modular Frames**: Standalone modules that interact with the controller via `self.controller`.
4.  **Library Transfer (`library_transfer.py`)**:
     *   Builds and strictly validates the shared `luno.library.export` v1 JSON manifest. It contains metadata, ordered playlist memberships, and optional provider IDs only; it never contains audio bytes, local paths, secrets, queue state, or history.

---

## 🔀 Data Flow

### Library Scan → UI
1. `_refresh_library()` starts at most one generation-tagged thread calling standalone `scan_library(roots)` (imported from `engine.py`); repeated refresh requests coalesce into one pending scan and stale results are ignored
2. Returns `dict[playlist_name → list[file_paths]]`; root-level tracks represented by `Unsorted` are ordered newest-first by filesystem modification time, with filename tie-breaking
3. `_finalize_library_update(new_lib)` hashes the result, stores it in `self._library`
4. Calls `_populate_sidebar()` → rebuilds sidebar cards
5. Calls `_populate_track_list("")` → rebuilds track list
6. Broadcasts `on_library_updated()` to all views

### Track Name Editing
1. Library and Recently Played context menus call `LunoMusicPlayer._ctx_edit_track_info()` for one selected local file
2. The dialog collects an optional artist and required track name, sanitizes them, and renames the audio file to `Artist - Title.ext` (or `Title.ext` when artist is blank)
3. Live queue/history/favorite path references are updated, then the generation-tagged library scan refreshes search, ordering, and the Unsorted view

### Playback
1. User clicks track → `engine.play_with_context(path, context_list)`
2. Sets `queue` to `context_list` (or the playlist containing `path`)
3. Sets `queue_idx` to track's position
4. Calls `play_current()` → clears stale scrub position/time, loads `pygame.mixer.music`, starts playback, records mixer-clock base state plus monotonic fallback state, probes duration via `mutagen.File` (supports mp3/m4a/flac/etc.)
5. `_poll()` loop (400ms heartbeat) — unconditional reschedule after every invocation except when controller is closing:
   - Uses `get_position()` (canonical elapsed from `pygame.mixer.music.get_pos()` while audio is busy, clamped to `[0, duration]`, with monotonic fallback for startup/end/unavailable mixer-clock states)
   - Updates main scrubber/time and Song Page scrubber/time
   - Completion detection uses **generation latch** (`_completion_dispatched`): at most one advance per track
   - **Duration-based completion**: when `duration > 0` and `elapsed >= duration`
   - **Mixer-idle completion**: `pygame.mixer.music.get_busy()` false with `elapsed > 0.5s` startup grace — works for unknown-duration tracks too
   - Advances via `next_track()` or `play_current()` (repeat), which resets the latch and clears the scrub to 0

### Favorites
1. Favorites are persisted as normalized absolute file paths in the `favorite_songs` array of `~/.luno_config.json`.
2. The controller intersects the persisted set with every completed library scan, so missing files are removed from favorite state.
3. The desktop sidebar exposes a reserved virtual `Favorites` playlist; it never creates or scans a physical Favorites folder. Its play/shuffle context is the current favorite track list.
4. Track context menus, the queue menu, the player bar, and Song Page toggle favorite state. Library rows show an outlined or green filled star immediately before the playlist name, and the library action row can stably move favorites ahead of other tracks.

### Download Pipeline
1. `DiscoverView`'s Add to Library action → `LunoMusicPlayer._download_track()` → `Downloader.download_sync_tracks()`; `DownloadView._download_selected()` / `_queue_direct_url()` → `Downloader.download_single()`, `_download_all()` → `Downloader.download_sync_tracks()`, `_start_csv_dl()` → `Downloader.download_csv_tracks()`, and URL Queue / Sync All actions → `Downloader.download_playlist_sync()`
2. `Downloader` admits one active download/write job at a time, rejects duplicate starts, and propagates cooperative cancellation through yt-dlp progress hooks. Playlist track timeouts run in the owning job and never leave a detached yt-dlp worker running while the next track starts.
3. `Downloader._create_opts()` → yt-dlp options: 44.1kHz, 192k MP3, loudnorm, EmbedThumbnail
4. Discovery download status, logs, and completion callbacks are marshalled through `controller.after(0, ...)`; the sync-track path creates the destination folder and treats a missing audio output as a failure instead of reporting a false success. A completed discovery download requests a fresh scan and resets the library track list to its first row.
5. Progress hook → coalesced `controller.after(0, ...)` → UI update
6. On complete → `_refresh_library()` → coalesced library scan
7. Playlist sync compares candidates against the whole local music library root, not only the destination playlist folder, so Sync All avoids duplicating tracks that already exist in another playlist. It tracks successful/matched YouTube IDs in `.downloaded_vids` and permanent failures in `.sync_failed_vids` inside the playlist folder.

### Artwork Fetching
1. `PlaylistArt` or `SongPageView` requests art → `ArtCache.get(path, size, callback)`; downloader search cards use a separate bounded 128-entry thumbnail cache and bounded in-flight executor
2. `ArtCache` checks a bounded in-memory LRU cache (maximum 96 entries and 16 megapixels) → if miss, queues the key and drains it through `ThreadPoolExecutor` under the in-flight limit
3. Worker reads `mutagen.mp3.MP3` → rejects oversized APIC payloads/images → extracts, crops, and resizes via `PIL.Image`
4. Returns `ImageTk.PhotoImage` via a bounded `controller.after(0, ...)` callback; stale path cache entries are pruned after each library update

### Song Video Preview
1. The Song Page's discreet Video control probes YouTube with yt-dlp on a worker thread. Resolution searches up to 10 results across up to three query variants (`artist - title`, `title artist`, and title-only), deduplicates IDs, ranks title matches, and constructs canonical watch URLs from IDs; it does not begin playback or download during the probe.
2. After confirmation, yt-dlp downloads one merged MP4 into `~/.luno_previews`, outside the library scan. The existing audio engine continues playing normally.
3. System FFmpeg uses one persistent real-time raw-frame decoder per playback segment, decodes at the downloaded video's native frame rate, and presents only the newest frame to Tk at approximately 30 FPS. Frames are aspect-fitted into the widest available artwork area without distortion; square album artwork remains undistorted while video uses letterboxing as needed. The audio engine's elapsed position is the video clock: pause stops decoding and resume starts a new decoder at the current timestamp; scrubbing likewise seeks without allowing an independent stream to drift. Decoder drift beyond a quarter second causes a restart from the audio clock. The Video control is grey when no usable video is available or playback is stopped and green only while the current track's video is active. Missing FFmpeg, unavailable sources, or failed downloads leave normal artwork/audio intact.
4. The Song Page `Preload next` checkbox uses the engine's active context queue (`engine.queue` and `queue_idx`) to resolve/download only the immediate successor into the same temporary directory. When that successor becomes current, its staged video is adopted automatically; it remains outside `_library` and the audio queue.

### Export / Import
1. Export actions snapshot `_library` and selected paths, then build the manifest and probe metadata on a worker thread.
2. The manifest stores stable provider IDs when available and marks metadata-only tracks with `ambiguity_confirmation_required`.
3. Import validates the untrusted JSON, matches local files by normalized artist/title and compatible duration, and additively copies matched files into sanitized playlist folders.
4. Missing tracks are never deleted or silently replaced. After confirmation, desktop missing tracks are queued through the existing YouTube/CSV downloader; unresolved tracks remain reported.

---

## 📂 File Registry & Responsibilities

### 🛰 Core Support
- **`theme.py`**: The design system. Defines the full color palette (`BG`, `BG2`, `BG3`, `BG4`, `SIDEBAR`, `PLAYER_BG`, `GREEN`, `TEXT`, `TEXT2`, `TEXT3`, etc.), the `PLAYLIST_GRADIENTS` list (10 curated pairs), and the `playlist_gradient(name)` function which deterministically maps a playlist name to a gradient pair via MD5 hashing.
- **`widgets.py`**: Custom Canvas components: `RoundButton`, `SlimSlider` (non-destructive redraw — track/fill/thumb items are created once and moved with `coords()`/`itemconfig()` on scrub ticks instead of `delete("all")`+recreate, which leaks Tk canvas-item memory on the 400 ms scrub heartbeat), `PlaylistArt` (Spotify-style dynamic 4-quadrant thumbnails via `ArtCache`, with performance-optimized rendering — see Protocols), `Spinner` (animated green indeterminate loading arc — mobile CircularProgressIndicator parity; a self-cancelling `after` loop that pauses while unmapped and cancels on destroy), and **`ModernScrollbar`**.
- **`engine.py`**: The audio engine and supporting services. Contains:
  - `LunoEngine`: Wraps `pygame.mixer`. Manages `queue` (list of paths), `queue_idx`, `user_queue_count` (Up Next behavior), `_original_queue` (shuffle restore), `shuffle`, `repeat`, `crossfade_secs`. Methods: `play_with_context()`, `play_current()`, `toggle_play()`, `next_track()`, `prev_track()`, `toggle_shuffle()`, `on_scrub_moved()`, `on_volume_moved()`, `get_position()` (canonical mixer-clock position with monotonic fallback), `_poll()`. Uses `pygame.mixer.music.get_pos()` as the preferred live playback clock, maintains seek/base state for absolute elapsed time after scrubbing, uses `time.monotonic()` only as a safe fallback, probes duration with `mutagen.File`, reschedules heartbeat unconditionally after completion paths, and keeps get_busy mixer-idle detection for unknown-duration tracks. Playback-load failures invoke the controller's `_on_playback_failed(path)` hook before the automatic skip so failed preview rows are marked and their files removed (mobile recoverFromPlaybackError parity).
  - `DiscoveryService`: Last.fm API client. Two-stage: `track.getSimilar` → `artist.getTopTracks` fallback. Cache limited to 10 entries, **keyed by (seed, limit)** — mobile parity so "Load 25 more" grows the cached list instead of returning the earlier smaller page. Results carry the largest usable Last.fm image URL (mega→extralarge→large→medium→small, https-normalized, default/noimage placeholders rejected — mobile `pickLargestImage` parity).
  - `scan_library(roots)`: Walks directories, returns `dict[playlist → paths]`.
  - `find_duplicates(library, allowed_keys)`: Groups tracks by `_normalize_for_dupe`.
  - `probe_soundcloud(query)`, `parse_exportify_csv(filepath)`: Supporting utilities.
- **`previews.py`**: The temporary recommendation preview pipeline (mobile `resolveRecommendation`/`downloadPreview` parity). `resolve_preview_source(artist, title)` probes `ytsearch5:` with `extract_flat` and picks the first candidate with a 1s–3h duration (else the first with unknown duration) — used under a hard 15s controller deadline ("Finding a playable source timed out. Tap to retry."). `download_preview(...)` streams the resolved source through FFmpeg to a 44.1 kHz / 192k MP3 **without loudnorm** (transient previews mirror the mobile raw-stream path), embeds the thumbnail, enforces a cooperative 5-minute deadline ("Preview download timed out. Tap to retry.") and a 512 MiB size cap ("This source is too large for a quick preview…"). The preview pipeline never uses the library `Downloader` single job slot, so preview chains run while real downloads are in flight.
- **`video_preview.py`**: Optional Song Page video support. Resolves a YouTube result and downloads a merged temporary MP4 into `PREVIEW_DIR`; it never adds video to `_library` or the audio queue. The Song Page uses system FFmpeg to decode frames into Tk and degrades to album art when the binary or source is unavailable.
- **`rec_artwork.py`**: Recommendation row artwork (mobile `RecommendationArtworkService` + `RecommendationArtwork` parity). `find_artwork(artist, title)` runs the cached Deezer cover search (`api.deezer.com/search` with **quoted-term queries** `'"artist" "title"'` plus a plain-text fallback — Deezer's `artist:"X" track:"Y"` field query returns no results), preferring cover_xl→big→medium, with nulls cached too. `render_design(index, size)` draws one of **75 deterministic abstract designs** (index = `crc32(normalized key) mod 75` — stable across processes) used for loading/missing/error row states; `load_image(url)` downloads and center-crops a square thumbnail.
- **`library_transfer.py`**: Portable transfer contract. Provides `build_manifest()`, `validate_manifest()`, `load_manifest()`, `write_manifest()`, `plan_import()`, and `apply_import_plan()` for the path-free v1 JSON format used by Android.
- **`downloader.py`** (root `downloader.py` — not the view): The `Downloader` class — wraps `yt-dlp` for downloading audio. Static methods: `download_single()`, `download_youtube_url()`, `download_csv_tracks()`, `download_sync_tracks()`, and `download_playlist_sync()`. A class-level job coordinator admits one active download/write job, exposes cancellation for shutdown, and keeps yt-dlp/FFmpeg work inside its owning daemon job. Forces 44.1kHz, 192k MP3, EBU R 128 loudnorm. Uses `EmbedThumbnail` postprocessor. *Note: `views/downloader.py` is the UI view that calls this module.*
- **`utils.py`**: Helpers: `resource_path` (PyInstaller asset resolution), `format_time`, `split_track_name`, `hex_to_rgb`, `rgb_to_hex`, `draw_gradient_rect`, and cross-platform mouse/trackpad scroll binding with precise Tk 9 delta normalization. Track-name editing uses the same filename parser after sanitizing user-entered names.

### 🖼 View Modules (`views/`)
- **`library.py`**: Virtual scrolling canvas (56px rows). Header displays playlist title, dynamic album art (via `PlaylistArt`), and a playlist-settings cog that opens playlist URL, sync, duplicate, and transfer actions. Includes an **in-playlist search bar** (`Filter tracks…`) using a `tk.Label` overlay with `rely=0.5` vertical centering and a trailing clear-X, plus a compact mobile-parity `Sort ▾` control at the far right whose menu offers (`A-Z`, `Z-A`, `Recently added`, `Duration`) shared by All Music and individual playlists. Get Similar Tracks sits immediately to its left. Unsorted defaults to newest-first. The five action-row controls use equal flexible spacer columns equivalent to `justify-content: space-between`; Shuffle, Favorites first, sort, and Get Similar Tracks use the accent green, and active shuffle is shown with a bold underline. Track text is strictly truncated dynamically (`char_w = 0.85`) to ensure long titles never overflow (`w - 340` boundary). Each row has an outlined/filled favorite star about 16 px before its playlist name; clicking it toggles the track without changing selection, while track context menus retain favorite actions. The playing row's green equalizer bars are tagged `pulse` and animated in place by `_update_pulse` (`coords()`), so the 80 ms heartbeat never triggers a full `_redraw_track_list()`. The Library sidebar header returns to All Music, Unsorted navigation resets the virtual list to its first row, and Unsorted shows the subtitle `songs yet to find a home`. The reserved virtual Favorites playlist supports normal play/shuffle context and favorite-first ordering. The Get Similar Tracks action opens Discover after starting discovery for the current or selected track. Playlist actions export/import the portable manifest.
- **`downloader.py`**: **Scrollable canvas download workspace** following the native mobile download layout: a compact header/status row, source tabs, dark input/action rows, artwork-backed YouTube/SoundCloud search cards, a Direct URL form, and an expandable Downloaded songs section. The desktop-only Sync, URL Queue, Duplicates, and Allowed Dups workflows remain available as additional tabs; their existing tree selection, batch actions, and portable duplicate backups are preserved. Uses a dynamic `_search_limit` state to enable **Load More** functionality in YouTube/SoundCloud search. The **⚠ Duplicates** tab features **Playlist** and **Filename** columns. Action buttons support **bulk-exclusion** of entire duplicate sets; **double-clicking a group header** plays the first track. The **✓ Allowed Dups** tab includes **💾 Export** and **📂 Import** buttons for portable JSON backups of exclusions.
- **`recent.py`**: Recently Played view. Gradient header with "Recently Played" title. Displays `controller._recently_played` (max 100) in a virtual scrolling canvas (56px rows). Multi-selection via Shift+Click / Cmd+Click, context menu integration including selected JSON export, Edit Track Info, "Play All", and "Clear History" actions.
- **`settings.py`**: Premium Spotify-style UI with clean section headers, borderless inputs, and consistent padding. Includes step-by-step instructions for Last.fm API setup, manages storage paths, playback options, and Discovery keys, and exposes full-library JSON export/import.
- **`discover.py`**: Discovery Hub view. Displays recommended tracks from Last.fm based on current listening; features one-click "Add to Library" download integration. Logic includes **Context-Aware Shuffle**: Shuffling now respects the user's current view (All Music, Recently Played) instead of being limited to physical disk folders. The controller passes a flattened `context_list` to the engine, allowing global randomization.
- **`song_page.py`**: Premium Song Detail view. Displayed when clicking the player bar track info. Mimics Spotify's "Now Playing" aesthetic with a glassmorphism background (blurred album art overlay) and integrated controls (no back/✕ button — the nav tabs and mini-player already leave the page). Implements a unified, centered, stacked layout across all screen sizes with a large centerpiece artwork (up to 800x800) that dynamically scales based on available window dimensions. Blur generation uses a single-worker executor with generation checks so resize/path changes cannot apply stale artwork or accumulate workers. **Mobile full-player parity (2026-08-11):** the whole page is one scrollable column — artwork, metadata, favorite, and the scrollable **"Recommended for this song"** section share a page-level canvas (centered when content fits the viewport, scrollable when it overflows; artwork reserves ~230px of the first viewport for the section header + first rows). The section is seeded from the current track (artist fallback = queue playlist/folder name) and stays pinned to that seed while a preview session is active (`_discover_session_seed`), so transient preview tracks never re-seed the list; it has honest states (no track / no API key / "Finding similar songs..." / error + Retry / "No recommendations available for this song"), pages of 25 with **Load 25 more** up to 1,000 (exhausted detection = no new tracks on a page, load-more failures keep the list and surface a retryable footer error, scroll position preserved across load-more, auto-triggered by Discover at the last loaded recommendation), a **⟳ refresh** icon (disabled during sessions), a green **"⬇ Scroll for recommendations"** hint, a **Start discover mode / Discover mode: On** toggle, and mobile-parity header subtitles ("Temporary preview queue - tap a song to restart from there" / "Temporary preview - Next returns to your library" / "<n> similar songs - tap to preview"). The displayed artwork survives cache evictions: the view holds the last good `PhotoImage` for the current path and keeps showing it when the LRU-bounded `ArtCache` misses mid-display. Requests run through the controller's dedicated single-flight `_fetch_song_page_recs` machinery and rows are filtered by the shared `_filter_library_recs` ownership check; a result arriving after the API key was removed is dropped. **Flashing-free row updates (2026-08-11):** controller preview-state pushes (`_on_discover_ui_changed`) never rebuild the section — a `_rec_rows` widget registry plus `_update_rec_rows`/`_update_rec_row` patch only the changed rows in place (playing highlight, status line, trailing slots), coalesced to one pass per event-loop drain; the full re-render runs only for structural changes (new list, load-more, state machine transitions). **Per-row preview states (mobile `FullPlayerRecommendationRow` parity):** rows render the full mobile status priority — "Saving to playlist..." → "Download queued for playlist" → "Saved to library" → "Save failed: <msg>" → "Previewing now" → failure message → "Queued for temporary preview" → "Finding a playable source..." → "Downloading temporary preview..." → "Ready in temporary queue" → "<artist> - <match>% match" — with error coloring + soft-wrap for save/preview failures. **Trailing slots are fixed and consistent on every row** (left → right): a fixed 20px status slot — **animated green `Spinner`** while resolving/preparing, a **green check circle** (canvas-drawn, mobile CheckCircle parity) when ready, empty otherwise; a secondary action — green **▶ Play** pill (ready) or **⟳ Retry** pill (failed); and the save button — a compact **＋ Add to playlist** pill that is always the far-right element at a fixed width, becoming a spinner + "Saving…"/"Queued…" pill while a save is in flight and a green **✓ Saved** pill once permanent. The section header's loading state and the "Loading more…" footer use the same animated spinner instead of text glyphs. **Tapping any row starts (or restarts) a temporary preview session at that recommendation** (see Discovery Protocol). **Per-row artwork:** 48px thumbs (mobile 48dp parity) from the Last.fm image URL with a per-key Deezer fallback (once per key) and a deterministic abstract design from `rec_artwork.py` for loading/missing/error; PhotoImages are cached (bounded, keyed by normalized recommendation key) on a 2-thread `luno-rec-art` executor with generation guards.
- **`error_log.py`**: Debug log viewer. Displays `controller._error_logs` (list of strings) in a read-only `tk.Text` widget with `ModernScrollbar`. "Clear Logs" button resets the list.

---

## 🔐 State Management & Data Structures

### 1. The Normalization Key (Logic Consistency)
For syncing, names are normalized: `key = re.sub(r'[\W_]', '', f"{artist}{title}").lower()`
For **duplicate detection**, the stronger `_normalize_for_dupe()` function is used. It:
- Strips parenthetical tags: `(Official Audio)`, `[Lyrics]`, `(HD)`, `(Remastered)`, etc.
- Normalizes `feat.`/`ft.`/`featuring` → `feat`
- Strips file extensions and all non-alphanumeric characters
- Lowercases everything for case-insensitive comparison

### 2. Title Splitting
When parsing YouTube names or filenames, the app looks for `" - "` to separate Artist from Title. The Edit Track Info dialog writes entered values back into that filename shape, so existing search, playback labels, discovery lookup, duplicate detection, and export fallback all use the new names without a separate metadata database.

### 3. The Library Dict (`_library`)
Stores aggregated and folder-specific track paths. Now includes empty directories to support new playlist creation.

### 4. Global Settings (`~/.luno_config.json`)
Persisted across sessions: `music_root`, `lastfm_key`, `auto_discovery`, `volume`, `crossfade`, `sleep_timer`, `global_keys` (media key toggle), and `favorite_songs` (normalized absolute paths for locally favorited tracks).

### 5. Session State
`LunoEngine` manages the live `queue`, `queue_idx`, `_original_queue` (for shuffle restore), and playback status (`playing`, `paused`).
**Queue Architecture**: The system uses a flat `queue` list, but tracks a `user_queue_count` integer to behave like a modern "Up Next" queue. "Play Next" inserts immediately after the current `queue_idx`. "Add to Queue" appends to `queue_idx + 1 + user_queue_count`, ensuring manually queued items play before the rest of the playlist context. `user_queue_count` decrements naturally during track progression and resets on hard skips/context changes.

### 6. Exclusion Management
Allowed duplicates are identified by normalized keys (derived via `_normalize_for_dupe`) and persisted in `.luno_allowed_dups.json` within the Master Library root. **Portable Backups** (JSON) can be exported/imported manually.

### 7. Favorites
`_favorite_paths` is a controller-owned set of normalized absolute file paths. It is persisted in `~/.luno_config.json`, pruned against scanned library paths, and projected as the reserved virtual `Favorites` playlist. Favorite state is independent of physical playlist membership; deleting a file also removes its favorite state. The portable library manifest intentionally does not include favorites because they are local playback metadata, matching Android's local Room flag behavior.

### 8. Portable Library Manifests
The transfer format is `luno.library.export` with `manifest_version: 1` and scopes `selected_tracks`, `playlists`, or `library`. Track records contain title, artist, album, duration, an optional `{provider,id}` source identity, and an explicit ambiguity flag. Playlist records contain only sanitized names, descriptions, and ordered track references. Imports are additive and match existing audio before optionally downloading missing references; no local path, secret, audio blob, playback history, or automatic deletion is allowed.

---

## 🔄 Technical Protocols (Strict Adherence)

### 1. UI Architecture (Modern Standard)
- **Global 3-Column Layout**: Uses `tk.PanedWindow` (sashwidth=6, bg=BG) for independent resizing — the bg-colored sash is invisible but leaves a grabbable drag area.
    - **Sidebar**: Nav (Adaptive) | Playlists (Vertical Pane). **Minimum width: 240px** to prevent the "Luno" logo from being clipped.
    - **Body**: Persistent center column for tabs (minsize=400).
    - **Queue**: Persistent right column (Canvas-based, minsize=240, matching the left sidebar). Spans full app height.
- **Adaptive Sidebar**: Navigation items (5 total: Home, Recent, Discover, Downloader, Settings) collapse into a single "☰ Menu" button if the top pane height is < 220px. Defaults to a fixed 235px height for the Nav section at startup so the playlist panel uses the space reclaimed by the removed Artists view.
- **Scrolling Policy**: Custom `ModernScrollbar`. Side panels and all canvas views (`library.py`, `downloader.py`) use **explicit redraw hooks** or **Bound Checked Clamping** in normalized `<MouseWheel>`, legacy button-wheel, and Tk 9 `<TouchpadScroll>` handlers. Tk 9 packed touchpad deltas are converted to canvas units with a per-binding fractional accumulator for proportional, bounded scrolling. All scrollable views employ **Clamped Logic**; sidebar uses `winfo_reqheight()` to reliably measure inner content vs. canvas viewport — if content fits, scrolling is disabled at both the wheel handler (early-return) and `scrollregion` (locked to canvas height). The library track canvas uses `total_items * row_height <= canvas_height`. Both apply `yview_moveto(0)` reset when content fits.
- **Canonical Playlist Ordering / Sidebar Search**: The sidebar header is a clickable `Library` button with an `Unsorted (N)` tab; `_populate_sidebar()` establishes playlist-card order as All Music → Favorites → Alphabetical user playlists. Search filtering applies only to playlist cards, not the Unsorted tab. Selecting Library or Unsorted resets the virtual track canvas to `yview_moveto(0)`. Search filtering uses **lightweight `_reshow_all_cards()`** — unpacks all cards via `pack_forget()` then re-packs in original children order. **No widget destruction** — eliminates visual flicker. `_lib_key_released` always unpacks all first, then re-packs matches (or all if query is empty) preserving canonical order. `populate_track_list()` in `library.py` applies the active mobile-parity sort mode to track rows; Unsorted starts in newest-first mode while other scopes start A-Z.
- **Treeview Theming (macOS)**: Enforces `bordercolor`, `lightcolor`, and `darkcolor` attributes mapped to the background color in the `clam` theme configuration to thoroughly eliminate native OS 3D white borders around `Results`, `Queue`, and `Library` data tables.
- **Gradient Header Pattern**: Settings and Recent headers use a `<Configure>` callback that: (1) deletes old gradient items, (2) redraws gradient via `draw_gradient_rect`, (3) calls `tag_raise("title")` to keep text above gradient rectangles. The Downloader header intentionally uses the flat mobile-parity title/subtitle/status treatment instead. Gradient title text must use `tags="title"`.
- **Context Logic**: Playing a song from a playlist context force-refreshes the current queue to match that playlist's tracks. Launching a track from a search result **retains** the active search filter instead of reverting the view to the full unfiltered library (accomplished by directing `play_current` to trigger a silent `_redraw_track_list` instead of a full `_populate`).
- **Sash Interaction**: Draggable vertical boundary between nav and playlists marked by a visual `BG4` row handle.
- **Text Truncation Policy**: Global policy to prevent UI overlap. Library header title and track list rows use dynamic truncation placeholders (`...`) based on available canvas/frame width. Queue items use width-aware character limits (capped around 26-32 chars) for single-line consistency within the 240 px minimum queue pane.
- **Instant Input Feedback**: Slider widgets (`SlimSlider`) trigger commands during both `B1-Motion` and `ButtonRelease-1`, ensuring instantaneous audio/UI feedback.
- **No-Flicker Queue/Recent Panels (2026-08-11)**: The queue panel (`_update_queue`) preserves its scroll position across rebuilds (only explicit tab switches reset to top) and swaps artwork in place — async art arrivals update only the affected row's label via the `_q_art_targets` registry instead of rebuilding the whole panel and snapping the scroll. The Recent view's 80 ms playing-indicator pulse animates only the tagged `pulse` canvas bars (`_update_pulse`) instead of redrawing the entire canvas per tick. The Library track canvas uses the **same targeted pulse** — its 80 ms `_pulse_tick` moves the four tagged equalizer bars via `_update_pulse` (`coords()`) rather than a full `_redraw_track_list()`, so the whole-canvas delete/recreate churn never runs on a timer while a track plays.
- **Downloader Hub Strategy**: Uses `restrictfilenames: False` and `ignoreerrors: True`. The unified Downloader view is **wrapped in a scrollable canvas** for spacing and follows the mobile download hierarchy: `Downloader` header/subtitle, active-download status, compact source tabs (`Search`, `YT / CSV`, `Direct URL`, plus desktop Sync / URL Queue / Duplicates / Allowed Dups), thin conditional progress, source-specific input rows, artwork-backed search result cards, and a collapsed Downloaded songs accordion populated from `Unsorted`. The desktop-only tree workflows remain intact for playlist preview, sync, duplicate review, and allowed-duplicate backups. YT/CSV tab uses a **PREVIEW** workflow: fetch metadata → populate tree → then download via action buttons. Download status is shown **inline** (`Downloading {title} — x/y`) using yellow/green cycling. A collapsible download log at the bottom keeps a bounded recent in-memory history. **"+ New"** button next to SAVE TO combo enables in-place playlist creation.
- **Default Download Destination**: All downloads default to **Unsorted** (MUSIC_ROOT) unless the user explicitly selects a destination from the "SAVE TO" dropdown (Strict Prioritization). No auto-created subfolders.
- **URL Sanitization**: Validates that inputs start with `http` before processing to prevent terminal noise or garbage text from triggering engine errors.
- **Playlist Persistence**: Forced parsing of `list=` parameters in YouTube URLs to ensure full playlist extraction even when pointing to a specific video.
- **Discovery Protocol**: Employs **Last.fm `track.getSimilar`** API for Spotify-style recommendations with an **`artist.getTopTracks` fallback** for niche tracks. Features a **Discovery Hub** (`views/discover.py`) with a dedicated navigation tab. Supports both **Automatic** (trigger on track start) and **Manual** (trigger via the Library's "✨ Get Similar Tracks" action, which opens Discover after starting a request—using **Smart Selection** to target active library rows if nothing is playing). Implements a **query cache** (10 entries, keyed by seed + requested limit — mobile parity) to minimize API redundancy. **Duplicate Check**: All recommendations are cross-referenced against a normalized index of the user's library (`artist - title`) and filtered out to ensure only new music is shown — now via the shared `_filter_library_recs` helper (dedupes within a result list too), used by both the Discover tab and the song page's "Recommended for this song" section.
- **Discovery Request Bound**: Last.fm requests use one active daemon worker plus one latest pending request; rapid track changes replace stale pending work instead of creating an unbounded thread backlog. The song page's recommendation section has its **own** bound (`_fetch_song_page_recs` / `_start_song_page_recs_request`, thread name `luno-song-page-recs`) so the Discover tab and the song page never cancel each other's requests; results are delivered on the UI thread via `.after(0, ...)` only when the request is still the latest generation.
- **Discover Mode (song page, mobile RecommendationPreviewManager parity, 2026-08-11)**: recommendations are prepared into **`~/.luno_previews` (`PREVIEW_DIR`)** — a temporary staging area **outside the library root**, mirroring the Android `cacheDir/PREVIEW_DOWNLOAD_DIR`. Library scans and the Unsorted view never see preview audio; nothing ever lands in the library unless the user explicitly saves it. The dir is wiped at every app launch (mobile `startupCleanup`) and at session end. When the song page delivers a fresh recommendation list, the chain **passively prefetches the first five** (`DISCOVER_IMMEDIATE_COUNT`, mobile `PREFETCH_RECOMMENDATION_COUNT`) without touching playback; rows queued behind the active preparation show "Queued for temporary preview" (mobile prefetchQueuedKeys). Already-staged files are adopted, not re-downloaded (refresh-safe), and a prefetch for the same seed is never reset mid-session.
  **Preparation pipeline (mobile resolve→download parity):** one recommendation at a time, sequentially. Each row goes `waiting` → **`resolving`** ("Finding a playable source...", a `ytsearch5:` probe on the 2-thread `luno-preview-resolver` pool with a **15s hard deadline** → timeout shows "Finding a playable source timed out. Tap to retry.") → **`preparing`** ("Downloading temporary preview...", `previews.download_preview` on a worker thread — 44.1 kHz/192k MP3 **without loudnorm**, embedded thumbnail, cooperative **5-minute deadline** → "Preview download timed out. Tap to retry.", 512 MiB cap → "This source is too large for a quick preview...") → **`ready`** / **`failed`** (error + `⟳ Retry`, which bypasses the window; failures inside a session skip onward to the next recommendation). The pipeline lives in `previews.py` and **never touches the library `Downloader`'s single job slot**, so previews run while real downloads are in flight; a generation hook cancels in-flight work on toggle-off, handoff, or a new seed (cancelled downloads fail silently — no error row). Preview download failures also surface through `engine`'s `_on_playback_failed` hook: the row is marked failed, the file and queue item are removed, and the engine's automatic skip resumes the next item (mobile `recoverFromPlaybackError`).
  **Tap-to-preview sessions (mobile `startPreview`, non-Discover):** tapping ANY row starts a temporary preview session at that recommendation — the tapped row is prepared on demand (adopting a cached asset instantly), then **jumps into playback immediately** (mobile `playSequential`; a failed first item ends the session without hijacking playback — mobile `finishEmptyPreviewSession`). The session window is the tapped row + the next five (`_discover_window_start/end`), keeps a rolling window of at most five prepared ahead of playback (`_discover_consumed` + `DISCOVER_IMMEDIATE_COUNT`), and the physical queue is captured on entry. **Next/Previous route through active sessions** (`_user_next/_user_prev` on the transport buttons, XF86 keys, and media keys): non-Discover previews — Next leaves the block and plays the seed's physical successor, Previous returns to the seed, both tearing the session down (mobile skipToNext/skipToPrevious); Discover — Next walks into the queued block (a first press during preparation arms the auto-advance — mobile `advanceToDeferredRecommendation`; at a preparation gap the window re-anchors right behind the current preview; at the end it falls back to the player), Previous walks the block or restarts the window one row back. Tapping another row mid-session restarts the session at that row, dropping unplayed previews from the queue but keeping staged assets for the new window.
  **Discover mode ("Start discover mode", `_discover_set`):** captures the physical queue, **adopts every ready staged file into the queue behind the current track** (mobile asset adoption — no re-download) resuming the prefetch's progress, then keeps the rolling window of five; the block autoplays when the seed ends (`engine.next_track` walks it naturally) and never self-starts. Toggling off cancels pending previews, deletes staged audio (deferring the still-playing file), and restores the captured queue — a playing recommendation finishes before the seed's successor (mobile `endAfterCurrentRecommendation`); playback past the block rebuilds at the current position; a replaced queue is left alone. Playback leaving the block ends the session itself (mobile natural-handoff parity). **Discover infinite scroll:** when playback reaches the last loaded recommendation and the list is not exhausted, `_discover_maybe_auto_extend` triggers the next page automatically ("Load 25 more" grows the chain via `_discover_append_recs`; paging is a no-op during non-Discover sessions, and refresh is disabled during any session). **Seed stability:** while a session is active the song page keeps the section pinned to the session's seed song (`_discover_session_seed`), so transient preview tracks never re-seed or re-fetch the list; a different song's fresh list ends the old session.
  **Saving a preview** ("＋ Add to Library" on any row) opens the searchable playlist picker and follows the mobile `saveToPlaylist` chain: already saved → only a fast local copy into the chosen playlist ("Added <title> to <playlist>", no re-download); a staged preview → threaded copy with **"Saving to playlist..."** state, then permanent **"Saved to library"** (the staged copy stays until session cleanup so the queue entry keeps working); nothing staged → explicit destination + **"Download queued for playlist"** state that flips to permanent "Saved to library" or "Save failed: <msg>" when the job completes (`_discover_download_observed`). The destination is always chosen explicitly — previews are never dumped into Unsorted silently.
- **Background Media Keys (macOS)**: Uses `MPRemoteCommandCenter` in the primary app process (no subprocess listener). When PyObjC and MediaPlayer are available, `playCommand`, `pauseCommand`, `togglePlayPauseCommand`, `nextTrackCommand`, and `previousTrackCommand` are registered with retained handler blocks/tokens via `addTargetWithHandler_`. Callbacks only enqueue onto a thread-safe `queue.SimpleQueue` and return `MPRemoteCommandHandlerStatus.Success` promptly. A Tk `after` timer (`_poll_media_commands`, 50 ms) drains the queue and dispatches to `_media_play`/`_media_pause`/`_media_toggle_play`/`_user_next`/`_user_prev` (which route through active recommendation sessions before falling back to `engine.next_track`/`engine.prev_track`). Feature-detects PyObjC/MediaPlayer and degrades gracefully when unavailable. Global keys toggle (`global_keys_var`) registers/unregisters handlers via `removeTarget_`. Now Playing metadata (title, artist, duration, elapsed, playback rate) is published truthfully from engine state. Rate is 1.0 when actively playing, 0.0 when paused/stopped. Elapsed carries through pause/resume and updates after seek. Periodically refreshes at ~2 s cadence during `_poll_media_commands`. Metadata is cleared on explicit unregister/shutdown. No `defaults write com.apple.Music`, no subprocess, no `LUNO_RUN_LISTENER`, no `_listen.py`. **Persistence requirement**: The `MPRemoteCommandCenter` instance and all handler/token registrations must be stored as instance attributes (e.g. `self._mp_command_center`) — never as local variables — to prevent PyObjC from garbage-collecting the ObjC→Python bridge wrapper, which would cause a native segfault on media key press.
- **Media-Key Readiness Guard (`_media_ready()`)**: A centralized gate added to prevent startup/shutdown crashes when media keys arrive before or after the playback system is ready. `_media_ready()` returns `True` only when `_closing` is `False`, the engine exists, and `engine.mixer_ready` is `True`. Every media-key entry point — XF86 keyboard bindings, `MPRemoteCommandCenter` handlers, `_poll_media_commands`, `_media_play`, `_media_pause`, and `_media_toggle_play` — checks `_media_ready()` before proceeding. `engine.play_current()` also guards against `_closing` to prevent Tk widget access after window destruction, while `engine.next_track()`, `prev_track()`, `toggle_play()`, and `stop()` all check `mixer_ready`. Any media-key event that arrives before full initialization or after shutdown begins is silently ignored — no crash, no queued stale commands.
- **Decoupled Search Pattern**: The sidebar search strictly filters playlist cards, while the center search bar handles in-playlist track filtering. This prevents state-bleeding between navigation and content discovery.
- **Entry UI Standards**: All `tk.Entry` widgets must use `highlightthickness=0` and matching background colors to eliminate native focus borders.
- **Global Multi-Selection Logic**: Standardized across All Music and Recent views. Implements **Anchor-Based Range Selection** (Shift+Click) and **Individual Toggling** (Cmd/Ctrl+Click). The controller tracks the active tab to transparently hand off batch selection paths to single-track actions like "Add to Queue" or "Delete from Library".

### 2. Threading & Performance
- **Heavy tasks** (yt-dlp, duplicate scans, discovery, library scans) MUST be threaded; UI updates MUST use `.after(0, callback)`. Download/write jobs and metadata/artwork workers use bounded concurrency, progress/status callbacks are coalesced, and blocking the main loop with OS scans is prohibited.
- **Transfer tasks** (manifest metadata probing, validation, local matching, and file merges) run off the Tk thread; completion dialogs/status mutations return through `.after(0, callback)`.
- **Progressive UI Loading**: For result-heavy views (Duplicates, Search), rows MUST be inserted in small batches (e.g. 20 tracks / 10ms) using `self.after()` to ensure the main interface remains responsive.
- **PlaylistArt Performance Protocol**: The `PlaylistArt` widget uses three mandatory optimizations to prevent scroll lag:
    1. **Configure size-guard**: `<Configure>` only triggers a redraw if the widget's `width` or `height` actually changed (not on scroll-induced reflows).
    2. **Debounced art callbacks**: `_on_art_loaded` batches redraws via a 30ms `self.after()` coalesce, so N images arriving in a burst cause only 1 redraw.
    3. **State fingerprint**: Before drawing, a `(name, image_ids, size)` tuple is compared against `_last_state`. If identical, the draw is skipped entirely.

### 3. Audio & Playlist Patterns
- **Shuffle Protocol**: `LunoEngine` maintains `_original_queue` for sequential restoration. Toggling shuffle OFF reverts to this order anchored to the current track. Fresh shuffle cycles avoid immediately replaying the track at the previous cycle boundary when another track is available.
- **Playback Engine**: Live desktop elapsed time prefers `pygame.mixer.music.get_pos()` while the mixer reports audio busy, so the footer reflects the backend audio clock instead of an independent UI timer. `LunoEngine` keeps explicit base position/monotonic state for seek offsets, pause/resume, startup grace, and codecs where pygame returns unavailable positions. Duration probing uses `mutagen.File` (generic) so non-MP3 files (M4A, FLAC, WAV) report duration correctly. The `_poll()` heartbeat reschedules unconditionally (400 ms) after every invocation except during controller shutdown — even after auto-advance or repeat — ensuring the footer scrubber stays live and future completions fire.
- **Completion Detection**: Two independent paths, both gated by a `_completion_dispatched` latch to prevent dual-fire:
  1. **Duration-based**: when `duration > 0` and elapsed ≥ duration
  2. **Mixer-idle**: `pygame.mixer.music.get_busy()` returns `False` with elapsed > 0.5 s startup grace — works for tracks with unknown duration (0.0)
- **`get_position()`**: Canonical position method returning clamped `[0, duration]` using mixer `get_pos()` plus the current seek base while playing, paused `seek_pos` while paused, and 0.0 while stopped; falls back to monotonic base deltas when the mixer clock is unavailable.
- **Audio Standards**: Mandatory **44.1kHz** sample rate and **192k** MP3 bitrate. Enforces **EBU R 128 Loudness Normalization** (Target: -14 LUFS, LRA: 11) via FFmpeg `loudnorm` filter during all yt-dlp downloads to ensure consistent volume playback across the entire library.
- **Metadata & Album Art**: Track paths are stored absolutely in treeviews `raw_path`. The app integrates the **Deezer API** to fetch high-resolution (up to 1000x1000) album covers. If Deezer yields no results (e.g. for custom YouTube rips), the system falls back to a custom **`yt-dlp` `ytsearch1:` routine** to scrape the original YouTube thumbnail. Image insertion is handled purely in-memory via the `mutagen` library (using the `APIC` frame format) to explicitly avoid re-encoding tracks with FFmpeg. Thumbnails fetched from YouTube in `.webp` format are natively converted on-the-fly to strict `image/jpeg` using `Pillow` to guarantee universal MP3 ID3 compatibility. Automatically applies to all yt-dlp downloads via `EmbedThumbnail` postprocessor.
- **ArtCache (Lazy Loading)**: A threaded `ArtCache` sits on the main Controller. It uses a bounded in-flight `ThreadPoolExecutor` and `Pillow` (`PIL.Image`) to dynamically uncompress and resize thumbnails only when requested by the UI. Stored pixels, source bytes/pixels, and stale path entries are bounded. `PlaylistArt` asynchronously utilizes this cache to build Spotify-style 4-quadrant grid thumbnails for playlists with 4+ tracks, falling back to full single images (1-3 tracks) or gradient fallbacks (0 tracks / no embedded art).

### 4. Cleanup
- Traps `SIGINT` and `WM_DELETE_WINDOW` to ensure active downloads are cancelled, artwork workers are shut down, `_save_config()` and `pygame.mixer.quit()` are called before the process exits.

---

## ⚠️ Known Code Smells

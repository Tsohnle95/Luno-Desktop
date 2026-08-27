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
├── src-tauri/resources/bin/   # Reviewed release binaries (ffmpeg, ffprobe, yt-dlp) plus the luno-download-helper script, staged via release/provision.sh and hash-pinned by release/desktop-binaries.json
├── src-tauri/src/gate3_tests.rs # Deterministic playback, queue, clock, recovery, and synthetic-WAV contract tests
├── src-tauri/src/media_keys.rs  # macOS MPRemoteCommandCenter + MPNowPlayingInfoCenter claim so hardware play/pause/next/previous control Luno (not Apple Music); honors the global_keys setting live
├── src-tauri/src/library/artwork.rs # Lazy embedded-artwork extraction (ID3 APIC, ISO-BMFF covr, FLAC PICTURE) served as data: URLs through a bounded in-memory cache (`ARTWORK_CACHE_BYTES`, 64 MiB)
├── src/state/track_artwork.ts   # React hook that lazily fetches embedded artwork only for the currently visible rows (BOUNDED 200-entry LRU data-URL cache + in-flight dedupe; an unbounded Map here grew past WebKit memory limits on long scroll sessions and killed the web content process)
├── src/state/play_stats.ts      # Client-side listening stats for Home: localStorage play counters, download-completion journal, deterministic Made-for-you shuffle, popularity ranking (all tolerant of missing/corrupt storage)
├── src/components/icons.tsx     # Inline SVG icon set (stroke-based, aria-hidden) used by the top bar, transport, and action clusters
├── src/components/VideoStage.tsx # Presentation-only video surface: audio-authoritative play/pause+drift sync, end-of-media hold, determinate green download-progress ring
├── src/views/HomeView.tsx       # Home dashboard (greeting + six mobile-parity card sections: Made for you / Favorited songs / Recently played / Most popular songs / Most popular playlists / Recently downloaded); default landing view of the library route
├── test_library_transfer.py   # Manifest validation, export, matching, and additive merge tests
├── test_media_keys.py        # Deterministic mocked unit tests for media key logic
├── test_downloader_sync.py   # Deterministic mocked tests for playlist synchronization
├── src/app/gate3Smoke.tsx    # Development-only real-bridge playback/queue smoke probe
├── src/app/gate4Smoke.tsx    # Development-only real-bridge downloader/settings/transfer smoke probe
├── src/app/gate5Smoke.tsx    # Development-only real-bridge discovery/preview/artwork/video staging smoke probe
├── src/test/gate3_smoke.test.ts # Deterministic command/event lifecycle coverage for the Gate 3 probe
├── src-tauri/src/gate5_tests.rs # Deterministic target-policy fixtures for discovery, previews, artwork, video, and shutdown
├── src-tauri/src/video/stream.rs # Range-aware `luno-video://` streaming protocol for staged MP4s (≤2 MB 206 windows; store-containment enforced)
├── src-tauri/src/video/runtime.rs # Live video pipeline: yt-dlp YouTube search (relevance-filtered), ≤720p MP4 download into the staging store, cancellation, and VideoState JSON
├── src-tauri/src/runtime/gate5_smoke.rs # Opt-in native Gate 5 command seam using caller-provided synthetic fixture roots
├── test_tkvideo.py            # Ad-hoc Tk/video experiment (not part of the app)
├── pytest.ini                 # Default pytest collection excludes dependency-bound ad-hoc scripts
├── test_dl.py
├── test_focus.py
├── test_label_ph.py
├── scripts/                   # Release supply-chain tooling: provision-desktop-binaries.sh, verify-desktop-bundle.sh
├── release/                   # Pinned binary manifest (desktop-binaries.json), THIRD_PARTY_NOTICES.md, hash-verifying provisioner (provision.sh)
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

### Rust Target Runtime Boundary
The Tauri target exposes the versioned command envelope through `src-tauri/src/main.rs`.
`AppState` owns configured scan roots, session-only recent track IDs (capped at 100),
Shuffle is TRUE-random (2026-08-25): `shuffle_current_first` Fisher-Yateses
the remaining tracks from OS entropy (time ^ pid ^ call counter) instead of
the old fixed rotation that replayed the identical order every launch, and
`next_track`/`previous_track` walk the EXISTING (possibly shuffled) order via
`start_current_track` — they no longer re-enter `play_with_context`, which
used to re-shuffle on every skip, pin Previous at index 0 ("can't go back a
couple songs"), and clobber the original order so toggle-off restored the
shuffled sequence. Toggle-off still restores the true original playlist.
normalized favorite paths, and authoritative `QueueState`, `PlaybackClock`, and
`FailureRecovery` playback state. Hardware MEDIA KEYS route to Luno through
`media_keys.rs` (MPRemoteCommandCenter targets + a published now-playing
dictionary claim the system routing away from Apple Music); every key press
funnels into `AppState::handle_media_key`, which is status-aware (toggle
pauses/resumes/starts) and honors the persisted `global_keys` setting live.
Config loading and marker inspection are read-only
IPC journeys over caller-provided fixture roots; malformed config is reported as an
invalid request without rewriting the source. Library scans use the task registry
with one active generation and one coalesced follow-up, injectable scan work for
deterministic tests, stale-result guards, and cancellation-aware publication,
including checks at the real `OsFileSystem` scan boundary. A scan that cannot read
EVERY root keeps the previous non-empty library and publishes an error event instead
of an empty snapshot (transient unmounts no longer blank the library). The version-1 command
bridge also exposes validated local audio promotion into a configured library
root through the existing atomic promotion contract. Gate 4 commands for
downloader search/direct URL/CSV/URL-queue validation, safe-root selection and
production download dispatch, typed settings persistence, transfer export, and transfer preview/apply
dispatch through the same `AppState` event boundary;
ALL Tauri command handlers run their dispatch inside
`spawn_blocking` — sync commands would execute on the MAIN thread, and
blocking work there (Last.fm discovery uses a 10 s-timeout `reqwest::blocking`
client; downloader search shells out to yt-dlp) froze the entire window for
tens of seconds per track change. The frontend additionally fetches discovery
data ONLY while its surface is visible: Song Page recommendations require
route `song_page`, and the Discover feed requires route/embed `discover`.
download job snapshots are projected through the React bridge without exposing
local paths as track identity. Track IDs derive from filename metadata
(`Artist - Title.ext` — the FIRST `" - "` splits artist from title; no
separator means artist is empty and rows show "Unknown artist"). When two
files would derive the SAME id (same stem in different folders, or repeated
downloads of one source), the first occurrence keeps the canonical id and
later collisions append `#<8-hex path hash>` so distinct files stay distinct,
playable tracks (this previously collapsed them into ONE song that appeared
on every row/page). Downloads are named `Artist - Title.<ext>` whenever their
metadata is known for exactly this reason. The Tauri target serves album art
lazily:
library rows carry no `artwork_url` (the scan intentionally avoids reading
metadata for every file); the visible rows call the `track_artwork` command,
which resolves the track id to its scanned path, extracts the first embedded
picture through Symphonia, and returns a `data:` URL backed by a 64 MiB-bound
in-memory cache (`src-tauri/src/library/artwork.rs`). The desktop window is a
Spotify-style frame: a black shell with rounded floating panels and a compact
(56 px) full-width top bar. Sidebar and queue panel widths are USER-RESIZABLE and OPEN AT THEIR MINIMUM WIDTHS (2026-08-25 user-set; sidebar 240, queue 240):
the shell's grid columns are pixel-driven state (`sidebarWidth` 240–440,
`queueWidth` 240–480) and two absolutely-positioned `.luno-resize-handle`
separators (8px INVISIBLE hit strips at the pane boundaries — the col-resize
cursor is the only affordance) manage panels ENTIRELY through edge-drags
(2026-08-24 night): dragging a divider into the last 150 px before its window
edge SNAPS the panel closed; pulling an edge back out past that line snaps it
open at its minimum width and continues live-resizing. The handles stay
mounted while a panel is collapsed (hugging the window edge) so closed panels
can be dragged back open — there are NO dedicated close/open buttons anymore
(topbar show-sidebar button, sidebar hide button, queue ✕ header button, and
the player-bar queue toggle were all removed). The center column is inline
`minmax(0,1fr)` so it ALWAYS absorbs every leftover pixel of horizontal width
as side panels shrink (an earlier `minmax(var(--luno-content-min),1fr)`
allowed the song-page media slot to sit as a small centered block). The queue
panel's Shuffle/Repeat/Clear pill row was REMOVED (transport shuffle/repeat
live in the player bar; row management stays in the hover overlay); the Queue
header keeps only the Queue / Recently played tabs. The Tauri window uses `titleBarStyle: "Overlay"`
with `hiddenTitle` (tauri.conf.json), so the webview is full-height, the
traffic lights float over the bar's left side (`.luno-history` carries a
72 px left margin to clear them), and the header strip carries
`data-tauri-drag-region` for window dragging (the attribute must ALSO sit on
the structural children `.luno-history`, `.luno-top-nav`, and
`.luno-top-center`, because Tauri's drag script matches the exact mousedown
target; the capability file MUST include `core:window:allow-start-dragging`
because `core:window:default` does not grant it, and Shell ADDITIONALLY calls
`getCurrentWindow().startDragging()` directly from the header's pointerdown
handler for any non-interactive target — belt-and-braces so dragging works
even when the injected script's target matching fails). Bar order: back/forward history
→ `Primary navigation` holding ONLY a centered [Home pill + "What do you want
to play?" search] group → green profile avatar absolutely pinned into the FAR
top-right corner with equal ~1rem offsets (top 10px, right 10px). The search drives the library filter
through `normalizeFilterText` (NFKD accent-stripping, curly quotes mapped to
ASCII apostrophes, en/em dashes to hyphens, whitespace collapsed — applied to
BOTH the query and row text), so macOS smart-punctuation input like `l’in`
still matches `L'indécis`; the in-view filter box mounts ONLY via its own hero
toggle button and never auto-appears because the top search has text, and
BOTH the nav group and search are not rendered while route is `settings`.
The avatar opens
the dedicated two-pane settings page, which takes over the ENTIRE window
(focus mode hides the sidebar and queue panels for route `settings` and sets
the grid template to `minmax(0,0fr) minmax(content-min,1fr) minmax(0,0fr)` —
BOTH side tracks must collapse to zero because a hidden queue otherwise still
reserves its 240px minimum as dead black space — with panel gaps zeroed and
the main radius removed for one seamless surface; every shell pane owns
a FIXED grid slot — sidebar col 1, main col 2, queue col 3 — so hiding a pane
collapses its track to zero width instead of reshuffling `main` into an empty
column, which previously black-screened the page). Its
left rail is the app's sole navigation surface outside Home: one flat,
icon-free tab list (text-only buttons are the standard rail treatment)
(Home / Recent Songs / Discover / Downloader / Error Log, then the settings
categories Library / Discovery / Playback / Transfer / Troubleshooting; the
"Go to" section header and Song Page entry were removed by product decision).
Rail tabs do NOT navigate away — except **Home**, which always exits to the
home dashboard: Shell keeps a `settingsScreen` state and passes
the matching view element (Recent, Discover, Downloader, or
Error Log) through the `screen` prop, so the screen renders inside the
settings surface on the right of the rail (picking a settings category closes
the embedded screen via `onScreenClosed`). The profile avatar is a TOGGLE:
clicking it opens settings, clicking again returns to the last non-settings
route (`lastNonSettingsRoute` ref).
All settings panels stay mounted with `hidden` on inactive ones while only
the active category renders on the right; the content pane stretches to the
FULL remaining width (no max-width cap) and embedded rail screens fill it via
`.luno-settings-screen`; Error Log is also reachable from
Troubleshooting's "Open logs". The "Your Library"
sidebar is collapsible, with pinned All Music/Favorites/Unsorted rows plus
Playlists/Albums chips whose entries are artwork-backed cards with song
counts, filterable client-side. Startup migration
status is intentionally silent — `migration_status` still feeds the shell's
`data-migration-state` attribute, but completed/skipped/failed text is never
rendered. All scrollers set `overscroll-behavior: none`, so rubber-banding can
never reveal blank negative space. Album groups are derived client-side from
track album metadata and open as `album:<name>` library scopes. Route
`library` has two faces: the HomeView dashboard is the DEFAULT landing view
(time-based greeting plus six mobile-parity card rows in order: **Made for
you** (deterministic shuffle of catalogue ids keyed to membership, 50),
**Favorited songs**, **Recently played**, **Most popular songs**,
**Most popular playlists** ({n} plays subtitle, opens the playlist scope),
and **Recently downloaded**; each section header may carry a "Show all" link
into library/recent/downloader), and
opening any sidebar scope or typing a search swaps in LibraryView; both Home
affordances (nav Home pill) return to the dashboard. Because the backend keeps
no play counters and completed jobs die at shutdown, Home popularity is
CLIENT-SIDE (`src/state/play_stats.ts`): localStorage play counters are
bumped when `recentHistory` grows a NEW head (mount priming prevents startup
inflation), and succeeded download jobs are journalled per track
(newest-first, deduped) so "Most popular" and "Recently downloaded" survive
restarts without touching the versioned IPC contract. LibraryView scrolls as
ONE page: the hero (scope gradient keyed by name hash, cover mosaic of up to
four artworks, eyebrow, title, meta) scrolls away with the content, and ONLY
the column head (`# / Title / Album / Playlist / duration`) stays pinned via
sticky positioning as a direct child of the page scroller. The virtualization
window subtracts the measured hero height + 36 px head ("rows start" offset)
from the live scroll position (local scroll = scrollTop − rowsStart). The
list remains a virtualized 56 px-row list whose
window is computed by the exported pure `virtualWindow(scrollTop, anchorHeight,
count)` (default overscan 20) from the LIVE DOM scroll position — the scroll
handler stores `scrollRef.scrollTop` and the rAF callback reads that freshest
value at frame time (one state update per frame, never a stale event-time
capture). The start index is clamped to
`min(floor(max(0, scrollTop − anchorHeight) / 56), count − 1)` so the window
ALWAYS contains at least one row when any match exists — a stale scroll offset
past a shrunken (search-filtered) list previously rendered an empty window over
existing rows, which looked like songs disappearing while playback still
worked. The zero-match empty state carries a one-click **Clear search**
action (2026-08-25) instead of a dead end. The DOM scroller and state both reset to 0 on scope/query/sort changes;
playlist/scope navigation lives in the sidebar's `Playlists` slot rather than
inside the track list, so music categories can never crowd out the songs; the
"Video preview unavailable; artwork fallback is active." runtime status is
shown only on the Song page, where video preview is relevant. The right queue
panel has Spotify-style chrome: Queue / Recently played tabs plus a close
button (reopened from the player bar's queue toggle), a Shuffle/Repeat/Clear
tool row, a Now playing card, and a "Next from: <playlist>" section derived
from the playlist containing the current track; row management actions are an
absolutely-positioned overlay revealed on hover/focus (they reserve NO layout
space, which keeps titles full-width in the narrow panel) while preserving
backend queue indices. The aside itself never scrolls — `.luno-queue-body` is
the single scroll container (no nested `luno-scroll`) — and queue/recent rows
resolve embedded artwork lazily through the same `track_artwork` hook as the
library list. `queueTracks`/`recentTracks` in the shell are memoized behind
joined-id string keys so playback-heartbeat snapshots (position-only changes)
cannot re-render the panel and stall scrolling. The footer mini player is a two-row bar: album art +
title/artist on the left (click opens the Song page), centered transport
controls including shuffle/repeat toggles mirroring the playback snapshot,
favorite + mute + volume + a queue-panel toggle on the right, and a wide
centered scrubber (up to 72% of the app width) along the bottom; the debug
"IPC contract vN" label is not part of the footer. Both sliders are ref-driven
and pointer-owned: the range inputs
carry no controlled `value` (only `defaultValue`), the browser owns the thumb
during the gesture (the app never calls `setPointerCapture` on a native range
input — WKWebView keeps that capture indefinitely, gluing the thumb to the
pointer with no `pointerup`), and the gesture ends on window-level
`pointerup`/`pointercancel` listeners that are active only while a drag is in
progress, so a release anywhere still commits. The volume slider tracks the
thumb live while dragging (frame-coalesced `set_volume` commands, latest
value wins) and commits once more on release, persisting `settings.volume` —
the audio level follows the drag instead of jumping at release; the gesture is
never ended by button-less `pointermove`, because WKWebView can report
`buttons: 0` mid-drag and that would commit a stale value and swallow the real
release. After a seek the thumb rests at the
committed position until the backend `position` is within 200 ms of it on
either side (so backward seeks do not snap back to the stale pre-seek
position), falling back to the live position after ~1.5 s; the volume slider
mirrors the persisted `settings.volume` when idle. The master volume is
tracked in `AppState` (seeded from the loaded config) and carried across
`load_streaming` output-state replacement on every track change, so a slider
setting survives next/previous/auto-advance and a restart instead of snapping
back to full volume.
Production downloads run through the bundled
`luno-download-helper` (resolved via `bundle_helper_path`, resource-dir path
with a `CARGO_MANIFEST_DIR` fallback for development) with yt-dlp
`bestaudio/best` extraction converted by the reviewed LGPL FFmpeg into M4A
(AAC-LC), staged with an atomic rename and promoted on the MP4 `ftyp`
signature; see `docs/decisions/download-format-m4a.md`. Destinations default to
`Unsorted` under the configured library root, and destination directories are
created inside the root when missing.
Startup migration validates the legacy discovery inputs through
`validate_existing_path`, which canonicalizes standard macOS system redirects
(for example `/tmp` -> `/private/tmp`) instead of rejecting them, preserves the
full shape of not-yet-created targets such as
`HOME/Library/Application Support/com.ty.luno`, and keeps a symlinked leaf
unresolved for explicit leaf checks. Migration input or execution failures
never abort app setup: they are recorded as a bounded `MigrationStatus::Failed`
reason and the app opens with empty state; empty isolated HOME surfaces as
`MigrationStatus::Skipped`. Bounded entry inspections (marker files, preview
inventory, retry inventory) use a default budget of 1,048,576 directory entries
(`DEFAULT_RETRY_ENTRIES`, overridable via `LUNO_MIGRATION_ENTRY_BUDGET`); the
former 4,096 bound was too small for ordinary libraries — a 4,000-song tree
with folder art trips it, turning a legitimate first-run migration into a
startup failure banner.

---

## 🔀 Data Flow

### Library Scan → UI
1. `_refresh_library()` starts at most one generation-tagged worker calling standalone `scan_library(roots)` (imported from `engine.py`); repeated refresh requests coalesce into one pending scan, which the worker consumes as exactly one fresh follow-up generation after its terminal result, and stale results are ignored
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
1. User clicks track → the React bridge sends `play_with_context(track_id, context)`; the Rust macOS migration surface resolves the track ID to its scanned local path (a MISS now heals through `heal_track_id`: when the exact id is gone but another id sharing its `#`-free base exists — the duplicate-collision flip case — that file plays instead of rejecting with "track is not available locally"; ambiguous multi-sibling cases still fail); since 2026-08-25 the WHOLE request heals under ONE lock — every `context` entry is resolved/healed onto the CURRENT map, duplicates collapse, and the effective (healed) track id flows into queue, ack, and load task, so a click that raced a rescan can never store dangling ids (Now Playing resolving nothing, auto-advance skipping, repeat clicks failing — the recurring "clicked a searched song and it disappeared / no matches" family; covered by `play_with_context_heals_a_stale_context_across_a_suffix_flip`), replaces `AppState`'s `QueueState`, and returns an acknowledgment snapshot immediately. **Atomic library swap:** `replace_library` builds the whole id→path map locally and swaps it in with ONE lock write — clearing first left a rebuild-sized window where clicks on still-visible rows resolved against an EMPTY map and failed. The backend **streams**: it probes the file for duration, starts a producer thread that decodes incrementally into a ~2-second buffer ahead of the cpal output callback, and publishes the `Playing` snapshot once the output stream starts — audio begins in under a second instead of after a full-file decode (a newer selection cancels the previous `playback-load` task before it starts). Symphonia is compiled WITH the `aac` feature (plus mp3/isomp4/flac/wav/pcm): without it every downloaded `.m4a` failed with "unsupported codec", stalled at 0ns, and auto-skipped — songs appeared unplayable. **Seeking** uses Symphonia coarse seek (with an early copy of the seek target out of the state mutex to avoid a self-deadlock) and lands in hundreds of milliseconds; playback resumes from the target. **Volume** is a `set_volume` command that scales output samples in `0..=1`. Play/pause/resume: `pause` toggles, and the dedicated `resume` command continues a PAUSED track in place (`play_with_context` always reloads from zero — sending it as "resume" made every unpause restart the song). A cpal stream error (device switched/unplugged) flags `OutputState.failed`; the next `play()` tears the dead stream down and rebuilds it on the current default device — without this, play() saw live commands and silently played into a dead stream (frozen position, no sound). TWO further guards harden the loop: the heartbeat task tolerates transient tick failures (up to 10 consecutive before dying — one error used to kill snapshots forever, freezing scrubber/status/UI until restart), and a STALL WATCHDOG (`playback_stall`) restores Tk-era mixer-idle detection: if status is Playing but the backend position freezes for ~4 s the backend is nudged to rebuild its stream, and if it is still frozen after ~8 s the watchdog advances to the next track (a silent device-switch death previously left tracks stuck at 0:16–0:33 with play/pause clicks doing nothing, because status said Playing so `resume` no-op'd). Worker progress prints `[luno-play]` traces (request/rejection/load failure/playing) alongside `[luno-video]`. Backend `Event::Error`s are surfaced in the Shell runtime-status strip (`data-state="backend-error"`), so decode/recovery failures are no longer invisible. Rust non-macOS audio remains explicitly unavailable; Rust VIDEO commands are LIVE (see Song Video Preview).
2. Sets `queue` to `context_list` (or the playlist containing `path`)
3. Sets `queue_idx` to track's position
4. The Rust target's `AppState` replaces the authoritative queue context, loads the selected track through `CpalSymphoniaBackend`, starts the cancellation-owned heartbeat, records non-preview history, and publishes snapshots including queue/index/status/position/duration/shuffle/repeat.
5. `_poll()` loop (400ms heartbeat) — desktop pygame behavior remains unconditional except when controller is closing; the Rust target uses the task registry and cancellation/closing guards:
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
4. Discovery download status, logs, and completion callbacks are marshalled through `controller.after(0, ...)`; the sync-track path creates the destination folder and treats missing or signature-invalid audio output as a failure instead of reporting a false success. A completed discovery download requests a fresh scan and resets the library track list to its first row.
5. Progress hook → coalesced `controller.after(0, ...)` → UI update
6. On complete → `_refresh_library()` → coalesced library scan
7. Playlist sync compares candidates against the whole local music library root, not only the destination playlist folder, so Sync All avoids duplicating tracks that already exist in another playlist. It tracks successful/matched YouTube IDs in `.downloaded_vids` and permanent failures in `.sync_failed_vids` inside the playlist folder.

### Artwork Fetching
1. `PlaylistArt` or `SongPageView` requests art → `ArtCache.get(path, size, callback)`; downloader search cards use a separate bounded 64-entry thumbnail cache and bounded in-flight executor (4 concurrent)
2. `ArtCache` checks a bounded in-memory LRU cache (maximum 48 entries and 4 megapixels, color cache 200 entries) → if miss, queues the key and drains it through `ThreadPoolExecutor` (2 workers) under the in-flight limit (8); source images larger than 4 MB / 4 Mpx are rejected
3. Worker reads `mutagen.mp3.MP3` → rejects oversized APIC payloads/images → extracts, crops, and resizes via `PIL.Image` with prompt intermediate close
4. Returns `ImageTk.PhotoImage` via a bounded `controller.after(0, ...)` callback; stale path cache entries are pruned after each library update

### Song Video Preview (React/Tauri — live)
1. Opening the Song Page fires `bridge.videoStage(trackId)` once the backend is available. `Command::VideoSource`/`VideoStage` fast-path an already-staged track; otherwise they publish a `loading` VideoState event and spawn a task-registry worker (`video-<generation>`). A REJECTED spawn publishes Unavailable before returning the Busy error — a published Loading with no live worker left the UI on "Loading video preview..." forever. Every helper subprocess (yt-dlp search, ffprobe, ffmpeg transcode) runs under `video::runtime::run_with_timeout` (30s/20s/180s) and downloads carry an overall 300s budget: wedged network/subprocesses settle the stage as Unavailable instead of hanging.
   **Song-page VIDEO MODE (2026-08-23, persisted `video_mode` in `.luno_config.json`):** the ⋯ menu is retired — a hover toggle on the media stage (music-mode mini pill, visible over artwork AND video) issues `SetVideoMode`; the backend also persists it immediately via the remembered settings home. When ON, the BACKEND drives everything — every track start (manual selection, next/previous, auto-advance) kicks `video-prefetch`, a single-flight worker that stages the CURRENT track through the normal publish pipeline and then sequentially pre-downloads the NEXT THREE queue tracks' best videos into the staging store's Next slot via `prefetch_track` (own cancel flag; never touches Current or the shared stager generation). `stage_track` promotes a prefetched candidate with `promote_prefetched` (Next→Current rename) before searching, so each handoff is instant and network-free. Toggling OFF cancels the in-flight prefetch pass only. **Concurrency guardrails (2026-08-23 evening):** `save_settings` now MERGES non-null fields into the existing config file instead of replacing it (a partial writer or second instance previously clobbered `video_mode` mid-download), and `main.rs` claims localhost:48713 at startup — a second Luno exits immediately (`[luno] another Luno instance is already running`); isolated QA fixtures set `LUNO_INSTANCE_PORT` to opt out. **SUPERSEDE-FIRST KICK (2026-08-24 night):** `kick_video_pipeline` used to early-return while a previous pass was still alive (before cancelling it), so switching songs mid-staging left the NEW song's video waiting on the entire old chain; it now cancels + installs a fresh flag FIRST and always spawns. The pass gate resets only when the exiting worker's flag is still the installed one (`video_mode_changed_since`), and the Next-slot acquisition polls `video_stager.try_lock()` with 100 ms cancellation checkpoints instead of a blocking lock that parked the worker (deaf to cancellation) for whole downloads. **PREFETCH INDEX RECORDING (2026-08-24 night):** `prefetch_track` now records `index.json` on its FRESH-DOWNLOAD success path too (only the already-cached branch used to) — without that entry `stage_track` paid the whole multi-search YouTube lookup again before discovering the very file prefetch had just parked, which was exactly the "skipped song still spun ~30s despite being cued" bug; with the entry, the index fast-path auto-promotes `next-<id>.mp4 → current-<id>.mp4` so the skip is instant and network-free.
2. The worker (`src-tauri/src/video/runtime.rs`) searches YouTube via the discovered yt-dlp binary (`ytsearch8:` with `--playlist-items 1:8`, three query forms via `video::source::lookup`), downloads through the shared `download_ladder` with `--merge-output-format mp4` and one of the codec-preference ladders in `video::runtime` (`FORMAT_LADDER_QUICK` ≤**360p** since 2026-08-26, `--concurrent-fragments 8`, for interactive first-frame pulls; `FORMAT_LADDER_HIGH` ≤720p — since 2026-08-25 — for prefetch and quality upgrades; the former ≤1080p `FORMAT_LADDER_FULL` produced 118–197 MB staged files and was removed) using yt-dlp's STOCK client rotation into the PERSISTENT staging store at `<app_data>/videos` (OUTSIDE the library root; since 2026-08-23 the startup sweep keeps current-*/next-* MP4s and only removes temp/partial files, so the index fast-path can adopt them instantly across launches) — do NOT add `--extractor-args` client overrides: forcing tv/web changes the served format ladder and can match NOTHING (this regressed every download once). **PARALLEL SEARCH FORMS (2026-08-24 night):** `find_sources` runs all query-form yt-dlp searches CONCURRENTLY and feeds buffered results to `lookup` in its fixed form order — sequential forms paid 1–3 back-to-back searches (~5–15 s each) before a download could start; wall time now collapses to roughly one search. **TWO-TIER STAGING (2026-08-24 night):** interactive current-track downloads use the QUICK ≤**360p** H.264 ladder (`FORMAT_LADDER_QUICK`, shared `download_ladder`) so the first frame lands in seconds-to-tens-of-seconds (480p of long music films meant hundreds of MB and multi-minute first frames), then the worker's `luno-video-upgrade` thread replaces the file IN PLACE with the high ≤720p ladder (`FORMAT_LADDER_HIGH`) via `upgrade_staged_video` (hidden `.upgrading-*` temp inside the store → audio verify → atomic rename) and republishes VideoState with `revision: 2`; the frontend appends a `#luno-r2` fragment to the asset src so the element hot-swaps onto the upgraded bytes and the sync planner re-seeks it to the audio position. Prefetch and cache/prefetch adoptions always run HIGH quality (`needs_upgrade: false`). **TIER-AWARE CACHE INDEX (2026-08-25 audit):** `index.json` entries now carry a tier (`{"id","tier":"quick"}` objects; legacy bare-string entries read as "high"). Interactive quick-pass successes record `quick`; a successful HD upgrade flips every key mapping to that video id back to `high` (`set_tier_for_video_id`). Adoption paths consult the tier so an INTERRUPTED upgrade (restart / superseded by a skip) re-runs on the next play instead of serving 480p as final forever  **PREFETCH DEDUPE (2026-08-26 audit):** `prefetch_track` treats an existing CURRENT-slot copy of the same video id (`current-<id>.mp4`) as a cache hit — recording the key at that file's existing tier — instead of re-downloading the identical file into Next (full-size duplicates observed in the wild).— previously such files were adopted with `needs_upgrade:false` and never improved. UP TO 3 CANDIDATES are tried in relevance order — YouTube refuses some licensed videos to every downloader client ("not available on this app"), so first hits are not always downloadable. Candidate admission is TWO-TIER (`video::source::lookup`): strict matches (every title word + artist evidence) come first in query-form order, then relaxed artist-anchored matches (≥ half the title words, minimum 2 — word-soup bait stays out) as best-overlap-first fallback material, so ONE refused upload can never dead-end staging (the live "search -> 1 candidates" failure). Failures carry the yt-dlp ERROR line as their reason. Every outcome publishes a `video_state` event (`ready`+staged path, or `unavailable`); a newer request cancels the previous one through an AtomicBool and generation guard. The PINNED yt-dlp must stay CURRENT: the 2025.08.11 bundle was refused by YouTube for nearly all content with exactly this error while a 2026.x binary resolved it — refresh via `release-input/bin/` + hash-pinned `release/desktop-binaries.json` + `release/provision.sh`, never by swapping the resource ad hoc.
   **Persistent video cache INDEX (2026-08-23):** `<app_data>/videos/index.json` maps a normalized artist+title key to its downloaded YouTube id (written on every successful stage/prefetch/adoption; BOTH store cleanups — `VideoStage::at` startup sweep and `cleanup(preserve)` — preserve it). `stage_track` consults the index BEFORE any network work: an indexed track whose MP4 still exists adopts instantly with ZERO search/download latency (and promotes `next-<id>.mp4` → `current-<id>.mp4` itself), so re-opening a song page is instant even though the filename scheme requires the YouTube id that only a search used to provide. Without the index, EVERY restage paid multiple sequential yt-dlp searches (~10–30 s each) before reaching the cache check. `prefetch_track` also records ids and skips its search for already-cached tracks. Additionally, same-track staging kicks are COALESCED via `video_staging_inflight`: the song-page effect and the video-mode auto-stager both fire within milliseconds of a track start, and their twin workers raced through identical searches while overwriting each other's publications. `tauri.conf.json` now lists `$APPDATA/videos` under `assetProtocol.scope` for documentation; the RUNTIME `scope.allow_directory(video_root)` in `main.rs` setup remains the operative grant (config alone was never the blocker).
3. The webview plays the staged file through the app's RANGE-AWARE `luno-video://localhost` STREAMING PROTOCOL (2026-08-25): main.rs registers an asynchronous custom scheme whose handler runs `video::stream::response_for` on the async runtime; EVERY media response is a ≤2 MB 206 byte-window with `Accept-Ranges: bytes`, and requests are contained to the video store by canonicalize+prefix checks (traversal/symlink/extension refused, unit-tested). This REPLACES the stock asset protocol for `<video>` srcs: it buffered whole staged files when the first request carried no Range header and wry handed WebKit ONE NSData blob per load/seek-rebuffer — a major aggravator of the 2026-08-25 song-switch freeze. The CSP carries `media-src 'self' asset: http://asset.localhost luno-video: blob:`; Shell builds srcs via `convertFileSrc(staged_path, "luno-video")`; `<video>` time follows the authoritative AUDIO position through the tested `planVideoSync` planner (see TRANSPORT FOLLOW / SYNC PLANNER below) — programmatic seeks are recorded so their `seeked` echoes are NOT reported back as video_seek commands (that echo loop re-rendered the whole shell several times per second). Play/pause/seek/fullscreen commands mutate + republish the shared VideoState. Format policy lives entirely in the `FORMAT_LADDER_*` consts of `video::runtime` (H.264-first chains; never force extractor clients) — YouTube frequently serves ONLY segmented HLS here (no progressive MP4s), container filters match nothing, and WKWebView plays H.264/AAC natively (never VP9/AV1); ffmpeg merges. AUDIO MUST BE AAC: plain `ba` yields Opus, which WKWebView cannot decode inside MP4 (black stage, no error surfaced); `ensure_playable_audio` ffprobes every staged file and transcodes non-AAC audio to AAC 192k (video copied, faststart) through the bundled ffmpeg before adoption — cached pre-policy files are verified too. A `<video>` load error is wired through Shell (`onError`) to flip the stage to its artwork-fallback state instead of an unexplained black box. Completed downloads are reused when the same video id is requested again, and every staging run begins its OWN store generation — `adopt` rejects foreign counters (StaleGeneration), which previously discarded every finished download.
   **TRANSPORT FOLLOW (2026-08-24):** pausing/resuming the SONG always pauses/resumes the picture — enforced on BOTH layers. Backend: `pause_track`/`resume_track` flip the shared VideoState Playing↔Paused IN THE SAME BREATH as the audio toggle (playback_status is written before the video follow block; the heartbeat previously sampled status+video out of order and resurrected a freshly-paused clip), and `heartbeat_tick` carries follow arms ((Playing, Ready|Paused) → Playing, (_, Playing) → Paused). Frontend: Shell derives the `<video>`'s `playing` prop from the AUDIO snapshot (`projection.playback?.status === "playing"`), NOT from `video.status` — deriving it from the second event stream was exactly what let a paused song's picture keep playing; the audio transport is the single source of truth and the picture stops in the same render even if a video_state event is delayed or lost.
   **SYNC PLANNER (2026-08-24 evening):** all transport decisions live in the exported pure `planVideoSync` (`src/components/VideoStage.tsx`; unit-tested matrix in `video_stage.test.tsx`). Key insight: between 400 ms heartbeat publications the free-running `<video>` is routinely ~300–500 ms ahead of the last published AUDIO position WITHOUT being desynced, so sub-second chasing produced visible glitches — pause rewound the frozen frame (old 0.2 s paused tolerance), resume re-seeked on every unpause (0.2 s), and corrections only fired at ≥1.5 s so medium scrubs never propagated. Thresholds now: paused scrub-follow >800 ms (smaller deltas FREEZE in place), resume plays IN PLACE within ≤600 ms and seek-rejoins beyond, playing correction ≥900 ms. Scrub latency: Shell feeds sync from `playback.position_ms` (published synchronously on every seek/pause/resume command AND each heartbeat tick) instead of the video-event stream, and an in-flight seek is NEVER restarted while ticks stay within 500 ms of its pending target — WKWebView restarts an async seek on every `currentTime` assignment, which previously made big scrubs land late (seek thrash).
   **NON-BLOCKING LOAD (2026-08-24 night):** track switches hung the ENTIRE transport for as long as file IO took (~20 s reported): `start_selected_track` held the global audio mutex across `load_streaming`'s file open+probe (`open_track` = cold disk / iCloud materialization / device teardown joins), starving pause/seek/volume AND every heartbeat tick behind it. Now the duration probe runs BEFORE the lock via `CpalSymphoniaBackend::probe_duration` and the precomputed value flows through the new `AudioBackend::load_streaming_with_duration(path, known_duration)` default-off trait method; the swap-in under the lock only joins threads and spawns the producer.
      **SWITCH-FREEZE FIXES (2026-08-25):** switching songs under video mode froze the window 20+ s; three stacked causes were fixed together. (a) SINGLE KICK PER SWITCH — only `start_selected_track` kicks `kick_video_pipeline` now (manual picks, next/previous, and auto-advance all funnel through it); `play_with_context` no longer fires a second pass that just superseded the first mid-search. (b) BOUNDED BACKGROUND FOOTPRINT — prefetch depth is the NEXT THREE tracks (was five; briefly two) at the ≤720p `FORMAT_LADDER_HIGH` ladder (was five × ≤1080p; one session pulled 350 MB in three minutes with 118/142 MB single files), and a `video_heavy_work` mutex SERIALIZES the upgrade transcode against prefetch downloads (polled `try_lock` with cancellation checkpoints — never a blocking lock); interactive quick-tier staging stays ungated so first frames never wait behind background work. (c) RANGE-AWARE DELIVERY — see the streaming-protocol note in step 3 above plus `docs/verification/receipts/video-mode-switch-freeze.md`.
**END-OF-MEDIA HOLD:** when a clip finishes BEFORE its song (common — 720p clips run shorter), VideoStage records the end position (`endedHoldShouldStay`, exported for tests) and HOLDS the final frame: without it the heartbeat's resume arm kept re-flagging the ended video Playing and play() restarted it from zero in a loop until the song ended. The hold releases ONLY when the authoritative AUDIO position seeks backward past end − 1.5 s (a real seek-back/replay intent); pausing and resuming does not replay it. A source change resets the hold.
   **SHUFFLE-SKIP LAG / REGISTRY STARVATION (2026-08-25 evening):** with video mode on, skipping songs lagged the whole app: every stage request spawned a worker that BLOCKED on the staging-store mutex, and the song-page poll (2 s while Loading) plus each skip piled blocked threads into the 32-slot task registry until playback-load/heartbeat submissions failed Busy. Staging workers now take a CANCELLABLE BOUNDED WAIT — poll `try_lock` every 100 ms until the store frees, supersede, shutdown, or `VIDEO_STAGE_WAIT_CAP_SECS` (300) — with inflight dedupe collapsing repeated polls/skips to ONE waiter per track; an interim defer-on-busy variant was reverted because it stranded deferred tracks with no retrier (videos took minutes). Plus a pressure gate (`VIDEO_STAGE_PRESSURE_LIMIT` = 24 of 32 slots) that defers staging while the runtime is saturated. Both red-green pinned by `stage_requests_never_pile_up_behind_a_busy_store` and `stage_defers_when_task_registry_is_under_pressure`. Also: the test harness now initializes volume to 0 (`cfg!(test)`) so unit tests can never emit audio through real speakers.

   **PAUSED-SONG VIDEO READINESS (2026-08-25):** "a paused song's video never loads; resuming much later finally does" had two stacked causes. (1) The OLD double-kick-per-switch meant the second pass SUPERSEDED the first mid-search AND skipped the slot as already-Loading — a Loading CORPSE with no live worker that no later pass would touch (the kick's skip now consults `current_video_live`: resolved, or Loading WITH an inflight marker; corpses restage). (2) Resume never caught the pipeline up — `resume_track` now runs one idempotent catch-up `kick_video_pipeline` pass whenever the current track's video is unresolved, BEFORE audio work, off the UI thread. Staging itself was proven pause-independent (`video_stages_to_ready_while_playback_is_paused`, index-cached fixture, zero network); both regressions are covered red-green by `kick_restages_a_loading_slot_with_no_live_worker` and `resume_kicks_the_video_pipeline_when_the_current_video_is_unresolved`. Receipt: `docs/verification/receipts/paused-video-readiness.md`.

   **DOWNLOAD PROGRESS RING (2026-08-24):** while staging publishes Loading, a 500 ms pump thread (`luno-video-progress`) folds yt-dlp's parsed percentage into `VideoState.download_progress` (0–100, only while Loading, only for the owning track); VideoStage renders it as a DETERMINATE green SVG ring (`--luno-green`, fills clockwise to a complete circle at 100 %, percentage centered, chrome styles self-contained in the component) over artwork — replacing the bare "Downloading video… N%" text. With no percentage yet it shows an INDETERMINATE spinning arc + "Loading video preview...". Geometry/helpers are exported pure (`VIDEO_RING_LENGTH`, `ringDashOffset`) and covered by `src/test/views/video_stage.test.tsx` (now actually included in vitest.config.ts). The ring is gated on the CURRENT track's staging status ONLY — Shell passes `downloadProgress` whenever `video.status === "loading"` and lets the loading status through even with music mode OFF (mode decides whether the staged src AUTO-PLAYS, not whether progress is visible). The same ring is exported as `DownloadRing` for reuse; it renders ONLY on the song-page media stage (a player-bar variant was tried and REMOVED by product decision — phantom Loading states made it flicker constantly). **VIDEO EVENT VALIDATION FIX (2026-08-24 night):** the TS contract validator demanded EXACTLY the seven original VideoState keys, so EVERY pushed `video_state` event (which serializes `download_progress`) failed envelope validation and was silently discarded — the UI survived only on command responses + the 2 s poll. The validator now accepts optional `download_progress` (0–100) and optional `revision` keys; `mergeVideoSnapshot` compares revision so a revision-only upgrade publication is never folded into "unchanged".
   **BUSY STAGING = GENERATION-REUSE SPINNER (2026-08-24 night, final):** when the staging store is busy, `video_stage_track` claims the UI IMMEDIATELY by publishing `Loading` for the requested track while REUSING the slot's current generation — never minting a new one. Minting fresh generations was tried and REVERTED: real worker results carrying older generations (e.g. `Unavailable` for a track with NO video) were then dropped by the frontend staleness filter → permanent spinners on video-less tracks and a phantom footer indicator. With reuse, the event stream stays monotonic, `completion_may_publish` (pure fn + unit test in state.rs) lets the busy worker's OWN completion win over the placeholder, and Shell's song-page poll keeps re-requesting every 2 s until the CURRENT track holds a RESOLVED status (`ready|playing|paused|unavailable|error`). Net behavior: spinner appears instantly on every request; video-less tracks settle to "Video unavailable — retry?" as soon as their staging runs.
   **SONG-PAGE WHEEL REDIRECT v2 (2026-08-24 night):** the page nests TWO scrollers (.luno-song-page-scroll inner, .luno-main-content outer) and rich children (playing `<video>`) swallow vertical wheel deltas in WKWebView. v1 pinned all wheels to the inner scroller — which often ISN'T the scrollable one — so scrolling stayed dead. v2 walks candidate scrollers inner→outer and forwards the delta to whichever can actually absorb it in that direction (`scrollHeight > clientHeight`, not at edge), preventDefault only on success.
   **SONG-PAGE WHEEL REDIRECT v2 (2026-08-24 night):** the page nests TWO scrollers (.luno-song-page-scroll inner, .luno-main-content outer) and rich children (playing `<video>`) swallow vertical wheel deltas in WKWebView. v1 pinned all wheels to the inner scroller — which often ISN'T the scrollable one — so scrolling stayed dead. v2 walks candidate scrollers inner→outer and forwards the delta to whichever can actually absorb it in that direction (`scrollHeight > clientHeight`, not at edge), preventDefault only on success.
4. `main.tsx` passes `videoConnected={backendAvailable}` so the Song Page's connection badge reflects IPC truth; without a configured helper the stage degrades to artwork exactly as before. `currentVideoForTrack` correlates video state by TRACK IDENTITY ONLY — video events and playback snapshots carry INDEPENDENT generation counters, and comparing them (an earlier implementation) made every staged video invisible; event ordering is guaranteed by the sequenced publication queue. The Song Page media slot also exposes an explicit green **Load music video** control (top-left of the stage, becomes "Retry music video" after a failed attempt, hidden while loading or once a source is playing) wired to the same staging request. Worker progress prints `[luno-video]` traces (candidate count, download start/reuse, staged path, failures) to stderr; a staging request that cannot start (store missing or another download holding the store) publishes Unavailable/Busy instead of exiting silently, which previously left the UI on "loading" forever.

### Discovery & Recommendations (Last.fm)
1. The Last.fm API key lives in a `0600` key file under the app data directory (`~/Library/Application Support/com.ty.luno/lastfm-key`), read at startup into the discovery service; the macOS Keychain item (`com.ty.luno`) is legacy seed material read AT MOST ONCE, silently, via a UI-denied SecItemCopyMatching — startup can never raise a Keychain prompt (2026-08-23). Settings ▸ Discovery saves/clears the key FILE live through the non-contract `save_discovery_key` / `discovery_key_status` Tauri commands (the same pattern as `track_artwork`), so Discover and Song Page recommendations start working without a restart.
2. The Song Page's "Recommended for this song" section is wired for the first time in React: Shell calls `Command::Discover { artist, title, limit: 10 }` seeded by the current track, rendering honest idle/loading/error(+Retry)/ready states, match scores, tap-to-preview (shared `preview_tap` session machinery) and save-to-playlist (first non-Unsorted playlist) actions.
3. Downloads/discover mode were already fully implemented end-to-end (search → preview → save/download with the bundled helper); the missing pieces were this key management surface and the song-page wiring above.

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
Persisted across sessions: `music_root`, `lastfm_key`, `auto_discovery`, `volume`, `crossfade`, `sleep_timer`, `global_keys` (media key toggle), and `favorite_songs` (normalized absolute paths for locally favorited tracks). The Rust migration target intentionally ignores the legacy `crossfade` preference and does not persist or expose an equivalent setting.

### 5. Session State
`LunoEngine` manages the live `queue`, `queue_idx`, `_original_queue` (for shuffle restore), and playback status (`playing`, `paused`). The Rust target `AppState` owns equivalent `QueueState`, `PlaybackClock`, and `FailureRecovery` objects and publishes authoritative queue/current-index/status/position/duration/generation snapshots through the versioned event stream.
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
- **URL Sanitization**: Validates that inputs start with `http` before processing to prevent terminal noise or garbage text from triggering engine errors. Video rendering prefers a validated staged preview path (rejecting traversal) over a safe `http`/`https` source URL.
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

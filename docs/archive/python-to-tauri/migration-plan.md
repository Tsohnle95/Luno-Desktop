# Luno Desktop Migration Plan

## Objective

Rebuild the desktop application as a visually and functionally equivalent
desktop product using:

```text
React + TypeScript       UI and presentation state
Tauri 2                  desktop shell, secure IPC, packaging
Rust                     application domain, services, concurrency, persistence
FFmpeg/yt-dlp            media conversion and provider extraction where retained
```

The migration is a rewrite of the architecture, not a rewrite of product
behavior. The existing Python/Tk application is the behavioral reference. The
new application must preserve user-visible behavior, data formats, queue
semantics, error handling, keyboard controls, temporary-file rules, and
portable transfer compatibility while replacing Tkinter, the controller-owned
object graph, pygame, and ad-hoc daemon-thread coordination.

## Non-Negotiable Compatibility

- Preserve the Luno visual language: dark Spotify-like palette, green accent,
  gradients, artwork treatments, typography hierarchy, spacing, icons, layout
  proportions, and interaction states.
- Preserve the application shell: sidebar, main content area, persistent queue
  panel, and persistent bottom player bar.
- Preserve all existing screens and their workflows: Library, Recent, Discover,
  Downloader, Settings, Error Log, and Song Page.
- Preserve audio normalization for permanent downloads: 44.1 kHz, stereo, 192
  kbps MP3, loudnorm behavior, and embedded thumbnails.
- Preserve temporary recommendation previews as 44.1 kHz/192 kbps MP3 without
  loudnorm and keep them outside the library.
- Preserve temporary Song Page video staging outside the library.
- Preserve `luno.library.export` version 1 and all checked-in fixtures.
- Preserve existing JSON and marker-file import compatibility, while writing
  new data with versioned, atomic schemas.
- Preserve queue behavior, shuffle restoration, repeat, seeking, completion
  detection, failure recovery, favorites, history, and media keys.
- Resolve the crossfade discrepancy before parity sign-off: the preference and
  fade helpers exist, but the active engine transition path does not currently
  invoke them. Choose implemented fades or explicitly documented no-op behavior.
- Preserve background-operation responsiveness: no filesystem scan, yt-dlp,
  FFmpeg, metadata extraction, artwork mutation, or network request may block
  the React/WebView thread.
- Preserve graceful degradation when FFmpeg, ffprobe, PyObjC-equivalent
  platform services, API keys, network providers, or media files are absent.

## Current Product Inventory

### Application Shell

- Window title `Luno`; resizable window with current minimum-size behavior.
- Three-column layout: adaptive left sidebar, center view, right queue/recent
  panel.
- Persistent bottom player bar.
- Sidebar logo, Home/Library, Recent, Discover, Downloader, Settings, and
  playlist navigation.
- Adaptive navigation collapse to a menu when vertical space is constrained.
- Playlist pane splitter and minimum sidebar/body/queue widths.
- Sidebar playlist search that filters cards without disturbing canonical order.
- Playlist order: All Music, Favorites, then alphabetized user playlists.
- About action showing the current Luno version.
- Dark theme, deterministic playlist gradients, green accent, custom scrollbars,
  clamped scrolling, touchpad and mouse-wheel support.

### Library and Playlists

- Configurable master music folder and default platform music root.
- Scan immediate child playlist folders and root-level Unsorted tracks.
- Recognize MP3, M4A, WAV, FLAC, MP4, and WEBM according to current behavior.
- Preserve Unsorted newest-first ordering with filename tie-breaking.
- All Music virtual playlist.
- Favorites virtual playlist, independent of physical folders.
- Empty playlist creation, playlist deletion, emptying, and safeguards for root
  and protected folders.
- Playlist URL storage and playlist synchronization.
- Header artwork, playlist label/name/count, Unsorted subtitle, and settings cog.
- Playlist actions: URL edit, destination selection, duplicate checking, sync,
  export, import, dump/copy, empty, delete.
- Action row: play all, shuffle, Favorites first, Get Similar Tracks, and sort.
- Sort modes: A-Z, Z-A, Recently added, Duration; preserve per-scope sort state.
- In-playlist title/artist filter and clear control.
- 56 px virtualized track rows with artwork, title, artist, playlist, favorite,
  selection, playing equalizer, and width-aware truncation.
- Click selection, double-click playback, Shift range selection, Cmd/Ctrl
  individual selection.
- Track context actions: play now, play next, add to queue, add to playlist,
  edit metadata, export selection, favorite, remove from queue, delete.
- Playlist smart actions: Most Played, Recently Added, and artist-based copies.
- Track editing with optional artist, required title, sanitization, filename
  rename, and live reference updates across library, queue, history, and
  favorites.
- Permanent deletion with confirmation.

### Queue and Recently Played

- Right panel tabs for Queue and Recently Played.
- Queue sections Now Playing and Next In Queue.
- User queue insertion semantics: Play Next immediately after current; Add to
  Queue after manually queued items and before remaining context tracks.
- Queue artwork, title, artist, favorite, click-to-play, removal, and context
  actions.
- Queue scroll preservation across in-place updates.
- Recently Played maximum of 100, newest first, virtualized 56 px rows,
  artwork, favorites, playing indicator, multi-select, Play All, Clear History,
  context actions, export, edit, and delete.
- History remains session-only.

### Audio Playback

- 44.1 kHz, signed 16-bit, stereo output equivalent.
- MP3/M4A/FLAC/WAV and supported format duration probing.
- Queue, current index, original shuffle order, user queue count, playing,
  paused, volume, shuffle, repeat, and crossfade state.
- Play with context list and playlist/search context preservation.
- Play/pause, previous, next, shuffle, repeat, scrubber, volume, and favorite.
- Mixer-clock-equivalent authoritative elapsed time with monotonic fallback.
- Position clamping and duration-aware progress.
- Pause/resume position preservation.
- Seeking that preserves paused state and restarts media at the requested time.
- Known-duration and mixer-idle completion detection with startup grace.
- Completion generation latch preventing duplicate advancement.
- Repeat restart, playlist-end behavior, shuffle current-track-first behavior,
  shuffle restoration, and boundary-repeat avoidance.
- Missing/unplayable track logging, automatic skip, temporary preview cleanup,
  and stop after five consecutive failures.
- Crossfade preference Off/2s/5s/10s/15s is persisted and fade helpers exist,
  but the active transition path currently does not invoke them; crossfade is a
  migration target/known gap rather than an existing behavior to overstate.
- Queue clearing resets playback, metadata, scrubber, and queue UI.
- Sleep timer Off/5/15/30/60 minutes; timer pauses playback, reports firing,
  and resets to Off.

### Player Bar and Platform Controls

- 60 px artwork, title, artist, favorite, and click-through to Song Page.
- Center transport controls and scrubber with elapsed/total time.
- Volume icon/slider and Up/Down 5% volume changes.
- Artwork-derived accent line.
- macOS media session integration: play, pause, toggle, next, previous,
  Now Playing title/artist/duration/elapsed/rate, readiness guards, retained
  handler lifetime, and clean unregister.
- XF86/media-key bindings and global enable/disable preference.

### Discover Hub

- Header/subtitle, empty state, recommendation cards, match percentage, artwork,
  and Add to Library.
- Last.fm track-similar primary lookup and artist-top-tracks fallback.
- API-key requirement, HTTPS artwork normalization, placeholder rejection,
  cache keyed by seed and requested limit. The source can temporarily hold
  eleven entries and later clear the cache, so decide whether the rewrite fixes
  this to bounded LRU or preserves the observed clear-all behavior.
- Library normalization filtering and intra-result deduplication.
- One active request plus latest pending request; generation-stale results ignored.
- Automatic discovery on track start and manual Get Similar Tracks.
- Add-to-library queued/downloaded states and library refresh.

### Song Page / Now Playing

- Glassmorphism blurred artwork background with generation-safe async rendering.
- Unified centered stacked layout with page-level scrolling and responsive 16:9
  media stage.
- Large artwork, title, artist, favorite, and Recommended for this Song.
- Recommendation seed fallback to playlist/folder artist and session seed pinning.
- Honest recommendation states: no track, no key, loading, error/retry, empty,
  and ready.
- Pages of 25 up to 1,000, load-more, exhaustion detection, preserved scroll,
  refresh, scroll hint, discover-mode toggle, and mobile-parity subtitles.
- Recommendation artwork cache chain: Last.fm, Deezer, deterministic abstract
  fallback; row state artwork and bounded caches.
- Tap-to-preview sessions, staged adoption, first-five prefetch, sequential
  resolve/download, deadlines, retry, rolling five-item window, next/previous
  routing, Discover mode queue adoption, autoplay-at-seed-end, infinite paging,
  teardown/restoration, playback-failure recovery, and save-to-playlist.
- Explicit playlist selection for saving; no silent Unsorted save.
- Song video toggle: current staged adoption, YouTube relevance search, merged
  MP4 staging, FFmpeg rendering synchronized to audio, resize, seek, pause,
  fullscreen F/Escape, and artwork fallback.

### Downloader Workspace

Preserve the scrollable hub and all seven tabs:

1. Search: YouTube/SoundCloud query, clear, search, load more, artwork cards,
   selection, download, bounded search artwork/cache/workers.
2. YT/CSV: URL preview, metadata tree, Exportify/CSV picker, Track Name/Artist
   Name(s)/Album Name handling, Download CSV, Skip Existing, SoundCloud First.
3. Direct URL: HTTP/HTTPS validation, optional title/artist, queue/submit.
4. Sync: playlist URL, check sync, sync all, destination mapping, progress,
   per-track logs, playlist URL configuration.
5. URL Queue: multiline URLs, fetch names, track counts, remove/clear, automatic
   or explicit destination, sequential Sync & Download All, logs, duplicate URL
   rejection.
6. Duplicates: background grouped scan, Track/Playlist/Filename columns,
   double-click play, delete, allow, refresh, bulk group exclusion, confirmation.
7. Allowed Dups: portable JSON export/import of normalized exclusion keys.

Also preserve collapsed Downloaded Songs and Download Log accordions, bounded
color-coded logs, inline progress, SAVE TO destination behavior, `+ New`
playlist creation, URL sanitization, playlist URL parsing, and default Unsorted
destination unless explicitly changed.

### Download and Media Services

- One active normal download/write job with busy rejection and cancellation.
- Define existing-output behavior per download operation. Every path must use a
  temporary output, atomic rename, expected-audio verification, and deterministic
  missing/partial-output failure handling; current verification is stronger for
  sync than for single-download APIs.
- yt-dlp best-audio extraction, FFmpeg conversion, 44.1 kHz/192 kbps/stereo,
  loudnorm, embedded thumbnail, retries, timeouts, and existing-output rules.
- Sequential playlist sync, cross-library duplicate matching, downloaded ID
  markers, legacy marker compatibility, permanent failure markers, 120-second
  track timeout, and actual-output verification.
- SoundCloud-first lookup option and YouTube fallback.
- Separate recommendation preview manager with 15-second resolve deadline,
  5-minute download deadline, 512 MiB cap, no loudnorm, separate concurrency,
  cancellation, cleanup, and playback recovery. Deadlines must be hard at the
  child-process boundary with cancel tokens, terminate/kill escalation, timer
  cleanup, late-result rejection, and partial-file cleanup.
- Song video provider resolver with up to ten results and three query forms,
  title relevance, artist evidence, canonical IDs, staged MP4, ffprobe FPS,
  timestamped frame handling, process termination, seek interruption, and
  stale-generation protection.

### Artwork and Metadata

- Embedded APIC extraction through Mutagen-equivalent metadata parsing.
- Preserve distinct cache ownership unless deliberately unified: main artwork is
  bounded to 96 entries/16 megapixels, Downloader search thumbnails use a
  separate bounded 128-entry cache, and recommendation artwork has a separate
  null-aware cache. A unified Rust service must preserve these observable limits.
- Playlist four-quadrant artwork composition and deterministic gradients.
- Async player/library/queue artwork updates without full redraws.
- Settings Fetch Missing Artwork: Deezer first, YouTube thumbnail fallback,
  JPEG APIC embedding, progress, success/failure logs.
- Recommendation artwork Last.fm/Deezer/fallback chain with null caching.
- Filename parsing using first `" - "` separator and all current normalization
  policies for sync, duplicates, discovery, transfer, and display.

### Settings and Diagnostics

- Last.fm setup instructions, masked key, auto-recommendations, immediate save.
- Master music folder picker.
- Full-library JSON export/import.
- Fetch Missing Artwork.
- Crossfade, sleep timer, and global media keys.
- Open Logs navigation.
- Read-only Error Log, custom scrollbar, and Clear Logs.

### Persistence and Transfer

Preserve and migrate:

- `~/.luno_config.json`: music root, Last.fm key, auto discovery, volume,
  crossfade, sleep timer, global keys, favorites.
- `<MUSIC_ROOT>/.luno_playlist_urls.json`.
- `<MUSIC_ROOT>/.luno_allowed_dups.json`.
- Playlist `.downloaded_vids`, `.sync_failed_vids`, and legacy `.downloaded.txt`.
- `~/.luno_previews` startup/session cleanup and exclusion from scans.
- `luno.library.export` v1 schema, strict validation, fixtures, selected/
  playlist/library scopes, provider IDs, ambiguity flags, additive imports,
  duration-compatible matching, missing-track confirmation, and no deletion.

Provider IDs are not fully wired through the current desktop import path. The
rewrite must build or persist a local provider-ID index and pass it to import
planning. Ambiguity confirmation must be per record or match, not one global
yes/no that queues every metadata-only ambiguity.

## Target Architecture

### Rust Domain Model

Replace path/list/dictionary authority with typed models:

```text
TrackId, PlaylistId, ProviderId
Track { id, path, title, artist, album, duration, artwork, favorite }
Playlist { id, name, path, url, ordered_track_ids }
LibraryIndex { playlists, tracks, virtual_views, scan_generation }
PlaybackState { queue, current_index, position, duration, status, context }
DiscoverySession { seed, recommendations, mode, window, captured_queue }
DownloadJob { id, kind, track, state, progress, cancellation }
PreviewAsset { id, path, state, source, generation }
VideoState { current_track, staged_path, state, decoder_state }
```

Track identity must support old absolute-path favorites while introducing stable
IDs. Define explicit matching policies for exact path, provider identity,
metadata identity, duplicate identity, and fuzzy search. Do not silently merge
the existing normalization rules.

### Rust Services

- `AppState`: owns services and the authoritative state snapshot.
- `LibraryService`: scan, watch, reconcile, metadata extraction, playlist
  mutation, filename editing, favorites projection.
- `PlaybackService`: audio backend, queue state, clock, seek, completion,
  shuffle/repeat/crossfade, failure recovery.
- `DownloadService`: normal single-job coordinator, yt-dlp/FFmpeg subprocesses,
  progress, cancellation, marker files, output validation.
- `PreviewService`: recommendation staging, resolver/download deadlines,
  queue/session integration, cleanup.
- `VideoService`: source lookup, MP4 staging, decoder/player lifecycle,
  fullscreen metadata, seek/resize synchronization.
- `DiscoveryService`: Last.fm, request coalescing, generation control, cache,
  recommendation filtering.
- `ArtworkService`: embedded-art extraction, remote lookup, file mutation,
  cache, bounded work.
- `TransferService`: exact v1 compatibility, import plan, matching, additive
  copy, missing-track download handoff.
- `SettingsService`: versioned atomic settings, migration, keychain adapter.
- `MediaSessionService`: macOS now-playing/media-key adapter and platform gates.
- `LogService`: structured bounded logs exposed to Error Log.

All new persistence writers must use temp-file, flush/fsync, permission
preservation, and atomic replace. Legacy files are read through compatibility
parsers; new writes use versioned schemas. This applies to config, playlist URL
metadata, allowed duplicates, download markers, settings, and manifests.

### Tauri Commands and Events

Commands must be typed and capability-scoped. Core command groups:

- app lifecycle, settings, logs
- library scan/watch/playlists/tracks/favorites
- playback and queue
- discovery and recommendations
- preview sessions
- downloads and sync
- artwork fetch/embed
- video staging/playback
- export/import

Events must include request/job/session generations where stale results are
possible. Minimum event families: library snapshots, playback snapshots,
track-started, queue-changed, download progress/state, preview row/session
state, discovery state, artwork completion, video state, logs, and shutdown.

React should render backend snapshots and use local UI state only for transient
presentation state such as open dialogs, selection anchors, active tab, and
scroll positions. No frontend code should scan files, spawn tools, mutate MP3
tags, own playback queues, or decide stale-result validity.

### React Frontend

Suggested structure:

```text
src/
  app/                 shell, routing, providers, event bridge
  components/          sidebar, queue, player, dialogs, rows, artwork
  views/               library, recent, discover, downloader, settings,
                       song-page, error-log
  features/            playback, library, discovery, downloads, transfer,
                       recommendations, video
  state/               typed backend projection and UI-only stores
  styles/              tokens, gradients, layout, responsive rules
  test/                interaction and visual regression tests
src-tauri/
  src/                 commands, events, state, services, adapters
  capabilities/        least-privilege permissions
```

Use CSS grid/flex, `aspect-ratio`, `object-fit`, virtualized lists, and
ResizeObserver. The video stage must be a normal layout child, not a separate
window or manually painted frame surface. Use HTML video where practical;
retain Rust/FFmpeg only where exact audio-clock synchronization requires it.

## Migration Phases

### Phase 0: Behavioral Freeze and Baseline

- Tag the Python implementation as the behavioral reference.
- Capture screenshots and interaction recordings for every screen and major
  state at desktop widths and constrained widths.
- Record startup/shutdown, scan, play, pause, seek, queue, shuffle, download,
  sync, discovery, preview, video, settings, import/export, and failure flows.
- Expand tests around undocumented source behavior and known discrepancies.
- Inventory real config, marker, preview, library, and export fixtures without
  committing user data.
- Define acceptance IDs for every feature in this document.

### Phase 1: Contracts and Compatibility Harness

- Freeze `luno.library.export` v1 fixtures and implement Rust codec tests.
- Create JSON schemas for new settings, library snapshots, queue snapshots,
  jobs, and events.
- Add golden tests for filename parsing, all normalization policies, scan order,
  duration matching, duplicate grouping, and marker compatibility.
- Build a Python-vs-Rust differential harness for pure behavior.
- Decide and document stable TrackId/PlaylistId migration rules.

### Phase 2: Rust Infrastructure and Subprocess Adapters

- Create Tauri 2 workspace and CI for macOS/Windows/Linux.
- Implement structured logging, shutdown orchestration, cancellation tokens,
  bounded task executors, child-process registry, and atomic persistence.
- Wrap yt-dlp and FFmpeg with explicit process handles, stdout/stderr capture,
  hard cancellation deadlines, temporary outputs, atomic promotion, and
  platform executable discovery.
- Add application data directory migration for existing preview/config files.
- Add hard child-process timeout tests, termination escalation tests, and late
  result/partial-output cleanup tests for yt-dlp and FFmpeg.

### Phase 3: Library and Persistence

- Port scanner semantics exactly, including root/playlist ordering and supported
  extensions; expose scan errors.
- Define watcher event coalescing, create/rename/delete handling, partial-file
  filtering, active-write behavior, and generation rules while downloads/scans
  are active.
- Implement filesystem watcher plus debounced/full reconciliation.
- Implement typed tracks/playlists, favorites, playlist URL metadata, duplicate
  allowances, filename editing, playlist CRUD, and smart playlist copies.
- Migrate old path-based favorites and marker files.
- Implement transfer export/import before feature migration depends on it.

### Phase 4: Audio and Queue Core

- Select and spike Rust audio backend with format coverage and 44.1 kHz output.
- Implement authoritative clock, duration probing, queue context, user queue,
  shuffle/repeat, crossfade, seek, completion, failure skip, and sleep timer.
- Resolve crossfade first: add a transition test to the Python baseline, then
  either port real fades or record the current no-op behavior as intentional
  compatibility.
- Implement event snapshots and command idempotency.
- Add a headless playback conformance suite against Python behavior.
- Implement media-session abstraction and macOS adapter.

### Phase 5: Shell and Core UI

- Recreate tokens, gradients, typography, spacing, icons, responsive shell,
  sidebar, queue panel, bottom player, custom scroll behavior, and window state.
- Add virtualized 56 px Library/Recent rows and in-place artwork/equalizer updates.
- Match selection, context menus, dialogs, keyboard shortcuts, and focus behavior.
- Use screenshot/visual regression baselines at each supported window size.
- Define baseline sizes, screenshot tolerances, font fallback policy,
  sidebar-collapse thresholds, and the 56 px row tolerance.

### Phase 6: Library, Recent, Settings, Error Log

- Complete Library and Recent screens against the domain/event APIs.
- Implement Settings and persistence forms.
- Implement Error Log and structured backend log subscriptions.
- Validate all playlist, favorite, edit, delete, sort, filter, export/import,
  and queue workflows.

### Phase 7: Downloader and Sync

- Port all seven Downloader tabs and their responsive layouts.
- Implement job state/progress/cancellation and one-normal-job policy.
- Port search, CSV, direct URL, playlist sync, URL Queue, duplicates, allowed
  duplicates, logs, downloaded songs, and destination behavior.
- Compare output naming, normalization, marker files, and failure classification
  with the Python implementation.

### Phase 8: Discover and Recommendation Previews

- Port Last.fm/Deezer provider adapters, caches, filtering, request bounds,
  recommendation artwork, and Add to Library.
- Implement preview manager and row state machine with deadlines, retry,
  cancellation, cleanup, queue adoption, and playback failure recovery.
- Implement non-Discover preview sessions and Discover mode exactly.
- Validate next/previous, queue restoration, handoff, infinite paging, seed
  stability, and save-to-playlist behavior.

### Phase 9: Song Page and Video

- Recreate Song Page glassmorphism, artwork cache, responsive media stage,
  recommendations, and page scroll.
- Implement the current inline Video checkbox behavior and asynchronous staged
  adoption/download/preload.
- Prefer HTML video rendering with CSS aspect-ratio and fullscreen. Use a Rust
  synchronization adapter to correct video time to the audio clock.
- Preserve source relevance, staging, ffprobe/FPS behavior, pause/seek/resize,
  cleanup, and artwork fallback.
- Add visual resize stress tests and media lifecycle tests.

### Phase 10: Packaging and Release Migration

- Replace PyInstaller with Tauri bundling and signed installers.
- Preserve bundle identifier `com.ty.luno` where platform migration allows it.
- Package or provision FFmpeg/ffprobe and yt-dlp with license notices and update
  strategy; define fallback when unavailable.
- Add macOS permissions, media-session entitlements, Windows integration, and
  Linux runtime checks.
- Add platform build/package smoke gates, executable discovery tests, license
  and update-policy checks, bundle inspection, and clean-machine launch tests.
- Implement first-run migration from old config/markers/previews.
- Keep rollback path to Python release until parity gates pass.

## Verification Gates

### Automated

- Rust unit/integration tests for all domain services.
- TypeScript component and interaction tests.
- Tauri command/event contract tests.
- Python-vs-Rust differential tests for pure behavior.
- Existing transfer fixtures unchanged and passing.
- Download/sync tests with fake yt-dlp/FFmpeg executables.
- Playback tests with deterministic fake audio backend.
- Cancellation, shutdown, stale-generation, and atomic-write tests.
- Accessibility tests for keyboard/focus/labels.
- Screenshot and responsive visual regression tests.

### Manual Acceptance

- Launch with valid, missing, and corrupt configuration.
- Scan large library while interacting with every screen.
- Resize continuously at minimum, default, and wide window sizes.
- Play every supported audio format; pause, seek, shuffle, repeat, crossfade,
  queue, favorites, media keys, and sleep timer.
- Exercise all downloader tabs, cancellation, retries, sync markers, duplicate
  workflows, and shutdown during active jobs.
- Exercise recommendation prefetch, tap preview, Discover mode, handoff,
  save-to-playlist, failure recovery, and cleanup.
- Exercise Song Page artwork/video/no-video/resize/fullscreen/seek states.
- Export/import between old Python and new Tauri versions in both directions.
- Run a versioned real-user-data corpus covering corrupt/partial config, missing
  roots, legacy and malformed download markers, stale/moved favorites, old
  preview directories, unsupported files, provider-ID matches, and mixed
  ambiguous import records; verify expected state and rollback behavior.
- Verify no temporary preview files enter the music library.
- Verify app shutdown terminates audio, yt-dlp, FFmpeg, and worker tasks.

Classify existing tests during porting as parity, differential, integration, or
obsolete ad-hoc experiments. Preserve behavior from the engine, media-key,
discover, downloader-sync, transfer, contract-fixture, artwork-cache, and video
tests; explicitly classify `test_tkvideo.py`, `test_dl.py`, `test_focus.py`, and
`test_label_ph.py` rather than treating every experiment as a release gate.

## Risks and Decisions Required

- Select a Rust audio backend with equivalent codec and seek behavior.
- Decide whether to retain yt-dlp as a managed external dependency or replace
  individual provider operations; do not reimplement YouTube extraction casually.
- Decide whether HTML video can meet synchronization tolerance or whether video
  decoding must remain a Rust/FFmpeg service.
- Define stable identity migration for moved/renamed files and favorites.
- Decide whether API keys move to the OS keychain while retaining old config
  import.
- Resolve source/documentation discrepancies before freezing scan semantics.
- Define platform support and FFmpeg/yt-dlp licensing/distribution policy.
- Define whether Android and desktop should share Rust domain/transfer code or
  only continue sharing the language-neutral export contract.

## Definition of Done

Migration is complete only when the new Tauri application passes the automated
and manual gates, reproduces every listed workflow and visual state, imports all
supported existing user data, preserves transfer compatibility, handles missing
dependencies gracefully, and can run without the Python/Tk runtime. The old
implementation remains available as a rollback/reference build until a release
has completed a full-user-data migration trial.

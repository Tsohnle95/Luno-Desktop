# Architectural Sweep Findings

## Project Overview
Luno is a desktop music player (v4) built with Python/Tkinter, following a **Controller-View-Engine (CVE)** pattern. The application manages a local music library, provides playback via pygame, supports yt-dlp downloads, Last.fm discovery, and artwork fetching.

## Architecture: CVE Pattern

### 1. Controller (`music_player.py` / `LunoMusicPlayer`)
- **Shared State**: `_library` (Playlists → Paths), `tk.Variables` (status, search, etc.), `_frames`
- **Orchestration**: Manages tab switching (`_show_tab`), broadcasts state changes to views
- **ArtCache**: `ThreadPoolExecutor`-backed cache for album art thumbnails (96 max entries, 16 MB pixels max)
- **Watchdog**: 30-second `self.after()` polling loop (`_init_watchdog`) that auto-detects filesystem changes by calling `_refresh_library()`. Skips active downloads, syncs, transfers, scans. Manual and watchdog refreshes coalesce behind one active scan.
- **Media key handling**: MPRemoteCommandCenter in-process registration; callbacks enqueue onto `_media_command_queue`; `_poll_media_commands` drains it.
- **Config persistence**: `~/.luno_config.json` for music_root, lastfm_key, volume, crossfade, sleep_timer, global_keys, favorite_songs
- **Session state**: `_discover_active`, `_discover_seed`, `_discover_chain` state machines for discovery mode

### 2. Engine (`engine.py` / `LunoEngine`)
- **Audio Logic**: Wraps `pygame.mixer`. Manages `queue` (list of paths), `queue_idx`, `user_queue_count` (Up Next behavior), `_original_queue` (shuffle restore), `shuffle`, `repeat`, `crossfade_secs`.
- **Heartbeat**: 400ms `poll()` loop that updates scrubber and checks for track completion
- **Duplicate Exclusion**: Loads/saves allowed duplicate keys from `.luno_allowed_dups.json` within MUSIC_ROOT
- **Completion latch**: `_completion_dispatched` — at most one advance per track generation, checked in `_poll` to prevent dual-firing
- **Duration-based completion**: when `duration > 0` and `elapsed >= duration`
- **Mixer-idle completion**: `pygame.mixer.music.get_busy()` false with `elapsed > 0.5s` startup grace — works for unknown-duration tracks
- **Position tracking**: `get_position()` prefers `pygame.mixer.music.get_pos()` with monotonic fallback; `_clamp_position()` clamps to [0, duration]
- **Scrub handling**: `on_scrub_moved()` sets `seek_pos`, resets `track_start`/`_playback_base_pos`/`_playback_base_mono`; `pygame.mixer.music.play(start=pos)`
- **Volume**: `on_volume_moved()` sets `self.volume` and `pygame.mixer.music.set_volume()`

### 3. Views (`views/*.py` — all inherit `tk.Frame`)
- **library.py**: Virtual scrolling canvas (56px rows). Header displays playlist title, dynamic album art (`PlaylistArt`), playlist-settings cog. In-playlist search bar (`Filter tracks…`) using `tk.Label` overlay with `rely=0.5` vertical centering. Sort control (`A-Z`, `Z-A`, `Recently added`, `Duration`). Favorites star (16px before playlist name). Playing row's green equalizer bars tagged `pulse` animated via `_update_pulse` (`coords()`) at 80ms heartbeat — avoids full canvas redraw. Get Similar Tracks opens Discover.
- **downloader.py**: Scrollable canvas download workspace. Header/status row, source tabs (Search, YT/CSV, Direct URL), desktop-only tabs (Sync, URL Queue, Duplicates, Allowed Dups). Dynamic `_search_limit` for Load More functionality. Duplicates tab features Playlist and Filename columns; bulk-exclusion of duplicate sets; double-click header plays first track. Allowed Dups tab includes Export/Import buttons for portable JSON backups.
- **recent.py**: Recently Played view. Gradient header. Displays `controller._recently_played` (max 100) in virtual scrolling canvas (56px rows). Shift+Click/Cmd+Click multi-selection. Context menu: JSON export, Edit Track Info, Play All, Clear History.
- **settings.py**: Premium Spotify-style UI. Last.fm API key management, storage paths, playback options (crossfade, sleep timer), global media keys, full-library JSON export/import, fetch missing album art.
- **discover.py**: Discovery Hub view. Recommended tracks from Last.fm based on current listening. One-click "Add to Library" download integration. Context-Aware Shuffle: shuffling respects user's current view (All Music, Recently Played) instead of being limited to physical disk folders.
- **song_page.py**: Now Playing detail view. Glassmorphism background (blurred album art overlay). Unified centered stacked layout. Large centerpiece artwork (up to 800x800) dynamically scaling. Recommended for this song section with pages of 25, Load 25 more up to 1,000. Seed stability: while session active, section pinned to session's seed song (`_discover_session_seed`), so transient preview tracks never re-seed. States: no track / no API key / "Finding similar songs..." / error + Retry / "No recommendations available". Mobile parity: whole page is one scrollable column — artwork, metadata, favorite, and scrollable "Recommended for this song" section share page-level canvas.
- **error_log.py**: Debug log viewer. Displays `controller._error_logs` (list of strings) in read-only `tk.Text` with `ModernScrollbar`. "Clear Logs" button resets the list.

## Data Flow

### Library Scan → UI
1. `_refresh_library()` starts at most one generation-tagged thread calling standalone `scan_library(roots)` (imported from `engine.py`); repeated refresh requests coalesce into one pending scan; stale results ignored
2. Returns `dict[playlist_name → list[file_paths]]`; root-level tracks represented by `Unsorted` ordered newest-first by filesystem mtime, filename tie-breaking
3. `_finalize_library_update(new_lib)` hashes result, stores in `self._library`
4. Calls `_populate_sidebar()` → rebuilds sidebar cards
5. Calls `_populate_track_list("")` → rebuilds track list
6. Broadcasts `on_library_updated()` to all views

### Track Name Editing
1. Library and Recently Played context menus call `LunoMusicPlayer._ctx_edit_track_info()` for one selected local file
2. Dialog collects optional artist and required track name, sanitizes them, renames audio file to `Artist - Title.ext` (or `Title.ext` when artist is blank)
3. Live queue/history/favorite path references updated, then generation-tagged library scan refreshes search, ordering, and Unsorted view

### Playback
1. User clicks track → `engine.play_with_context(path, context_list)`
2. Sets `queue` to `context_list` (or playlist containing `path`)
3. Sets `queue_idx` to track's position
4. Calls `play_current()` → clears stale scrub position/time, loads `pygame.mixer.music`, starts playback, records mixer-clock base state plus monotonic fallback state, probes duration via `mutagen.File`
5. `_poll()` loop (400ms heartbeat) — unconditional reschedule after every invocation except when controller is closing:
   - Uses `get_position()` (canonical elapsed from `pygame.mixer.music.get_pos()` while audio is busy, clamped to [0, duration], with monotonic fallback for startup/end/unavailable mixer-clock states)
   - Updates main scrubber/time and Song Page scrubber/time
   - Completion detection uses generation latch (`_completion_dispatched`): at most one advance per track
   - Duration-based completion: when `duration > 0` and `elapsed >= duration`
   - Mixer-idle completion: `pygame.mixer.music.get_busy()` false with `elapsed > 0.5s` startup grace — works for unknown-duration tracks
   - Advances via `next_track()` or `play_current()` (repeat), which resets the latch and clears scrub to 0

### Favorites
1. Persisted as normalized absolute file paths in `favorite_songs` array of `~/.luno_config.json`
2. Controller intersects persisted set with every completed library scan, so missing files removed from favorite state
3. Desktop sidebar exposes reserved virtual `Favorites` playlist; never creates or scans physical Favorites folder
4. Track context menus, queue menu, player bar, and Song Page toggle favorite state. Library rows show outlined/filled star before playlist name. Library action row can stably move favorites ahead of other tracks.

### Download Pipeline
1. `DiscoverView`'s Add to Library action → `LunoMusicPlayer._download_track()` → `Downloader.download_sync_tracks()`; `DownloadView._download_selected()` / `_queue_direct_url()` → `Downloader.download_single()`, `_download_all()` → `Downloader.download_sync_tracks()`, `_start_csv_dl()` → `Downloader.download_csv_tracks()`, URL Queue / Sync All → `Downloader.download_playlist_sync()`
2. `Downloader` admits one active download/write job at a time, rejects duplicate starts, propagates cooperative cancellation through yt-dlp progress hooks. Playlist track timeouts run in owning job and never leave detached yt-dlp worker running while next track starts.
3. `_create_opts()` → yt-dlp options: 44.1kHz, 192k MP3, loudnorm, EmbedThumbnail
4. Discovery download status, logs, and completion callbacks marshalled through `controller.after(0, ...)`; sync-track path creates destination folder and treats missing audio output as failure instead of false success
5. Progress hook → coalesced `controller.after(0, ...)` → UI update
6. On complete → `_refresh_library()` → coalesced library scan

### Artwork Fetching
1. `PlaylistArt` or `SongPageView` requests art → `ArtCache.get(path, size, callback)`; downloader search cards use separate bounded 128-entry thumbnail cache and bounded in-flight executor
2. `ArtCache` checks bounded in-memory LRU cache (max 96 entries, 16 megapixels) → if miss, queues key and drains it through `ThreadPoolExecutor` under in-flight limit
3. Worker reads `mutagen.mp3.MP3` → rejects oversized APIC payloads/images → extracts, crops, resizes via `PIL.Image`
4. Returns `ImageTk.PhotoImage` via bounded `controller.after(0, ...)` callback; stale path cache entries pruned after each library update

### Song Video Preview
1. Song Page's discrete Video control probes YouTube with yt-dlp on worker thread. Resolution searches up to 10 results across up to three query variants (`artist - title`, `title artist`, and title-only), deduplicates IDs, requires title relevance and artist evidence when artist known, constructs canonical watch URLs from IDs; does not begin playback or download during probe
2. After successful probe, yt-dlp downloads one merged MP4 into `~/.luno_previews`, outside the library scan. Preview lookup, preload, and current-track adoption share same artist/title fallback and filename key. Existing audio engine continues playing normally.
3. System FFmpeg decodes frames ahead into small timestamped buffer using source video's measured frame rate; Tk selects newest frame due against audio clock instead of pacing or dropping on fixed timer. Single-song media stage spans available content width at responsive 16:9 rectangle with reduced surrounding content height. Artwork crop-filled without distortion; video aspect-fitted with letterboxing. Video renderer resized/restarted when stage changes dimensions; staged adoption waits for Tk layout realization, preventing square-first player or stale-size video after window resizing.

### Export / Import
1. Export actions snapshot `_library` and selected paths, then build manifest and probe metadata on worker thread
2. Manifest stores stable provider IDs when available and marks metadata-only tracks with `ambiguity_confirmation_required`
3. Import validates untrusted JSON, matches local files by normalized artist/title and compatible duration, and additively copies matched files into sanitized playlist folders
4. Missing tracks never deleted or silently replaced. After confirmation, desktop missing tracks queued through existing YouTube/CSV downloader; unresolved tracks remain reported

## State Management & Data Structures

### 1. The Normalization Key (Logic Consistency)
For syncing: `key = re.sub(r'[\W_]', '', f"{artist}{title}").lower()`
For duplicate detection, stronger `_normalize_for_dupe()` function:
- Strips parenthetical tags: `(Official Audio)`, `[Lyrics]`, `(HD)`, `(Remastered)`, etc.
- Normalizes `feat.`/`ft.`/`featuring` → `feat`
- Strips file extensions and all non-alphanumeric characters
- Lowercases everything for case-insensitive comparison

### 2. Title Splitting
When parsing YouTube names or filenames, app looks for `" - "` to separate Artist from Title. Edit Track Info dialog writes entered values back into that filename shape, so existing search, playback labels, discovery lookup, duplicate detection, and export fallback all use the new names without separate metadata database.

### 3. The Library Dict (`_library`)
Stores aggregated and folder-specific track paths. Includes empty directories to support new playlist creation.

### 4. Global Settings (`~/.luno_config.json`)
Persisted across sessions: `music_root`, `lastfm_key`, `auto_discovery`, `volume`, `crossfade`, `sleep_timer`, `global_keys` (media key toggle), and `favorite_songs` (normalized absolute paths for locally favorited tracks). Rust migration target intentionally ignores `crossfade` preference.

### 5. Session State
`LunoEngine` manages live `queue`, `queue_idx`, `_original_queue` (for shuffle restore), and playback status (`playing`, `paused`).

**Queue Architecture**: Flat `queue` list, but tracks `user_queue_count` integer to behave like "Up Next" queue. "Play Next" inserts immediately after current `queue_idx`. "Add to Queue" appends to `queue_idx + 1 + user_queue_count`, ensuring manually queued items play before rest of playlist context. `user_queue_count` decrements naturally during track progression and resets on hard skips/context changes.

### 6. Exclusion Management
Allowed duplicates identified by normalized keys (derived via `_normalize_for_dupe`) and persisted in `.luno_allowed_dups.json` within Master Library root. Portable backups (JSON) can be exported/imported manually.

### 7. Favorites
`_favorite_paths` is controller-owned set of normalized absolute file paths. Persisted in `~/.luno_config.json`, pruned against scanned library paths, projected as reserved virtual `Favorites` playlist. Favorite state independent of physical playlist membership; deleting a file also removes its favorite state. Portable library manifest intentionally does not include favorites (matches Android's local Room flag behavior).

### 8. Portable Library Manifests
Transfer format is `luno.library.export` with `manifest_version: 1` and scopes `selected_tracks`, `playlists`, or `library`. Track records contain title, artist, album, duration, optional `{provider,id}` source identity, and explicit ambiguity flag. Playlist records contain only sanitized names, descriptions, and ordered track references. Imports are additive and match existing audio before optionally downloading missing references; no local path, secret, audio blob, playback history, or automatic deletion allowed.

## Technical Protocols (Strict Adherence)

### 1. UI Architecture (Modern Standard)
- **Global 3-Column Layout**: Uses `tk.PanedWindow` (sashwidth=6, bg=BG) for independent resizing — bg-colored sash invisible but leaves grabbable drag area.
  - **Sidebar**: Nav (Adaptive) | Playlists (Vertical Pane). Minimum width: 240px.
  - **Body**: Persistent center column for tabs (minsize=400).
  - **Queue**: Persistent right column (Canvas-based, minsize=240, matching left sidebar). Spans full app height.
- **Adaptive Sidebar**: Navigation items (5 total: Home, Recent, Discover, Downloader, Settings) collapse into single "☰ Menu" button if top pane height < 220px. Defaults to fixed 235px height for Nav section at startup so playlist panel uses reclaimed space.
- **Scrolling Policy**: Custom `ModernScrollbar`. Side panels and all canvas views use explicit redraw hooks or bounded checked clamping in normalized `<MouseWheel>`, legacy button-wheel, and Tk 9 `<TouchpadScroll>` handlers. Tk 9 packed touchpad deltas converted to canvas units with per-binding fractional accumulator for proportional, bounded scrolling. All scrollable views employ **Clamped Logic**; sidebar uses `winfo_reqheight()` to reliably measure inner content vs. canvas viewport — if content fits, scrolling disabled at both wheel handler (early-return) and `scrollregion` (locked to canvas height). Library track canvas uses `total_items * row_height <= canvas_height`. Both apply `yview_moveto(0)` reset when content fits.
- **Canonical Playlist Ordering / Sidebar Search**: Sidebar header clickable `Library` button with `Unsorted (N)` tab; `_populate_sidebar()` establishes playlist-card order as All Music → Favorites → Alphabetical user playlists. Search filtering applies only to playlist cards, not the Unsorted tab. Selecting Library or Unsorted resets virtual track canvas to `yview_moveto(0)`. Search filtering uses **lightweight `_reshow_all_cards()`** — unpacks all cards via `pack_forget()` then re-packs in original children order. **No widget destruction** — eliminates visual flicker. `_lib_key_released` always unpacks all first, then re-packs matches (or all if query empty) preserving canonical order. `populate_track_list()` in `library.py` applies active mobile-parity sort mode to track rows; Unsorted starts in newest-first mode while other scopes start A-Z.
- **Treeview Theming (macOS)**: Enforces `bordercolor`, `lightcolor`, and `darkcolor` attributes mapped to background color in `clam` theme configuration to eliminate native OS 3D white borders around Results, Queue, and Library data tables.
- **Gradient Header Pattern**: Settings and Recent headers use a `<Configure>` callback that: (1) deletes old gradient items, (2) redraws gradient via `draw_gradient_rect`, (3) calls `tag_raise("title")` to keep text above gradient rectangles. Downloader header intentionally uses flat mobile-parity title/subtitle/status treatment instead. Gradient title text must use `tags="title"`.
- **Context Logic**: Playing a song from a playlist context force-refreshes current queue to match that playlist's tracks. Launching a track from a search result **retains** the active search filter instead of reverting view to full unfiltered library (accomplished by directing `play_current` to trigger silent `_redraw_track_list` instead of full `_populate`).
- **Sash Interaction**: Draggable vertical boundary between nav and playlists marked by visual `BG4` row handle.
- **Text Truncation Policy**: Global policy to prevent UI overlap. Library header title and track list rows use dynamic truncation placeholders (`...`) based on available canvas/frame width. Queue items use width-aware character limits (capped around 26-32 chars) for single-line consistency within 240 px minimum queue pane.
- **Instant Input Feedback**: Slider widgets (`SlimSlider`) trigger commands during both `B1-Motion` and `ButtonRelease-1`, ensuring instantaneous audio/UI feedback.
- **No-Flicker Queue/Recent Panels (2026-08-11)**: Queue panel (`_update_queue`) preserves scroll position across rebuilds (only explicit tab switches reset to top) and swaps artwork in place — async art arrivals update only affected row's label via `_q_art_targets` registry instead of rebuilding whole panel and snapping scroll. Recent view's 80ms playing-indicator pulse animates only tagged `pulse` canvas bars (`_update_pulse`) instead of redrawing entire canvas per tick. Library track canvas uses same targeted pulse — its 80ms `_pulse_tick` moves four tagged equalizer bars via `_update_pulse` (`coords()`) rather than full `_redraw_track_list()`, so whole-canvas delete/recreate churn never runs on timer while track plays.
- **Downloader Hub Strategy**: Uses `restrictfilenames: False` and `ignoreerrors: True`. Unified Downloader view wrapped in scrollable canvas for spacing; follows mobile download hierarchy: Downloader header/subtitle, active-download status, compact source tabs (Search, YT/CSV, Direct URL, plus desktop Sync/URL Queue/Duplicates/Allowed Dups), thin conditional progress, source-specific input rows, artwork-backed search result cards, and collapsed Downloaded songs accordion populated from Unsorted. Desktop-only tree workflows remain intact for playlist preview, sync, duplicate review, and allowed-duplicate backups. YT/CSV tab uses PREVIEW workflow: fetch metadata → populate tree → then download via action buttons. Download status shown inline (`Downloading {title} — x/y`) using yellow/green cycling. Collapsible download log at bottom keeps bounded recent in-memory history. "+ New" button next to SAVE TO combo enables in-place playlist creation.
- **Default Download Destination**: All downloads default to **Unsorted** (MUSIC_ROOT) unless user explicitly selects destination from "SAVE TO" dropdown (Strict Prioritization). No auto-created subfolders.
- **URL Sanitization**: Validates that inputs start with `http` before processing to prevent terminal noise or garbage text from triggering engine errors.
- **Playlist Persistence**: Forced parsing of `list=` parameters in YouTube URLs to ensure full playlist extraction even when pointing to specific video.

### 2. Discovery Protocol
- Employs **Last.fm `track.getSimilar`** API for Spotify-style recommendations with **`artist.getTopTracks` fallback** for niche tracks.
- Features **Discover Hub** (`views/discover.py`) with dedicated navigation tab.
- Supports both **Automatic** (trigger on track start) and **Manual** (trigger via Library's "✨ Get Similar Tracks" action, which opens Discover after starting request—using **Smart Selection** to target active library rows if nothing is playing).
- Implements **query cache** (10 entries, keyed by seed + requested limit — mobile parity) to minimize API redundancy.
- **Duplicate Check**: All recommendations cross-referenced against normalized index of user's library (`artist - title`) and filtered out to ensure only new music shown — now via shared `_filter_library_recs` helper (dedupes within result list too), used by both Discover tab and song page's "Recommended for this song" section.

### 3. Discovery Request Bound
- One active daemon worker plus one latest pending request; rapid track changes replace stale pending work instead of creating unbounded thread backlog.
- Song page's recommendation section has its own bound (`_fetch_song_page_recs` / `_start_song_page_recs_request`, thread name `luno-song-page-recs`) so Discover tab and song page never cancel each other's requests; results delivered on UI thread via `.after(0, ...)` only when request is still latest generation.

### 4. Discover Mode (song page, mobile RecommendationPreviewManager parity, 2026-08-11)
- Recommendations prepared into `~/.luno_previews` (`PREVIEW_DIR`) — temporary staging area **outside the library root**, mirroring Android `cacheDir/PREVIEW_DOWNLOAD_DIR`.
- Library scans and Unsorted view never see preview audio; nothing lands in library unless user explicitly saves it.
- Dir wiped at every app launch (mobile `startupCleanup`) and at session end.
- When song page delivers fresh recommendation list, chain **passively prefetches the first five** (`DISCOVER_IMMEDIATE_COUNT`, mobile `PREFETCH_RECOMMENDATION_COUNT`) without touching playback; rows queued behind active preparation show "Queued for temporary preview" (mobile prefetchQueuedKeys). Already-staged files adopted, not re-downloaded (refresh-safe), and prefetch for same seed never reset mid-session.

**Preparation pipeline (mobile resolve→download parity):** one recommendation at a time, sequentially. Each row goes `waiting` → **`resolving`** ("Finding a playable source...", a `ytsearch5:` probe on the 2-thread `luno-preview-resolver` pool with **15s hard deadline** → timeout shows "Finding a playable source timed out. Tap to retry.") → **`preparing`** ("Downloading temporary preview...", `previews.download_preview` on worker thread — 44.1 kHz/192k MP3 **without loudnorm**, embedded thumbnail, cooperative **5-minute deadline** → "Preview download timed out. Tap to retry.", 512 MiB cap → "This source is too large for a quick preview...") → **`ready`** / **`failed`** (error + `⟳ Retry`, which bypasses the window; failures inside a session skip onward to next recommendation). Pipeline lives in `previews.py` and **never touches the library `Downloader`'s single job slot**, so previews run while real downloads are in flight; a generation hook cancels in-flight work on toggle-off, handoff, or new seed (cancelled downloads fail silently — no error row). Preview download failures also surface through `engine`'s `_on_playback_failed` hook: row marked failed, file and queue item removed, engine's automatic skip resumes next item.

**Tap-to-preview sessions (mobile `startPreview`, non-Discover):** tapping ANY row starts a temporary preview session at that recommendation — tapped row prepared on demand (adopting cached asset instantly), then **jumps into playback immediately** (mobile `playSequential`; a failed first item ends the session without hijacking playback — mobile `finishEmptyPreviewSession`). Session window is the tapped row + next five (`_discover_window_start/end`), keeps rolling window of at most five prepared ahead of playback (`_discover_consumed` + `DISCOVER_IMMEDIATE_COUNT`), and physical queue captured on entry. **Next/Previous route through active sessions** (`_user_next/_user_prev` on transport buttons, XF86 keys, and media keys): non-Discover previews — Next leaves the block and plays the seed's physical successor, Previous returns to the seed, both tearing the session down (mobile skipToNext/skipToPrevious); Discover — Next walks into the queued block (first press during preparation arms the auto-advance — mobile `advanceToDeferredRecommendation`; at a preparation gap the window re-anchors right behind the current preview; at the end it falls back to the player), Previous walks the block or restarts the window one row back. Tapping another row mid-session restarts the session at that row, dropping unplayed previews from the queue but keeping staged assets for the new window.

**Discover mode ("Start discover mode", `_discover_set`):** captures the physical queue, **adopts every ready staged file into the queue behind the current track** (mobile asset adoption — no re-download) resuming the prefetch's progress, then keeps the rolling window of five; the block autoplays when the seed ends (`engine.next_track` walks it naturally) and never self-starts. Toggling off cancels pending previews, deletes staged audio (deferring the still-playing file), and restores the captured queue — a playing recommendation finishes before the seed's successor (mobile `endAfterCurrentRecommendation`); playback past the block rebuilds at the current position; a replaced queue is left alone. Playback leaving the block ends the session itself (mobile natural-handoff parity).

**Discover infinite scroll:** when playback reaches the last loaded recommendation and the list is not exhausted, `_discover_maybe_auto_extend` triggers the next page automatically ("Load 25 more" grows the chain via `_discover_append_recs`; paging is a no-op during non-Discover sessions, and refresh is disabled during any session).

**Seed stability:** while a session is active the song page keeps the section pinned to the session's seed song (`_discover_session_seed`), so transient preview tracks never re-seed or re-fetch the list; a different song's fresh list ends the old session.

**Saving a preview ("＋ Add to Library" on any row):** opens the searchable playlist picker and follows the mobile `saveToPlaylist` chain:
- Already saved → only a fast local copy into the chosen playlist ("Added <title> to <playlist>", no re-download)
- Staged preview → threaded copy with **"Saving to playlist..."** state, then permanent **"Saved to library"** (the staged copy stays until session cleanup so the queue entry keeps working)
- Nothing staged → explicit destination + **"Download queued for playlist"** state that flips to permanent "Saved to library" or "Save failed: <msg>" when the job completes (`_discover_download_observed`). The destination is always chosen explicitly — previews are never dumped into Unsorted silently.

## File Registry & Responsibilities

### Core Support
- **`theme.py`**: The design system. Defines full color palette (`BG`, `BG2`, `BG3`, `BG4`, `SIDEBAR`, `PLAYER_BG`, `GREEN`, `TEXT`, `TEXT2`, `TEXT3`, etc.), `PLAYLIST_GRADIENTS` list (10 curated pairs), and `playlist_gradient(name)` function which deterministically maps a playlist name to a gradient pair via MD5 hashing.
- **`widgets.py`**: Custom Canvas components: `RoundButton`, `SlimSlider` (non-destructive redraw — track/fill/thumb items created once and moved with `coords()`/`itemconfig()` on scrub ticks instead of `delete("all")`+recreate, which leaks Tk canvas-item memory on the 400ms scrub heartbeat), `PlaylistArt` (Spotify-style dynamic 4-quadrant thumbnails via `ArtCache`, with performance-optimized rendering — see Protocols), `Spinner` (animated green indeterminate loading arc — mobile CircularProgressIndicator parity; self-cancelling `after` loop that pauses while unmapped and cancels on destroy), and **`ModernScrollbar`**.
- **`engine.py`**: The audio engine and supporting services. Contains:
  - `LunoEngine`: Wraps `pygame.mixer`. Manages `queue` (list of paths), `queue_idx`, `_original_queue` (shuffle restore), `shuffle`, `repeat`, `crossfade_secs`. Methods: `play_with_context()`, `play_current()`, `toggle_play()`, `next_track()`, `prev_track()`, `toggle_shuffle()`, `on_scrub_moved()`, `on_volume_moved()`, `get_position()` (canonical mixer-clock position with monotonic fallback), `_poll()`. Uses `pygame.mixer.music.get_pos()` as the preferred live playback clock, maintains seek/base state for absolute elapsed time after scrubbing, uses `time.monotonic()` only as a safe fallback, probes duration with `mutagen.File`, reschedules heartbeat unconditionally after completion paths, and keeps get_busy mixer-idle detection for unknown-duration tracks. Playback-load failures invoke the controller's `_on_playback_failed(path)` hook before the automatic skip so failed preview rows are marked and their files removed (mobile recoverFromPlaybackError parity).
  - `DiscoveryService`: Last.fm API client. Two-stage: `track.getSimilar` → `artist.getTopTracks` fallback. Cache limited to 10 entries, **keyed by (seed, limit)** — mobile parity so "Load 25 more" grows the cached list instead of returning the earlier smaller page. Results carry the largest usable Last.fm image URL (mega→extralarge→large→medium→small, https-normalized, default/noimage placeholders rejected — mobile `pickLargestImage` parity).
  - `scan_library(roots)`: Walks directories, returns `dict[playlist → paths]`.
  - `find_duplicates(library, allowed_keys)`: Groups tracks by `_normalize_for_dupe`.
  - `probe_soundcloud(query)`, `parse_exportify_csv(filepath)`: Supporting utilities.
- **`previews.py`**: The temporary recommendation preview pipeline. `resolve_preview_source(artist, title)` probes `ytsearch5:` with `extract_flat` and picks the first candidate with a 1s–3h duration (else the first with unknown duration) — used under a hard 15s controller deadline. `download_preview(...)` streams the resolved source through FFmpeg to a 44.1 kHz / 192k MP3 **without loudnorm** (transient previews mirror the mobile raw-stream path), embeds the thumbnail, enforces a cooperative 5-minute deadline and a 512 MiB size cap. The preview pipeline never uses the library `Downloader` single job slot, so preview chains run while real downloads are in flight.
- **`video_preview.py`**: Optional Song Page video support. Resolves a YouTube result and downloads a merged temporary MP4 into `PREVIEW_DIR`; never adds video to `_library` or the audio queue. The Song Page uses system FFmpeg to decode frames into Tk and degrades to album art when the binary or source is unavailable.
- **`rec_artwork.py`**: Recommendation row artwork. `find_artwork(artist, title)` runs the cached Deezer cover search (`api.deezer.com/search` with **quoted-term queries** `'"artist" "title"'` plus a plain-text fallback — Deezer's `artist:"X" track:"Y"` field query returns no results), preferring cover_xl→big→medium, with nulls cached too. `render_design(index, size)` draws one of **75 deterministic abstract designs** (index = `crc32(normalized key) mod 75` — stable across processes) used for loading/missing/error row states; `load_image(url)` downloads and center-crops a square thumbnail.
- **`library_transfer.py`**: Portable transfer contract. Provides `build_manifest()`, `validate_manifest()`, `load_manifest()`, `write_manifest()`, `plan_import()`, and `apply_import_plan()` for the path-free v1 JSON format used by Android.

### View Modules (`views/`)
- **`library.py`**: Virtual scrolling canvas (56px rows). Header displays playlist title, dynamic album art (via `PlaylistArt`), and a playlist-settings cog that opens playlist URL, sync, duplicate, and transfer actions. Includes an **in-playlist search bar** (`Filter tracks…`) using a `tk.Label` overlay with `rely=0.5` vertical centering and a trailing clear-X, plus a compact mobile-parity `Sort ▾` control at the far right whose menu offers (`A-Z`, `Z-A`, `Recently added`, `Duration`) shared by All Music and individual playlists. Get Similar Tracks sits immediately to its left. Unsorted defaults to newest-first. The five action-row controls use equal flexible spacer columns equivalent to `justify-content: space-between`; Shuffle, Favorites first, sort, and Get Similar Tracks use the accent green, and active shuffle is shown with a bold underline. Track text is strictly truncated dynamically (`char_w = 0.85`) to ensure long titles never overflow (`w - 340` boundary). Each row has an outlined/filled favorite star about 16 px before its playlist name; clicking it toggles the track without changing selection, while track context menus retain favorite actions. The playing row's green equalizer bars are tagged `pulse` and animated in place by `_update_pulse` (`coords()`), so the 80 ms heartbeat never triggers a full `_redraw_track_list()`. The Library sidebar header returns to All Music, Unsorted navigation resets the virtual list to its first row, and Unsorted shows the subtitle `songs yet to find a home`. The reserved virtual Favorites playlist supports normal play/shuffle context and favorite-first ordering. The Get Similar Tracks action opens Discover after starting discovery for the current or selected track. Playlist actions export/import the portable manifest.
- **`downloader.py`**: **Scrollable canvas download workspace** following the native mobile download layout: a compact header/status row, source tabs, dark input/action rows, artwork-backed YouTube/SoundCloud search cards, a Direct URL form, and an expandable Downloaded songs section. The desktop-only Sync, URL Queue, Duplicates, and Allowed Dups workflows remain available as additional tabs; their existing tree selection, batch actions, and portable duplicate backups are preserved. Uses a dynamic `_search_limit` state to enable **Load More** functionality in YouTube/SoundCloud search. The **⚠ Duplicates** tab features **Playlist** and **Filename** columns. Action buttons support **bulk-exclusion** of entire duplicate sets; **double-clicking a group header** plays the first track. The **✓ Allowed Dups** tab includes **💾 Export** and **📂 Import** buttons for portable JSON backups of exclusions.
- **`recent.py`**: Recently Played view. Gradient header with "Recently Played" title. Displays `controller._recently_played` (max 100) in a virtual scrolling canvas (56px rows). Multi-selection via Shift+Click / Cmd+Click, context menu integration including selected JSON export, Edit Track Info, "Play All", and "Clear History" actions.
- **`settings.py`**: Premium Spotify-style UI with clean section headers, borderless inputs, and consistent padding. Includes step-by-step instructions for Last.fm API setup, manages storage paths, playback options, and Discovery keys, and exposes full-library JSON export/import.
- **`discover.py`**: Discovery Hub view. Displays recommended tracks from Last.fm based on current listening; features one-click "Add to Library" download integration. Logic includes **Context-Aware Shuffle**: Shuffling now respects the user's current view (All Music, Recently Played) instead of being limited to physical disk folders. The controller passes a flattened `context_list` to the engine, allowing global randomization.
- **`song_page.py`**: Premium Song Detail view. Displayed when clicking the player bar track info. Mimics Spotify's "Now Playing" aesthetic with a glassmorphism background (blurred album art overlay) and integrated controls (no back/✕ button — the nav tabs and mini-player already leave the page). Implements a unified, centered, stacked layout across all screen sizes with a large centerpiece artwork (up to 800x800) that dynamically scales based on available window dimensions. Blur generation uses a single-worker executor with generation checks so resize/path changes cannot apply stale artwork or accumulate workers. **Mobile full-player parity (2026-08-11):** the whole page is one scrollable column — artwork, metadata, favorite, and the scrollable **"Recommended for this song"** section share a page-level canvas (centered when content fits the viewport, scrollable when it overflows; artwork reserves ~230px of the first viewport for the section header + first rows). The section is seeded from the current track (artist fallback = queue playlist/folder name) and stays pinned to that seed while a preview session is active (`_discover_session_seed`), so transient preview tracks never re-seed the list; it has honest states (no track / no API key / "Finding similar songs..." / error + Retry / "No recommendations available for this song"), pages of 25 with **Load 25 more** up to 1,000 (exhausted detection = no new tracks on a page, load-more failures keep the list and surface a retryable footer error, scroll position preserved across load-more, auto-triggered by Discover at the last loaded recommendation), a **⟳ refresh** icon (disabled during sessions), a green **"⬇ Scroll for recommendations"** hint, a **Start discover mode / Discover mode: On** toggle, and mobile-parity header subtitles ("Temporary preview queue - tap a song to restart from there" / "Temporary preview - Next returns to your library" / "<n> similar songs - tap to preview"). The displayed artwork survives ca...

- **`error_log.py`**: Debug log viewer. Displays `controller._error_logs` (list of strings) in a read-only `tk.Text` widget with `ModernScrollbar`. "Clear Logs" button resets the list.

## Binary Dependencies
- `yt-dlp`: Metadata extraction and downloading; installed by `requirements.txt` into `.venv`.
- `ffmpeg`: Essential for audio conversion, resampling, and embedding metadata.
- `pyinstaller`: macOS desktop app packaging; installed by `requirements.txt` into `.venv`.
- `pygame`: Playback backend (`pygame.mixer`); installed by `requirements.txt` into `.venv` and bundled by PyInstaller.
- `mutagen`: Generic audio metadata/duration probing for playback (`mutagen.File`) plus ID3 artwork/tag handling.
- `Pillow`: Dynamic rendering/resizing of embedded ID3 artwork and YouTube thumbnail conversion; installed by `requirements.txt` into `.venv` and bundled by PyInstaller.
- `requests`: HTTP client for Deezer API, Last.fm API, and YouTube thumbnail fetching; installed by `requirements.txt` into `.venv`.

## Repository Hygiene
- The repository tracks source/configuration only. Do not commit local environments, generated packages, bytecode, test caches, or macOS metadata.
- Root `.gitignore` intentionally covers `.venv/`, `venv/`, `env/`, `build/`, `dist/`, `__pycache__/`, `*.py[cod]`, `.pytest_cache/`, and `.DS_Store`.
- If generated artifacts are accidentally tracked, remove them from the index with `git rm -r --cached` when the local files should be preserved, or `git rm -r` when they should be removed from disk too.
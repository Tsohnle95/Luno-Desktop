# Frontend (Shell, Views, Projections)

Document role: canonical owner for the React frontend: shell/routing,
route views, reusable components, and frontend state projections. It owns
what the UI renders and how it talks to the backend — never backend
policy.

## What this owns

- `Shell.tsx` route selection and command orchestration.
- Route-level views in `src/views/` and their unique responsibilities.
- Shared components, artwork fetching/caching, and the 56 px virtual-list
  contract.
- Frontend projections over backend snapshots and bounded caches.

What it does not own: command semantics and queue/scan/job policy (Rust);
envelope shapes (`docs/modules/contracts.md`).

## Entry points

- `src/main.tsx` — app bootstrap.
- `src/app/Shell.tsx` — route state (`library` default), settings overlay,
  download queue orchestration, discover/refresh effects, video element
  wiring. `ROUTES`, `routeLabels`.
- `src/app/bridge.ts` — command sender + `backend-event` subscriber (see
  contracts doc).
- `src/app/videoProbe.tsx`, `gate2Smoke.tsx` … `gate5Smoke.tsx` — smoke
  probes, not production UI paths.
- `src/views/` — `HomeView`, `LibraryView` (+ `libraryPlaylists`,
  `libraryAlbums`, `playlistTracks` helpers), `RecentView`, `SongPageView`,
  `DiscoverView`, `DownloaderView`, `PlaylistSyncReviewView`, `SettingsView`,
  `ErrorLogView`.
- `src/components/` — `PlayerBar`, `Queue`, `TrackRow`, `Artwork`,
  `RecommendationArtwork`, `PlaylistArtwork`, `Recommendations`, `DownloadTabs`,
  `PlaylistDialogs`, `PlaylistPicker`, `EditTrackDialog`, `TransferDialogs`,
  `VideoStage`, `icons`.
- `src/state/` — `backend_projection.ts` (snapshot store + subscriptions),
  `ui_state.ts`, `play_stats.ts`, `track_artwork.ts` (bounded LRU by count
  and bytes; evicted entries retry lazily, never eagerly).
- `src/styles/` — `tokens.css`, `layout.css`.

## Invariants

- Views render backend snapshots; they must not reimplement queue order,
  scan results, or job state. All mutations go through bridge commands.
- The shell topbar has no Back/Forward chevrons. Playlist creation is
  available directly from the Your Library heading; the Playlists/Albums,
  filter, and list areas keep clear vertical spacing.
- Heavy work stays off the UI thread: artwork arrives via the
  `track_artwork` command as bounded data URLs; visible library rows request
  missing durations through `track_duration`, which probes on a blocking
  worker instead of extending every library scan.
- UI mutations from backend events are scheduled via `.after(0, callback)`.
- Virtual-list rows are 56 px with scroll-region clamping where the
  library list applies — preserve both when touching list rendering.
- Song recommendations follow the audible library seed so the first temporary
  window can prepare before the song page opens. Discover owns that work while
  enabled; otherwise the prefetch effect is idempotent for the current seed.
  Retryable backend task-capacity errors keep retrying with backoff while that
  seed remains current; an active preview projection always has a backend
  completion driver.
- The song page, player bar, queue, and video correlation follow Discover's
  projected selection while that mode is enabled, including a pending item;
  otherwise `PlaybackSnapshot.audible_track_id` identifies a sounding focused
  recommendation even after the physical queue is restored. A focused preview
  replaces only the visible current queue row;
  its successors remain the captured physical context. Discover queue rows
  resolve recommendation keys and keep placeholder slots for unresolvable ids
  so row indices stay aligned with the backend queue.
- Video staging requests use that same audible identity, including focused
  recommendation keys. Embedded artwork is requested for the displayed
  library track, including when the physical queue points elsewhere; the
  resolved image feeds both the backdrop and media stage. Missing artwork
  renders the same deterministic custom design, which stays visible until a
  staged video decodes its first frame.
- Recommendation rows select through `discover_select` while Discover is
  enabled and through focused preview otherwise. Their Add to playlist action
  remains available while audio is pending or unavailable; the backend reuses
  a prepared file or starts the durable playlist download path. When a
  selected recommendation is stopped, the footer Play control retries it
  through the owning Discover or focused-preview command.
- Playlist selection marks destinations that already contain any selected
  track, matching by stable ID or normalized artist/title so physical copies
  are recognized. Playlist destination fields in the add dialog and Downloader
  both filter by playlist name. Sidebar, Unsorted, popular-playlist, and picker
  covers share a four-track artwork mosaic.
- Downloader search remains usable during transfers. Each clicked result
  captures its source, metadata, and chosen destination before another search
  can replace the results; up to three jobs run concurrently, each with its own
  progress and cancellation control. Fresh sessions default to Unsorted.
- "Recently added" sorts by `Track.added_ms` (portable first-added time,
  seeded from file mtime for existing files), not queue order; the library
  table shows that timestamp as Date added and derives playlist names from
  each playlist's ordered track IDs.
- The library table probes duration for virtualized visible rows and displays
  results in the Duration column. These reads do not change track identity or
  persist metadata. The library hero keeps Play and Shuffle controls without
  the former scan icon or selected-count badge.
- The transient unavailable-playlist notice can be dismissed independently of
  the persistent Settings → Sync Review destination.
- Playlist URL editing loads the saved source list before mounting the form and
  submits the full list together, so adding or removing one link cannot
  overwrite another link with stale UI state.
- Video element behavior is CSS-driven; fullscreen toggles must not call
  `window.setFullscreen()` (blacks out WKWebView over the hide
  transition).
- Tauri's WKWebView implements no native `window.confirm`/`window.prompt`
  (they returned false/undefined and silently dropped deletes and playlist
  creation), so confirmations and name entry use the in-app
  `ConfirmDialog`/`PlaylistDialog`. `.luno-dialog*` styles own the fixed
  modal backdrop.
- The track context menu is available from every track surface that shows a
  real library track: library/recent rows, the queue, home cards, and the
  footer now-playing track. Edit/delete are hidden for non-library
  recommendation rows (they have no library file).
- The Queue tab shows the authoritative current track followed only by its
  upcoming queue rows. Earlier plays are shown in Recently played, which is
  driven by backend TrackStarted events after output actually starts.
- A physical playlist's context menu owns the YouTube link editor,
  `Sync playlist`, `Review unavailable songs`, and `Log` actions. Sync stays
  disabled until a link is present; React sends only the playlist ID, then
  reflects backend-owned job progress, completion, cancellation, and errors.
  The Downloader Sync tab lists linked physical playlists; its `Save to`
  destination selects the playlist to sync through the same command and into
  the same folder. Both sync actions first try playlist lookup without browser
  cookies. If YouTube reports an account-access error, the status strip offers
  browser selection and retry. Playlist sync jobs never use the single-download Retry
  action. Active sync status names the current song until the job ends. If unresolved
  entries remain after a completed sync, the status offers a review action
  instead of claiming the playlist is up to date. The review dialog shows the
  original title, channel, duration, and unavailable reason, then displays
  local library title matches plus YouTube candidate title/channel/duration as
  review cues. A user can explicitly map a title match as the local
  replacement, dismiss that unavailable entry, retry the original, or choose
  another YouTube source. Local replacement and dismissal state persists per
  playlist; replacement paths are rechecked so deleted files become reviewable
  again. Choosing a YouTube candidate downloads it immediately through
  the normal single-download pipeline into the linked playlist folder; the
  selected source remains available for an explicit sync retry if that
  download fails. A successful file refreshes the issue list and clears the
  resolved original entry. Settings → Sync Review provides a persistent
  playlist list and opens this same dialog, even after the transient sync
  notice has gone away. Log shows only dated successful sync additions; a
  separate error view shows backend-owned sync failure details. React does not
  enumerate videos or decide which playlist entries are already satisfied.

## Core flows

```text
backend-event → backend_projection.apply → subscribers → view re-render
track visible → bridge.trackArtwork(id) → data URL → bounded LRU
download click → captured source/destination → bounded Downloader queue
→ bridge.startDownload* → per-job events/progress/cancel → coalesced rescan
→ library snapshot
playlist context menu → set_playlist_url / sync_playlist → sync job events
→ status notice + refreshed library snapshot
```

## Tests

- `src/test/views/` — per-view tests (`library`, `downloader`,
  `settings`, `shell`, `song_page`, `video_stage`).
- `src/test/dialogs.test.tsx`, `play_stats.test.ts`,
  `artwork_eviction.test.ts`, gate smoke tests.
- `src/test/accessibility/u44-contract.ts`, `src/test/visual/u44-geometry.ts`
  — geometry/contract helpers.

## Related canonical docs

- `docs/modules/contracts.md` — envelopes and bridge validators.
- `docs/modules/library.md` — what library snapshots guarantee.
- `docs/modules/playback.md` — video/queue state the UI reflects.

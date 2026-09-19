# Frontend (Shell, Views, Projections)

Document role: canonical owner for the React frontend: shell/routing,
route views, reusable components, and frontend state projections. It owns
what the UI renders and how it talks to the backend — never backend
policy.

## What this owns

- `Shell.tsx` routing, navigation history, and command orchestration.
- Route-level views in `src/views/` and their unique responsibilities.
- Shared components, artwork fetching/caching, and the 56 px virtual-list
  contract.
- Frontend projections over backend snapshots and bounded caches.

What it does not own: command semantics and queue/scan/job policy (Rust);
envelope shapes (`docs/modules/contracts.md`).

## Entry points

- `src/main.tsx` — app bootstrap.
- `src/app/Shell.tsx` — route stack (`library` default), settings overlay,
  download queue orchestration, discover/refresh effects, video element
  wiring. `ROUTES`, `routeLabels`.
- `src/app/bridge.ts` — command sender + `backend-event` subscriber (see
  contracts doc).
- `src/app/videoProbe.tsx`, `gate2Smoke.tsx` … `gate5Smoke.tsx` — smoke
  probes, not production UI paths.
- `src/views/` — `HomeView`, `LibraryView` (+ `libraryPlaylists`,
  `libraryAlbums`, `playlistTracks` helpers), `RecentView`, `SongPageView`,
  `DiscoverView`, `DownloaderView`, `SettingsView`, `ErrorLogView`.
- `src/components/` — `PlayerBar`, `Queue`, `TrackRow`, `Artwork`,
  `RecommendationArtwork`, `Recommendations`, `DownloadTabs`,
  `PlaylistDialogs`, `PlaylistPicker`, `EditTrackDialog`, `TransferDialogs`,
  `VideoStage`, `icons`.
- `src/state/` — `backend_projection.ts` (snapshot store + subscriptions),
  `ui_state.ts`, `play_stats.ts`, `track_artwork.ts` (bounded LRU by count
  and bytes; evicted entries retry lazily, never eagerly).
- `src/styles/` — `tokens.css`, `layout.css`.

## Invariants

- Views render backend snapshots; they must not reimplement queue order,
  scan results, or job state. All mutations go through bridge commands.
- Heavy work stays off the UI thread: artwork arrives via the
  `track_artwork` command as bounded data URLs; rows request lazily.
- UI mutations from backend events are scheduled via `.after(0, callback)`.
- Virtual-list rows are 56 px with scroll-region clamping where the
  library list applies — preserve both when touching list rendering.
- Song recommendations follow the audible library seed so the first temporary
  window can prepare before the song page opens. Discover owns that work while
  enabled; otherwise the prefetch effect is idempotent for the current seed.
- The song page, player bar, queue, and video correlation follow the AUDIBLE
  track: during a focused preview or discover-mode recommendation the shell
  passes that recommendation as `nowPlaying` instead of the library track still
  behind it. A focused preview replaces only the visible current queue row;
  its successors remain the captured physical context. Discover queue rows
  resolve recommendation keys and keep placeholder slots for unresolvable ids
  so row indices stay aligned with the backend queue.
- Recommendation rows select through `discover_select` while Discover is
  enabled and through focused preview otherwise. Their Add to playlist action
  remains available while audio is pending or unavailable; the backend reuses
  a prepared file or starts the durable playlist download path.
- "Recently added" sorts by `Track.added_ms` (file mtime), not queue order.
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

## Core flows

```text
backend-event → backend_projection.apply → subscribers → view re-render
track visible → bridge.trackArtwork(id) → data URL → bounded LRU
download click → DownloaderView queue → bridge.startDownload* → job events
→ progress render → coalesced rescan → library snapshot
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

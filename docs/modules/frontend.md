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
- Home's listening-count display and one-time import of the former browser
  play-count journal; authoritative counts and playlist attribution remain
  backend-owned.

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
  `ArtworkManager`, `ErrorLogView`.
- `src/components/` — `PlayerBar`, `Queue`, `LibrarySidebar`, `TrackRow`,
  `Artwork`, `RecommendationArtwork`, `PlaylistArtwork`, `Recommendations`, `DownloadTabs`,
  `PlaylistDialogs`, `PlaylistPicker`, `EditTrackDialog`, `TransferDialogs`,
  `VideoStage`, `icons`.
- `src/state/` — `backend_projection.ts` (snapshot store + subscriptions),
  `ui_state.ts`, `play_stats.ts`, `track_artwork.ts` (bounded LRU by count
  and bytes; evicted entries retry lazily, never eagerly).
- `src/state/appearance.ts` — renderer appearance preference, persisted in
  localStorage and applied to the document before the first React render.
- `src/styles/` — `tokens.css`, `layout.css`.

## Invariants

- Settings → Appearance offers Green (the default, `#1db954`) and Orange
  (`rgb(250, 141, 118)`). Selection immediately updates shared accent tokens
  across controls, focus rings, and selection highlights, and survives relaunch.
  This visual preference is renderer-owned and does not alter backend settings
  or library transfer data. The orange application icon is independent of the
  selected accent; playlist artwork retains its own colour palette.
- Views render backend snapshots; they must not reimplement queue order,
  scan results, or job state. All mutations go through bridge commands.
- The shell topbar places Back/Forward chevrons beside the native macOS
  traffic lights. History restores the route, library scope, Home/collection
  selection, search text, and Settings section; a new destination discards
  forward history. Search keystrokes do not create navigation entries.
  Playlist creation is available directly from the Your Library heading;
  the Playlists/Albums, filter, and list areas keep clear vertical spacing.
- The memoized Your Library sidebar owns its filter and category state, so
  opening, closing, or typing in its filter does not rerender route content.
  The filter stays mounted for a short opacity/height transition, and its
  pointer-outside dismissal clears the query.
- All Music, Favorites, Unsorted, and physical playlist entries use the same
  four-cell split-artwork tile; one to three tracks repeat to fill equal quadrants,
  independently of source image dimensions or loading/fallback state. Recently
  downloaded uses this same tile in its sidebar and collection hero,
  with the normal icon when no artwork tracks
  are available. The sidebar filter closes and clears when a pointer lands
  outside both the filter and its trigger.
- The home dashboard orders its sections as Made for you, Recently played,
  Most popular songs, Favorite songs, Most popular playlists, Favorite artists,
  and Recently
  downloaded. The download section ranks library tracks by their first-added
  timestamp and overlays local successful-download timestamps when available.
  Its Show all action opens the derived Recently downloaded collection in
  Library, also pinned in the sidebar; it does not create a physical playlist.
  Favorite artists are derived from artists with favorited songs, grouped
  case-insensitively and ranked by favorite-song count then successful starts;
  each artist opens all their library songs. Show all opens the matching
  collection for Made for you, Favorite songs, Most popular songs, and the
  full popular-playlist/artist grids. Song collections reuse the library
  table and preserve their ranking until a sort header is selected. Full
  playlist/artist grids request artwork for cards near the viewport, releasing
  demand as cards leave it; overview artwork shares do not truncate these
  collections. The song search leaves card grids for All Music and continues
  filtering within song-list collections.
  Popular song cards show backend successful-start counts.
  Playlist counts include only starts attributed to that playlist; track
  counts are never summed into every playlist containing a song. The old
  localStorage counts are imported once and removed after the backend save.
- Heavy work stays off the UI thread: artwork arrives via the
  `track_artwork` command as bounded data URLs; Shell keeps a low-priority,
  bounded-batch warmup active for library tracks, while audible and visible
  requests preempt queued warmup work. Missing durations use the blocking
  `track_duration` command instead of extending every library scan.
- The Settings rail includes Home, Downloader, Artwork, and Sync Review
  shortcuts. Downloader embeds the same `DownloaderView` used by its main
  route, so its search, queue, sync, and destination controls share the same
  state and behavior.
- ArtworkManager scans the selected local collection on entry and lists every
  track without embedded artwork. Search filters that list and can scan only
  matching tracks in the selected scope; provider lookup remains user-triggered
  for one selected track, supports title-only searches, and requires review
  before applying MusicBrainz/Cover Art Archive covers or YouTube video
  thumbnails. YouTube thumbnails are labeled as such because they may not be
  album art. The playlist hero banner menu opens the manager at its scope.
- Favorite-only events update the matching projected track without advancing
  the scan generation, so visible rows keep valid duration probe results.
  Track maps depend on the current track array so favorites stay current in
  queue, footer, and home projections even when the scan epoch is unchanged.
- Artwork requests hold per-view demand leases. Scrolling or unmounting drops
  queued requests no longer needed by that view; shared IDs stay queued while
  any view still needs them, and an in-flight extraction remains shared.
  Audible-track art outranks virtualized library rows, which outrank static
  sidebar art, which outrank the low-priority library warmup. Six requests may
  run at once, with at most two warmup requests active. The process LRU stays
  bounded; the Rust cache also stores downscaled results in the app cache
  (512 MiB cap) so evicted and later-scrolled art does not need to be extracted
  again. A cold item stays on a neutral tile until its first extraction ends;
  deterministic custom artwork appears only after a missing or failed result
  is known. Rendered virtual rows load resolved images without browser lazy
  loading delay.
- UI mutations from backend events are scheduled via `.after(0, callback)`.
- Virtual-list rows are 56 px with scroll-region clamping where the
  library list applies — preserve both when touching list rendering.
- Song recommendations follow the audible library seed so the first temporary
  window can prepare before the song page opens. Discover owns that work while
  enabled; otherwise the prefetch effect is idempotent for the current seed.
  Retryable backend task-capacity errors keep retrying with backoff while that
  seed remains current; an active preview projection always has a backend
  completion driver.
- Temporary recommendation IDs mirror Rust's `safe_key` over Unicode code
  points. The shell resolves queue and audible IDs back to current
  recommendation metadata before rendering the footer or song page. Metadata
  for queued, audible, and active preview/Discover window IDs survives lookup
  refreshes, failures, and seed changes until those IDs leave playback; fresh
  lookup results replace retained metadata. Only
  a matching local library track can be favorited; preview-only tracks become
  eligible after they are saved into the library.
- The song page, player bar, queue, and video correlation follow Discover's
  projected selection while that mode is enabled, including a pending item;
  otherwise `PlaybackSnapshot.audible_track_id` identifies a sounding focused
  recommendation even after the physical queue is restored. A focused preview
  replaces only the visible current queue row;
  its successors remain the captured physical context. Discover queue rows
  resolve recommendation keys and keep placeholder slots for unresolvable ids
  so row indices stay aligned with the backend queue.
- Video staging follows the backend's active selection owner during a pending
  transition: Discover's projected row, a focused preview, then the selected
  library queue row before the last confirmed audible ID. Embedded artwork is
  still requested for the displayed library track, including when the
  physical queue points elsewhere; the resolved image feeds the backdrop and
  media stage. Missing artwork renders the same deterministic custom design,
  which stays visible until a staged video decodes its first frame.
- The stage's upper-right Music mode switch is the song page's video activation
  control. Enabling it starts current-track staging; retry/error details remain
  in the hover controls. There is no second Download video button over artwork.
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
  progress and cancellation control. The selected destination is owned by
  Shell, survives route changes and library snapshots, and defaults to
  Unsorted for a fresh session.
- "Recently added" sorts by `Track.added_ms` (portable first-added time,
  seeded from file mtime for existing files), not queue order; the library
  table shows that timestamp as Date added and derives playlist names from
  each playlist's ordered track IDs.
- The library table sorts from clickable Title, Date added, and clock headers;
  each header toggles its ascending/descending order. Missing durations are
  probed across the active filtered scope when duration sorting is selected,
  with four probes in flight at once; resolved rows reorder immediately while
  pending results stay after them, and an accessible progress status reports
  completion. Ordinary browsing still probes only visible rows. A bounded
  session cache keeps results across route changes and library generations;
  these reads do not change track identity or persist metadata. The Shell
  retains each scope's sort choice while the user visits Downloader.
  The top-bar search filters the active library scope, with no duplicate search
  control in the playlist cover area. The library hero keeps Play and Shuffle
  controls without the former scan icon or selected-count badge.
- The transient unavailable-playlist notice can be dismissed independently of
  the persistent Settings → Sync Review destination.
- Playlist URL editing loads the saved source list before mounting the form and
  submits the full list together, so adding or removing one link cannot
  overwrite another link with stale UI state.
- Video element behavior is CSS-driven; fullscreen toggles must not call
  `window.setFullscreen()` (blacks out WKWebView over the hide
  transition).
- Discovery intent can stay on while recommendations are empty or loading.
  Playback retains its physical queue until matching recommendations arrive;
  toggling intent does not insert a status banner or change transport.
- The single-song route owns one vertical scroller inside its page host. The
  host clips overflow; only wheel input over a native video uses a
  frame-batched fallback to the page scroller.
- Tauri's WKWebView implements no native `window.confirm`/`window.prompt`
  (they returned false/undefined and silently dropped deletes and playlist
  creation), so confirmations and name entry use the in-app
  `ConfirmDialog`/`PlaylistDialog`. `.luno-dialog*` styles own the fixed
  modal backdrop.
- Track-copy, empty-playlist, and delete-playlist actions first use the normal
  confirmation. If backend review finds that the requested operation would
  remove a song's last byte-identical file copy, a higher-priority alert dialog
  lists the affected files and requires a second confirmation. Removing one
  physical copy while another identical copy remains proceeds without the
  second warning; confirmation is rechecked at deletion time.
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

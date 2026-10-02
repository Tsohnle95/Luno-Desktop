# Playback, Video, and Media Keys

Document role: canonical owner for the authoritative playback queue,
playback clock, audio backend, failure recovery, video staging pipeline,
and OS media-key integration. It owns everything about what is playing
and what plays next.

## What this owns

- Queue policy: order, shuffle, repeat, next/previous, mutations.
- Successful library playback-start counts, queue playlist origin, and durable
  listening-statistics publication.
- Playback clock, seek, volume seeding, crossfade, sleep timer.
- Audio backend lifecycle (macOS cpal/Symphonia 0.6) and failure recovery.
- Video mode, staging pipeline, prefetch, and the `luno-video://` stream.
- Hardware media keys and Now Playing integration.

What it does not own: library snapshots and track identity
(`docs/modules/library.md`); download/discovery jobs
(`docs/modules/downloads-discovery.md`).

## Entry points

- `src-tauri/src/playback/queue.rs` — `QueueState`: authoritative order,
  shuffle, repeat, index.
- `src-tauri/src/playback/statistics.rs` — listening counts persisted under
  the stable runtime home with atomic replacement.
- `src-tauri/src/playback/clock.rs` — `PlaybackClock`.
- `src-tauri/src/playback/backend.rs` — `AudioBackend` trait (+
  `FakeBackend` under test); `backend_adapter.rs` (`CpalSymphoniaBackend`,
  macOS production path).
- `src-tauri/src/playback/recovery.rs` — `FailureRecovery`, auto-skip on
  undecodable fixtures.
- `src-tauri/src/playback/crossfade.rs`, `sleep_timer.rs`.
- `src-tauri/src/video/` — `staging.rs` (`VideoStage`, index cache),
  `runtime.rs`, `player.rs`, `source.rs`, `stream.rs`
  (`STREAM_SCHEME`, ≤2 MB ranged responses), `player.rs`.
- `src-tauri/src/state.rs` — `set_playing`, `video_stage_track`,
  `kick_video_pipeline`, `prefetch_plan`, ownership guards
  (`completion_may_publish`), pressure gate
  (`VIDEO_STAGE_PRESSURE_LIMIT`, `VIDEO_PREFETCH_DEPTH`).
- `src-tauri/src/media_keys.rs`, `src-tauri/src/platform/media_session.rs`
  — MPRemoteCommandCenter claim, Settings ▸ Playback honors.
- `src/components/VideoStage.tsx`, `src/components/PlayerBar.tsx`,
  `src/components/Queue.tsx` — frontend consumers.

## State ownership

- `AppState` owns the playback queue, current index/track, playback
  status, volume, video mode, video slots (`video_current`), staging
  inflight markers, and prefetch plan. The frontend reflects; it never
  reorders.

## Invariants

- Skips walk the existing shuffled order — `Next`/`Previous` never
  rebuild or re-shuffle. Toggling shuffle off restores true playlist order.
- Focused recommendation playback captures the physical queue. With Discover
  off, Previous/Next and natural completion restore that queue and move through
  its physical order even while a recommendation is audible. Prepared
  recommendation audio remains available.
- Discover owns a temporary recommendation projection while enabled. The
  projection shows the complete recommendation plan in order while preparation
  stays limited to the five-item rolling window. Pending rows are valid
  transport targets, and Previous/Next plus natural completion walk the
  recommendation order. Turning shuffle off during Discover keeps the mode
  active and resynchronizes the same projected queue. Toggling Discover changes
  queue policy only: it keeps the backend stream, playback clock, and audible
  item running. Disabling restores the exact captured physical queue and index
  while the recommendation continues; later Previous/Next and natural
  completion follow the physical queue.
- The backend `PlaybackSnapshot.current_index` is authoritative; the frontend
  never re-derives it from `track_started` (duplicate ids made first-occurrence
  `indexOf` regress the queue panel). `audible_track_id` is separately
  authoritative for the sounding item when it is outside the physical queue.
- Each async audio start and heartbeat carries a playback-owner generation.
  Selecting a queue item claims the new owner before replacing the queue;
  stale load failures, EOF callbacks, and stall recovery cannot advance or
  publish against that replacement. File decoding stays outside the short
  ownership lock, and only the matching owner may start and publish audio.
- Focused and Discover recommendation IDs are temporary keys derived from the
  trimmed artist/title pair. Frontend key sanitization follows Rust's Unicode
  code-point `safe_key`, so queue IDs resolve to the matching recommendation
  metadata in every playback surface.
- An explicit library queue edit while Discover is active ends the temporary
  projection before applying the edit to the physical queue. Projected
  recommendation rows cannot be removed or reordered as physical tracks.
  Focused preview updates its captured physical queue after a library edit so
  later handoff keeps that edit. Late Discover completions cannot reapply a
  disabled projection over the edited queue.
- Removing the currently-playing queue row restarts (or stops) output so audio
  matches the newly-current track instead of leaving the removed song playing.
  That replacement retires any focused preview session so a late completion
  cannot restart the old recommendation.
- Single-instance guard: a localhost port claim (`LUNO_INSTANCE_PORT`
  override for QA) prevents two instances last-writer-winning shared
  config.
- Staging requests collapse: repeated requests for one track dedupe to a
  single cancellable waiter; a finishing worker publishes only into an
  empty slot, its own placeholder, or a slot nothing newer claimed.
- The video ID and quality-tier mapping lives in the music-root
  `.luno_video_index.json` sidecar, so it survives app rebuilds and follows a
  copied library. Downloaded MP4s remain session cache files and can be
  reacquired from the saved mapping.
- A `Loading` video state always has a live inflight worker. Registry pressure
  defers before publishing, and cancellation/store-wait exits settle their
  owned slot, so playback-load/heartbeat stay available without orphaned
  spinners.
- Video source lookup uses the fast public results path first, falls back to
  sequential yt-dlp queries, and stops after three strict candidates.
- Prefetch plans the audible item plus exactly five authoritative successors:
  physical successors for focused preview, projected successors for Discover.
- Current and successor downloads use a viewable H.264 quick tier: a
  progressive MP4 up to 480p when available, otherwise smaller 360p split
  streams before the 480p fallback. The current file publishes immediately,
  then a prioritized cancellable H.264 replacement up to 1080p swaps in place
  and republishes the video revision before speculative successor work resumes.
- Initial recommendation preparation owns quick-video assets for the same five
  temporary items as its audio window. Video and audio failures remain
  independent, and selecting an item adopts its prefetched video instead of
  downloading it again.
- App fullscreen is owned by an explicit shell state that collapses all grid
  chrome and lets the fixed video stage cover the complete viewport; Escape
  exits and controls appear through the existing hover layer.
- `null` progress covers source lookup, extractor setup, and transfers whose
  byte total is still unknown. Numeric progress is computed from yt-dlp's
  downloaded bytes over the selected video-plus-audio format plan; it stays
  monotonic, remains below 100 while the child/merge is active, and reaches
  100 only after the quick playable artifact is adopted.
- Pause never gates staging: videos stage and publish `Ready` while
  paused; resume runs a catch-up pipeline pass when current video is
  unresolved. Dead `Loading` slots (no live worker) restage.
- Song-page video staging starts on explicit user intent or while video mode
  is enabled. Enabling mode admits staging for the audible track at the
  command boundary and reports missing-helper or capacity failures to the UI;
  repeated requests coalesce with that track's live worker even under task
  pressure. Recommendation video preparation remains owned by the preview
  pipeline and is adopted independently of song-page video mode.
- During a track transition, video staging follows the active selection owner:
  Discover's projected current row, then a focused preview, then the selected
  library queue row, with the last confirmed audible ID as fallback. The
  playback pipeline updates audible identity before kicking video staging, so
  a previous track cannot start a competing lookup for the new selection.
- Master volume is shared atomically with the macOS output callback. Mute
  applies during a slow producer shutdown or track load, without waiting for
  the backend mutex, and survives each output-state replacement.
- The macOS decoder supports WebM audio with Vorbis or mono/stereo
  Opus alongside the existing M4A/AAC, MP3, FLAC, and WAV formats. It selects
  a registered audio codec from the container's tracks, skips video tracks,
  and keeps streaming output normalized to 44.1 kHz. Opus decoder state is
  reset after seeks; start-of-stream pre-skip is not applied again.
- Video responses are small ranged windows (stock asset protocol buffered
  whole files and froze the window on 100 MB+ MP4s).
- Media keys control Luno via MPRemoteCommandCenter instead of falling
  through to Apple Music; test-harness volume is zeroed to keep `cargo
  test` silent on the production audio path.
- `AppState::publish_playback` publishes truthful Now Playing metadata
  (title/artist/album/duration/elapsed) plus
  `MPNowPlayingInfoCenter.playbackState` (Playing/Paused) on every snapshot.
  A launch-only claim without `playbackState` loses key routing on current
  macOS; disabling Settings ▸ Playback releases the claim (Stopped + cleared
  info).
- Queue selections publish their cursor while audio loads, but enter recent
  history only after the backend starts output. Streaming EOF advances as soon
  as the decoded output buffer drains; the multi-second stall watchdog remains
  a recovery path for a dead stream, not normal track completion.
- Track counts increment at that successful audio-start edge: repeated starts
  count separately, while pause/resume and failed loads do not. A queue started
  with `play_from_playlist` retains that source through next and repeat; a
  playlist count increments only if the started local track is still a member.
  Counts are atomically saved in `.luno_play_statistics.json` under the runtime
  home and survive app rebuilds. Historical playlist-origin counts are not
  inferred from per-track totals.

## Core flows

```text
play_with_context → claim playback owner → healed queue → backend load
→ owner-checked start → clock → owner-checked heartbeat
→ auto-advance / repeat / failure-recovery skip
next/previous → index walk (no rebuild) → backend load → video pipeline kick
kick_video_pipeline → prefetch plan → dedupe/inflight → stage → Ready event
→ luno-video:// ranged stream → <video>
media key → MediaKeys → handle_media_key → playback command → dispatch
```

## Tests

- `src-tauri/src/gate3_tests.rs` — playback/queue semantics.
- `state.rs` unit tests — shuffle walk-back, staging pileup/pressure,
  pause-staging, corpse restage, resume kick, prefetch plan, stale-context
  healing.
- `src/test/gate3_smoke.test.ts`, `src/test/views/video_stage.test.tsx`.

## Related canonical docs

- `docs/modules/library.md` — track IDs the queue references.
- `docs/modules/frontend.md` — PlayerBar/Queue/VideoStage rendering.
- `docs/modules/contracts.md` — playback/video command envelopes.

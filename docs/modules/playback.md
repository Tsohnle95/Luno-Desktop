# Playback, Video, and Media Keys

Document role: canonical owner for the authoritative playback queue,
playback clock, audio backend, failure recovery, video staging pipeline,
and OS media-key integration. It owns everything about what is playing
and what plays next.

## What this owns

- Queue policy: order, shuffle, repeat, next/previous, mutations.
- Playback clock, seek, volume seeding, crossfade, sleep timer.
- Audio backend lifecycle (macOS cpal/symphonia) and failure recovery.
- Video mode, staging pipeline, prefetch, and the `luno-video://` stream.
- Hardware media keys and Now Playing integration.

What it does not own: library snapshots and track identity
(`docs/modules/library.md`); download/discovery jobs
(`docs/modules/downloads-discovery.md`).

## Entry points

- `src-tauri/src/playback/queue.rs` — `QueueState`: authoritative order,
  shuffle, repeat, index.
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
  projection anchors to the audible recommendation, pending rows are valid
  transport targets, and Previous/Next plus natural completion walk the
  recommendation order. Toggling Discover changes queue policy only: it keeps
  the backend stream, playback clock, and audible item running. Disabling
  restores the exact captured physical queue and index while the recommendation
  continues; later Previous/Next and natural completion follow the physical
  queue.
- The backend `PlaybackSnapshot.current_index` is authoritative; the frontend
  never re-derives it from `track_started` (duplicate ids made first-occurrence
  `indexOf` regress the queue panel).
- Removing the currently-playing queue row restarts (or stops) output so audio
  matches the newly-current track instead of leaving the removed song playing.
- Single-instance guard: a localhost port claim (`LUNO_INSTANCE_PORT`
  override for QA) prevents two instances last-writer-winning shared
  config.
- Staging requests collapse: repeated requests for one track dedupe to a
  single cancellable waiter; a finishing worker publishes only into an
  empty slot, its own placeholder, or a slot nothing newer claimed.
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
- Master volume is shared atomically with the macOS output callback. Mute
  applies during a slow producer shutdown or track load, without waiting for
  the backend mutex, and survives each output-state replacement.
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

## Core flows

```text
play_with_context → healed queue → backend load → clock → heartbeat
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

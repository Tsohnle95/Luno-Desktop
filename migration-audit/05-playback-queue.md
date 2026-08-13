# Playback and Queue Audit

This dossier audits the implemented Python desktop runtime and the native Android
playback reference. The migration target is not present in this repository; plan
items are labeled separately from source-backed behavior.

## Playback Start, Queue Context, and Up Next
- Status: implemented
- Evidence:
  - `engine.py:76-106` (desktop context queue replacement, current index, shuffle start)
  - `engine.py:44-59` (desktop queue/state fields)
  - `engine.py:238-283` (next/previous and playlist cycling)
  - `MusicController.kt:289-349,857-892` (Android request, atomic queue replacement, pending play)
  - `MusicController.kt:599-642` (Android Play Next/Add to Queue insertion semantics)
  - `test_engine_playback.py:257-283`
  - `MusicControllerTest.kt:49-203,364-425`
- User behavior: Starting a track establishes its playlist/selection context and starts it. Desktop Play Next inserts immediately after the current track; Add to Queue inserts after existing manual items and before the remaining context. Android exposes equivalent `playNext` and `addToQueue`, plus queue reordering and clearing.
- State and data flow: Desktop `LunoEngine.queue`, `queue_idx`, `_original_queue`, `user_queue_count`, and controller `_queue_playlist_paths` are the live state. Android `MediaController` owns the Media3 timeline; `MusicController` tracks manually queued URIs, exposes queue snapshots/revisions, and uses one immutable pending request before connection. Android queue replacement uses one `setMediaItems` operation to avoid transient timeline races.
- Persistence and compatibility: Queue state is runtime state on desktop. Android persists only the last current item/position, not the full queue (`MusicService.kt:171-210`); the shared library-export contract explicitly excludes queue state (`00-repository-census.md:128-135`).
- Async, lifecycle, and failure behavior: Desktop playback calls are UI-thread operations, while missing files and load failures schedule auto-skip with `.after(100, ...)` (`engine.py:119-129,180-191`). Android pending requests are last-user-request-wins and dispatched once after connection (`MusicController.kt:136-140,964-969`).
- UI states and visual requirements: Desktop queue redraws are triggered on context and queue changes (`engine.py:105,176`); Android queue state is represented by `queueRevision` and QueueSheet. Exact queue presentation is outside this behavioral audit.
- Migration implementation requirements: Preserve context replacement, current index, manual Up Next ordering, queue clear/reorder, and last-request-wins connection behavior. Do not import Android's Media3 timeline as a desktop persistence format.
- Verification requirements: Port the desktop queue tests and Android pending/queue tests to the migration conformance suite; add connected-player tests for actual insertion and reorder boundaries.
- Confidence: high
- Open questions: Android connected-player insertion/reorder behavior is source-backed but mostly tested through seams; full service/device verification is not committed.

## History
- Status: implemented
- Evidence:
  - `engine.py:168-174` (desktop most-recent-first history, deduplication, cap 100)
  - `music_player.py:736-739,4204-4212` (controller history and play/clear actions)
  - `MusicController.kt:172-205,1171-1183` (Android transition recording and cap)
  - `RecentlyPlayedStore.kt:15-18,60-98` (Android persistence)
  - `MusicControllerTest.kt:427-495`
- User behavior: Starting a non-transient track moves it to the front of Recently Played, removes an older occurrence, and retains at most 100 entries. Users can play all recent tracks and clear history.
- State and data flow: Desktop history is a controller list of local paths. Android history is `StateFlow<List<MediaTrack>>` backed by app-private SharedPreferences JSON; transient recommendation previews are excluded.
- Persistence and compatibility: Desktop history is in-memory for the session and is not written by `_save_config` (`music_player.py:4673-4684`). Android history survives process restart but is intentionally outside the shared manifest contract.
- Async, lifecycle, and failure behavior: Desktop history updates during successful `play_current`; Android records on Media3 item transition and during state hydration after reconnection.
- UI states and visual requirements: Desktop `views/recent.py` renders the list and supports context actions; Android QueueRecentsTabs/RecentsScreen are source-backed UI surfaces, with exact visual parity not audited here.
- Migration implementation requirements: Preserve path normalization/update-on-replay semantics and the desktop session-only compatibility behavior unless a deliberate persistence migration is approved. Preserve Android's transient exclusion only for Android preview state.
- Verification requirements: Test cap, replay-to-front, transient exclusion, persistence/reopen, clear, and play-in-context.
- Confidence: high
- Open questions: Whether the desktop migration should intentionally add durable history is a product/compatibility decision; current desktop source does not do so.

## Shuffle and Repeat
- Status: implemented
- Evidence:
  - `engine.py:193-213,238-277` (desktop shuffle enable/restore, cycle boundary guard)
  - `MusicController.kt:493-551,1077-1092` (Android repeat modes and shuffle randomization)
  - `test_engine_playback.py:257-283`
  - `MusicControllerTest.kt:64-87,364-388`
- User behavior: Desktop shuffle keeps the current track first, randomizes the rest, restores original order when disabled, and avoids an immediate repeat across a shuffled cycle boundary. Repeat restarts the current track at completion. Android cycles repeat Off → All → One and uses Media3 shuffle, randomizing to a different item when enabled.
- State and data flow: Desktop has boolean `shuffle` and `repeat`; Android exposes Media3 repeat/shuffle state flows and derives playback order from the Media3 timeline.
- Persistence and compatibility: Neither runtime persists shuffle/repeat mode in the shared export. Desktop config persistence does not include these modes (`music_player.py:4673-4684`).
- Async, lifecycle, and failure behavior: Desktop completion dispatch invokes repeat or next through the poll heartbeat. Android commands are guarded by `safePlayerCommand`, converting command exceptions to user-visible playback errors (`MusicController.kt:895-912`).
- UI states and visual requirements: Transport controls expose both modes on desktop and Android full player; active shuffle/repeat state is source-backed, while detailed styling belongs to the UI audit.
- Migration implementation requirements: Preserve desktop's deterministic current-first shuffle and restoration semantics, not merely a random boolean. Preserve Android finite preview behavior, which explicitly sets shuffle/repeat off (`MusicController.kt:329-339`).
- Verification requirements: Add conformance tests for current preservation, restoration, boundary repeat avoidance, repeat-one, repeat-all, and finite queue mode.
- Confidence: high
- Open questions: Android's exact repeat behavior at timeline edges should receive an instrumented Media3 test; unit coverage currently focuses on no-throw/request semantics.

## Seek, Clock, and Completion
- Status: implemented
- Evidence:
  - `engine.py:295-359` (canonical mixer clock, monotonic fallback, clamp, seek/pause handling)
  - `engine.py:367-399` (400 ms heartbeat, duration/idle completion latch)
  - `MusicController.kt:435-451,1062-1075` (bounded seek and 250 ms progress polling)
  - `MusicService.kt:171-210,250-259` (position persistence/restore)
  - `test_engine_playback.py:288-341,384-440,456-599`
- User behavior: Desktop scrubber seeks by percentage, preserves position through pause/resume, clamps to duration, and advances once when known duration is reached or the mixer becomes idle after a 0.5-second startup grace. Unknown-duration tracks can therefore complete. Android seeks with nonnegative, duration-bounded positions and exposes live progress through a 250 ms coroutine poll.
- State and data flow: Desktop prefers `pygame.mixer.music.get_pos()` while busy and falls back to monotonic elapsed time; `_playback_base_pos` and `_playback_base_mono` make seeks absolute. Android uses Media3 `currentPosition` and `duration`, with a persisted current position for service restoration.
- Persistence and compatibility: Desktop scrub position is not persisted. Android persists current URI, metadata, position, and play-when-ready in `playback_restore` preferences, excluding transient previews.
- Async, lifecycle, and failure behavior: Desktop `_poll` catches exceptions and unconditionally reschedules unless closing, with a generation latch preventing duplicate advance. Android progress jobs stop when not playing and are cancelled on release.
- UI states and visual requirements: Desktop updates both mini-player and Song Page scrubbers/time labels (`engine.py:331-342`); Android full player and MiniPlayer consume controller state flows.
- Migration implementation requirements: Preserve clock source precedence, duration/idle dual completion, startup grace, clamp, seek-reset, and exactly-once completion. Avoid using wall-clock time.
- Verification requirements: Run and port the existing clock, pause/resume, seek, completion, unknown-duration, exception-reschedule, and latch tests.
- Confidence: high
- Open questions: Android has no dedicated service-level completion/seek integration test in the inspected unit file; rely on Media3 instrumented coverage before parity sign-off.

## Failure Recovery
- Status: implemented
- Evidence:
  - `engine.py:119-129,180-191` (missing/load failure logging, skip throttle, callback, auto-skip)
  - `music_player.py:3204-3230` (preview failure row/file/queue cleanup)
  - `MusicController.kt:227-235,895-912` (Media3 and command error flows)
  - `MusicController.kt:721-738` (remove failed current and resume successor)
  - `RecommendationPreviewManager.kt:274-275,1207-1209,1758`
  - `MusicControllerTest.kt:521-595`
- User behavior: Desktop skips missing or unloadable tracks, logs errors, stops after more than five consecutive missing tracks, and marks/removes failed transient previews. Android emits sanitized connection/player errors; preview recovery removes failed current items and resumes the next item when available.
- State and data flow: Desktop failure increments `consecutive_fails`; Android distinguishes media-item failures with the failed URI and preview manager consumes that signal.
- Persistence and compatibility: Error messages are not playback-state persistence. Android explicitly strips paths/URIs from user-facing errors (`MusicController.kt:1004-1016`).
- Async, lifecycle, and failure behavior: Desktop breaks recursive failure skipping with `.after`; Android wraps commands and guards stale controller futures after release.
- UI states and visual requirements: Desktop writes error logs; Android MainShell collects connection/playback flows for Snackbars (`mobile-app/project_brain.md:374-379`).
- Migration implementation requirements: Preserve automatic skip, consecutive-failure stop threshold, preview-specific cleanup, sanitized errors, and no late callback after shutdown.
- Verification requirements: Test missing files, load exceptions, five-plus failure stop, preview removal, sanitized errors, and late connection completion.
- Confidence: high
- Open questions: No end-to-end test demonstrates Android decoder failure followed by automatic next-item playback; add one with a fake Media3 player/service boundary.

## Sleep Timer
- Status: implemented
- Evidence:
  - `music_player.py:4263-4288` (Off/finite timer scheduling and pause trigger)
  - `music_player.py:761-763,4655-4658,4673-4684` (state and preference persistence)
  - `views/settings.py:185-191` (settings action wiring)
  - `mobile-app/project_brain.md:256-262` (Android explicitly excludes sleep timer)
- User behavior: Desktop supports Off and configured minute values, cancels/replaces an existing timer, pauses active playback when it fires, updates the settings label, and resets the selector to Off. The selected preference is restored but the timer is not auto-started on launch.
- State and data flow: Tk `after` owns one `_sleep_after_id`; firing calls `engine.toggle_play` only if actively playing.
- Persistence and compatibility: Desktop stores `sleep_timer` in `~/.luno_config.json`, but not remaining time. Android has no source implementation and its settled scope excludes this feature.
- Async, lifecycle, and failure behavior: Timer uses the Tk event loop; `_on_closing` does not explicitly cancel `_sleep_after_id`, though `_closing` and process destruction terminate the loop.
- UI states and visual requirements: Settings label reports off, countdown selection, or fired/paused.
- Migration implementation requirements: Implement desktop behavior including no auto-start, replacement/cancellation, and pause-only semantics. Do not claim Android parity; classify it planned/excluded for that runtime.
- Verification requirements: Add headless timer tests for Off, replacement, fire while playing, fire while paused, and restored preference without scheduling.
- Confidence: high
- Open questions: Whether the migration should add shutdown cancellation before window destruction is a small lifecycle hardening decision.

## Crossfade
- Status: unknown
- Evidence:
  - `music_player.py:738,4291-4318` (desktop setting and fade helper functions)
  - `migration-plan.md:127-133` (plan identifies transition path as not invoking helpers)
  - `mobile-app/project_brain.md:256-262` (Android explicitly excludes crossfade)
- User behavior: Desktop exposes and persists an Off/seconds crossfade preference and contains fade-out/fade-in helpers. However, source search finds no call from playback completion/transition into either helper, so active audible crossfade is not evidenced. Android does not implement it and explicitly excludes it.
- State and data flow: Desktop `_crossfade_secs` is set from `_cf_var`; helpers schedule volume steps via Tk `.after`, but the engine completion path directly calls `play_current`/`next_track` (`engine.py:383-394`).
- Persistence and compatibility: Desktop preference is in `~/.luno_config.json`; Android has no crossfade state. The shared contract excludes playback state.
- Async, lifecycle, and failure behavior: Helper callbacks are asynchronous and have no visible closing guard; because invocation is absent, actual transition failure behavior is unknown.
- UI states and visual requirements: Settings control exists; actual transition audio is not source-proven.
- Migration implementation requirements: Treat preference UI as implemented but crossfade behavior as unresolved. Before parity sign-off, decide whether to wire helpers, replace them with a real overlap-capable backend, or explicitly remove the setting. Do not port Android exclusion as desktop behavior.
- Verification requirements: Add a transition test that proves volume envelope and next-track timing, plus cancellation/shutdown tests.
- Confidence: high for the absence finding; low for intended product behavior.
- Open questions: The migration plan itself calls for resolving this discrepancy (`migration-plan.md:39-43,127-131`).

## Media Keys, Session, and Lifecycle
- Status: implemented
- Evidence:
  - `music_player.py:4361-4519` (desktop local/XF86 keys, macOS MPRemoteCommandCenter, main-thread command queue, Now Playing updates)
  - `music_player.py:4730-4761` (desktop shutdown)
  - `MusicService.kt:19-81,142-168` (Android MediaSessionService, player attributes, noisy handling, release order)
  - `MusicService.kt:146-152,171-259` (task removal and restore)
  - `AndroidManifest.xml:33-40` (declared media playback service)
  - `MusicController.kt:824-853`
  - `test_media_keys.py:47-154`
  - `MusicControllerTest.kt:581-611`
- User behavior: Desktop supports XF86 play/next/previous and optional global macOS media commands; callbacks enqueue commands and Tk drains them on the main thread. Now Playing metadata reports title, artist, duration, rate, and canonical elapsed position. Android exposes Media3 notification/lock-screen/session controls, handles audio becoming noisy, restores last non-transient item/position, and releases safely.
- State and data flow: Desktop media callbacks never mutate playback directly; `_poll_media_commands` dispatches to engine/controller methods. Android `MusicController` is a StateFlow wrapper over a single service-owned ExoPlayer/MediaSession.
- Persistence and compatibility: Desktop persists global-key preference and clears Now Playing at shutdown. Android persists last item/position/play-when-ready in service preferences; full queue is not restored.
- Async, lifecycle, and failure behavior: Desktop stops media handlers, cancels heartbeat, saves settings, quits pygame, and destroys Tk on close. Android `MusicService.onDestroy` persists, releases MediaSession before ExoPlayer, shuts down artwork executor; `MusicController.release` is terminal and rejects late futures/listeners. `onTaskRemoved` stops only when no media items exist.
- UI states and visual requirements: Both runtimes provide transport/session surfaces; Android notification permission is declared but playback/session behavior is independent of permission result per `mobile-app/project_brain.md:374-379`.
- Migration implementation requirements: Preserve command marshalling, readiness/closing guards, Now Playing truthfulness, session metadata, noisy handling, restore semantics, and release ordering. The migration must define an equivalent OS media-session boundary rather than copying Tk/macOS APIs.
- Verification requirements: Run media-key tests; add migration tests for command queue draining, closing, stale callbacks, service restore, task removal, and release order. Device lock-screen/headset/Bluetooth behavior remains manual verification.
- Confidence: high
- Open questions: Android `onTaskRemoved` intentionally retains a service with a non-empty queue even when not playing; confirm this is desired battery/lifecycle behavior.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`, `project_brain.md`, `mobile-app/project_brain.md`, `engine.py`, `music_player.py`, `views/settings.py`, `test_engine_playback.py`, `test_media_keys.py`, `mobile-app/app/src/main/AndroidManifest.xml`, `MusicController.kt`, `MusicService.kt`, `RecentlyPlayedStore.kt`, `RecommendationPreviewManager.kt`, `MusicControllerTest.kt`, `migration-plan.md`.
- Tests/fixtures inspected: `test_engine_playback.py`, `test_media_keys.py`, `mobile-app/app/src/test/java/com/luno/mobile/playback/MusicControllerTest.kt`; no playback fixture media files are tracked.
- Verification run: `python3 -m pytest test_engine_playback.py test_media_keys.py` -> `43 passed` (the configured `.venv/bin/python` had no pytest); `cd mobile-app && ./gradlew testDebugUnitTest --tests 'com.luno.mobile.playback.MusicControllerTest'` -> `BUILD SUCCESSFUL`.
- Claims requiring review: Desktop active crossfade is not evidenced because helper invocation is absent; Android sleep timer/crossfade are excluded rather than implemented; Android service-level decoder/completion behavior lacks an inspected end-to-end test.
- Coverage gaps: No migration target exists; no real audio-device, lock-screen, Bluetooth/headset, or crossfade envelope test; Android connected Media3 behavior is not fully covered by unit seams.
- Status: complete

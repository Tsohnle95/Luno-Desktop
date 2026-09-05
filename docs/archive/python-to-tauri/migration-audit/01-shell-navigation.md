# Shell And Navigation Audit

This dossier audits the existing desktop shell/platform controls and the native
Android shell/navigation. It does not treat the proposed Tauri shell as
implemented.

## Desktop Shell Layout And Navigation
- Status: implemented
- Evidence:
  - `music_player.py:711-715` (window geometry, minimum size, resizable root)
  - `music_player.py:1072-1105` (three-column shell and persistent player)
  - `music_player.py:1108-1166` (sidebar navigation and adaptive menu trigger)
  - `music_player.py:1172-1251` (Library/Unsorted controls, playlist search, bounded sidebar scroll)
  - `music_player.py:1253-1285` (view registry and `_show_tab`)
  - `project_brain.md:257-271` (documented shell dimensions and clamping requirements)
- User behavior: The desktop opens at 1180x800 with a 960x660 minimum. The
  shell has a minimum 240 px sidebar, a flexible center view with a 400 px
  minimum, a minimum 240 px queue column, and a persistent 94 px player bar.
  Navigation exposes Home/library, Recent, Discover, Downloader, and Settings.
  The sidebar can collapse its navigation into a `Menu` label when its vertical
  space is constrained. Library and Unsorted are direct sidebar controls;
  playlist search filters playlist cards only.
- State and data flow: `LunoMusicPlayer._frames` owns Library, Download,
  Recent, Settings, Discover, Error Log, and Song Page frames. `_show_tab` hides
  all frames, shows the requested frame, and refreshes the relevant view for
  library, recent, error-log, and song-page routes. The controller owns the
  current tab and selected playlist; playlist/sidebar rebuilding is separate
  from frame navigation.
- Persistence and compatibility: Shell selection itself is session state, not
  persisted. The controller persists configuration and playlist metadata during
  close (`music_player.py:4753-4755`); the dossier does not find a persisted
  current route or window geometry.
- Async, lifecycle, and failure behavior: Library refresh and other heavy work
  are initiated outside the UI thread and UI callbacks use `after`; the shell
  starts the watchdog after 30 seconds (`music_player.py:844-845`). Window close
  is bound to `_on_closing` (`music_player.py:854-855`), which cancels polling,
  stops media controls, clears Now Playing, quits pygame, and destroys the Tk
  root (`music_player.py:4746-4760`).
- UI states and visual requirements: The three columns use draggable
  `PanedWindow` sashes. The sidebar playlist canvas locks its scroll region and
  returns to offset zero when content fits (`music_player.py:1231-1245`). The
  project brain requires the 56 px virtual-list row contract and bounded scroll
  behavior, but those row details are view-level concerns rather than shell
  routing.
- Migration implementation requirements: Reproduce the three-column shell,
  persistent queue/player, adaptive navigation, route refresh hooks, playlist
  selection/reset semantics, minimum widths, and bounded scrolling. Do not
  infer a route persistence requirement from the current implementation.
- Verification requirements: Add shell-level route/render tests for all frame
  names, current-tab updates, missing/invalid route handling, and responsive
  sidebar collapse. Preserve existing virtual-list and scroll-clamping tests.
- Confidence: high
- Open questions: The source does not establish whether future migration should
  persist window size or the selected route; both are currently unknown.

## Desktop Player Navigation And Platform Controls
- Status: implemented
- Evidence:
  - `music_player.py:1535-1568` (player info, favorite, Song Page entry point)
  - `music_player.py:1585-1593` (shuffle, previous, play, next, repeat controls)
  - `music_player.py:3188-3202` (session-aware next/previous routing)
  - `music_player.py:4361-4410` (keyboard/media command behavior and readiness guards)
  - `music_player.py:4421-4466` (macOS MPRemoteCommandCenter registration)
  - `music_player.py:4468-4492` (unregistration and Now Playing cleanup)
  - `music_player.py:4494-4525` (main-thread command drain and 50 ms reschedule)
  - `test_media_keys.py:31-154` (deterministic media-key command tests)
- User behavior: Clicking player artwork/title/artist opens Song Page. Player
  controls support shuffle, previous, play/pause, next, repeat, and favorite.
  Previous/next first route through active temporary-preview sessions, then to
  the physical engine queue. XF86 play/next/previous bindings are available;
  on macOS, global media commands are registered when the global-keys setting
  is enabled and PyObjC MediaPlayer support is available.
- State and data flow: External callbacks enqueue symbolic commands into a
  thread-safe queue. Tk drains that queue on the UI thread, checking closing
  state, engine existence, and mixer readiness before dispatch. The command
  path can start the current library context when no queue exists, or toggle
  the engine otherwise. Preview routing can consume next/previous before the
  normal engine operation.
- Persistence and compatibility: The global media-key preference is part of
  desktop config (`music_player.py:764-768` and `project_brain.md:239-240`).
  Current Now Playing state is explicitly cleared on unregister/close; no
  cross-platform media-control persistence is shown.
- Async, lifecycle, and failure behavior: Platform callbacks return promptly
  and do not mutate Tk directly. The 50 ms poll reschedules until closing.
  Registration is guarded against duplicate setup and unavailable PyObjC;
  failures are logged. Close unregisters handlers and clears Now Playing.
- UI states and visual requirements: The player remains visible below the
  shell, while Song Page is entered from player metadata. No platform-specific
  desktop control artwork or external-device test fixture is checked in.
- Migration implementation requirements: Implement a platform adapter with
  capability detection, prompt command enqueueing, main-thread dispatch,
  readiness/closing guards, registration teardown, and Now Playing cleanup.
  Preserve preview-aware next/previous semantics. Keep XF86 handling separate
  from macOS-specific APIs.
- Verification requirements: Retain the existing `test_media_keys.py` cases;
  add platform-adapter tests for registration failure, duplicate registration,
  teardown, disabled global keys, and callback-after-close. Manual verification
  is required for actual macOS Control Center/keyboard routing.
- Confidence: high for implemented behavior; low for external-device coverage.
- Open questions: Windows/Linux media-control equivalents are not implemented
  or evidenced in the desktop source. Whether the migration must support them
  is unknown.

## Android Shell And Navigation
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/MainActivity.kt:18-55` (Compose entry and controller lifecycle)
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MainShell.kt:156-180` (shell state and controller)
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MainShell.kt:247-302` (transition mask and notification route)
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MainShell.kt:450-547` (bottom navigation and Create action)
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MainShell.kt:549-749` (settings drawer routes and controls)
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MainShell.kt:757-943` (Scaffold, header, import/status strips, NavHost, mini-player)
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/navigation/NavGraph.kt:30-73` (route registry and transition policy)
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/navigation/NavGraph.kt:75-195` (route destinations)
  - `mobile-app/app/src/main/AndroidManifest.xml:22-40` (singleTop activity and MediaSessionService)
- User behavior: Android starts at Home. The bottom bar presents Home,
  Download, Your Library, Discover, and a non-navigating Create action. The
  Luno header opens a scrollable Settings drawer. Drawer actions reach Downloads,
  Export/Import, Recently played, Duplicate checker, and library-folder/artwork
  operations. Playlist details, Made for you, full player, recents, and export
  import are deep routes. Full player hides the mini-player and bottom bar;
  other routes show the mini-player when playback has an active item.
- State and data flow: `MainShell` owns the NavController, drawer, modal/dialog
  state, transition mask, and one-shot notification permission policy. It passes
  playback and export callbacks into `LunoNavHost`; `NavGraph` maps routes to
  Compose screens and uses route callbacks to set the mask before navigation.
  Bottom navigation uses `popUpTo`, `launchSingleTop`, and `restoreState` for
  non-Home tabs; Home pops back to the existing Home route when possible.
- Persistence and compatibility: `MainActivity` creates/releases
  `MusicController`; the activity is `singleTop` and forwards notification
  intents to a `StateFlow`, which opens the full player. Navigation stack state
  is Compose navigation state, not a shared desktop persistence format. Android
  persistence belongs to Room, SharedPreferences, and repositories outside
  this shell.
- Async, lifecycle, and failure behavior: The transition mask waits for the
  library and Discover loading state, with a six-second non-Discover cap. Shell
  collects controller connection/playback errors into Snackbars. Folder import
  and artwork fetch expose process-lifetime status flows in the shell. Activity
  destruction detaches the preview manager, releases the controller, and the
  MediaSession service releases its session before ExoPlayer
  (`MusicService.kt:154-168`).
- UI states and visual requirements: The shell uses a dark background wash,
  persistent header gradient, green transition mask, bottom navigation, drawer,
  import/artwork progress strips, Snackbar host, and mini-player. The NavHost
  disables per-screen enter/exit transitions (`NavGraph.kt:62-73`) because the
  shell mask owns the reveal.
- Migration implementation requirements: Android shell behavior is an
  implemented parity reference, not desktop runtime code. Preserve route names
  and deep-link intent behavior only where the target product requires Android
  compatibility. Preserve full-player exclusion of bottom chrome, drawer
  access to operational controls, and loading/error visibility.
- Verification requirements: Run Android unit tests and lint; add Compose
  navigation tests for bottom-bar state restoration, drawer destinations,
  full-player chrome hiding, notification-intent routing, and transition-mask
  timeout. Existing tests cover controller lifecycle/pending play and the
  notification permission policy, but not the shell composables.
- Confidence: high for source-defined behavior; medium for visual behavior
  because no committed screenshot/manual device evidence was inspected.
- Open questions: No Android UI test source for `MainShell` or `NavGraph` was
  found. Back-stack behavior for process death and external deep links beyond
  the notification intent is therefore unknown.

## Migration Shell Plan
- Status: planned
- Evidence:
  - `migration-plan.md:10-12` (planned Tauri 2/Rust/React stack)
  - `migration-plan.md:27-40` (shell and media-key preservation goals)
  - `migration-plan.md:55-61` (three-column/adaptive shell targets)
  - `migration-plan.md:316-324` (planned MediaSessionService and IPC surface)
  - `migration-plan.md:399-442` (future workspace, responsive shell, and visual gates)
  - `migration-audit/00-repository-census.md:158-177` (no target implementation tracked)
- User behavior: The plan proposes preserving desktop sidebar, center view,
  persistent queue, bottom player, adaptive collapse, media keys, and platform
  packaging in a future Tauri application. These are acceptance targets, not
  shipped routes or platform controls.
- State and data flow: The plan proposes Rust domain services plus Tauri
  commands/events and a React projection. No `src/`, `src-tauri/`, command
  handlers, route registry, or platform adapter exists in the audited tree.
- Persistence and compatibility: Existing desktop config and the v1 transfer
  contract are migration inputs; no new shell-state reader/writer exists.
- Async, lifecycle, and failure behavior: Planned IPC events, subprocess
  management, cancellation, and platform gates have no source/test evidence.
- UI states and visual requirements: Planned responsive shell, sidebar,
  queue/player, and scroll behavior must be derived from the desktop source and
  tests, not treated as current implementation.
- Migration implementation requirements: First establish the target shell and
  route contracts, then port the desktop behavior documented above. Define
  explicit platform capabilities and teardown behavior before claiming media
  controls complete.
- Verification requirements: The plan calls for Tauri command/event tests,
  platform build/package smoke gates, executable discovery tests, responsive
  shell checks, and media-key coverage. None can be run until the target exists.
- Confidence: high that this feature is planned-only.
- Open questions: Target route names, supported desktop OS set, media-control
  API choices, window-state persistence, and whether Android route names should
  be shared are unresolved.

## Audit Receipt
- Files inspected: `migration-audit/README.md`,
  `migration-audit/00-repository-census.md`, `project_brain.md`,
  `mobile-app/project_brain.md`, `music_player.py`,
  `mobile-app/app/src/main/java/com/luno/mobile/MainActivity.kt`,
  `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MainShell.kt`,
  `mobile-app/app/src/main/java/com/luno/mobile/ui/navigation/NavGraph.kt`,
  `mobile-app/app/src/main/java/com/luno/mobile/playback/MusicService.kt`,
  `mobile-app/app/src/main/AndroidManifest.xml`, and `migration-plan.md`.
- Tests/fixtures inspected: `test_media_keys.py` (11 passed with
  `python3 -m pytest test_media_keys.py`),
  `mobile-app/app/src/test/java/com/luno/mobile/playback/MusicControllerTest.kt`,
  `mobile-app/app/src/test/java/com/luno/mobile/playback/NotificationPermissionPolicyTest.kt`
  (Android `./gradlew testDebugUnitTest lintDebug` passed).
- Claims requiring review: Desktop platform support beyond macOS; Android
  process-death/deep-link back-stack behavior; visual parity without device or
  screenshot evidence; all future Tauri shell and platform-adapter claims.
- Coverage gaps: No desktop shell integration tests, no Android Compose shell
  tests, no real macOS media-key/Now Playing test, no Windows/Linux adapter
  evidence, and no Tauri target source.
- Status: complete

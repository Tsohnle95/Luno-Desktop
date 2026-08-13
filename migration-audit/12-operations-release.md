# Operations and Release Audit

This dossier covers the desktop Python/Tk application only. It audits the
checked-in operational surface; planned migration behavior is not treated as
implemented.

## Packaging and Build
- Status: implemented
- Evidence:
  - `build.command:1-12`
  - `Luno.spec:3-55`
  - `requirements.txt:1-10`
  - `README.md:39-40`
  - `.gitignore:12-14`
- User behavior: A developer can activate the local `.venv`, run the PyInstaller
  spec, and open `dist/Luno.app` after a successful build. The spec emits a
  macOS app named `Luno.app` with stable bundle identifier `com.ty.luno`, icon,
  `views/` data, and PyObjC hidden imports.
- State and data flow: Packaging starts at `music_player.py` and includes the
  `views` data directory; generated `build/` and `dist/` outputs are ignored.
  The spec contains a machine-specific `/Users/ty/luno/.venv/...` `pathex`, so
  it is not a portable build recipe.
- Persistence and compatibility: The bundle identifier is explicit, but no
  version, update channel, artifact naming/versioning, or migration manifest is
  defined in the packaging files.
- Async, lifecycle, and failure behavior: `build.command` fails the shell on
  PyInstaller failure and does not open the app, but it has no dependency
  checks, signing, notarization, smoke test, artifact checksum, or rollback.
- UI states and visual requirements: `console=False` produces a windowed app;
  PyInstaller traceback support remains enabled by `disable_windowed_traceback=False`.
- Migration implementation requirements: Replace the absolute spec path with a
  reproducible environment-relative build; define versioned artifacts and a
  packaged-app smoke test before release. Preserve `com.ty.luno`, `views/`, and
  the resource path contract.
- Verification requirements: Run the local PyInstaller build on supported macOS
  with a clean environment, launch the resulting app, and verify bundled views,
  PyObjC media keys, playback, and download failure states. Current CI only runs
  pytest (`.github/workflows/desktop.yml:22-26`).
- Confidence: high
- Open questions: Which macOS versions and architectures are release targets,
  and who owns signing/notarization credentials?

## Dependencies and FFmpeg/yt-dlp Provisioning
- Status: implemented
- Evidence:
  - `requirements.txt:1-10`
  - `README.md:6,39-40`
  - `music_player.py:27-28`
  - `downloader.py:133-168`
  - `previews.py:106-140`
  - `views/song_page.py:81-116`
- User behavior: Python dependencies are installed from a requirements file.
  Downloads and temporary previews invoke yt-dlp postprocessors that require
  FFmpeg; Song Page video requires both `ffmpeg` and `ffprobe` on `PATH`.
- State and data flow: The app appends Homebrew locations to `PATH` at import
  time, while yt-dlp is a Python package. Neither FFmpeg nor yt-dlp is bundled
  by `Luno.spec`; FFmpeg is discovered as an external executable and yt-dlp is
  expected to be present in the build environment.
- Persistence and compatibility: `requirements.txt` pins mutagen and PyObjC
  but leaves Pillow, PyInstaller, pygame, requests, and yt-dlp unpinned, so
  rebuilds are not fully reproducible. Downloader audio is normalized to 44.1
  kHz/192k with loudnorm; previews use 44.1 kHz/192k without loudnorm.
- Async, lifecycle, and failure behavior: Downloader has socket/retry settings
  and cooperative cancellation hooks, but `nocheckcertificate=True` disables
  TLS certificate verification for yt-dlp requests. Missing binaries are runtime
  failures/degraded behavior rather than a release-time validation result.
- UI states and visual requirements: Missing yt-dlp is logged by the controller
  (`music_player.py:870`); missing FFmpeg is handled by Song Page video fallback,
  while normal audio/download operation has no single preflight UX.
- Migration implementation requirements: Establish a supported FFmpeg delivery
  strategy (bundled, managed, or explicit installer), validate executable
  versions at startup/build time, pin the complete dependency set, and decide
  whether certificate bypass is acceptable. Preserve 44.1 kHz output behavior.
- Verification requirements: Test a clean packaged install with no developer
  PATH; verify download, thumbnail embedding, preview, and video probe paths;
  test absent binaries and network/TLS failures without false success.
- Confidence: high
- Open questions: Is external FFmpeg an intentional end-user prerequisite, and
  what yt-dlp/FFmpeg versions are supported?

## Permissions and Secrets
- Status: implemented
- Evidence:
  - `music_player.py:59-75`
  - `music_player.py:4621-4642`
  - `music_player.py:4673-4689`
  - `Luno.spec:23-39,50-55`
  - `views/settings.py:78-92`
  - `.gitignore:19-23`
- User behavior: The user enters a Last.fm API key in Settings; the field is
  masked, but the key is persisted in plaintext in `~/.luno_config.json`.
  Library/config/preview paths are created under the user's home directory.
- The macOS bundle declares no entitlements, sandbox profile, or documented
  permission/access policy; access to the chosen music root and home-directory
  files is therefore an undocumented runtime assumption.
- State and data flow: Config loads and saves `lastfm_key` alongside playback
  preferences and favorite paths. Playlist URL metadata is written under the
  selected music root. No keychain, OS secure storage, environment-secret
  contract, or file permission hardening is implemented.
- Persistence and compatibility: Existing JSON files are directly overwritten,
  not atomically replaced or versioned. The repository ignores Android signing
  material, but has no desktop signing secret configuration.
- Async, lifecycle, and failure behavior: Config I/O errors are printed and the
  application continues; there is no recovery/backup path for a failed write.
- UI states and visual requirements: Settings explains how to create an API key
  and masks entry text, but masking does not protect the persisted file.
- Migration implementation requirements: Migrate desktop config without
  exposing secrets in logs or transfer manifests; use OS credential storage or
  documented permissions and atomic backups, and explicitly define user-data
  access permissions for library roots.
- Verification requirements: Inspect file permissions on first run and after
  save, verify secret redaction in logs/crash reports, and test malformed or
  unwritable config recovery.
- Confidence: high
- Open questions: Must existing plaintext keys be migrated, deleted, or offered
  for one-time import into secure storage?

## Logging and Diagnostics
- Status: implemented
- Evidence:
  - `music_player.py:1813-1819`
  - `music_player.py:732,1264-1283`
  - `downloader.py:59-71`
  - `music_player.py:4670-4671,4686-4689`
- User behavior: The app retains at most 500 in-memory error-log entries and
  exposes them in the Error Log view; yt-dlp warning/error messages can be
  routed there. Entries are also printed to the terminal.
- State and data flow: Logging is an in-process list of formatted timestamp,
  area, and message strings. There is no persistent log file, structured log
  schema, rotation, crash reporter, or release diagnostics bundle.
- Persistence and compatibility: Logs disappear on exit and are not part of
  the library transfer contract. Exception/config failures use direct `print`,
  bypassing the bounded UI log.
- Async, lifecycle, and failure behavior: Background callbacks can report
  errors through the controller, but terminal-only diagnostics are unavailable
  from a windowed packaged launch unless separately captured.
- UI states and visual requirements: Error Log is read-only with Clear Logs;
  this is useful for session diagnosis but not operational incident recovery.
- Migration implementation requirements: Define privacy-aware structured logs,
  bounded persistence/rotation, user export, and crash diagnostics. Redact API
  keys and URLs containing credentials before display or upload.
- Verification requirements: Exercise yt-dlp, config, FFmpeg, and shutdown
  errors; confirm useful diagnostics in a packaged no-console launch and confirm
  500-entry/rotation bounds.
- Confidence: high
- Open questions: Is local persistent logging required, or should diagnostics
  remain opt-in and user-exported?

## Shutdown and Subprocess Cleanup
- Status: implemented
- Evidence:
  - `music_player.py:4730-4761`
  - `downloader.py:34-57,87-119`
  - `views/song_page.py:171-210,1337-1362`
  - `music_player.py:820-826`
- User behavior: Window close sets a closing flag, cancels the active download,
  stops media-key handlers, clears Now Playing, quits pygame, saves config, and
  destroys the Tk window. Video decoders are terminated and killed after a
  0.5-second wait when stopped.
- State and data flow: yt-dlp work runs in one daemon job and cancellation is
  cooperative through progress hooks. Preview resolver/art executors use
  `shutdown(wait=False, cancel_futures=True)`, so already-running work is not
  synchronously joined before process exit. Startup removes the entire preview
  directory; session-end cleanup is represented in controller logic, not a
  separate crash-safe service.
- Persistence and compatibility: Close saves config and playlist metadata before
  exit, but writes are non-atomic. `signal_handler` only quits pygame and exits,
  bypassing the normal save/cancel/cleanup sequence.
- Async, lifecycle, and failure behavior: Daemon workers can be abandoned at
  process exit. The download coordinator marks jobs done in `finally`, but
  `cancel_active()` does not join the worker. FFmpeg video subprocesses have
  explicit termination cleanup; ffprobe has a five-second timeout.
- UI states and visual requirements: Close is wired to `WM_DELETE_WINDOW`; no
  visible shutdown progress or “download cancellation in progress” state is
  established.
- Migration implementation requirements: Centralize lifecycle ownership,
  cancel and join workers/subprocesses with bounded deadlines, make signal/close
  paths converge, and preserve deferred deletion of currently playing previews.
- Verification requirements: Test close during download, preview resolution,
  FFmpeg decode, artwork work, and config writes; inspect for orphan processes,
  incomplete files, stale media-key registration, and preview cleanup after a
  forced termination.
- Confidence: high
- Open questions: Is immediate exit preferred over bounded graceful shutdown,
  and should incomplete downloads be resumed or removed?

## CI, Release, and Rollback
- Status: implemented
- Evidence:
  - `.github/workflows/desktop.yml:1-26`
  - `build.command:1-12`
  - `README.md:24-30,39-40`
  - `.gitignore:12-14`
- User behavior: Pushes to `main` and pull requests run the desktop pytest gate
  on `macos-latest`. A release operator can build and launch locally, but no
  checked-in workflow publishes a desktop artifact.
- State and data flow: CI installs current requirements plus pytest and runs
  `python -m pytest`; it does not build the PyInstaller app, install/probe
  FFmpeg, archive artifacts, sign/notarize, or test a clean packaged runtime.
- Persistence and compatibility: Build outputs are ignored and there is no
  release version/tag policy, artifact retention policy, checksum, channel,
  migration preflight, backup, or rollback command. There is therefore no
  repository evidence of an operational rollback path.
- Async, lifecycle, and failure behavior: CI failure blocks only the test job;
  release promotion and rollback are manual/undefined. No staged rollout or
  previous-artifact selection is implemented.
- UI states and visual requirements: No release smoke-test evidence verifies the
  bundled window, assets, media keys, FFmpeg fallback, or no-console diagnostics.
- Migration implementation requirements: Add a desktop release pipeline with
  pinned dependencies, package smoke tests, signed/notarized artifacts, checksums,
  supported-platform matrix, retained previous versions, and a documented
  rollback procedure that preserves user data.
- Verification requirements: Keep `python -m pytest` as the desktop gate; add a
  clean-build/package/launch gate and release artifact validation. Do not use
  Android/Gradle tests as evidence for this desktop unit.
- Confidence: high
- Open questions: What distribution channel, signing identity, architecture
  matrix, and rollback owner are intended?

## Migration Summary
- Status: documented-only
- Evidence:
  - `migration-plan.md:148-180,537-550`
  - `migration-implementation-plan.md`
  - `00-repository-census.md:148-180`
- User behavior: The proposed Tauri/Rust/React release and subprocess model is a
  migration target, not an implementation present in this repository.
- State and data flow: No `src/` or `src-tauri/` target, release manifest, updater,
  keychain integration, or Rust process supervisor is tracked.
- Persistence and compatibility: Existing desktop JSON/config and temporary
  files remain the source behavior to migrate; no target rollback or upgrade
  transaction is evidenced.
- Async, lifecycle, and failure behavior: Planned cancellation and lifecycle
  requirements must be validated against the Python evidence above.
- UI states and visual requirements: None established for release operations.
- Migration implementation requirements: Treat every release, provisioning,
  secrets, and rollback item above as acceptance work, not completed parity.
- Verification requirements: Re-audit after the target release pipeline exists.
- Confidence: high
- Open questions: All target release implementation details remain open.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`,
  `project_brain.md`, `README.md`, `requirements.txt`, `Luno.spec`, `build.command`,
  `.github/workflows/desktop.yml`, `.gitignore`, `music_player.py`, `engine.py`,
  `downloader.py`, `previews.py`, `views/song_page.py`, `views/settings.py`.
- Tests/fixtures inspected: `.github/workflows/desktop.yml:22-26` and the
  desktop test gate definition; no Android/Gradle tests run.
- Claims requiring review: Supported macOS architectures/versions, external
  FFmpeg policy, certificate bypass policy, secure-key migration, and release
  ownership are not defined by source/configuration.
- Coverage gaps: No packaged-app smoke test, signing/notarization configuration,
  release artifact, updater, rollback procedure, crash corpus, or orphan-process
  test is tracked. Runtime behavior was source-audited, not exercised against a
  real release build.
- Status: complete

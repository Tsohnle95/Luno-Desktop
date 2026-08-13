# Repository Census Audit

This census establishes the repository boundaries and authority map for the
desktop migration. It describes the repository as observed on 2026-08-13.
Claims about behavior below are deliberately limited to the cited source,
tests, or configuration. Plans and brains are not treated as implementation.

## Authority And Repository Boundary
- Status: implemented
- Evidence:
  - `migration-audit/README.md:1-39`
  - `README.md:3-22`
  - `docs/architecture/README.md:3-33`
  - `docs/decisions/README.md:1-16`
- User behavior: The repository contains two applications, a Python/Tk desktop
  application and a native Kotlin/Compose Android application. They are
  separate runtimes and intentionally share only the library-export wire
  contract.
- State and data flow: Desktop state is controller/engine/view based; Android
  state is owned by Room repositories, playback services, workers, and Compose
  UI. The shared manifest carries metadata, playlist membership, and optional
  provider IDs, not paths, secrets, audio, queue, favorites, or history.
- Persistence and compatibility: `luno.library.export` v1 is the cross-platform
  compatibility boundary. Desktop and Android implement it independently.
- Async, lifecycle, and failure behavior: Desktop heavy filesystem, network,
  and media work must stay off the Tk thread and return through Tk's event loop;
  Android background work is in playback workers/repositories. These are
  runtime constraints, not shared code.
- UI states and visual requirements: The migration target is the desktop
  product; Android is a parity reference where explicitly implemented, not a
  replacement desktop runtime.
- Migration implementation requirements: Treat source/tests/config as highest
  authority, then project brains and architecture decisions, then plans and
  roadmap proposals. Do not infer implementation from a plan.
- Verification requirements: Preserve the checked-in contract fixtures and run
  the desktop and Android gates independently.
- Confidence: high
- Open questions: None for authority order; feature-specific dossiers must
  resolve source/documentation contradictions.

## Desktop Implementation Surface
- Status: implemented
- Evidence:
  - `music_player.py:325` (`LunoMusicPlayer` controller)
  - `engine.py:26-72` (`LunoEngine`, pygame initialization and state)
  - `library_tools.py:8` (`scan_library`)
  - `downloader.py:73-115` (`Downloader` and one-job coordinator)
  - `views/__init__.py`, `views/*.py`
  - `test_engine_playback.py:196-630`
  - `test_downloader_sync.py:79-219`
  - `test_discover_mode.py:294-885`
- User behavior: The Python application is the behavioral reference for the
  migration. It owns the desktop shell, library, playback, downloader,
  discovery, Song Page, settings, recent history, and error-log workflows.
- State and data flow: `LunoMusicPlayer` owns controller state and frames;
  `LunoEngine` owns queue/playback state; library scanning and duplicate logic
  are in `library_tools.py`; discovery is in `discovery.py`; downloads are in
  `downloader.py`; temporary previews/video are separate services.
- Persistence and compatibility: Desktop code reads/writes its existing JSON,
  marker, preview, and library structures. `library_transfer.py:290-350`
  builds path-free manifests and `library_transfer.py:73-110` validates them.
- Async, lifecycle, and failure behavior: The engine initializes pygame at
  44.1 kHz/stereo (`engine.py:31-41`) and polls playback (`engine.py:66-72`);
  downloads are admitted through a locked single active job
  (`downloader.py:76-115`).
- UI states and visual requirements: `views/library.py`, `views/recent.py`,
  `views/discover.py`, `views/downloader.py`, `views/settings.py`,
  `views/song_page.py`, and `views/error_log.py` are the desktop UI authority.
- Migration implementation requirements: Port observable behavior from source
  and tests, including queue semantics, persistence formats, async boundaries,
  failure handling, and the documented 56 px virtual-list row contract. The
  target Tauri/Rust/React architecture is not present in this repository.
- Verification requirements: `python -m pytest` is the configured desktop
  gate; classify ad-hoc tests separately from deterministic parity tests.
- Confidence: high
- Open questions: Crossfade is represented in engine state but its active
  transition behavior requires a dedicated parity decision, as noted in
  `migration-plan.md:39-43` and `migration-plan.md:127-131`.

## Android Implementation Surface
- Status: implemented
- Evidence:
  - `mobile-app/app/build.gradle.kts:8-79,107-180`
  - `mobile-app/settings.gradle.kts:24-37`
  - `mobile-app/app/src/main/AndroidManifest.xml:5-40`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/AppDatabase.kt:17-75`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/MusicService.kt:19`
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/MusicController.kt:55`
  - `mobile-app/app/src/test/java/com/luno/mobile/**`
  - `mobile-app/app/src/androidTest/java/com/luno/mobile/**`
- User behavior: Android is a native offline-first product with Compose UI,
  Media3 playback, Room persistence, SAF import, WorkManager downloads, and
  discovery/transfer features present in its source tree.
- State and data flow: Room schema version 9 is append-migrated in
  `AppDatabase.kt`; `MusicService` owns the Media3 service/player boundary;
  repositories own storage/discovery/transfer and workers own background
  downloads.
- Persistence and compatibility: Android uses Room migrations and implements
  the shared manifest codec. The Android build adds the repository contract
  directory to test resources (`app/build.gradle.kts:77-79`).
- Async, lifecycle, and failure behavior: Media3 service lifecycle, WorkManager
  workers, and Android tests are separate from desktop Tk/pygame lifecycle.
- UI states and visual requirements: Compose screens under
  `mobile-app/app/src/main/java/com/luno/mobile/ui/` are Android authorities;
  screenshots, brains, and roadmap text cannot establish desktop behavior.
- Migration implementation requirements: Use Android only as a parity source
  where the desktop migration explicitly needs cross-platform product intent;
  do not port Android-only implementation assumptions into the desktop runtime.
- Verification requirements: `cd mobile-app && ./gradlew testDebugUnitTest lintDebug`
  is the documented Android gate; release CI additionally builds and verifies a
  signed APK (`.github/workflows/android-release.yml:48-58`).
- Confidence: high
- Open questions: Android and desktop may continue sharing only the manifest;
  broader shared runtime code is explicitly discouraged by
  `docs/architecture/README.md:35-41`.

## Contract And Fixtures
- Status: implemented
- Evidence:
  - `contracts/library-export/v1/README.md:3-21`
  - `contracts/library-export/v1/schema.json:1-53`
  - `contracts/library-export/v1/valid.json`
  - `contracts/library-export/v1/invalid-unknown-field.json`
  - `test_contract_fixtures.py:12-27`
  - `test_library_transfer.py:17-79`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/export/LibraryManifestCodec.kt:16-80`
  - `mobile-app/app/src/test/java/com/luno/mobile/data/export/LibraryManifestCodecTest.kt`
- User behavior: Users can transfer selected tracks, playlists, or a library
  through a metadata-only JSON manifest; imports are additive and reject unsafe
  or unknown fields.
- State and data flow: Track references are bounded IDs; source identities are
  optional; metadata-only records require explicit ambiguity confirmation.
- Persistence and compatibility: v1 and its fixtures are frozen migration
  inputs. Local paths, audio bytes, secrets, queue state, favorites, and history
  are forbidden by contract.
- Async, lifecycle, and failure behavior: Desktop and Android perform their own
  import matching and missing-track handling after strict codec validation.
- UI states and visual requirements: Contract files define wire behavior only;
  UI states belong to each application.
- Migration implementation requirements: Preserve exact v1 acceptance and
  rejection behavior, fixture contents, additive semantics, and no-path rule.
- Verification requirements: Run Python fixture tests and Android manifest codec
  tests; retain both valid acceptance and unknown-field rejection coverage.
- Confidence: high
- Open questions: Provider-ID matching completeness is a migration concern,
  not a reason to alter the v1 contract.

## Plans And Proposals
- Status: planned
- Evidence:
  - `migration-plan.md:1-20`
  - `migration-plan.md:273-371`
  - `migration-plan.md:373-495`
  - `migration-implementation-plan.md`
  - `docs/product/roadmap.md:1-18`
  - `dispatch-plan.json:2-20`
  - `TODO.md:1-18`
- User behavior: The proposed migration is a React/TypeScript + Tauri 2 + Rust
  rewrite intended to preserve the Python app's behavior, not a currently
  shipped implementation.
- State and data flow: The plan proposes typed Rust domain services, Tauri
  commands/events, and a React projection; no `src/` or `src-tauri/` target
  implementation is tracked in this census.
- Persistence and compatibility: The plan requires migration of existing config,
  markers, previews, favorites, and transfer data, but those target writers and
  readers are not present.
- Async, lifecycle, and failure behavior: Planned Rust subprocess management,
  cancellation, atomic persistence, and event generations remain acceptance
  requirements, not evidence of implementation.
- UI states and visual requirements: Planned shell/screens and visual parity
  requirements are inventory and design targets only.
- Migration implementation requirements: Build future dossiers from the
  implemented Python source/tests and contract fixtures; label every plan-only
  item as planned or unknown until code and tests exist.
- Verification requirements: The plan's Rust/TypeScript/Tauri gates are future
  gates. Current executable gates are the Python pytest workflow and Android
  Gradle workflow.
- Confidence: high
- Open questions: The plan lists unresolved audio backend, provider, identity,
  keychain, licensing, and crossfade decisions (`migration-plan.md:537-550`).

## Tracked, Generated, Vendor, And Local Boundaries
- Status: implemented
- Evidence:
  - `git ls-files` census: 42 root files, 112 Android files, 17 audit files,
    3 docs files, and 4 contract files (working-tree observation)
  - `.gitignore:1-26`
  - `.gitmodules:1-4`
  - `mobile-app/settings.gradle.kts:27-37`
  - `project_brain.md:73-89,105-108`
- User behavior: Only tracked source, tests, configuration, docs, contracts,
  and the pinned submodule reference are repository inputs.
- State and data flow: `mobile-app/vendor/NewPipeExtractor` is a shallow Git
  submodule pinned at v0.26.4 and substituted into the Android composite build;
  it is vendor dependency code, not Luno-owned migration authority.
- Persistence and compatibility: Tracked Room schema snapshots under
  `mobile-app/app/schemas/` are migration evidence. Local `local.properties`,
  signing keys, and generated APKs are not portable repository inputs.
- Async, lifecycle, and failure behavior: Ignored build outputs and caches do
  not establish runtime behavior; source/configuration and tests do.
- UI states and visual requirements: Ignored `.DS_Store`, bytecode, APKs,
  PyInstaller `build/`/`dist/`, `.venv/`, Gradle caches, and `.opencode`
  dependencies are excluded from the authority map.
- Migration implementation requirements: Do not commit generated artifacts,
  credentials, local environment files, or vendor working-tree changes. Keep
  the submodule pinned and inspect its own source only when Android extraction
  behavior requires it.
- Verification requirements: Check tracked status and submodule status before
  accepting a census; run tests from configured environments rather than from
  generated artifacts.
- Confidence: high
- Open questions: The desktop brain lists `build.command` in its file tree
  (`project_brain.md:36`) but it is absent from `git ls-files`; conversely,
  tracked `migration-implementation-plan.md` is not listed in that tree. These
  are documentation-registry discrepancies for later review.

## Audit Receipt
- Files inspected: `AGENTS.md`, `README.md`, `project_brain.md`,
  `mobile-app/project_brain.md`, `migration-audit/README.md`,
  `migration-plan.md`, `migration-implementation-plan.md`, `docs/architecture/README.md`,
  `docs/decisions/README.md`, `docs/product/roadmap.md`, `TODO.md`,
  `dispatch-plan.json`, `.gitignore`, `.gitmodules`, `pytest.ini`,
  `requirements.txt`, both CI workflows, Gradle settings/build/version files,
  desktop source/view modules, Android source/test trees, contract files, and
  Git tracked/ignored/submodule listings.
- Tests/fixtures inspected: `test_contract_fixtures.py`,
  `test_library_transfer.py`, `test_engine_playback.py`,
  `test_downloader_sync.py`, `test_discover_mode.py`, the remaining desktop
  test modules, Android unit/instrumented test paths, and both v1 JSON fixtures.
- Claims requiring review: Crossfade behavior; provider-ID import completeness;
  documentation file-tree discrepancies; all planned Tauri/Rust/React behavior.
- Coverage gaps: No Tauri target exists to inspect; no real-user data corpus,
  release signing secrets, or full-device/manual UI evidence is committed; the
  vendored submodule internals were boundary-counted but not audited.
- Status: complete

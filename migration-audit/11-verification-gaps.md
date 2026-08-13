# Verification Gaps Audit

This dossier audits the desktop verification surface only. It does not assess
Android or Gradle verification.

## Desktop Test Gate And Collection
- Status: implemented
- Evidence:
  - `README.md:24-40`
  - `pytest.ini:1-5`
  - `.github/workflows/desktop.yml:1-26`
  - `migration-audit/00-repository-census.md:69-75,217-235`
- User behavior: The documented desktop gate is `python -m pytest`. CI runs it
  on `macos-latest` with Python 3.12 after installing `requirements.txt` and
  pytest.
- State and data flow: Pytest uses repository-default discovery. The config
  excludes `test_audio_flet.py`, `test_audio_flet2.py`, and `test_tkvideo.py`,
  but does not exclude the other ad-hoc root scripts.
- Persistence and compatibility: Contract fixture tests load the checked-in
  v1 JSON files directly (`test_contract_fixtures.py:9-28`); this is a useful
  deterministic compatibility check, not a full app migration check.
- Async, lifecycle, and failure behavior: The configured gate has no explicit
  timeout, subprocess cleanup check, display setup, or fixture isolation step.
  A CI timeout or collection failure would therefore be the only protection
  for several lifecycle classes.
- UI states and visual requirements: CI has no screenshot, responsive, Tk
  interaction, accessibility, or manual smoke stage.
- Migration implementation requirements: A future desktop target needs a
  reliable collection gate that cannot import or execute experiments, plus
  separate unit, integration, and visual/manual jobs.
- Verification requirements: First make collection deterministic by moving
  experiments out of `test*.py` or excluding them. Then add a headless
  collection check and a macOS Tk smoke job. Keep the existing Python gate as
  the baseline behavior gate.
- Confidence: high
- Open questions: Whether the intended supported CI matrix is macOS only or
  whether a dependency-light Linux unit job should also be added.

## Ad-Hoc Scripts And Fixtures
- Status: implemented
- Evidence:
  - `test_dl.py:1-30`
  - `test_focus.py:1-18`
  - `test_label_ph.py:1-24`
  - `test_tkvideo.py:1-75`
  - `pytest.ini:1-5`
  - `project_brain.md:57-70`
- User behavior: `test_dl.py` performs a real yt-dlp download at import/runtime;
  `test_focus.py` and `test_label_ph.py` create Tk roots and enter mainloops;
  `test_tkvideo.py` is an interactive video prototype with a machine-specific
  media path. These are experiments, not deterministic acceptance tests.
- State and data flow: The three scripts not excluded by pytest can affect
  collection through network access, display availability, or external files.
  `test_tkvideo.py` is excluded, while the other experiments are not.
- Persistence and compatibility: `test_dl.py` writes output using the current
  working directory template (`test_dl.py:3-22`); no temporary fixture protects
  the repository from accidental media output.
- Async, lifecycle, and failure behavior: These scripts have no assertions for
  cleanup or subprocess shutdown. The Tk scripts schedule destruction after
  one second, but still require a display during collection/import.
- UI states and visual requirements: They are useful exploratory evidence for
  focus and placeholder behavior, but provide no stable screenshots or visual
  baselines.
- Migration implementation requirements: Classify these as manual experiments
  or convert their useful cases into isolated tests. Never let live network,
  Tk root creation, or a user-specific absolute path participate in the default
  migration gate.
- Verification requirements: Add explicit collection tests proving only
  deterministic tests are collected; add mocked yt-dlp and Tk fixtures for any
  retained behavior.
- Confidence: high
- Open questions: `test_audio_flet.py` and `test_audio_flet2.py` are named in
  pytest config but absent from the tracked census; their source/status should
  be resolved separately.

## Behavior Coverage
- Status: implemented
- Evidence:
  - `test_engine_playback.py:1-7,257-634`
  - `test_downloader_sync.py:46-219`
  - `test_discover_mode.py:1-13,294-900`
  - `test_art_cache.py:12-193`
  - `test_library_transfer.py:16-90`
  - `test_media_keys.py:31-149`
  - `test_video_preview.py:28-53`
  - `test_contract_fixtures.py:12-28`
- User behavior: Existing deterministic coverage is strongest for playback
  clock/queue transitions, sync markers and duplicate admission, Discover
  staging/session teardown, transfer validation, artwork cache bounds, media
  command routing, and video-source relevance.
- State and data flow: Most tests bind methods to lightweight stubs or replace
  pygame/yt-dlp (`test_engine_playback.py:176-218`, `test_discover_mode.py:127-181`,
  `test_downloader_sync.py:62-75`). This isolates domain logic but does not
  verify controller-view wiring, real event-loop ordering, or filesystem scan
  output.
- Persistence and compatibility: Transfer fixtures cover valid acceptance and
  unknown-field rejection only. Desktop config, playlist metadata, favorites,
  recent history, allowed-duplicate backups, preview cleanup, and legacy JSON
  migration have no equivalent end-to-end persistence test in the tracked
  desktop suite.
- Async, lifecycle, and failure behavior: There are useful cancellation and
  generation assertions, but they generally use synchronous stubs and do not
  prove late callbacks are harmless against destroyed Tk widgets.
- UI states and visual requirements: No tracked test instantiates
  `LunoMusicPlayer` or the views as a complete shell. Virtual-list 56 px rows,
  scroll clamping, resize layout, popup pointer containment, spinner states,
  empty/error/retry states, and accessibility/focus behavior lack automated
  visual verification.
- Migration implementation requirements: Port pure behaviors first, but add
  integration coverage around controller, engine, views, persistence, and
  worker boundaries before declaring parity.
- Verification requirements: Create a behavior matrix mapping every source
  workflow to a test ID and expected state transitions. Include missing-file,
  malformed metadata, permission, unavailable binary, network timeout, and
  shutdown cases.
- Confidence: high
- Open questions: Crossfade is present in engine state but has no transition
  test, consistent with the unresolved parity note in
  `migration-audit/00-repository-census.md:76-78`.

## Visual And Manual Gaps
- Status: planned
- Evidence:
  - `project_brain.md:257-283`
  - `migration-plan.md:370-385,440-441,481-508,519-535`
  - `views/library.py:1-20`
  - `views/song_page.py:26-210,1337-1362`
- User behavior: The source documents strict layout and scroll contracts and
  complex Song Page/video states, but no committed screenshot or interaction
  baseline verifies them.
- State and data flow: The video renderer uses FFmpeg subprocesses and Tk
  callbacks (`views/song_page.py:81-129,143-169`), making timing, resize, and
  teardown behavior unsuitable for pure unit assertions alone.
- Persistence and compatibility: Visual checks do not replace persistence or
  contract tests; both need independent acceptance IDs.
- Async, lifecycle, and failure behavior: Manual checks are required for
  window resize during playback, pause/seek decoder restart, fullscreen Escape
  cleanup, popup hover transitions, and closing while work is active.
- UI states and visual requirements: Missing coverage includes desktop sizes,
  narrow panes, empty/loading/error/retry states, queue scroll retention,
  56 px row geometry, album-art fallback, and keyboard/focus labels.
- Migration implementation requirements: Establish screenshot baselines at
  supported sizes and scripted/manual smoke flows for shell navigation, library,
  downloader, Discover, Song Page, settings, recent, and error log.
- Verification requirements: Require visual review receipts with OS, window
  size, fixture corpus, and observed state. Add responsive and accessibility
  checks rather than relying on source descriptions.
- Confidence: high
- Open questions: Which macOS versions and display scale factors are release
  targets for the visual baseline?

## Differential Tests And Acceptance Verification
- Status: planned
- Evidence:
  - `migration-plan.md:380-406,427-440,500-535,555`
  - `migration-audit/00-repository-census.md:148-180`
  - `engine.py:31-41`
  - `downloader.py:34-57,73-119`
  - `music_player.py:4730-4761`
- User behavior: The migration plan calls for Python-vs-Rust differential tests,
  fake media executables, deterministic playback, cancellation/shutdown tests,
  visual regression, and explicit acceptance IDs. None of the proposed target
  harness or target implementation exists in this repository.
- State and data flow: Pure parity candidates include filename/title parsing,
  normalization, scan ordering, duplicate grouping, queue insertion/shuffle,
  transfer codec, and download marker decisions. Side-effect flows require
  fakes that record event order and output ownership.
- Persistence and compatibility: Differential fixtures must include legacy
  config/marker data, path normalization, malformed manifests, additive import,
  and no-path enforcement. The checked-in transfer fixtures are the frozen
  cross-platform boundary (`contracts/library-export/v1/README.md:3-21`).
- Async, lifecycle, and failure behavior: `Downloader` owns a daemon job and
  cooperative cancellation (`downloader.py:87-119`), while app shutdown cancels
  it and shuts down pools without waiting (`music_player.py:4730-4760`). There is
  no test proving the job/thread/process has stopped before teardown completes,
  nor that late callbacks cannot mutate destroyed views.
- UI states and visual requirements: Acceptance must combine automated state
  checks with screenshot/manual evidence; source/docs alone cannot establish
  parity.
- Migration implementation requirements: Define acceptance IDs per feature,
  run the Python reference and migration against identical generated fixtures,
  compare normalized outputs/events, and retain mismatch artifacts.
- Verification requirements: Add subprocess tests covering FFmpeg/yt-dlp
  timeout, terminate-to-kill escalation, partial-output cleanup, no detached
  worker, and shutdown during active download/video decode. Add a full launch,
  play/pause/seek/queue/download/Discover/import/export/close acceptance flow.
- Confidence: high
- Open questions: The migration target's subprocess ownership and event schema
  must be fixed before differential assertions can be finalized.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`,
  `project_brain.md`, `README.md`, `pytest.ini`, `.github/workflows/desktop.yml`,
  `migration-plan.md`, desktop source modules, `views/song_page.py`, all tracked
  root `test*.py` files, contract files, and git-tracked file/status listings.
- Tests/fixtures inspected: `test_engine_playback.py`, `test_downloader_sync.py`,
  `test_discover_mode.py`, `test_art_cache.py`, `test_library_transfer.py`,
  `test_contract_fixtures.py`, `test_media_keys.py`, `test_video_preview.py`,
  ad-hoc `test_dl.py`, `test_focus.py`, `test_label_ph.py`, `test_tkvideo.py`,
  and `contracts/library-export/v1/{valid.json,invalid-unknown-field.json}`.
- Claims requiring review: Whether untracked/ignored ad-hoc files are present in
  CI; intended visual baseline matrix; target subprocess ownership; crossfade
  acceptance semantics; legacy persistence corpus.
- Coverage gaps: Default collection is vulnerable to live/network/Tk imports;
  pytest could not be executed here because `python` is unavailable and
  `.venv/bin/python` lacks pytest. No full-shell integration, real subprocess,
  shutdown, differential, screenshot, accessibility, packaging smoke, or
  migration acceptance harness is committed.
- Status: complete

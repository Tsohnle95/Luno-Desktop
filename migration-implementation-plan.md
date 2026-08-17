# Desktop Migration Implementation Plan

## Executive Scope

Rewrite the Python/Tk/Pygame desktop application as React/TypeScript in a Tauri
2 shell with Rust-owned domain services, preserving current desktop behavior and
visual language. This is implementation guidance, not evidence that a target
exists. Python source/tests are authoritative. Android is outside this audit and
gate.

Preserve the three-column shell, Library, Recent, Discover, Downloader, Settings,
Error Log, Song Page, queue/player, all playlist and download workflows,
temporary preview/video isolation, `luno.library.export` v1, legacy persistence,
keyboard controls, and failure states. Release blockers are crossfade semantics,
direct-download output validation and atomic promotion, persistence atomicity,
provider-ID wiring, and complete end-to-end/visual/shutdown/real-data evidence.

## Authority And Current State

- `music_player.py` owns `_library`, views, settings, favorites, history, transfer
  orchestration, watchdog, media controls and UI callbacks. `engine.py` owns
  Pygame playback and queue state.
- `library_tools.py` scans immediate child folders as playlists and root files as
  Unsorted. `downloader.py` owns one normal download/write job and permanent
  44.1 kHz/stereo/192 kbps MP3/loudnorm output. `discovery.py`, `previews.py`,
  `video_preview.py` and artwork are separate asynchronous surfaces.
- Current config, URL metadata, duplicate exclusions, markers and manifest writes
  are not comprehensively atomic. Direct downloads can report success without
  checking audio output. Normal desktop transfer wiring does not pass a provider
  index. Crossfade helpers are not called by the transition path.
- Existing verification is strongest for pure playback, sync, Discover sessions,
  transfer validation, art cache, media keys and video relevance. Full-shell,
  persistence, real subprocess, visual, shutdown and end-to-end gates are absent
  or inconclusive (`migration-audit/11-verification-gaps.md:76-189`).

## Feature Matrix

| Feature | Current observed behavior | Compatibility guarantee | Target implementation | Evidence | Gate | Open decision/discrepancy | Fallback |
|---|---|---|---|---|---|---|---|
| Shell/navigation | 1180x800 start, 960x660 minimum, 3 columns, adaptive sidebar, 94 px player, 56 px rows | Same routes, panes, states, close and controls | React shell/CSS/typed events | `01-shell-navigation.md:7-50`; `02-visual-system.md:20-35` | Route/resize/screenshots | Fonts/breakpoints | Python |
| Library/watch | Immediate child playlists, root Unsorted, six extensions, coalesced scans | Same order, stale rejection, watchdog gating | Rust LibraryService/watch | `03-library-filesystem.md:7-22` | Fake filesystem/generation/UI | Symlink/permission fixtures | Manual scan |
| Playlist CRUD | Physical folders, URL metadata, safeguards, dump/copy, smart actions | Preserve folders, confirmations and protected roots | Typed mutation/copy/derived services | `03-library-filesystem.md:24-40`; `14-audit-review.md:45-49` | CRUD/delete/copy/smart tests | URL identity on rename | Explicit mapping |
| Edit/delete | Filename is metadata identity; rename repairs references; deletion confirms | Collision/sanitize/queue/history/favorite/art repair | Filesystem/index transaction or journal | `03-library-filesystem.md:78-94`; `13-user-journeys.md:61-76` | Race/partial-failure tests | Durable TrackId vs path | Path identity |
| Favorites/recent | Favorites durable normalized paths; history session-only max 100 | Prune/remap favorites; do not invent durable history | Library projection/session history | `04-persistence-migration.md:23-41` | Restart/rename/100/replay tests | Durable history | Session-only |
| Queue/playback | Context queue, Up Next, shuffle/repeat, clock/seek/completion/failure | Exact queue/timing semantics and media keys | Rust PlaybackService/backend | `05-playback-queue.md:7-95` | Headless conformance/manual formats | Backend selection | Python engine |
| Crossfade | Setting/helpers exist, transition does not invoke them | No claim until decided | Real overlap backend or approved no-op | `05-playback-queue.md:116-130` | Envelope/timing test | **BLOCKER** | No-op if approved |
| Downloads | One normal job; sync validates output; direct path can false-success | Validate every output before success and atomically promote | Rust subprocess supervisor/temp/rename | `06-download-sync.md:42-128` | Fake tools/partial/cancel/crash | **BLOCKER** | Block release |
| Search/direct/CSV | YouTube/SoundCloud, load more, HTTP validation, Exportify fields, row continuation | Same destinations, progress, logs and cancellation | Job commands/events | `06-download-sync.md:3-79` | Search/CSV/direct tests | SoundCloud error policy | Best effort |
| Sync/URL queue | Whole-root dedupe, markers, 120s timeout, sequential queue | Legacy markers and truthful partial results | Marker store/coordinator | `06-download-sync.md:81-130` | Marker/crash/timeout tests | URL queue durability | Session-only |
| Duplicates | Exact normalized groups, delete/allow, bulk exclusion and backup | Same normalization and confirmation | Duplicate service/validated backup | `06-download-sync.md:132-160` | Tag/allowed/malformed/delete tests | Backup version | Legacy read |
| Artwork | APIC, bounded cache, Deezer then YouTube missing-art fetch, fallback | Same limits/source order/stale handling | ArtworkService/tag job | `08-song-art-video.md:52-89` | Cache/APIC/fetch/visual tests | Retry exactness | Placeholder/log |
| Discovery/preview | Last.fm fallback/filter/cache; prefetch, tap, Discover adoption, save | Isolation, deadlines, generations, queue restoration | Discovery/Preview services | `07-discovery-previews.md:7-262` | Session/cancel/promotion tests | Save durability unknown | No crash promise |
| Song/video | Scroll page, art/recs, staged MP4, audio-clock frames/fullscreen | Optional video, never library/queue | React stage/Rust lifecycle | `08-song-art-video.md:8-167` | Fake decoder/process/manual | Tool/legal policy | Artwork-only |
| Transfer | Strict path-free v1, additive/no-delete, duration match; provider wiring incomplete | Exact fixtures/schema and truthful matching | TransferService/provider index | `10-transfer-contracts.md:3-25` | Provider/duration/copy/fixture tests | **BLOCKER**; ambiguity per record | Metadata fallback |
| Settings/log/release | Plaintext key, 500 memory logs, PyInstaller, no rollback | Legacy read, redaction, clean package and rollback | Settings/log/release services | `12-operations-release.md:81-240` | Corrupt/key/log/package/shutdown | Keychain/OS/signing open | Python artifact |

## Visual Token And UI Inventory

Canonical desktop values are `#121212`, `#181818`, `#282828`, `#1DB954`,
`#FFFFFF`, `#B3B3B3`, status colors and deterministic playlist gradients
(`theme.py:3-35`). Preserve Helvetica Neue/Helvetica and Menlo roles with
explicit fallback. Preserve 240/400/240 pane minimums, 94 px player, 220 px
Library header, 150 px artwork, 56 px rows, thin custom scrollbars, clamped
wheel/touchpad behavior, truncation, pulse bars and in-place art updates
(`02-visual-system.md:20-78`).

Create tokens and screenshot IDs for shell/sidebar/search/order, playlist cards,
Library actions/filter/sort/rows, Queue/Recent, player/sliders, all Downloader
tabs/tree/accordions, Settings, Error Log, Song Page art/recommendations/video,
dialogs, and every empty/loading/error/retry/selected/favorite/playing/
downloading/unavailable state. No screenshot or accessibility baseline exists;
source descriptions are not proof.

## Data And Persistence Migration

- Read `~/.luno_config.json` tolerantly; validate types/root, preserve it on
  failure, atomically write a versioned schema, and import a plaintext Last.fm
  key only through a real macOS Keychain adapter. Production currently uses
  `NoopKeychain`, so key-bearing legacy migration must block until that adapter
  is wired.
- Read the root URL map, allowed-duplicate JSON, `.downloaded_vids`,
  `.sync_failed_vids`, and legacy `.downloaded.txt`; canonicalize IDs, bind
  success only to verified output, and map URL names to stable playlists with
  explicit rename policy.
- Never scan/import/export `~/.luno_previews`; clean stale/partial files and
  defer current/promoted assets. Remap/prune path favorites. Do not invent
  durable history because desktop history is session-only.
- Preserve v1 exactly: strict path/unknown-field rejection, scopes, ordering,
  providers, ambiguity flags, additive copy, collision suffixes and no deletion.
  Build a provider-ID index and pass it to manifest build/import planning;
  ambiguity confirmation must be per record/match unless explicitly changed.
- New writers use same-directory temp files, flush/fsync, permission handling,
  atomic replace, backup/recovery metadata and interrupted-write tests. This is a
  blocker because current writers are direct and non-atomic.

## Dependency Graph And Ownership

```text
settings/root -> LibraryService -> LibraryIndex -> views/destinations/favorites
LibraryIndex -> playback context/queue -> player/Song Page/media session
yt-dlp+FFmpeg -> temp audio -> validate -> atomic promotion -> scan -> play
markers follow validated promotion; import/save/copy use the same boundary
Last.fm -> filter/cache -> preview staging -> temporary queue -> audio clock
preview -> explicit save/download -> permanent library -> scan
track identity/path -> rename/delete -> queue/history/favorites/art/transfer
playback clock -> player/Song Page scrubber/video frames/media session
all workers -> generation/session/shutdown guards -> UI event projection
```

Rust owns domain state, filesystem, tags, queues, subprocesses, persistence,
cancellation and stale-result validity. React owns rendering and transient
route/dialog/selection/focus/scroll state only. Typed capability-scoped Tauri
commands and generation/job/session-tagged events are the sole bridge. The v1
manifest remains language-neutral and excludes paths, secrets, audio, queue,
history and favorites.

## Ordered Phases

1. Freeze Python reference; capture all screens/states and a private corpus of
   corrupt config, roots, markers, favorites, partial outputs, previews,
   transfer manifests, provider IDs and ambiguous records.
2. Freeze v1 fixtures; define command/events, identity/rename, normalization,
   scan order, marker semantics and acceptance IDs; build differential fixtures.
3. Create Tauri/Rust foundation: bounded tasks, cancellation/join deadlines,
   subprocess registry, atomic persistence, redacted logs, capabilities and
   shutdown ownership.
4. Port library/persistence: scan/watch, playlists, URL metadata, favorites,
   rename/delete, dump/copy, smart actions, duplicates, missing-art fetch,
   legacy readers and provider-ID indexing.
5. Port playback/backend, 44.1 kHz output, queue, clock, seek, completion,
   shuffle/repeat, failure recovery, sleep timer and media session; resolve
   crossfade before parity.
6. Port shell/tokens/UI, virtual 56 px rows, scroll, player, queue, routes,
   selection/context menus, focus/accessibility and screenshot baselines.
7. Complete Library, Recent, Settings, Error Log, CRUD and transfer dialogs.
8. Port all seven Downloader tabs, output validation, atomic promotion, markers,
   logs, progress, destinations and cancellation.
9. Port discovery, prefetch, tap sessions, Discover mode, save, cleanup and
   paging with generation/session ownership.
   - 9W-1 (2026-08-16): production discovery wiring — `discover` command
     contract (both sides), keychain-key read into `DiscoveryService` at
     startup, dispatch with library filtering and truthful MissingApiKey /
     provider-error states, bounded limit; Discover view seeds from the
     current track with loading/error/empty states. Unit: `DISCOVER` gate
     (typecheck, 45 frontend, 98 Rust, diff --check).
   - 9W-2: preview sessions — tap/next/previous/cancel/invalidate commands,
     `PreviewAssetStore` + encode worker wiring, prefetch window and
     generation-guarded events.
   - 9W-3: Discover mode + save — enable/disable/advance, queue adoption
     handoff, explicit save-to-playlist with dedupe and staged-redownload
     avoidance.
   - 9W-4: Discover UI + player integration — artwork/CSP, preview playback
     through the native player, keyboard/accessibility pass, native
     verification receipt and sign-off annotation.
10. Port Song Page/art/video, audio-clock rendering, resize/seek/fullscreen and
    fallback.
11. Run differential, fake filesystem/subprocess, persistence crash, visual,
    accessibility, manual, shutdown and full-journey gates.
12. Package/sign, migrate first-run data, run clean-machine/user-data trial,
    staged rollout and Python rollback rehearsal.

## Verification, Corpus And Rollback

Automated gates cover Rust services/IPC, TypeScript interactions/accessibility,
Python-vs-target pure behavior, v1/provider/duration/ambiguity, fake filesystem,
fake yt-dlp/FFmpeg validation and cancellation, playback, preview/video,
persistence crash/restart and redaction. Visual/manual gates cover narrow,
default and wide windows, every route/state, long metadata, art fallback,
scroll/focus/keyboard/media keys, popup/fullscreen, missing binaries/providers,
real supported formats, and close during every operation. Differential journeys
cover scan-play-favorite-queue, discover-preview-save, download-scan-play, rename
repair, export-import-download-scan, video alongside audio, and crash/close at
each stage.

The private corpus includes missing/permission roots, unsupported files, mtime
ties, stale/moved favorites, malformed/interrupted config/URL/exclusion/markers,
legacy markers, duplicates, wrong/partial outputs, preview leftovers,
missing/invalid art, missing binaries, provider-ID and metadata-only manifests,
duration mismatches, collision copies and shutdown snapshots. Keep Python and a
known-good package until all gates pass. Snapshot durable inputs; on failure stop
target writes, preserve originals/backups, relaunch Python, and record the
migration version. Never delete audio or legacy metadata during rollback.

No missing dependency, skipped command, timeout, collection failure or truncated
receipt is a pass. Current local pytest inconclusive receipts remain
inconclusive until the configured desktop gate runs. No Android/Gradle command is
used here.

## Blockers And Unresolved Decisions

- Crossfade decision and audible envelope test.
- Direct-download expected-audio validation, partial cleanup and atomic promotion.
- Atomic/corruption-safe config, URL, exclusion, marker and manifest persistence.
- Provider-ID index/controller wiring; current codec support is not sufficient.
- TrackId versus filename/path identity; URL mapping on rename; per-record
  ambiguity; durable history; keychain import; video/audio backend; FFmpeg/yt-dlp
  distribution/licensing; supported OS/signing/update policy; fonts, breakpoints
  and visual tolerances. Video commands remain explicitly unavailable. The shell
  geometry check is a contract check, not rendered screenshot evidence.

## Definition Of Done

Replace Python only after every matrix item has target code, automated evidence,
visual/manual receipt where required, and a completed real-user-data migration
and rollback trial. Until then Python is the behavioral fallback.

## Audit Receipt

- Files inspected: `AGENTS.md`, `README.md`, `project_brain.md`,
  `migration-plan.md`, `migration-audit/README.md`, completed dossiers `00-08`
  and `10-14`, cited desktop source/tests, contract fixtures and build/CI files.
- Tests/fixtures inspected: desktop playback, sync, Discover, transfer,
  artwork, media-key, video and contract tests/receipts cited by those dossiers.
  No Android/Gradle tests were run.
- Claims requiring review: crossfade, direct output validation, persistence
  atomicity, provider-ID wiring, identity/ambiguity, visual/manual coverage,
  shutdown, packaging and rollback.
- Coverage gaps: target source is present but the complete target harness, screenshots,
  real-process corpus, packaged smoke receipt, crash corpus or rollback trial.
- Status: complete, with blockers unresolved.

## Historical U45 Final Gate (Superseded)

- Status: **BLOCKED** on 2026-08-13; no parity or release sign-off is claimed.
- Final reviewer findings: production still uses `NoopKeychain`, so key-bearing
  legacy migration must block until a real macOS Keychain adapter is wired;
  frontend backend/video availability remains explicitly unavailable because
  runtime IPC is not implemented; and the shell geometry check is contract
  evidence only, not rendered screenshot evidence.
- Reference-only checks passed (`python3 -m pytest -q`: 109 passed; U44 subset:
  6 passed; both reference/journey manifests parse; `git diff --check` passed).
- Required target receipts remain unresolved: `cargo test`, `npm run typecheck`,
  target differential and journey execution, visual screenshots, accessibility,
  packaged clean-machine smoke, signing/notarization, migration/rollback corpus,
  and shutdown/orphan-process checks.
- Release inputs are incomplete: managed binary input files are absent and
  `release/desktop-binaries.json` retains placeholder checksums.
- Keep the Python fallback and legacy data unchanged. Reopen sign-off only when
  `docs/verification/final-parity.md` has no inconclusive or blocked acceptance
  and `docs/release/sign-off.md` is explicitly approved.

## Historical R07 Final Audit (Superseded)

- Status: **BLOCKED** on 2026-08-14. The audited source HEAD is
  `cba3a17a1fce8cd4faed98f510da5039f1776702`; the audited worktree was dirty
  only in pre-existing `TODO.md` and `dispatch-plan.json`.
- The R06 manifest mechanically contains 37 feature IDs and 6 journey IDs;
  all 43 referenced artifacts exist. Results are 2 reference-only feature
  passes, 35 inconclusive feature IDs, 6 inconclusive journeys, 1 failed
  receipt (`rust_cargo`, exit 101), and 5 release-blocker IDs. R05 remains
  BLOCKED with no package, managed binaries, reviewed checksums, signing
  credentials, or signed/notarized artifact. No standalone R04 receipt file
  is present under `docs/`.
- R07 ran `python3 -m pytest -q` (`109 passed`), parsed the R05/R06 and test
  manifests, mechanically checked every referenced R06 artifact, and ran
  `git diff --check`. All available checks passed; they do not convert
  target-unavailable or reference-only evidence into parity.
- The release remains blocked by unavailable target migration rejection,
  download workflow, transfer handoff, persistence recovery, and packaging
  evidence, plus the failed Rust gate. Browser/runtime/journey/video/shutdown
  checks remain inconclusive, not failed, where the required tool or manual
  run was unavailable. No binary, screenshot, user data, signature, or package
  was fabricated.

## Current Release Evidence

- Status: **BLOCKED** on 2026-08-14; no parity or release sign-off is claimed.
- Audited source HEAD is
  `969a83319a96603a3608c041fc8abf28e2f8a3f7`. The exact first-parent command is
  `git log --first-parent --format='%H %s' --reverse 93915b329b35085358c8924017a93bbb1ee14fd7^..969a83319a96603a3608c041fc8abf28e2f8a3f7`,
  covering `93915b32`, `8343bfbf`, `e6d2372c`, and `969a8331`.
- Current receipts pass: `npm ci`, `npm run typecheck`, `npm run build`,
  `npm test` (13 passed), `cargo test --manifest-path src-tauri/Cargo.toml`
  (31 passed, 1 ignored), and `npm run tauri build` (unsigned local aarch64 DMG).
- Video commands remain explicitly unavailable. Target runtime, visual,
  accessibility, migration/rollback, shutdown/orphan-process, manual journey,
  and real-user evidence remains inconclusive because no reviewed execution
  receipt exists. Cargo/unit tests pass, but physical macOS audio
  backend/device/codec target-runtime validation and packaged target playback
  were not exercised; this is distinct from video backend unavailability.
- Release inputs remain incomplete: `release-input/bin/` is absent and
  `release/desktop-binaries.json` retains placeholder checksums. No signing or
  notarization evidence exists.
- Keep the Python fallback and legacy data unchanged. Reopen sign-off only when
  the required target and release evidence is reviewed and complete; this
  document does not claim release readiness.

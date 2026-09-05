# Desktop Migration Synthesis

Status: complete, with explicit blockers below.

## Executive Scope

Migrate the implemented Python/Tk/Pygame desktop product to React/TypeScript,
Tauri 2, and Rust. This is a behavioral and visual rewrite, not a redesign.
Python desktop source and deterministic desktop tests are the authority; the
target does not exist in this repository. Preserve the shell, Library, Recent,
Discover, Downloader, Settings, Error Log, Song Page, queue/player, playlists,
downloads, discovery/previews, video, transfer contract, persistence
compatibility, keyboard controls, and failure states. Android is not in scope.

Release blockers are crossfade semantics, validated direct-download output and
atomic promotion, persistence atomicity/recovery, provider-ID controller
wiring, and complete end-to-end/visual/shutdown/real-data verification. A
timeout, skipped command, collection failure, missing dependency, or truncated
receipt is inconclusive, never a pass.

## Authority And Current State

| Area | Current observed behavior | Compatibility guarantee | Target implementation | Evidence | Gate | Open decision/discrepancy | Fallback |
|---|---|---|---|---|---|---|---|
| Boundary | Python/Tk owns audited desktop workflows; no target source | Desktop only; no invented behavior | Tauri shell, React, Rust services | `README.md:3-22`; `00-repository-census.md:41-75` | Build/smoke | None | Python release |
| Shell | Three columns, adaptive sidebar, queue, 94 px player, 56 px rows, clamped scroll | Same geometry, routes, states, interaction | React grid/flex, virtual rows, typed events | `01-shell-navigation.md:7-50`; `02-visual-system.md:20-35` | Route/resize/screenshots | Font/breakpoints | Reference captures |
| Library | Immediate child folders are playlists; root tracks are Unsorted; scans coalesce and reject stale generations | Same extensions, order, async behavior | Rust LibraryService/watch/reconcile | `03-library-filesystem.md:7-22` | Fake filesystem/generation/UI integration | Permission/symlink fixtures | Manual full scan |
| Playlists | Physical-folder CRUD, URL metadata, safeguards, dump/copy, smart actions | Preserve folders, confirmations, protected roots | Typed mutation/copy/derived-playlist services | `03-library-filesystem.md:24-40`; `14-audit-review.md:45-49` | CRUD/delete/copy/smart tests | URL identity on rename | Name mapping confirmation |
| Playback | Pygame 44.1 kHz stereo; context queue, Up Next, shuffle/repeat, clock/seek/completion/failure | Exact queue and timing semantics | Rust backend and PlaybackService | `05-playback-queue.md:7-95`; `AGENTS.md:10-16` | Headless conformance/manual formats | Backend/crossfade | Python engine |
| Crossfade | Preference/helpers exist, transition path does not invoke them | Do not claim audible parity | Real overlap backend or approved no-op | `05-playback-queue.md:116-130` | Envelope/timing test | **BLOCKER** | Documented no-op |
| Downloads | One normal write job; permanent output 44.1 kHz/192k MP3/loudnorm; sync checks output, direct path does not | Every path validates audio before success | Rust subprocess supervisor, temp output, atomic promotion | `06-download-sync.md:42-128`; `project_brain.md:165-173` | Fake tools/missing partial/cancel/crash | **BLOCKER** direct validation/atomicity | Block release |
| Persistence | Config, URLs, exclusions, markers, manifest writes are direct/non-atomic; history session-only | Read legacy without loss; no invented history | Versioned atomic readers/writers and recovery | `04-persistence-migration.md:7-21,43-97` | Interrupted/corrupt/restart corpus | **BLOCKER**; keychain open | Preserve originals/backups |
| Transfer | Strict path-free v1, additive/no-delete; controller does not pass provider index | Exact schema/fixtures; truthful matching | Rust TransferService plus provider index | `10-transfer-contracts.md:3-25` | Fixtures/provider/duration/copy tests | **BLOCKER** provider wiring; global ambiguity | Metadata match + confirmation |
| Discovery/preview | Last.fm fallback/filter/cache; first-five staging; temporary MP3, queue adoption/restoration, explicit save | Isolation, deadlines, generations, save truth | Rust Discovery/Preview services | `07-discovery-previews.md:7-262` | Session/cancel/promotion tests | Queued-save durability unknown | No crash-recovery promise |
| Song/video | Scrollable page, art/recs, staged MP4, audio-clock frames, fullscreen | Optional video, never library/queue | React stage plus Rust process lifecycle | `08-song-art-video.md:8-167` | Fake decoder/process/manual | Tool/legal policy | Artwork fallback |
| Artwork | APIC first; bounded cache; Deezer then YouTube missing-art fetch; deterministic fallback | Cache limits, source order, stale guards | Rust ArtworkService/tag job | `08-song-art-video.md:52-89` | Cache/APIC/fetch/visual tests | Fetch retry exactness | Placeholder/log |
| Release | PyInstaller macOS baseline, no packaged smoke/signing/rollback | Clean package, migration and rollback | Tauri bundle/provisioning/release | `12-operations-release.md:7-39,187-240` | Package/clean-machine/shutdown | OS/signing/update policy | Previous Python artifact |

## Complete Feature Coverage Matrix

| Feature | Current status | Acceptance |
|---|---|---|
| Startup/config/root | Implemented; tolerant keys, direct write | Valid/missing/corrupt/unwritable config; legacy keys; atomic versioned write |
| Navigation/shell/player | Implemented | All routes, adaptive menu, panes, player, close, media keys, focus |
| Scan/watch | Implemented | Six extensions, Unsorted newest-first/ties, coalescing, stale rejection, watchdog gating |
| All Music/Unsorted/Favorites | Implemented | Virtual scopes, favorite pruning/order, no Favorites folder |
| Playlist CRUD/URL | Implemented | Sanitization, empty-folder visibility, URL validation, rename mapping |
| Empty/delete safeguards | Implemented | Root/protected safety, confirm, partial error reporting |
| Dump/copy | Implemented UI surface | Explicit destination, collision/partial errors, refresh, no deletion |
| Smart actions | Implemented inventory | Most Played, Recently Added, artist copies with expected content/order |
| Search/sort/selection | Implemented | Filter/clear, four sorts, 56 px rows, range/multi-selection/context menus |
| Edit/delete | Implemented | Rename collision/sanitize/reference repair; confirmed permanent deletion |
| Queue/recent | Implemented | Up Next ordering, shuffle restoration, repeat, 100-entry session history, scroll retention |
| Crossfade | Unknown | Baseline decision and audible test before parity sign-off |
| Discover hub | Implemented | Key/no-key/loading/error/empty, fallback, cache/filter/generation, save states |
| Preview/Discover mode | Implemented | Prefetch, tap, rolling window, handoff/restoration, deadlines, cleanup, explicit save |
| Song Page/video | Implemented desktop; renderer gaps | Paging to 1,000, seed pinning, art, 16:9 video, audio-clock pause/seek/resize/fullscreen |
| Downloader search | Implemented | YouTube/SoundCloud, load more, artwork, bounded work, one write job |
| Direct/CSV | Implemented with direct false-success gap | HTTP validation, CSV columns/quoting, per-row continuation, verified output |
| Sync/URL Queue | Implemented | Whole-root dedupe, list parsing, markers, 120s timeout, sequential truthful results |
| Duplicates/allowed | Implemented | Exact normalization, review/play/delete/allow, bulk action, validated backup |
| Missing artwork | Implemented desktop workflow | Deezer/YouTube fallback, JPEG APIC, progress, success/failure logs |
| Export/import | Implemented with wiring/atomicity gaps | Frozen v1, provider index, duration/ambiguity, additive/no-delete, handoff |
| Settings/log | Implemented | Immediate settings, sleep timer, crossfade decision, keys, art fetch, redacted logs |
| Packaging/shutdown | Python baseline | Tauri package, dependency preflight, bounded cancellation/join, no orphans, rollback |

## Visual Token And UI Inventory

Use desktop source values as canonical: `#121212`, `#181818`, `#282828`,
`#1DB954`, `#FFFFFF`, `#B3B3B3`, status colors, and ten deterministic playlist
gradient pairs (`theme.py:3-35`). Preserve Helvetica Neue/Helvetica and Menlo
roles with explicit fallback. Preserve 240/400/240 pane minimums, 94 px player,
220 px Library header, 150 px artwork, 56 px virtual rows, thin custom
scrollbars, clamped wheel/touchpad behavior, width-aware truncation, pulse bars,
and in-place artwork/queue updates (`02-visual-system.md:20-78`).

Inventory and test shell/sidebar/search/order, playlist cards, Library header and
actions, filter/sort, track rows, Queue/Recent, player/sliders, all Downloader
tabs/tree/accordions, Settings, Error Log, Song Page blur/art/recommendations/
video popup, dialogs, and empty/loading/error/retry/selected/favorite/playing/
downloading/unavailable states. Create CSS tokens with source value, rationale,
breakpoint, keyboard/focus behavior, screenshot ID, and tolerance. No committed
screenshot or accessibility baseline exists; source text is not visual proof.

## Data And Persistence Migration Map

| Input | Observed contract | Target migration |
|---|---|---|
| `~/.luno_config.json` | Root, key, discovery, volume, crossfade, sleep, media keys, favorites | Tolerant reader, type/root validation, optional one-time keychain import, atomic versioned writer |
| Root URL map | Name-keyed, unversioned direct JSON | Legacy read; stable playlist mapping and explicit rename policy; atomic writer |
| Allowed duplicates | Local normalized-key JSON list | Validate and preserve portable backup; atomic writer; exclude v1 |
| `.downloaded_vids`/`.sync_failed_vids` | Plain markers; success/failure IDs/reasons | Canonicalize; write only after verified output; atomic update |
| `.downloaded.txt` | Legacy final-token video IDs | Compatibility import with audit trail |
| `~/.luno_previews` | Disposable MP3/MP4 outside library | Never scan/export; remove stale/partial; defer current/promoted assets |
| Favorites/history | Favorites durable paths; history session-only max 100 | Remap/prune favorites; do not invent durable history |
| v1 manifest | Strict path-free metadata/playlists/provider IDs | Exact wire compatibility; atomic target export; provider index; per-record ambiguity |

All target writers use same-directory temp files, flush/fsync, permission handling,
atomic replace, backup/recovery metadata, and interrupted-write tests. This is a
blocker because current writers do not provide it.

## Cross-Feature Dependency Graph

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

Rust owns typed domain state, filesystem, metadata/tag mutation, queues,
subprocesses, persistence, cancellation and stale-result validity. React owns
rendering and transient route/dialog/selection/focus/scroll state only. Tauri
commands are typed and capability-scoped; events carry job/generation/session
IDs. The v1 manifest remains language-neutral and excludes paths, secrets,
audio, queue, history and favorites.

## Ordered Phases

1. Freeze Python reference; capture all screens/states and build a private corpus
   of corrupt config, roots, markers, favorites, partial outputs, previews,
   transfer manifests, provider IDs and ambiguous records.
2. Freeze v1 fixtures; define command/event schemas, identity/rename policy,
   normalization, scan order, marker semantics and acceptance IDs; build Python
   versus Rust differential fixtures.
3. Create Tauri/Rust foundation: bounded tasks, cancellation/join deadlines,
   subprocess registry, atomic persistence, redacted logs, capabilities and
   shutdown ownership.
4. Port library/persistence: scan/watch, playlists, URL metadata, favorites,
   rename/delete, dump/copy, smart actions, duplicates, missing-art fetch,
   legacy readers and provider-ID indexing.
5. Port playback: backend, 44.1 kHz output, queue, clock, seek, completion,
   shuffle/repeat, failure recovery, sleep timer, media session; resolve crossfade.
6. Port shell/tokens/UI foundation, 56 px virtual rows, scroll, player, queue,
   routes, selection/context menus, focus and accessibility; add visual baselines.
7. Complete Library, Recent, Settings, Error Log, CRUD and transfer dialogs.
8. Port all seven Downloader tabs, output validation, atomic promotion, markers,
   logs, progress, destination behavior and cancellation.
9. Port discovery, prefetch, tap sessions, Discover mode, save, cleanup and
   paging with generation/session ownership.
10. Port Song Page/art/video, audio-clock rendering, resize/seek/fullscreen and
    graceful fallback.
11. Run differential, fake filesystem/subprocess, persistence crash, visual,
    accessibility, manual, shutdown and full journey gates.
12. Package/sign, migrate first-run data, perform clean-machine/user-data trial,
    staged rollout and Python rollback rehearsal.

## Verification Gates

Automated gates: Rust service and IPC tests; TypeScript interaction/accessibility
tests; Python-vs-target pure behavior; v1 fixtures/provider/duration/ambiguity;
fake filesystem and stale generation; fake yt-dlp/FFmpeg output validation,
partial/cancel/deadline/terminate-kill/no-detached-worker; playback clock/queue;
preview/video lifecycle; persistence interruption/corruption/restart; and
secret redaction. Visual/manual gates cover every route at narrow/default/wide
sizes, long metadata, art fallback, scroll/focus/keyboard/media keys, popup and
fullscreen, missing binaries/providers, real supported audio formats, and close
during every operation. Differential journeys cover scan-play-favorite-queue,
discover-preview-save, download-scan-play, rename reference repair,
export-import-download-scan, video beside audio, and crash/close at each stage.

Current inconclusive local pytest receipts must remain labeled inconclusive until
the configured desktop environment runs the complete gate. No Android/Gradle
command is a gate for this plan.

## Corpus, Rollback, Blockers And Decisions

The private corpus includes missing/permission roots, unsupported files, mtime
ties, stale/moved favorites, malformed/interrupted config/URL/exclusion/markers,
legacy markers, duplicates, wrong/partial download outputs, preview leftovers,
missing/invalid art, missing binaries, provider-ID and metadata-only manifests,
duration mismatches, collision copies, and shutdown snapshots. Keep Python and a
known-good package until all gates pass. Snapshot durable inputs; on failure stop
target writes, preserve originals/backups, relaunch Python, and record migration
version. Never delete audio or legacy metadata during rollback.

Explicit blockers: crossfade decision/test; direct output validation and atomic
promotion; atomic/corruption-safe persistence; provider-ID index/controller
wiring; full end-to-end, visual, accessibility and shutdown evidence. Decisions:
TrackId versus filename identity, URL mapping on rename, per-record ambiguity,
durable history, keychain import, video/audio backend, FFmpeg/yt-dlp distribution
and licensing, supported OS/signing/update policy, fonts, breakpoints and visual
tolerances.

## Audit Receipt

- Files inspected: `AGENTS.md`, `README.md`, `project_brain.md`,
  `migration-plan.md`, `migration-audit/README.md`, completed dossiers `00-08`
  and `10-14`, cited desktop source/tests, contract fixtures and build/CI files.
- Tests/fixtures inspected: all desktop playback, sync, Discover, transfer,
  artwork, media-key, video and contract tests/receipts cited by those dossiers.
  No Android/Gradle tests were run.
- Claims requiring review: crossfade, direct output validation, persistence
  atomicity, provider-ID wiring, identity/ambiguity, visual/manual coverage,
  shutdown, packaging and rollback.
- Coverage gaps: no target source, complete target harness, screenshots,
  real-process corpus, packaged smoke receipt, crash corpus or rollback trial.
- Status: complete, with blockers unresolved.

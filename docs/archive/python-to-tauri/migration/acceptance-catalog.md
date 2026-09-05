# Desktop Migration Acceptance Catalog

This catalog freezes acceptance IDs for the Python reference and future desktop
migration. Evidence pointers identify source-backed behavior; they do not claim
that the React/Tauri/Rust target is implemented. `reference-corpus.md` contains
the sanitized fixture IDs.

## Feature Acceptance

| ID | Feature | Acceptance statement | Evidence |
|---|---|---|---|
| AC-SHELL-01 | Shell/navigation | Preserve three columns, adaptive sidebar, routes, queue, player, close lifecycle, and bounded scrolling. | `migration-audit/01-shell-navigation.md:7-50`; `migration-audit/02-visual-system.md:59-78` |
| AC-SHELL-02 | Visual system | Preserve desktop colors, typography roles, 56 px rows, artwork sizing, truncation, interaction states, and responsive geometry. | `migration-audit/02-visual-system.md:3-36` |
| AC-LIB-01 | Root handling | Missing, empty, and inaccessible roots must be safe and distinguishable from fabricated content. | `migration-audit/03-library-filesystem.md:7-22` |
| AC-LIB-02 | Scan contract | Scan immediate child folders as playlists, root audio as Unsorted, and only the six supported extensions. | `migration-audit/03-library-filesystem.md:7-20` |
| AC-LIB-03 | Ordering | Preserve Unsorted newest-first ordering and deterministic filename tie-breaking. | `migration-audit/03-library-filesystem.md:14-20` |
| AC-LIB-04 | Scan generations | Coalesce refreshes, reject stale generations, and deliver results off-worker onto the UI/event boundary. | `migration-audit/03-library-filesystem.md:15-20` |
| AC-LIB-05 | Favorites/recent | Favorites are local normalized paths, projected virtually and pruned; recent history remains session-only and capped at 100. | `migration-audit/03-library-filesystem.md:42-58`; `migration-audit/04-persistence-migration.md:23-41` |
| AC-LIB-06 | Playlist/edit/delete | Preserve sanitized folder CRUD, safeguards, rename collision handling, reference repair, and confirmed deletion. | `migration-audit/03-library-filesystem.md:24-40,78-94` |
| AC-PLAY-01 | Context queue | Starting a track replaces the playback context and selects the correct current index. | `migration-audit/05-playback-queue.md:7-25` |
| AC-PLAY-02 | Up Next | Play Next and Add to Queue preserve manual insertion ordering and queue clear/reorder behavior. | `migration-audit/05-playback-queue.md:7-25` |
| AC-PLAY-03 | Shuffle/repeat | Shuffle keeps current first and restores order; repeat behavior matches the Python engine. | `migration-audit/05-playback-queue.md:45-60` |
| AC-PLAY-04 | Clock/seek/completion | Preserve mixer-clock precedence, monotonic fallback, seek clamping, pause position, idle completion, and exactly-once advancement. | `migration-audit/05-playback-queue.md:62-78` |
| AC-PLAY-05 | Failure recovery | Missing/unloadable tracks log, skip asynchronously, and stop after the observed consecutive-failure threshold. | `migration-audit/05-playback-queue.md:80-97` |
| AC-PLAY-06 | Crossfade decision | Target v1 retires the legacy crossfade preference and behavior: transitions are immediate, crossfade parity is not claimed, and the retired feature is not an unresolved blocker. | `migration-audit/05-playback-queue.md:116-130` |
| AC-DL-01 | Download coordination | Permit one normal write job, expose busy/cancel states, and keep heavy work off the UI thread. | `migration-audit/06-download-sync.md:3-40` |
| AC-DL-02 | Output validation | Every download reports success only after expected audio output is validated and safely promoted. | `migration-audit/06-download-sync.md:42-79`; `migration-audit/14-audit-review.md:25-37` |
| AC-DL-03 | Sync/markers | Preserve whole-root dedupe, list parsing, sequential sync, 120-second timeout, legacy markers, and verified success markers. | `migration-audit/06-download-sync.md:81-130` |
| AC-DL-04 | Search/direct/CSV | Preserve search destinations, HTTP validation, CSV columns, row continuation, progress, and truthful errors. | `migration-audit/06-download-sync.md:3-79` |
| AC-DISC-01 | Discovery | Preserve seed precedence, Last.fm fallback, key/error/empty states, bounded requests, and UI-thread delivery. | `migration-audit/07-discovery-previews.md:8-45` |
| AC-DISC-02 | Recommendation filtering/art | Filter local and duplicate results; preserve limit-sensitive cache and deterministic fallback art. | `migration-audit/07-discovery-previews.md:47-89` |
| AC-DISC-03 | Preview isolation | Preview audio is temporary, outside library/export, encoded without loudnorm, and adopted without redownload when staged. | `migration-audit/07-discovery-previews.md:91-127` |
| AC-DISC-04 | Preview sessions | Preserve deadlines, cancellation, generation guards, first-five prefetch, five-ahead window, and retryable failures. | `migration-audit/07-discovery-previews.md:129-189` |
| AC-DISC-05 | Discover mode | Preserve queue capture/adoption, natural handoff, next/previous routing, restoration, and queue-intervention handling. | `migration-audit/07-discovery-previews.md:191-226` |
| AC-DISC-06 | Explicit save | Require a playlist, avoid staged redownloads, guard duplicate saves, and expose completion/failure truthfully. | `migration-audit/07-discovery-previews.md:228-262` |
| AC-ART-01 | Artwork/cache | Preserve APIC validation, source priority, bounded caches, stale-result rejection, and deterministic placeholders. | `migration-audit/08-song-art-video.md:52-89` |
| AC-VIDEO-01 | Video source/staging | Preserve relevance search, canonical IDs, temporary MP4 staging, terminal preload states, and artwork fallback. | `migration-audit/08-song-art-video.md:91-130` |
| AC-VIDEO-02 | Audio-clock video | Video follows audio for pause, seek, resize, EOF, fullscreen, and track changes without entering queue/library. | `migration-audit/08-song-art-video.md:132-167` |
| AC-VIDEO-03 | Video lifecycle | Preserve audio-clock pause/seek/resize/fullscreen cleanup and graceful degradation when tools or sources are unavailable. | `migration-audit/08-song-art-video.md:132-167` |
| AC-XFER-01 | Transfer contract | Preserve exact `luno.library.export` v1 schema, ordering, provider IDs, and ambiguity flags. | `migration-audit/10-transfer-contracts.md:3-23` |
| AC-XFER-02 | Transfer rejection | Reject unknown/path/audio/secret fields and retain additive/no-delete semantics. | `migration-audit/10-transfer-contracts.md:6-22` |
| AC-XFER-03 | Transfer matching | Match compatible metadata/duration, preserve playlist order, and apply sanitized collision suffixes. | `migration-audit/10-transfer-contracts.md:10-23` |
| AC-XFER-04 | Transfer handoff | Report missing/ambiguous records and do not overstate provider-ID support absent controller wiring. | `migration-audit/10-transfer-contracts.md:21-25` |
| AC-PERSIST-01 | Config | Read legacy/tolerant config without data loss; preserve keys/defaults and migrate to atomic versioned writes. | `migration-audit/04-persistence-migration.md:7-21` |
| AC-PERSIST-02 | Markers/URLs | Read URL, allowed-duplicate, success/failure, and legacy marker files with corruption tolerance and retryability. | `migration-audit/04-persistence-migration.md:43-60` |
| AC-OPS-01 | Diagnostics | Keep bounded error diagnostics, redact secrets/credential-bearing URLs, and preserve user-visible failure states. | `migration-audit/12-operations-release.md:118-147` |
| AC-OPS-02 | Packaging | Build from a portable environment, preserve bundle identity/assets, and preflight external media dependencies. | `migration-audit/12-operations-release.md:7-79` |
| AC-OPS-03 | Shutdown/rollback | Cancel and boundedly join work, clean temporary assets, suppress late callbacks, and preserve originals for rollback. | `migration-audit/12-operations-release.md:149-218`; `docs/migration/rollback.md` |

## Journey Acceptance

| ID | Journey | Required observable sequence | Evidence |
|---|---|---|---|
| JRN-SCAN-PLAY | Scan, browse, play, queue, favorite | Scan -> project scopes -> play context -> Up Next -> favorite/prune without exporting runtime state. | `migration-audit/13-user-journeys.md:8-24` |
| JRN-DISCOVER | Discover, preview, save | Recommend -> filter -> prefetch/tap -> temporary playback -> explicit playlist save -> scan visibility. | `migration-audit/13-user-journeys.md:26-42` |
| JRN-DOWNLOAD | Download, scan, play | Download/search/sync -> validated output -> rescan -> browse/play, with busy/cancel/failure truth. | `migration-audit/13-user-journeys.md:44-59` |
| JRN-RENAME | Rename, reference repair, rescan | Sanitize/collision-check -> rename -> repair queue/history/favorite/art references -> rescan. | `migration-audit/13-user-journeys.md:61-76` |
| JRN-TRANSFER | Export, import, download | Validate path-free manifest -> additive match/copy -> report missing/ambiguity -> optional handoff -> rescan. | `migration-audit/13-user-journeys.md:78-93` |
| JRN-VIDEO | Video alongside audio | Probe/stage optional video -> play beside audio -> pause/seek/resize/fullscreen -> cleanup; audio and library remain authoritative. | `migration-audit/13-user-journeys.md:95-110` |

## Decisions And Gaps

- `AC-PLAY-06` has an explicit target v1 decision: legacy crossfade preference
  and behavior are retired, with immediate transitions and no crossfade
  control. Crossfade parity is not claimed, and its absence is no longer an
  unresolved implementation blocker. The migration-audit crossfade finding is
  preserved as historical source evidence. `AC-DL-02`, `AC-PERSIST-01`, and
  `AC-XFER-04` remain release blockers until the direct-output atomicity,
  persistence recovery, and provider-index decisions are implemented and
  tested.
- Visual/manual IDs are represented by `CAP-*` in `reference-corpus.md`; no
  source-only citation is a visual pass.
- The configured desktop gate is `python -m pytest`. U01 receipt: exit `127`,
  `zsh:1: command not found: python` (inconclusive). Any unavailable, skipped,
  timed-out, collection-failing, or truncated run is inconclusive. Manifest
  syntax passed with `jq empty tests/reference/manifest.json` (exit `0`), and
  `git diff --check` passed (exit `0`).
- Unit handoff receipt: files changed are exactly this catalog,
  `docs/migration/reference-corpus.md`, and `tests/reference/manifest.json`;
  no target code, user data, secrets, audio, or absolute paths are included.

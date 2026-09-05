# Python Reference Corpus

Status: frozen for Wave 0 on 2026-08-13. This file describes deterministic,
sanitized inputs and expected observations for the Python desktop reference.
It is an inventory, not a copy of user data. Corpus records contain no audio,
secrets, absolute paths, network responses, or target implementation details.

## Rules

- Use synthetic roots such as `fixture://library/basic`, never a host path.
- Use placeholder values such as `artist-a`, `track-a`, and `provider-video-1`.
- Generate audio and image bytes in the test harness only; do not check them in.
- Keep expected state focused on observable metadata, ordering, events, and
  terminal states. Do not encode implementation-specific object layouts.
- Treat `contracts/library-export/v1/{valid.json,invalid-unknown-field.json}`
  as frozen wire fixtures. The manifest below adds sanitized scenario metadata
  and does not replace those contract fixtures.

## Fixture Inventory

| Fixture ID | Scenario | Sanitized inputs | Expected observations | Acceptance |
|---|---|---|---|---|
| REF-ROOT-MISSING | Missing library root | `fixture://missing-root` | Safe empty/error result; no fabricated tracks | AC-LIB-01 |
| REF-ROOT-EMPTY | Empty root | `fixture://empty-root` | Empty `Unsorted`; no crash | AC-LIB-02 |
| REF-SCAN-ORDER | Root and child folders | `fixture://scan-order`, files `artist-a - track-a.mp3`, `track-b.flac`, unsupported `notes.txt` | Immediate child folders are playlists; root audio is `Unsorted`; supported extensions only; root order is newest-first with filename tie-break | AC-LIB-03 |
| REF-SCAN-GENERATION | Overlapping scans | generations `7`, `8`; synthetic delayed result for `7` | Newer generation publishes; stale result is ignored; requests coalesce | AC-LIB-04 |
| REF-PLAY-CONTEXT | Play from context | tracks `track-a`, `track-b`, `track-c` | Context queue replaces prior context and starts selected index | AC-PLAY-01, JRN-SCAN-PLAY |
| REF-QUEUE-UP-NEXT | Manual queue insertion | current `track-a`; context `track-b`, `track-c`; manual `track-x` | Play Next is immediately next; Add to Queue follows existing manual items | AC-PLAY-02, JRN-SCAN-PLAY |
| REF-SHUFFLE-REPEAT | Shuffle and repeat | four synthetic tracks | Current track remains first; disabling shuffle restores order; repeat restarts current track | AC-PLAY-03 |
| REF-CLOCK-SEEK | Clock and completion | duration `120`; seek positions `0`, `60`, `120`; unknown duration | Position clamps; pause/resume preserves position; completion advances once for duration or mixer-idle path | AC-PLAY-04 |
| REF-FAILURE-SKIP | Playback failure | missing/unloadable synthetic tracks | Error is logged, failed track is skipped asynchronously, repeated failures stop after threshold | AC-PLAY-05 |
| REF-FAVORITES | Favorite projection | normalized synthetic paths, one missing path | Virtual Favorites contains live paths only; favorite state is local and not exportable | AC-LIB-05, JRN-SCAN-PLAY |
| REF-RENAME-REPAIR | Filename edit | `artist-a - track-a.mp3` -> `artist-b - track-b.mp3` | Sanitized rename updates queue, recent, favorite, now-playing references and rescans | AC-LIB-06, JRN-RENAME |
| REF-URL-MARKERS | URL and sync markers | synthetic playlist URL; success/failure IDs; legacy marker line | Whole-root dedupe; sequential sync; success only after audio output; permanent failures are retained; legacy ID is readable | AC-DL-03, AC-PERSIST-02 |
| REF-DIRECT-OUTPUT | Direct download terminal states | fake downloader completion with missing/partial/valid output | Missing or partial output is failure, not false success; valid output is normalized and discoverable | AC-DL-02, AC-DL-04, JRN-DOWNLOAD |
| REF-DOWNLOAD-JOB | Download coordination | two synthetic download requests; cancellation token | One normal write job; busy rejection; cooperative cancellation and bounded UI updates | AC-DL-01 |
| REF-DISCOVERY | Recommendation request | seed `artist-a / track-a`; empty first stage; fallback results; duplicate local result | Similar-track lookup falls back to artist top tracks; local/intra-result duplicates are filtered; missing key/error/empty states are honest | AC-DISC-01, AC-DISC-02 |
| REF-PREVIEW-SESSION | Temporary recommendation preview | five synthetic recommendations; staged and unstaged rows | Prefetch does not alter library; tap session uses a five-ahead window; staged assets are adopted; failures are retryable | AC-DISC-03, AC-DISC-04, JRN-DISCOVER |
| REF-DISCOVER-HANDOFF | Discover mode lifecycle | physical queue plus ready recommendation assets | Captured queue is restored on teardown; natural handoff and queue replacement are respected; preview files stay outside library | AC-DISC-05, JRN-DISCOVER |
| REF-PREVIEW-SAVE | Explicit promotion | staged recommendation and destination `playlist-a` | Save requires destination; staged copy does not redownload; duplicate save is guarded; scan sees audio only after promotion | AC-DISC-06, JRN-DISCOVER |
| REF-ARTWORK | Artwork cache/fallback | valid, oversized, invalid, and absent synthetic art | Bounded cache; invalid/oversized art falls back; async completion cannot update stale path | AC-ART-01 |
| REF-VIDEO | Video staging | relevant and irrelevant synthetic search results; staged MP4 key | Relevance and canonical ID rules apply; video is temporary and independent of audio queue/library; artwork fallback remains available | AC-VIDEO-01, AC-VIDEO-02, JRN-VIDEO |
| REF-TRANSFER-VALID | Export contract | checked-in valid v1 fixture plus synthetic metadata | Strict path-free metadata, ordering, provider IDs, and ambiguity flag are preserved | AC-XFER-01, JRN-TRANSFER |
| REF-TRANSFER-REJECT | Unsafe export contract | checked-in unknown-field fixture plus path/secret/audio variants | Unknown/path-bearing fields are rejected; no deletion occurs during import | AC-XFER-02, JRN-TRANSFER |
| REF-TRANSFER-MATCH | Additive import | metadata match, duration match/mismatch, collision name | Compatible match copies additively with sanitized collision suffix; missing/ambiguous records remain reported | AC-XFER-03, AC-XFER-04, JRN-TRANSFER |
| REF-CONFIG-CORRUPT | Configuration recovery | valid, missing-key, malformed, and nonexistent-root synthetic JSON | Tolerant read preserves safe defaults; original input is not destroyed; secrets never enter corpus or manifest | AC-PERSIST-01, AC-OPS-01 |
| REF-MARKER-CORRUPT | Marker recovery | malformed lines, legacy marker, interrupted marker model | Invalid records do not create false success; legacy IDs remain readable; retryability is retained | AC-PERSIST-02, AC-DL-03 |
| REF-SHUTDOWN | Close during work | active scan/download/preview/video operations | Cancellation/teardown suppresses late UI mutation; temporary assets and workers have explicit terminal handling | AC-OPS-03, AC-VIDEO-03, JRN-VIDEO |
| REF-SHELL-CAPTURE | Visual/manual shell states | synthetic library states at narrow/default/wide sizes | Capture receipt records OS, window size, fixture IDs, and observed state without committing screenshots or host paths | AC-SHELL-01, AC-SHELL-02 |

## Capture Matrix

The following capture IDs are planned manual/visual evidence, not claimed as
already captured: `CAP-SHELL-NARROW`, `CAP-SHELL-DEFAULT`,
`CAP-SHELL-WIDE`, `CAP-LIBRARY-STATES`, `CAP-DOWNLOADER-STATES`,
`CAP-DISCOVER-STATES`, `CAP-SONG-PAGE-STATES`, `CAP-SETTINGS-ERRORLOG`,
`CAP-QUEUE-RECENT`, and `CAP-RESIZE-FULLSCREEN`. Each receipt must record OS,
window size, fixture IDs, state, and artifact location. No screenshots or
machine-specific paths belong in this corpus.

## Verification Receipt

- Configured desktop gate: `python -m pytest` (from `migration-audit/11-verification-gaps.md:13-18` and `00-repository-census.md:73-74`).
- Focused deterministic evidence cited by the audits: playback/media keys,
  sync, Discover mode, transfer/contract fixtures, artwork cache, and video
  relevance tests.
- Exact unit receipt: `python -m pytest` -> exit `127`,
  `zsh:1: command not found: python` (inconclusive; no collection count).
- Manifest syntax receipt: `jq empty tests/reference/manifest.json` -> exit `0`.
- Diff hygiene receipt: `git diff --check -- docs/migration/reference-corpus.md
  docs/migration/acceptance-catalog.md tests/reference/manifest.json` -> exit
  `0`.
- Parent wave must rerun the configured gate in an environment with its
  declared Python/pytest dependencies and record the exact exit status and
  collection count.
- Inconclusive by design: screenshot/manual captures, full-shell integration,
  real yt-dlp/FFmpeg/network runs, persistence crash recovery, shutdown/orphan
  checks, and target differential tests have no committed harness. Missing,
  skipped, timed-out, or truncated checks must remain inconclusive.

# Desktop Migration Decision Checklist

Status: U02 decisions approved by the user; target source exists, but runtime,
toolchain, and required evidence remain pending. This checklist is the
integration handoff and release gate map.

| Area | Approved decision | Affected interface | Required evidence | Deferral/risk | Status |
|---|---|---|---|---|---|
| Track identity | Provider-first `TrackId`; normalized metadata plus compatible duration fallback | `LibraryService`, `TransferService`, `TrackId` | Provider index, fallback, collision, rename, and v1 fixture tests | Provider IDs may be missing or conflicting | Approved; implementation pending |
| Rename/root relocation | Explicit mapping; ambiguous candidates require confirmation; unresolved inputs preserved | `LibraryService`, migration journal/mapping result | Rename, moved-root, favorite/queue/art/marker repair and crash-recovery corpus | No silent repair; user action can leave records unresolved | Approved; implementation pending |
| Playlist URL rename | Explicit old-to-new mapping, never name-based silent reassignment | `LibraryService`, `SettingsService` URL store | URL map rename/relocation and collision tests | Legacy URL file has no stable playlist ID | Approved; implementation pending |
| Import ambiguity | Approval per record or concrete match, not global | `TransferService`, import dialog/events | Metadata-only and competing-candidate fixtures with partial approval | Unresolved records remain reported and unqueued | Approved; implementation pending |
| Desktop history | Session-only, max 100; no durable history migration | `PlaybackService`, Recent projection, settings migration | Restart, clear, replay/order, and manifest exclusion checks | Durable history is explicitly deferred as product work | Approved; implementation pending |
| Last.fm key | One-time plaintext import to macOS Keychain; no new plaintext writes or fallback | `SettingsService`, macOS Keychain adapter | First-run import, permissions, malformed config, redaction, and no-plaintext-write checks | Keychain unavailable blocks secure migration rather than downgrading | Adapter wired; real Keychain check inconclusive pending interactive access |
| Audio backend | `cpal` output plus `symphonia` decode/seek, subject to tests | `PlaybackService`, audio adapter | Supported-format, 44.1 kHz, seek/pause/clock/completion/failure tests | Actual codec/device coverage is absent today | Approved; implementation pending |
| Video backend | HTML video presentation with Rust audio-clock adapter | `VideoService`, Song Page events | Pause/seek/resize/EOF/fullscreen/audio-clock and fallback tests | Fall back to artwork if synchronization tolerance fails | Approved; implementation pending |
| Crossfade | Remove setting and behavior; do not implement or retain a no-op | `SettingsService`, `PlaybackService`, settings UI | Legacy setting migration clears/ignores it; immediate transition regression test | Existing user preference is intentionally not preserved | Approved; implementation pending |
| Platform | macOS-only release | Tauri capabilities, media session, packaging | Supported macOS versions/architectures, clean install, signing and notarization gates | Other platforms are deferred and not release claims | Approved; implementation pending |
| Media binaries | Managed pinned FFmpeg/ffprobe/yt-dlp with notices, checksums, integrity checks, update review | Release manifest, subprocess adapters, preflight | Clean-machine provisioning, version/checksum/license/update-review checks | Legal/update policy and artifact evidence remain release work | Approved; implementation pending |
| Visual acceptance | Strict geometry; perceptual color tolerance | React tokens/layout, screenshot harness | Narrow/default/wide screenshots, geometry assertions, perceptual color thresholds | No committed target baselines yet | Approved; implementation pending |

## Cross-Cutting Gates

- Preserve `luno.library.export` v1 exactly: path-free, strict validation,
  provider fields, ambiguity flags, additive import, and no deletion.
- Keep all filesystem, metadata, provider, binary, and decoder work off the
  React/WebView thread; stale events carry generation/job/session identity.
- Use versioned atomic persistence with recovery metadata and retain legacy
  inputs until migration and rollback evidence is complete.
- Treat unavailable, skipped, timed-out, failed, or truncated checks as
  inconclusive, never as passes.
- Keep macOS Python fallback and known-good release artifacts until the full
  user-data migration and rollback trial passes.

## Approval And Handoff

The human-approved decisions are all rows in this checklist. U02 owns the
decision record and checklist only; implementation belongs to the interfaces
listed above. Open evidence is explicitly deferred, not silently accepted.
Release sign-off requires the required evidence for every row plus the
cross-cutting gates. Any unresolved ambiguity, missing Keychain access, failed
format/seek test, video clock miss, binary integrity issue, or geometry drift
blocks parity approval.

References: `docs/decisions/desktop-migration.md`,
`migration-audit/15-synthesis.md`, `migration-audit/14-audit-review.md`,
`docs/migration/acceptance-catalog.md`.

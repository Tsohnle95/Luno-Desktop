# Desktop Migration Decisions

Status: approved decisions recorded; implementation and verification remain
pending. Scope is the macOS desktop migration only. The Python/Tk application
and deterministic desktop tests remain the behavioral authority until parity
gates pass.

## Context

The migration replaces the desktop controller-owned Python/Tk/Pygame runtime
with React/TypeScript, Tauri 2, and Rust services. The audits identify open
decisions around identity, legacy path/name mapping, ambiguity, history,
secrets, media backends, crossfade, platform scope, external binaries, and
visual acceptance. These decisions constrain the target interfaces; they do
not imply that target code or evidence exists.

## Decisions

### D1. Track identity

`TrackId` is canonical and provider-first. When a trusted provider identity is
available, the tuple `(provider, provider_id)` is the identity. If it is absent,
the migration uses normalized metadata as a deterministic fallback, preserving
the existing artist/title normalization policy and compatible duration checks.
Path is location data, not identity. Duplicate identity and fuzzy search remain
separate policies and must not silently merge tracks.

The provider index is owned by `TransferService`/`LibraryService` and is passed
to manifest export and import planning. Provider IDs are not considered wired
until that interface is exercised by tests.

### D2. Rename and root relocation mapping

Rename and library-root relocation use explicit migration mappings. A mapping
must record the old location/name, new location/name, matched identity, and
reason/evidence. Existing playlist URL metadata, favorites, queue/history
references, artwork references, and marker ownership are remapped only after a
unique match.

A rename or relocation with multiple plausible matches is not auto-applied. The
user receives an explicit confirmation containing the candidates and affected
references. No match is reported as unresolved and leaves the legacy input
intact. Playlist URL name changes likewise require an explicit old-to-new
playlist mapping; names are never silently treated as stable IDs.

### D3. Import ambiguity

Ambiguity approval is per manifest record or per concrete local match, never a
single global approval that queues every metadata-only record. Provider matches
are preferred; normalized metadata fallback requires compatible duration and
must surface competing candidates. Unresolved records remain reported and are
not copied, downloaded, or assigned silently.

### D4. Desktop history

Desktop Recently Played remains session-only, newest-first, de-duplicated, and
bounded to 100 entries. No history migration input or durable history writer is
added. Queue state, history, favorites, and secrets remain excluded from
`luno.library.export` v1. Durable history would be a separate product decision,
not an inferred compatibility change.

### D5. Last.fm secret migration

On first migration, read the legacy plaintext `lastfm_key` once and import it to
the macOS Keychain. New persistence writes never write the key to plaintext
JSON. The legacy file is preserved as an input/rollback artifact according to
the atomic persistence and rollback policy; it is not rewritten with a new
plaintext secret. Missing, invalid, or inaccessible Keychain storage is a
visible migration failure/deferral, not a silent plaintext fallback.

### D6. Audio backend

The target audio stack is `cpal` for device output and `symphonia` for decoding
and seeking, subject to format-coverage and seek-conformance tests. The output
contract remains 44.1 kHz, stereo, signed 16-bit equivalent, with the existing
authoritative audio clock semantics and monotonic fallback where required.
Unsupported formats, device failures, duration errors, pause/resume, seek,
completion, and failure-skip behavior must be tested before parity sign-off.

### D7. Video synchronization

The Song Page uses a normal HTML `<video>` element for presentation. A Rust
audio-clock adapter remains authoritative for timing and corrects video time;
pause, seek, resize, track handoff, EOF, and fullscreen lifecycle must not create
an independent video clock. Video remains temporary and outside the library and
audio queue. If HTML video cannot meet the measured synchronization tolerance,
video gracefully falls back to artwork while the decision is revisited.

### D8. Crossfade

The crossfade setting is removed rather than implemented or retained as a
no-op. Migration must not carry forward the legacy preference or present
crossfade controls. Existing playback transition semantics remain immediate;
the target must include a migration test that rejects/clears the obsolete
setting and confirms no fade is claimed. The Rust legacy config reader treats
`crossfade` as an obsolete unknown field: it neither exposes nor persists the
value, and it leaves the source config untouched for rollback. The target has
no crossfade runtime API, so this is an approved removal rather than a
crossfade parity claim.

### D9. Platform scope

Release scope is macOS only. Tauri permissions, media-session integration,
Keychain access, packaging, signing/notarization, supported architectures, and
clean-machine gates are defined for macOS. Windows/Linux support is deferred and
must not be implied by portable interfaces or release claims.

### D10. Managed media binaries

Release artifacts provision managed, pinned versions of FFmpeg, ffprobe, and
yt-dlp. Each version has license/notice material, checksums, executable
discovery and integrity checks, and an update review process. Unavailable or
tampered binaries produce an explicit degraded state; the app must not report
successful media work without the required verified tool. Updating a binary is
a reviewed release change, not an unbounded runtime self-update.

### D11. Visual acceptance

Geometry is strict: the target preserves the documented shell minimums,
responsive breakpoints, player/header dimensions, 56 px virtual rows, spacing,
aspect ratios, and scroll behavior. Color comparison uses perceptual tolerance
rather than exact RGB equality to accommodate rendering and color-management
variation. Any intentional geometry deviation requires a new approved decision;
color tolerance does not permit layout drift.

## Affected Interfaces

- `LibraryService`: typed `TrackId`/playlist identity, scan reconciliation,
  rename/root mapping, favorites remapping, and unresolved-candidate reporting.
- `TransferService`: provider-ID index input, metadata/duration fallback,
  per-record ambiguity decisions, strict v1 manifest compatibility, and
  additive/no-delete import results.
- `SettingsService`: legacy config reader, one-time Keychain import, versioned
  atomic settings writer, and removal of crossfade state/control.
- `PlaybackService`: cpal/symphonia decoder/output adapter, 44.1 kHz clock,
  queue/seek/completion/failure semantics, and no crossfade transition.
- `VideoService` plus the React Song Page: temporary HTML video staging and the
  Rust audio-clock correction/lifecycle adapter.
- `ReleaseService`/packaging: macOS-only gates, managed binary manifest,
  notices, checksums, integrity checks, and update review metadata.
- React visual system and screenshot harness: strict geometry assertions and
  perceptual color comparison thresholds.

## Explicit Deferrals And Risks

- Provider identity availability and collisions remain dependent on provider
  metadata; ambiguous records require user action.
- Filesystem rename plus reference repair needs transactional or compensating
  behavior and crash-recovery tests; this ADR does not choose an implementation
  journal.
- cpal/symphonia format support, device behavior, and seek accuracy are not
  evidenced by the current repository and are release gates.
- HTML video synchronization and codec behavior require process/media tests;
  artwork fallback is the approved degradation path.
- Keychain migration can be blocked by access or corrupted legacy data; no
  plaintext fallback is approved.
- Managed binary licensing, signing/notarization, and update review still need
  release artifacts and owner evidence.
- Visual baselines, accessibility, and real-user-data migration remain absent;
  source descriptions alone are not visual or migration proof.

## Approval

Human-approved for U02: D1-D11 above. Approval covers the decisions and scope,
not implementation completion. Release remains blocked until the checklist
evidence, compatibility tests, migration corpus, visual gates, shutdown gates,
and macOS packaging checks pass. Any change to identity, ambiguity, secret
handling, backend, platform scope, binary policy, or geometry requires a new
review.

## Evidence

- `migration-audit/15-synthesis.md`
- `migration-audit/04-persistence-migration.md`
- `migration-audit/05-playback-queue.md`
- `migration-audit/08-song-art-video.md`
- `migration-audit/10-transfer-contracts.md`
- `migration-audit/12-operations-release.md`
- `migration-audit/14-audit-review.md`
- `migration-work/U01.md` and `docs/migration/acceptance-catalog.md`

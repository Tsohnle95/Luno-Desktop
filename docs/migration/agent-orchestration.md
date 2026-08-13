# Desktop Migration Agent Orchestration

## Operating Model

Two persistent implementation lanes will collaborate through explicit contracts:

- **Platform lane:** Rust/Tauri domain state, persistence, filesystem, identity,
  playback, subprocesses, downloads, discovery, previews, artwork, video, and
  target-side fixtures.
- **Presentation lane:** React shell, routes, tokens, responsive layout, views,
  dialogs, focus/accessibility, screenshot baselines, and UI integration tests.

An integration owner controls shared contracts, `AppState`, event envelopes,
generated types, fixture changes, merge order, and wave gates. Neither lane may
silently change shared ownership boundaries.

## Agent Rules

Every worker receives a unit ID, file allowlist, acceptance IDs, dependencies,
verification command, and commit ownership. Workers must produce a handoff note
containing changed files, commands/events, state ownership, compatibility
decisions, fixtures, exact verification receipts, inconclusive checks, and
follow-up dependencies.

Workers must not spawn additional workers. Read-only parity auditors and test
agents may be dispatched separately, but implementation workers remain one unit
per agent. At most three implementation workers run concurrently.

## Shared-Surface Ownership

- IPC envelopes and generated types: U03/integration owner.
- Runtime state, task registry, and shutdown: U06/platform lane.
- Atomic persistence API: U07/platform lane.
- Track/Playlist identity: U11/platform lane, after U02 decisions.
- Library scan publication: U12/U13 platform lane.
- Validated media promotion: U30/platform lane.
- Frontend projection store: U24/presentation lane.
- Canonical visual tokens and shell geometry: U25/presentation lane.
- Shared fixtures: integration owner with both lanes approving changes.
- Project brain and architecture documentation: unit author updates only its
  affected section; broad rewrites are serialized.

## Review Cadence

1. Each worker self-checks its unit and commit.
2. The integration owner checks the complete wave gate before the next wave.
3. Every shared-contract change pauses both lanes until fixtures and projections
   are updated.
4. A fresh reviewer checks the combined migration diff at major wave boundaries.
5. Human decisions are required for crossfade, identity, ambiguity, keychain,
   backend, licensing, supported platforms, and visual tolerances.

## Stop Conditions

Stop instead of guessing when behavior conflicts with source/tests, a migration
could delete or overwrite user data, identity changes favorites or queues, a
provider match is ambiguous, security/licensing/platform policy is involved, or
required verification is unavailable, skipped, timed out, or truncated.

## Required Wave Gates

- Wave 0: acceptance catalog, decision records, frozen fixtures, and reference
  receipts.
- Wave 1: clean Tauri/Rust/React build, contract serialization, lifecycle,
  cancellation, atomic-write, and redacted-log tests.
- Wave 2: legacy corpus, identity differential tests, scan/mutation tests, and
  transfer fixtures.
- Wave 3: audio-format, queue-conformance, crossfade decision/test, and media
  session tests.
- Wave 4: responsive screenshots, focus/accessibility checks, and shell/UI
  integration journeys.
- Wave 5: fake yt-dlp/FFmpeg, download validation, preview isolation, artwork,
  video lifecycle, and Downloader/Discover UI tests.
- Wave 6: real-user-data migration, rollback, packaged smoke, full journeys,
  visual/manual receipts, shutdown, and no-inconclusive-gate sign-off.

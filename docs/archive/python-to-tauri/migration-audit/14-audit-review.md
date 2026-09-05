# Audit Review

## Findings

### High: Verification receipts conflict

The dossiers distinguish configured CI gates, focused test runs, and local
inconclusive runs inconsistently. The synthesis must never treat an unavailable
or skipped command as a pass. Reconcile `00-repository-census.md`,
`05-playback-queue.md`, `10-transfer-contracts.md`, and
`11-verification-gaps.md` into explicit verification categories.

### High: Crossfade is unresolved

The preference and helpers exist, but the active desktop transition path does
not invoke them. This is a migration blocker requiring a baseline decision and
regression test, not ordinary parity work.

### High: Critical end-to-end verification is missing

Scan-to-download-to-play, rename races, persistence recovery, shutdown, and
UI-thread integration lack complete desktop tests. The target plan must require
fake-filesystem, fake-subprocess, lifecycle, and UI integration coverage.

### High: Direct downloads can falsely succeed

The download audit reports weaker output verification for direct downloads than
playlist sync. The target must verify expected audio output, temporary files,
atomic promotion, cancellation, partial output, and failure reporting for every
download entry point.

### High: Persistence is not atomic or corruption-safe

Current config, marker, and manifest writers use direct writes and lack a
complete versioned recovery strategy. The target must use compatibility readers,
versioned atomic writers, interrupted-write tests, and recovery behavior for
each persisted desktop file.

### High: Provider-ID support is incomplete in desktop wiring

The codec supports provider IDs, but normal desktop export/import does not pass
`source_by_path` or maintain a provider index. The migration must not claim
provider matching is implemented desktop behavior until that wiring exists.

### Medium: Desktop feature coverage needs explicit additions

Playlist CRUD safeguards, dump/copy, smart playlist actions, permanent deletion,
missing-artwork fetch, and their error/UI states require explicit inventory and
acceptance criteria in the final plan.

### Medium: Visual and accessibility verification is absent

No committed screenshot baselines, accessibility audit, focus matrix, resize
matrix, or popup/empty/loading/error regression evidence exists. Require a
desktop visual/manual acceptance matrix and automated coverage where practical.

### Low: File registry contradiction

The census reports a discrepancy around `build.command` tracking while the
operations audit treats it as present. Resolve tracked-versus-working-tree
status before finalizing the repository registry.

## Synthesis Corrections

- Separate implemented behavior, desktop-only behavior, documented plans,
  unknowns, and inconclusive verification.
- Elevate crossfade, direct-download validation, provider-ID wiring, persistence
  atomicity, and shutdown cleanup as explicit migration blockers.
- Add omitted desktop workflows and UI states to the coverage matrix.
- Require end-to-end desktop journey tests and visual/manual receipts before
  parity sign-off.

## Audit Receipt

- Files inspected: All desktop dossiers `00` through `08` and `10` through
  `13`, `project_brain.md`, `migration-plan.md`, and `AGENTS.md`.
- Tests/fixtures inspected: Referenced desktop playback, discovery, downloader,
  transfer, artwork, media-key, and video tests through the dossiers.
- Claims requiring review: Verification receipts, crossfade, provider-ID
  transfer wiring, direct-download output validation, persistence atomicity,
  omitted desktop features, and `build.command` tracking.
- Coverage gaps: No target migration source, full-shell integration, packaged
  app, screenshot, accessibility, real subprocess, or real-user-data evidence.
- Status: complete

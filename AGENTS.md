# AGENTS.md — Luno Desktop Agent Entry Point

Luno Desktop is a desktop music app: React + TypeScript + Vite frontend in
`src/` talking to a Rust runtime in `src-tauri/` through a typed
command/event bridge (Tauri 2). Rust owns library, playback, downloads,
discovery, artwork, video, persistence, and migration. Stable app
identifier: `com.ty.luno`. Audio output is normalized to 44.1 kHz through
the reviewed FFmpeg path.

## Start here

1. Read this file.
2. Route via the tables below to **only** the task-relevant canonical doc.
3. Inspect the listed implementation symbols and nearby tests.
4. Expand outward only when evidence reveals another dependency or invariant.

Context is a scarce working set. Do not preload all docs, crawl the repo to
"understand everything", read whole large files when a section/symbol
suffices, reopen settled facts, or ingest generated, vendor, dependency, or build
output without a task-specific reason.

## Quick-start commands

```bash
npm ci
npm run typecheck
npm test                                   # vitest: src/test/**
npm run verify                             # typecheck + tests + docs check
cargo test --manifest-path src-tauri/Cargo.toml
npm run build                              # tsc + vite build
npx tauri build                            # production bundle (macOS)
./scripts/verify-desktop-bundle.sh "src-tauri/target/release/bundle/macos/Luno Desktop.app"
```

Supported toolchain: Node 22 (`.node-version`), Rust stable
(`rust-toolchain.toml`). Validation under any other toolchain is
exploratory, not authoritative. macOS 13+ is the only release target;
signing/notarization verify only in CI.

## Module map

| Area | Implementation | Responsibility |
|---|---|---|
| Bridge + contracts | `src/shared/contracts.ts`, `src/app/bridge.ts`, `src-tauri/src/contracts/`, `src-tauri/src/state.rs`, `src-tauri/src/main.rs` | Typed commands/events, AppState dispatch, transfer v1 codec |
| Frontend | `src/app/`, `src/views/`, `src/components/`, `src/state/` | Shell/routes, views, projections, bounded caches |
| Library + persistence | `src-tauri/src/library/`, `src-tauri/src/persistence/`, `src-tauri/src/migration/`, `src-tauri/src/domain/` | Scanning, identity keys, mutations, artwork, compat readers |
| Playback + video | `src-tauri/src/playback/`, `src-tauri/src/video/`, `src-tauri/src/media_keys.rs`, `src-tauri/src/platform/` | Queue policy, clock, audio backend, video pipeline, media keys |
| Downloads + discovery | `src-tauri/src/download/`, `src-tauri/src/discovery/`, `src-tauri/src/preview/`, `src-tauri/src/process/`, `src-tauri/src/runtime/` | Downloader, recommendations, previews, managed binaries, tasks |
| Transfer contract | `contracts/library-export/v1/`, `src-tauri/src/transfer/` | Path-free v1 export manifest shared with Luno Mobile |
| Release | `release/`, `release-input/`, `scripts/`, `src-tauri/tauri.conf.json` | Pinned binaries, provisioning, bundle verification |

## Task / symptom router

| Symptom or task | Read first | Likely implementation | Likely tests |
|---|---|---|---|
| IPC/bridge error, invalid envelope, event delivery | `docs/modules/contracts.md` | `src-tauri/src/contracts/`, `src/app/bridge.ts` | `src/test/bridge.test.ts` |
| Empty/stale library, scan, duplicates, missing artwork | `docs/modules/library.md` | `src-tauri/src/library/`, `src-tauri/src/state.rs` | `src-tauri/src/gate2_tests.rs`, `src/test/artwork_eviction.test.ts` |
| Playback, queue, shuffle/repeat, skip-back, media keys | `docs/modules/playback.md` | `src-tauri/src/playback/`, `src-tauri/src/state.rs` | `src-tauri/src/gate3_tests.rs`, `src/test/gate3_smoke.test.ts` |
| Download fails, helper, promotion, M4A output | `docs/modules/downloads-discovery.md` | `src-tauri/src/download/`, `src-tauri/src/process/` | `src-tauri/src/gate4_tests.rs`, `src/test/gate4_smoke.test.ts` |
| Discovery, previews, saving recommendations | `docs/modules/downloads-discovery.md` | `src-tauri/src/discovery/`, `src-tauri/src/preview/` | `src-tauri/src/gate5_tests.rs`, `src/test/gate5_smoke.test.ts` |
| Video never loads / stale video state | `docs/modules/playback.md` | `src-tauri/src/video/`, `src-tauri/src/state.rs` | `src-tauri/src/state.rs` video tests, `src/test/views/video_stage.test.tsx` |
| Transfer export/import, cross-repo round trip | `docs/modules/contracts.md` | `src-tauri/src/transfer/`, `contracts/library-export/v1/` | `src-tauri/src/gate4_tests.rs` |
| Settings, first-run migration, keychain, user data | `docs/modules/library.md` | `src-tauri/src/migration/`, `src-tauri/src/persistence/` | `src-tauri/src/gate2_tests.rs` |
| Shell, routes, views, virtual list, styling | `docs/modules/frontend.md` | `src/app/Shell.tsx`, `src/views/`, `src/state/` | `src/test/views/`, `src/test/dialogs.test.tsx` |
| Release, bundle, signing, binary provisioning | `docs/release/desktop.md` | `scripts/`, `release/desktop-binaries.json` | `scripts/verify-desktop-bundle.sh` |
| Setup, CI, verification, toolchain | `docs/verification/desktop.md` | `.github/workflows/`, `package.json` | canonical gate below |
| Architecture spanning several rows | `project_brain.md` | — | — |

## Execution policy

- `PATCH`: localized scope, clear acceptance, unchanged architecture.
  Inspect docs/source/tests → smallest change → narrow validation → diff
  review → canonical validation → commit. No task file.
- `FEATURE`: coordinated multi-file change, understood architecture.
  Concise in-context plan (outcome, components, sequence, validation,
  risks) → small testable slices → re-evaluate per slice.
- `PROJECT`: architecture, shared contracts, persistence, security,
  concurrency, recovery, or major uncertainty. Task file in
  `.agent/tasks/`, adversarial plan review, phase-by-phase with commits,
  final cumulative review, then delete the task file.
- `BATCH`: several independent outcomes in one request. Triage once,
  cluster, classify each unit separately, execute one unit at a time.

Full process: `docs/agent-execution.md`. Minimum process that keeps
expected repair cost low; no ceremony artifacts for direct work.

## Escalation

Stop patching and replan when: an architectural assumption proves false;
scope spreads beyond the expected subsystem; two repair attempts fail the
same way; an unexpected cross-component dependency, contract, or migration
appears; or edits can no longer be explained by a clear invariant.
Preserve verified work → summarize new evidence → revise plan/mode →
continue from known-good state.

## Architecture summary

The React frontend renders snapshots and sends typed commands; it never
recreates backend policy. `AppState` in `src-tauri/src/state.rs` owns session state and
dispatches every command through a blocking-worker boundary so the
Tauri/UI thread stays responsive; results return as versioned event
envelopes over `backend-event`. Library scans derive stable media keys
from filename metadata and publish generation-guarded snapshots. Playback
owns the authoritative queue and clock. Downloads promote atomically
inside the library root via the bundled helper. The only cross-repository
contract is the path-free `luno.library.export` v1 manifest shared with
Luno Mobile. Details: `project_brain.md`.

## Conventions

- Prefer self-explanatory code. Comment only what cannot be cheaply
  recovered: invariants, protocol quirks, trust assumptions,
  race-condition defenses, compatibility workarounds, non-obvious rationale.
- Keep heavy yt-dlp/filesystem work off the UI thread; schedule UI
  mutations with `.after(0, callback)`.
- Preserve the 56 px virtual-list row height and scroll-region clamping.
- Preserve Tauri/Rust playback lifecycle and existing migration readers.
  `legacy` in `src-tauri/src/migration/` and
  `src-tauri/src/persistence/legacy.rs` means
  user-data compatibility, not dead code.
- Never change `com.ty.luno`, the transfer v1 shape, or pinned managed
  binaries without a reviewed release-grade change and migration design.

## Definition of done

1. Targeted validation for the touched area passes.
2. Behavior the automated gates do not exercise (live UI appearance,
   restart/recovery, OS/media-key behavior, bundle install, device
   playback) gets the narrowest direct validation, or is explicitly
   reported as unverified — never inferred from unrelated tests.
3. Canonical gate passes under the supported toolchain:
   `npm run verify`, `cargo test --manifest-path src-tauri/Cargo.toml`,
   `npm run build`. A timeout, skip, or truncated run without a receipt is
   inconclusive, not a pass.
4. Diff scope reviewed; no unrelated product changes.
5. Durable docs updated only where truth changed (see below).

## Git ownership and safety

- Inspect `git status` before editing; preserve unrelated pre-existing
  user work exactly as found. Never reset, clean, stash, discard, or
  commit unrelated changes to manufacture a clean tree.
- Commit only independently sound, believed-correct work in logical
  units; include required doc updates with the behavior change.
- Commit per completed phase (`plan/review → phase → validate → commit`),
  not one giant commit. A commit is a recovery checkpoint.
- Never commit secrets, experimental debris, or local binary inputs.

## Documentation ownership

| Truth | Canonical owner |
|---|---|
| Repository routing / conventions | `AGENTS.md` (this file) |
| Agent execution process | `docs/agent-execution.md` |
| Cross-system architecture / invariants | `project_brain.md` |
| Bridge, envelopes, transfer contract | `docs/modules/contracts.md` |
| Frontend (shell, views, projections) | `docs/modules/frontend.md` |
| Library, persistence, migration | `docs/modules/library.md` |
| Playback, video, media keys | `docs/modules/playback.md` |
| Downloads, discovery, previews, toolchain | `docs/modules/downloads-discovery.md` |
| Setup, verification, CI, toolchain | `docs/verification/desktop.md` |
| Release, bundle, signing | `docs/release/desktop.md` |
| Durable decisions | `docs/decisions/` |
| Active multi-phase work | `.agent/tasks/<task>.md` |
| Executable behavior | source + tests |
| Completed history | Git |

Non-owner docs never override a canonical owner. If source/tests
contradict docs, investigate whether implementation drifted or docs went
stale, then fix the wrong surface.

Permanent docs describe current durable truth; task files describe active
work; source/tests own executable behavior; Git owns history. Pure
implementation refactors that change no behavior, ownership, contract,
routing, or invariant must not cause documentation churn. When
architecture evolves (ownership moves, subsystems split/merge, new
contracts appear), split/merge/rename docs and update this router — Git
owns history, docs describe the current system.

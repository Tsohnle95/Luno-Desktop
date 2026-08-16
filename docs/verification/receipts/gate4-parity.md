# Formal Gate 4 — Feature Parity

Receipt date: `2026-08-15`

## Verdict: Gate 4 Feature Parity

**Gate 4 remains BLOCKED / INCONCLUSIVE.** The available automated and
reference suites passed, but target-vs-reference feature parity is not
established because target runtime and manual evidence are unavailable. No
target differential adapter, desktop journey runner, visual/accessibility
runner, packaged runtime, or physical audio/runtime evidence is present. No
parity failure was observed in an executable target test, so this receipt
records no `FAIL` classification.

## Environment

- Host: macOS arm64
- Node: `v22.23.2`
- npm: `10.9.8`
- Rust: `rustc 1.97.1`, `cargo 1.97.1`
- Python: `python` unavailable; `python3` is `3.14.7`
- FFmpeg: `8.1`
- yt-dlp: `2026.03.17`
- Audited worktree `HEAD`: `f9e81c78e38a6c3a2fcb7aaa707f9516cd0088b5`

The worktree had pre-existing changes in 11 files before this receipt was
created. They were preserved. No secrets, Keychain entries, real
`~/.luno_config.json`, real music library, deployment, or commit was used.

## Fixture Strategy

The Python tests generated fake yt-dlp/FFmpeg executables and fixture files
under pytest temporary directories with isolated synthetic `HOME` values where
needed. Rust tests used their own isolated synthetic temporary directories.
No fixture bytes or host paths were added to the repository.
The frozen sanitized corpus and journey definitions were validated from:
`tests/reference/manifest.json`, `tests/journeys/manifest.json`,
`docs/migration/reference-corpus.md`, and
`docs/migration/acceptance-catalog.md`.

## Commands

| Command | Exit | Result | Evidence |
|---|---:|---|---|
| `python -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` | 127 | INCONCLUSIVE: `python` not found | shell receipt; configured command unavailable |
| `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` | 0 | PASS: 6 passed in 0.59s | `tests/differential/test_harness.py`, `tests/differential/test_u44_receipts.py` |
| `python3 -m pytest -q` | 0 | PASS: 109 passed in 7.71s | repository Python test suite |
| `python3 -m json.tool tests/journeys/manifest.json >/dev/null` | 0 | PASS | `tests/journeys/manifest.json` |
| `jq empty tests/reference/manifest.json` | 0 | PASS | `tests/reference/manifest.json` |
| `npm run typecheck` | 0 | PASS: `tsc --noEmit` | `package.json` |
| `npm test` | 0 | PASS: 2 files, 15 tests passed | `src/test/` |
| `npm run build` | 0 | PASS: 51 Vite modules transformed | `package.json`; generated `dist/` is ignored |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | PASS: 32 passed, 1 ignored, 0 failed; warnings only | `src-tauri/src/`; ignored test is interactive real Keychain and is not a parity pass |
| `cargo test --manifest-path src-tauri/Cargo.toml --tests` | 0 | BLOCKED/INCONCLUSIVE for integration parity: only the library unit target ran, 32 passed and 1 ignored | `src-tauri/Cargo.toml` has no configured integration test targets |
| `cargo test --manifest-path src-tauri/Cargo.toml --test library ...` | 101 | INCONCLUSIVE: no test target named `library` | `tests/library/` is not a Cargo integration target under `src-tauri/` |
| `cargo test --manifest-path src-tauri/Cargo.toml --test transfer ...` | 101 | INCONCLUSIVE: no test target named `transfer` | same target-layout limitation |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib library::` | 0 | BLOCKED/INCONCLUSIVE: 0 passed, 33 filtered out | no matching Rust unit-test namespace; zero tests is not a parity pass |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib playback::` | 0 | BLOCKED/INCONCLUSIVE: 0 passed, 33 filtered out | no matching Rust unit-test namespace; zero tests is not a parity pass |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib download::` | 0 | BLOCKED/INCONCLUSIVE: 0 passed, 33 filtered out | no matching Rust unit-test namespace; zero tests is not a parity pass |
| `git diff --check` | 0 | PASS | worktree diff hygiene |

The Cargo run emitted three non-fatal warnings (`unused_mut`, an unused
variable, and an unused Keychain symbol). They were not changed because no
bounded parity failure required a source edit. The current worktree still
contains the pre-existing application edits listed by `git status --short`;
they were preserved and not included in this verification change.

## Fixture Matrix

Each row classifies the complete frozen fixture scenario, not merely a related
source file. `PASS (reference/unit only)` is not target parity. Evidence paths
are repository-relative; every command and output is retained above.

| Fixture | Classification | Exact command / evidence path | Explanation |
|---|---|---|---|
| `REF-ROOT-MISSING` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:23` | No executed test covers missing-root behavior directly. |
| `REF-ROOT-EMPTY` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:24` | No executed test covers the empty-root fixture directly. |
| `REF-SCAN-ORDER` | PASS (reference/unit only) | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_harness.py:14-22` | Reference scan ordering and grouping are directly asserted; target differential parity is unavailable. |
| `REF-SCAN-GENERATION` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:26` | No executed test directly covers generations, coalescing, and stale-result rejection. |
| `REF-PLAY-CONTEXT` | PASS (reference/unit only) | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_harness.py:25-29` | Reference queue insertion behavior is directly asserted, but selected-index playback parity is not. |
| `REF-QUEUE-UP-NEXT` | PASS (reference/unit only) | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_harness.py:25-29` | Reference Play Next/Add to Queue ordering is directly asserted; target parity is unavailable. |
| `REF-SHUFFLE-REPEAT` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:29` | No executed evidence directly covers the complete shuffle/repeat fixture. |
| `REF-CLOCK-SEEK` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:30` | No completed target audio-clock/runtime evidence exists. |
| `REF-FAILURE-SKIP` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:31` | No executed evidence directly covers asynchronous skip and threshold behavior. |
| `REF-FAVORITES` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:32` | Rust unit completion does not identify a complete fixture-level favorites receipt. |
| `REF-RENAME-REPAIR` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:33` | No executed end-to-end rename/rescan fixture evidence exists. |
| `REF-URL-MARKERS` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:34` | No complete URL, sync, marker, and verified-audio-output run was recorded. |
| `REF-DIRECT-OUTPUT` | BLOCKED/INCONCLUSIVE | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_u44_receipts.py:16-23` | Fake tools prove deterministic signals only; missing/partial/valid terminal-state coverage is incomplete. |
| `REF-DOWNLOAD-JOB` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:36` | No target fake-subprocess coordination receipt covers busy and cancellation behavior. |
| `REF-DISCOVERY` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:37` | No executed provider/fallback fixture receipt covers the complete scenario. |
| `REF-PREVIEW-SESSION` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:38` | No completed target preview session or temporary-asset receipt exists. |
| `REF-DISCOVER-HANDOFF` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:39` | Queue capture/restoration and natural handoff were not exercised as a fixture. |
| `REF-PREVIEW-SAVE` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:40` | No executed explicit-save and post-promotion scan receipt exists. |
| `REF-ARTWORK` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:41` | No visual or runtime artwork cache/fallback fixture receipt exists. |
| `REF-VIDEO` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:42` | No video staging, audio-clock, or visual runtime receipt exists. |
| `REF-TRANSFER-VALID` | BLOCKED/INCONCLUSIVE | `npm test` (exit 0); `tests/contracts/contracts.test.ts`; `docs/migration/reference-corpus.md:43` | Contract tests pass, but no target transfer planner/handoff fixture receipt exists. |
| `REF-TRANSFER-REJECT` | BLOCKED/INCONCLUSIVE | `npm test` (exit 0); `tests/contracts/contracts.test.ts`; `docs/migration/reference-corpus.md:44` | Contract syntax/test evidence does not directly establish all unsafe-import rejection behavior. |
| `REF-TRANSFER-MATCH` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:45` | No completed target additive match/collision fixture receipt exists. |
| `REF-CONFIG-CORRUPT` | PASS (reference/unit only) | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `src-tauri/src/migration/first_run.rs:443-475` | Rust migration unit evidence directly covers tolerant configuration recovery; real-user persistence remains untested. |
| `REF-MARKER-CORRUPT` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:47` | No executed complete corrupted-marker fixture receipt exists. |
| `REF-SHUTDOWN` | BLOCKED/INCONCLUSIVE | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_u44_receipts.py:26-37` | The fake timeout is explicitly inconclusive; no complete multi-operation teardown fixture ran. |
| `REF-SHELL-CAPTURE` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:49,53-59` | Planned manual/visual captures were not produced. |

## Journey Matrix

| Journey | Classification | Exact command / evidence path | Explanation |
|---|---|---|---|
| `JRN-SCAN-PLAY` | BLOCKED/INCONCLUSIVE | `python3 -m json.tool tests/journeys/manifest.json >/dev/null` (exit 0); `tests/journeys/manifest.json:5` | Manifest syntax passed, but no target journey runner or manual artifact executed the sequence. |
| `JRN-DISCOVER` | BLOCKED/INCONCLUSIVE | `python3 -m json.tool tests/journeys/manifest.json >/dev/null` (exit 0); `tests/journeys/manifest.json:6` | Definitions were validated only; provider, preview, save, and rescan journey evidence is unavailable. |
| `JRN-DOWNLOAD` | BLOCKED/INCONCLUSIVE | `python3 -m json.tool tests/journeys/manifest.json >/dev/null` (exit 0); `tests/journeys/manifest.json:7` | No target download/search/sync journey runner or manual receipt exists. |
| `JRN-RENAME` | BLOCKED/INCONCLUSIVE | `python3 -m json.tool tests/journeys/manifest.json >/dev/null` (exit 0); `tests/journeys/manifest.json:8` | No completed rename and reference-repair journey exists. |
| `JRN-TRANSFER` | BLOCKED/INCONCLUSIVE | `python3 -m json.tool tests/journeys/manifest.json >/dev/null` (exit 0); `tests/journeys/manifest.json:9` | No target transfer handoff journey or manual artifact exists. |
| `JRN-VIDEO` | BLOCKED/INCONCLUSIVE | `python3 -m json.tool tests/journeys/manifest.json >/dev/null` (exit 0); `tests/journeys/manifest.json:10` | No packaged video/audio runtime or cleanup journey evidence exists. |

## Acceptance Matrix

Every frozen acceptance area is listed separately. A `PASS` is reference/unit
evidence only and never a target parity result.

| Acceptance | Classification | Exact command / evidence path | Explanation |
|---|---|---|---|
| `AC-SHELL-01` | BLOCKED/INCONCLUSIVE | `npm test` (exit 0); `tests/contracts/contracts.test.ts` | No shell journey or visual artifact. |
| `AC-SHELL-02` | BLOCKED/INCONCLUSIVE | `npm run build` (exit 0); `docs/migration/reference-corpus.md:53-59` | Build is not visual/manual evidence. |
| `AC-LIB-01` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:23` | No direct missing/inaccessible-root fixture run. |
| `AC-LIB-02` | PASS (reference/unit only) | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_harness.py:14-22` | Reference scan grouping is directly asserted. |
| `AC-LIB-03` | PASS (reference/unit only) | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_harness.py:14-20` | Reference newest-first ordering is directly asserted. |
| `AC-LIB-04` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml --tests` (exit 0); `src-tauri/Cargo.toml` | No configured integration target or complete generation receipt. |
| `AC-LIB-05` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:32` | No complete favorites/recent fixture receipt. |
| `AC-LIB-06` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:33` | Source/unit completion does not prove end-to-end CRUD and repair. |
| `AC-PLAY-01` | PASS (reference/unit only) | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_harness.py:25-29` | Reference queue context insertion is directly asserted. |
| `AC-PLAY-02` | PASS (reference/unit only) | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_harness.py:25-29` | Reference Play Next/Add to Queue ordering is directly asserted. |
| `AC-PLAY-03` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml --lib playback::` (exit 0); `docs/verification/receipts/gate4-parity.md:57` | Zero tests matched that namespace; no pass may be inferred. |
| `AC-PLAY-04` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml --lib playback::` (exit 0); `docs/verification/receipts/gate4-parity.md:57` | Zero tests matched; no audio/runtime evidence. |
| `AC-PLAY-05` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:31` | No complete failure-threshold fixture receipt. |
| `AC-PLAY-06` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/acceptance-catalog.md:25` | Release blocker; no baseline audible envelope test. |
| `AC-DL-01` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml --lib download::` (exit 0); `docs/verification/receipts/gate4-parity.md:58` | Zero tests matched; no target coordination receipt. |
| `AC-DL-02` | BLOCKED/INCONCLUSIVE | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_u44_receipts.py:16-23` | Fake output signal is incomplete validation/promote evidence. |
| `AC-DL-03` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:34` | No complete sync/marker receipt. |
| `AC-DL-04` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:35` | No complete search/direct/CSV receipt. |
| `AC-DISC-01` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:37` | No provider/error/empty-state target receipt. |
| `AC-DISC-02` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:37` | No complete filtering/art fixture receipt. |
| `AC-DISC-03` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:38` | No target preview-isolation receipt. |
| `AC-DISC-04` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:38` | No target preview-session receipt. |
| `AC-DISC-05` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:39` | No target Discover-mode receipt. |
| `AC-DISC-06` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:40` | No target explicit-save receipt. |
| `AC-ART-01` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:41` | No visual/runtime artwork receipt. |
| `AC-VIDEO-01` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:42` | No video source/staging runtime receipt. |
| `AC-VIDEO-02` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:42` | No audio-clock video runtime receipt. |
| `AC-VIDEO-03` | BLOCKED/INCONCLUSIVE | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_u44_receipts.py:26-37` | Fake timeout is explicitly inconclusive; no lifecycle receipt. |
| `AC-XFER-01` | BLOCKED/INCONCLUSIVE | `npm test` (exit 0); `tests/contracts/contracts.test.ts` | Contract tests do not establish target transfer parity. |
| `AC-XFER-02` | BLOCKED/INCONCLUSIVE | `npm test` (exit 0); `tests/contracts/contracts.test.ts` | No complete unsafe-field rejection fixture receipt. |
| `AC-XFER-03` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/reference-corpus.md:45` | No target additive matching receipt. |
| `AC-XFER-04` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `docs/migration/acceptance-catalog.md:43` | Provider-index decision and handoff evidence are absent. |
| `AC-PERSIST-01` | PASS (reference/unit only) | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `src-tauri/src/migration/first_run.rs:443-475` | Rust migration unit evidence directly covers tolerant config handling; real-user recovery is unavailable. |
| `AC-PERSIST-02` | BLOCKED/INCONCLUSIVE | `python3 -m pytest -q` (exit 0); `docs/migration/reference-corpus.md:47` | No complete marker-corruption fixture receipt. |
| `AC-OPS-01` | BLOCKED/INCONCLUSIVE | `cargo test --manifest-path src-tauri/Cargo.toml` (exit 0); `src-tauri/src/logging/redaction.rs:77-81` | Unit source exists, but no recorded fixture-level diagnostics receipt. |
| `AC-OPS-02` | BLOCKED/INCONCLUSIVE | `npm run build` (exit 0); `docs/migration/acceptance-catalog.md:47` | Build success is not packaged-environment/preflight evidence. |
| `AC-OPS-03` | BLOCKED/INCONCLUSIVE | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` (exit 0); `tests/differential/test_u44_receipts.py:26-37` | Timeout was correctly classified inconclusive; no target shutdown/orphan receipt. |

## Unresolved Blockers

- The required configured `python -m pytest` command cannot start (exit 127).
- Target Cargo integration targets referenced by the repository documentation
  are absent from `src-tauri/Cargo.toml`; attempted collection is therefore
  inconclusive, not a pass.
- No target-vs-reference adapter or target journey harness is available.
- No desktop/manual, visual, accessibility, packaged-runtime, physical audio,
  migration/rollback-with-user-data, or shutdown/orphan-process evidence is
  available.
- One real macOS Keychain test remains intentionally ignored; no Keychain was
  accessed during this run.

No bounded, evidence-backed implementation fix was required by the observed
results. Only this receipt was added.

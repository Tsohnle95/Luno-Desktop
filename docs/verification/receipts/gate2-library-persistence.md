# Formal Gate 2 Receipt: Library And Persistence

Date: 2026-08-15  
Scope: library, persistence, markers, playlist mutation, and migration/rollback
only. All target fixtures used temporary synthetic directories. The native smoke
also used a synthetic local audio fixture. No real HOME, Keychain, credentials,
provider, network, or audio playback was used.

## Provenance And Evidence

- Frozen corpus: `docs/migration/reference-corpus.md` (Wave 0, 2026-08-13).
- Acceptance rules: `docs/migration/acceptance-catalog.md`.
- Reference behavior and migration risks: `migration-audit/03-library-filesystem.md` and `migration-audit/04-persistence-migration.md`.
- Rollback contract: `docs/migration/rollback.md`.
- Target fixture tests: `src-tauri/src/gate2_tests.rs`.
- Target marker implementation: `src-tauri/src/download/markers.rs`.
- Target scan/index/favorite/repair/mutation/persistence implementations: `src-tauri/src/library/` and `src-tauri/src/persistence/`.
- Native smoke implementation and test seam: `src/app/gate2Smoke.tsx` and `src/test/gate2_smoke.test.ts`.
- Native runtime artifact: `docs/verification/artifacts/gate2-native-bridge-smoke.png` and its self-contained receipt `docs/verification/receipts/gate2-native-bridge-smoke.json`.

The four older scoped JSON receipts are preserved as narrower controlled-seam
receipts. Their historical classifications remain unchanged: they document
deterministic AppState, marker, scan, startup, and local promotion seams, not the
launched native bridge.

Reference/unit PASS means the frozen reference expectation and the target's
deterministic unit fixture both passed. Target parity PASS is limited to directly
observed synthetic behavior. The native smoke adds directly observed
Tauri/React command and event boundary evidence; it does not imply provider or
network evidence.

## Fixture Classifications

| Fixture | Reference/unit | Target parity | Evidence and observation |
|---|---|---|---|
| Root registration and config command path | PASS | PASS | Controlled target command fixtures observe configured-root registration and root-status behavior without HOME fallback. |
| REF-ROOT-MISSING | PASS | PASS | Real temporary missing child is reported as `Missing`, with no tracks or fabricated content. |
| REF-ROOT-EMPTY | PASS | PASS | Real temporary empty root is scanned safely and produces an empty snapshot. |
| REF-SCAN-ORDER | PASS | PASS | Immediate folders become playlists; supported root audio is `Unsorted`, newest-first, deterministic; `.txt` is excluded. |
| REF-SCAN-GENERATION | PASS | PASS (deterministic target seam) | Focused target tests directly exercise coalescing, generation ordering, stale suppression, follow-up spawn failure, cancellation, shutdown, and no late event publication. This remains seam evidence. |
| REF-FAVORITES | PASS | PASS | Normalized favorite paths project only live scanned paths and prune missing paths. |
| REF-RENAME-REPAIR | PASS | PASS | Rename repair updates queue, original queue, recent, favorite, and artwork references; stale paths prune. Physical UI rename/rescan was not exercised. |
| Playlist create/delete/repair safeguards | PASS | PASS | Sanitization, collision, protected-name, URL scheme, atomic allowed-key persistence, duplicate grouping, and allowed-key suppression pass. |
| REF-CONFIG-CORRUPT | PASS | PASS | Known fields and safe type filtering are read without rewriting input; malformed JSON returns a safe reader error. |
| REF-MARKER-CORRUPT | PASS | PASS | Current, failure, and legacy markers load; corrupt IDs do not become success/failure state; permanent failures remain retained and transient failures remain retryable. |
| REF-URL-MARKERS | PASS | PASS (controlled local seam) | Synthetic URL-bearing entries cover whole-root dedupe, validated success, failure, permanent retention, retry, and cancellation. No provider/network extraction was used. |
| REF-DIRECT-OUTPUT | INCONCLUSIVE | PASS (native controlled runtime boundary) | The launched native smoke promoted validated synthetic staged bytes, delivered `library_snapshot` through the Tauri/React bridge, and made the track discoverable through `getLibrary`. Provider/direct-download extraction remains unexercised. |
| Allowed-duplicate persistence | PASS | PASS | Valid keys round-trip atomically; malformed JSON is rejected without fabricating allowed state. |
| Atomic writes/recovery | PASS | PASS | Previous content is backed up and restored when target is absent, including corrupt recovery metadata. |
| Migration/rollback persistence | PASS | PASS | Synthetic migration preserves legacy input, writes state/version, removes stale preview only, rolls back an injected conversion crash, and retries cleanly. |
| Inaccessible roots | INCONCLUSIVE | PASS (observed host) | The Unix fixture reports `Inaccessible` on this macOS run; no broader host claim is made. |
| Bounded retryability | PASS | PASS (controlled local seam), otherwise INCONCLUSIVE for UI | Local sync proves transient retry and permanent retention. No target UI retry journey was exercised. |
| Target favorite/recent lifecycle | PASS | PASS | Controlled dispatch observes normalized favorite projection, pruning, session-only recent behavior, the 100-item cap, and unknown-track rejection. |
| Native Tauri/React bridge smoke | N/A | PASS (controlled runtime) | A clean isolated Tauri run showed the exact pass overlay, one discovered `Gate2 Smoke` track under `Runtime Demo`, staged-source consumption, a 22-byte destination, and clean process/port teardown. |

## Verification Commands

Commands were run from the repository root. Exit statuses and counts are
recorded below.

| Command | Result | Classification |
|---|---|---|
| `HOME=<fixture-root>/home TMPDIR=<fixture-root>/tmp CARGO_HOME=<pre-cached-cargo-home> CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml gate2_tests -- --nocapture` | exit `0`; 24 passed, 0 failed, 0 ignored, 55 filtered out | PASS; focused Gate 2 target evidence |
| `HOME=<fixture-root>/home TMPDIR=<fixture-root>/tmp CARGO_HOME=<pre-cached-cargo-home> CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml` | exit `0`; 78 passed, 0 failed, 1 ignored, 0 measured; doc-tests 0 | PASS; ignored test requires the real interactive macOS Keychain |
| `HOME=<fixture-root>/home TMPDIR=<fixture-root>/tmp CARGO_HOME=<pre-cached-cargo-home> CARGO_NET_OFFLINE=true cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | exit `1`; pre-existing unrelated Rust formatting drift | INCONCLUSIVE; no formatter rewrite applied |
| `env -u PYTHONPATH uv run --with pytest python -m pytest` | exit `0`; 109 passed in 14.40s | PASS |
| `npm test` | exit `0`; 19 tests passed | PASS |
| `npm run typecheck` | exit `0`; `tsc --noEmit` completed | PASS |
| `npm run build` | exit `0`; Vite transformed 52 modules and built successfully; existing outDir warning | PASS |
| `git diff --check` | exit `0`; no whitespace errors | PASS |
| `jq empty docs/verification/receipts/*.json` | exit `0`; all six JSON receipts valid, including the pre-existing `r06-manifest.json` | PASS |

The full Cargo run emitted one ignored interactive Keychain test and the same
pre-existing compiler warnings in `src/migration/first_run.rs:777`,
`src/download/sync.rs:157`, and `src/migration/keychain.rs:74`. Application
`HOME` and `TMPDIR` were isolated; the pre-cached Cargo dependency home was
the only shared cache. The native smoke process was terminated after
observation; `ps` showed no luno/vite/tauri process and port 1420 was not
listening.

## Artifact Inventory

- `docs/verification/artifacts/gate2-native-bridge-smoke.png`
- `docs/verification/receipts/gate2-native-bridge-smoke.json`
- `docs/verification/receipts/gate2-library-lifecycle.json` (older scoped seam receipt, preserved)
- `docs/verification/receipts/gate2-persistence-sync.json` (older scoped seam receipt, preserved)
- `docs/verification/receipts/gate2-scan-events.json` (older scoped seam receipt, preserved)
- `docs/verification/receipts/gate2-startup-ipc.json` (older scoped seam receipt, preserved)
- `src-tauri/src/gate2_tests.rs` (evidence source)

Scratch logs and fixtures were temporary and are not deliverables.

## Evidence Boundaries

The focused target seam tests continue to support deterministic scan
coalescing, stale-result rejection, cancellation, shutdown, marker handling,
and local promotion/output validation. The native smoke independently proves
the actual launched Tauri/React command/event boundary and promotion
discoverability journey: `registerRoot`, `promoteAudioOutput`, `scanLibrary`,
`library_snapshot`, and `getLibrary` were observed in that order through the
visible React/WebView overlay.

This evidence does not claim provider/network extraction, yt-dlp behavior,
real HOME persistence, Keychain behavior, credentials, audio playback, or
network behavior. No later-gate behavior is promoted to Gate 2.

## Formal Gate 2 Verdict

**PASS — Formal Gate 2 parity**

The controlled reference and target fixtures pass, the focused Gate 2 Rust
target has 24 passing tests, and the clean native smoke supplies equivalent
controlled runtime proof for the previously missing Tauri/React bridge and
validated promotion-to-scan-to-library-discovery boundary. The full Cargo,
frontend, Python, JSON, and diff checks recorded above also pass, subject only
to the pre-existing unrelated `cargo fmt --check` drift and the intentionally
ignored interactive Keychain test.

This formal PASS is bounded by the evidence above. Provider/network extraction,
real HOME/Keychain/credentials, audio playback, and network behavior remain
unexercised and are not represented as PASS.

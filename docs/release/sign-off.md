# Desktop Release Sign-Off

Status: **BLOCKED**. R07 final parity audit on 2026-08-14, audited source HEAD
`b76d9b5be9ee43b3a5a6576b0904d6b3858b3ebc`.

R06 receipt generation is separately recorded at `c208950f34bbc3fcc053e1b5d2cd49d593715c57`; later receipt-document corrections are not source-run commits and are not self-claimed as this document's commit.

Audited worktree state: ` M TODO.md` and ` M dispatch-plan.json`. R07's focused
documentation changes are not included in that pre-edit state. There is no
standalone R04 receipt document in `docs/`; R04 evidence is represented by the
later R05/R06 receipts and is not promoted to PASS independently.

The macOS Tauri release is not approved. The Python/PyInstaller release and
legacy user data must remain available as rollback fallback.

## Blocking Conditions

Machine-readable release blockers:

```json
[
  {"id":"AC-PLAY-06","status":"INCONCLUSIVE","reason":"target_migration_rejection_evidence_unavailable"},
  {"id":"AC-DL-02","status":"INCONCLUSIVE","reason":"target_fake_subprocess_workflow_receipts_unavailable"},
  {"id":"AC-XFER-04","status":"INCONCLUSIVE","reason":"target_planner_provider_handoff_evidence_unavailable"},
  {"id":"AC-PERSIST-01","status":"INCONCLUSIVE","reason":"target_crash_recovery_marker_receipt_unavailable"},
  {"id":"AC-OPS-02","status":"INCONCLUSIVE","reason":"packaged_tauri_artifact_and_reviewed_managed_binary_inputs_missing; checksum_placeholders_remain"},
  {"id":"rust_cargo","status":"FAILED","exit_code":101,"reason":"R06 Rust compilation reported concrete diagnostics"}
]
```

- U45 integration blockers are source-resolved: Tauri bootstrap/unsupported command errors, startup projection, typed Song Page video fallback, Discover placeholder, and seven-route shell contract. Their target build and runtime receipts remain inconclusive.

- Required target TypeScript is INCONCLUSIVE: `npm run typecheck` exited `127` because `tsc` was unavailable. The Rust gate is FAILED: `cargo test --manifest-path src-tauri/Cargo.toml` exited `101`. Missing-tool checks remain INCONCLUSIVE where the tool itself is unavailable.
- Visual screenshots, accessibility checks, and all manual journeys have no executed receipts or artifacts.
- Target differential, migration-corpus, crash-recovery, shutdown/orphan, and clean-machine smoke gates are unavailable or inconclusive.
- No signed/notarized bundle exists. Binary provisioning cannot pass because `release-input/bin/` is absent and the release manifest contains placeholder SHA-256 values.
- Provider-index transfer handoff and the approved crossfade-removal migration test lack target evidence.
- Production still uses `NoopKeychain`; key-bearing legacy migration must block until a real macOS Keychain adapter is wired.
- IPC transport, subscription, and command wiring are implemented in source, while target backend and video capability evidence remains unavailable or failed; the typed disconnected state and artwork fallback are not backend runtime evidence.
- The shell geometry check is a contract check only, not rendered screenshot evidence.

## Evidence

Reference checks passed: `python3 -m pytest -q` (`109 passed`), the U44 subset
(`6 passed`), both JSON manifest parses, and `git diff --check`. These checks do
not satisfy target parity or release sign-off. See
`docs/verification/final-parity.md` for the acceptance-by-acceptance audit and
exact receipts.

## Approval Rule

Do not sign or distribute the Tauri artifact until every acceptance ID and
journey has a successful required receipt, with no skipped gate, timeout,
missing dependency, missing artifact, or inconclusive result remaining.

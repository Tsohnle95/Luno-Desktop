# Desktop Release Sign-Off

Status: **BLOCKED**. U45 fan-in review on 2026-08-13.

The macOS Tauri release is not approved. The Python/PyInstaller release and
legacy user data must remain available as rollback fallback.

## Blocking Conditions

- U45 integration blockers are source-resolved: Tauri bootstrap/unsupported command errors, startup projection, typed Song Page video fallback, Discover placeholder, and seven-route shell contract. Their target build and runtime receipts remain inconclusive.

- Required target Rust and TypeScript gates are inconclusive: `cargo` and `tsc` are unavailable.
- Visual screenshots, accessibility checks, and all manual journeys have no executed receipts or artifacts.
- Target differential, migration-corpus, crash-recovery, shutdown/orphan, and clean-machine smoke gates are unavailable or inconclusive.
- No signed/notarized bundle exists. Binary provisioning cannot pass because `release-input/bin/` is absent and the release manifest contains placeholder SHA-256 values.
- Provider-index transfer handoff and the approved crossfade-removal migration test lack target evidence.
- Production still uses `NoopKeychain`; key-bearing legacy migration must block until a real macOS Keychain adapter is wired.
- Frontend backend and video availability remain explicitly unavailable because runtime IPC is not implemented.
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

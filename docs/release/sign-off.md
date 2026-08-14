# Desktop Release Sign-Off

Status: **BLOCKED**. Current evidence was refreshed on 2026-08-14 at source
HEAD `e6d2372c43638e0f5f6163fe57b53e689458e7fd` after commits `93915b32`,
`8343bfbf`, and `e6d2372c`. The macOS Tauri release is not approved; retain the
Python/PyInstaller fallback and legacy user data.

## Evidence

- `npm ci`: PASS, exit `0`, 106 packages installed, no vulnerabilities.
- `npm run typecheck`: PASS, exit `0`.
- `npm run build`: PASS, exit `0`, Vite built 51 modules.
- `npm test`: PASS, exit `0`, 13 tests passed in 2 files.
- `cargo test --manifest-path src-tauri/Cargo.toml`: PASS, exit `0`, 31 passed,
  1 ignored, 0 failed. The ignored test requires interactive macOS login
  Keychain access.
- `npm run tauri build`: PASS for local packaging, exit `0`; produced the
  unsigned local aarch64 DMG `src-tauri/target/release/bundle/dmg/Luno_0.1.0_aarch64.dmg`.
  Its observed local SHA-256 is
  `055c52e306c8725f26f5ae3fd6f432c3913cc14b58b5b666d2271c4530c28786` and is
  not a reviewed release checksum.
- Video commands remain explicitly unavailable and have no passing target
  receipt.

## Remaining Blockers

Release is blocked only by unresolved release prerequisites and target evidence:

- No reviewed `release-input/bin/` executables and no pinned reviewed checksums.
- No signing or notarization evidence.
- No target runtime/packaged smoke, visual screenshot, accessibility,
  migration/rollback, shutdown/orphan-process, manual journey, or real-user
  evidence.

These absent or unrun receipts are **INCONCLUSIVE**, not failed. The local
unsigned DMG is not approved for distribution. No artifact, signature,
notarization record, screenshot, or user evidence was fabricated.

## Approval Rule

Do not sign or distribute the Tauri artifact until the remaining target and
release prerequisites have reviewed, successful receipts with no inconclusive
required gate.

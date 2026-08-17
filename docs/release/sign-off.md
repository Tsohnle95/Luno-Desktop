# Desktop Release Sign-Off

## Current Receipt: 2026-08-16 (HEAD `0fef32a6` evidence base)

Observed run timestamp: `2026-08-16T19:47-19:52` (local, UTC-6), repository
root, verified source HEAD `0fef32a6` plus the documentation reconciliation
commit `d6419c9a` (no runtime change). This is a durable local evidence
receipt, not release approval.

### Deterministic Gates Re-Run At `0fef32a6`

| Command | Exit | Result |
|---|---:|---|
| `npm run typecheck` | 0 | PASS |
| `npm test` | 0 | PASS — 9 files / 45 tests |
| `CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml` | 0 | PASS — 98 passed, 1 ignored (interactive login Keychain; pre-existing) |
| `git diff --check` | 0 | PASS |

### Supply Chain Verified

- `release/desktop-binaries.json` contains reviewed pinned SHA-256 values
  (zero `REPLACE_WITH_REVIEWED` placeholders) for ffmpeg 7.1.1 + ffprobe 7.1.1
  (LGPL-only, locally built arm64) and yt-dlp 2025.08.11 (official release).
- The staged `src-tauri/resources/bin/` executables were hash-rechecked against
  the manifest: **ffmpeg, ffprobe, yt-dlp all MATCH**. `release/provision.sh`
  is now tracked and reproduces this staging with hash verification.
- `scripts/verify-desktop-bundle.sh` remains the structural bundle gate and is
  not replaced by hash checks alone.

### Resolved Since The 2026-08-15 Receipts

- **Isolated startup panic: RESOLVED.** Receipt
  `docs/verification/receipts/startup-isolated-home.md` records two fresh
  packaged launches (`PATH=/usr/bin:/bin`, isolated `HOME`): empty HOME no
  longer panics (`MigrationStatus::Skipped`, window + AX evidence), and a
  legacy-config HOME migrates to the correct app-data path additively with
  backup, no orphan processes. Artifacts under `docs/verification/artifacts/`.
- **Gate 5:** `docs/verification/receipts/gate5-discover-preview-video.md` —
  PASS for the opt-in synthetic seam through the real Tauri runtime (native
  command → IPC → WebView → shutdown). Real-provider, FFmpeg/media, manual
  journeys, and packaged-runtime parity remain **BLOCKED / unobserved**.
- **Gate 6:** `docs/verification/receipts/gate6-states-shutdown-r2.md` — PASS
  **with reservations**: populated library/queue/player, all seven routes,
  keyboard traversal/activation, error state, download lifecycle
  (search → select → running → cancel → cancelled), and close-while-work-active
  shutdown are native-observed. Loading row documented unreachable in the
  deployed boot order; video row unavailable in the dev stage (documented).
- **Crossfade: DECIDED, removed from v1.** `docs/decisions/desktop-migration.md`
  D8 and `src-tauri/src/playback/crossfade.rs` record the deferral; no
  audible-crossfade claim exists. This is no longer a pending blocker, and the
  `crossfade` preference is not exposed or persisted by the target.
- **Production wiring:** `628a7002` (settings boot, editable library root) and
  `0fef32a6` (download helper, real sources, M4A output) are committed with
  their native dev-runtime verification notes; destination defaults to
  `Unsorted` under the configured library root.

### Remaining Release Prerequisites (INCONCLUSIVE)

1. **Signing / notarization: BLOCKED externally.** `security find-identity
   -v -p codesigning` reports `0 valid identities`. No Developer ID signing,
   hardened-runtime signing, notarization, or stapling exists.
2. **Clean-machine install / rollback:** not performed; requires a clean macOS
   profile/machine without development PATH tools.
3. **Packaged real-download completion:** the 2026-08-16 packaged-UI QA run was
   terminated before the download-completion poll; boot render, settings, and
   search results were captured, but the terminal job state and post-scan
   visibility in the packaged app remain unobserved.
4. **Physical macOS audio device/codec measurement:** Cargo/unit and native
   playback evidence exist, but an external audible-output measurement on a
   physical device is not recorded.
5. **Real provider/network journeys** (live Last.fm/YouTube with authorized
   network) remain unobserved; only loopback/synthetic evidence exists.
6. **Target video backend:** unavailable in the target; documented as such.

### Verdict

**INCONCLUSIVE / BLOCKED for release approval.** Deterministic gates and the
documented implementation gates 2-6 are green at `0fef32a6`; release approval
is still gated on the six prerequisites above, chiefly the external signing
identity and clean-machine evidence. The local unsigned DMG is a historical
build artifact, not an approved release artifact. The Python/PyInstaller
fallback and legacy user data remain the rollback boundary
(`docs/migration/rollback.md`). The sections below are historical evidence and
do not reflect the current state.

## HISTORICAL — Gate 7 Fresh Package Receipt (2026-08-15T21:50Z, superseded)

Observed run timestamp: `2026-08-15T21:50:38Z` (UTC; temporary evidence-root
creation time).

This is a durable local evidence receipt, not release approval. The observed
package run succeeded, but release readiness remains **INCONCLUSIVE / BLOCKED**.
Before this receipt, HEAD was `f9e81c78e38a6c3a2fcb7aaa707f9516cd0088b5` and the
workspace contained unrelated Gate 2-6/application changes. The bounded
remediation earlier in this task changed the already-dirty
`src-tauri/tauri.conf.json` resource entries to explicit destinations, added
`src-tauri/resources/bin/.gitkeep`, and updated the project brain file registry.
This fresh package-evidence refresh then changed only
`docs/release/sign-off.md`. No commit, push, deploy, publish, network,
credential, signing, notarization, stapling, or release command was performed.

### Fresh Package Evidence

- `HOME=/tmp/luno-gate7-package2.Va2P4H/home TMPDIR=/tmp/luno-gate7-package2.Va2P4H/tmp CARGO_TARGET_DIR=/tmp/luno-gate7-package2.Va2P4H/target CARGO_HOME=/Users/ty/.cargo CARGO_NET_OFFLINE=true npm run tauri -- build --bundles dmg` exited `0`; `npm run build` ran first and Vite transformed 55 modules.
- The local DMG was `/tmp/luno-gate7-package2.Va2P4H/target/release/bundle/dmg/Luno_0.1.0_aarch64.dmg`, 52,527,211 bytes, SHA-256 `0695adc31755587b328b71cacca23d4547c82aa640aab5210107091b2c3ef6e5`. `hdiutil imageinfo` identified UDZO; readonly attach and forced detach both exited `0`.
- The DMG was copied only to `/tmp/luno-gate7-package2.Va2P4H/install/Luno.app`, not `/Applications`. Its required root resources are present: `desktop-binaries.json` and `THIRD_PARTY_NOTICES.md`; `Contents/Resources/bin` exists but has no managed executables beyond the source `.gitkeep`. No `_up_/release` directory exists.
- The manifest still contains three `REPLACE_WITH_REVIEWED_SHA256` placeholders. `Info.plist` reports `com.ty.luno`, version `0.1.0`, and minimum macOS `13.0`. The arm64 main executable is 66,528,528 bytes with SHA-256 `99b7b57a0f35ea55aea52dbfa5b23913cb590cf074ca6ff9707c2436630c214d`.
- Explicit Tauri resource destination mapping and the tracked empty source directory are fixed and verified. They do not make this package release-ready.

### Checks And Blockers

1. No reviewed binaries or non-placeholder checksums: `LUNO_BINARY_SOURCE_DIR=release-input/bin ./scripts/provision-desktop-binaries.sh` exited `1` with `missing reviewed binary: release-input/bin/ffmpeg`; no developer-PATH substitution occurred.
2. Bundle verification failed: `./scripts/verify-desktop-bundle.sh /tmp/luno-gate7-package2.Va2P4H/install/Luno.app` exited `1` with `non-executable bundled ffmpeg`.
3. The app is unsigned/ad-hoc: strict `codesign --verify --deep --strict --verbose=2` exited `1` with `code has no resources but signature indicates they must be present`; `codesign -dv --verbose=4` reported arm64 ad-hoc/linker-signed, no Team ID, and `Sealed Resources=none`; `spctl` exited `1` with the same resource error.
4. No signing identity exists: `security find-identity -v -p codesigning` exited `0` but found `0 valid identities`.
5. No notarization ticket exists: `xcrun stapler validate` exited `65`. No signing, notarization, or stapling invocation occurred.
6. Clean GUI/runtime evidence is unavailable: isolated direct launch with fixture-only temporary HOME and `PATH=/usr/bin:/bin` had `timed_out=false`, child exit `-6`, and stderr reported `unsafe legacy migration input` followed by a non-unwinding panic. No matching process remained; no GUI window, playback, accessibility, screenshot, rollback, or clean-machine install was observed.

Independent deterministic checks passed: `npm run typecheck` (0), `npm test` (0; 6 files/34 tests), `npm run build` (0; 55 Vite modules), `CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml` (0; 89 passed/0 failed/1 ignored), `npm run tauri -- --version` (0; tauri-cli 2.11.4), resource mapping JSON validation (0), and `git diff --check` (0). The invalid `npm test -- --runInBand` probe was superseded by plain `npm test` and is not a release failure.

Warning: The offline Cargo run emitted three existing non-fatal warnings: `unused_mut` in `migration/first_run.rs`, an unused variable in `download/sync.rs`, and `dead_code` for the macOS keychain symbol. The Vite build emitted its existing warning that `outDir ../dist` is outside the project root and is not emptied. These warnings did not change exit status or test counts.

The local unsigned DMG is a historical/local evidence artifact only. It is not
an approved release artifact, and this receipt makes no release approval claim.
Retain the Python/PyInstaller fallback and legacy data; `docs/migration/rollback.md`
remains the rollback boundary.

The older historical sections follow.

## HISTORICAL — Formal Gate 7 Receipt (2026-08-15T13:19Z, superseded)

Run timestamp: `2026-08-15T13:19:24Z` (UTC). This is a packaged-release
readiness receipt, not release approval. No commit, push, deploy, publish,
distribution, signing, notarization submission, credential access, secret
change, real user data, or unrelated-file deletion was performed.

### Provenance And Dirty Baseline

- Source HEAD before execution: `f9e81c78e38a6c3a2fcb7aaa707f9516cd0088b5`
- The pre-existing dirty baseline was the tracked modifications to
  `package.json`, `project_brain.md`, `src-tauri/Cargo.lock`,
  `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, the listed Rust source
  files under `src-tauri/src/`, and the listed frontend files under `src/`;
  plus untracked prior Gate 2-6 receipts and `src-tauri/src/gate2_tests.rs`
  through `gate5_tests.rs`. This run preserved that baseline and changed only
  this receipt.
- `git status --short --untracked-files=all` after execution reports the same
  pre-existing set; generated `dist/`, `src-tauri/target/`, and temporary DMG
  mount/launch directories are ignored or were removed.

### Required Checks

| Command | Exit | Classification | Observed evidence |
|---|---:|---|---|
| `npm run typecheck` | 0 | PASS | `tsc --noEmit` completed |
| `npm test` | 0 | PASS | 2 files, 15 tests passed |
| `npm run build` | 0 | PASS | Vite transformed 51 modules; emitted `dist/index.html`, CSS, and JS |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | PASS | 63 passed, 0 failed, 1 ignored; ignored test requires interactive macOS login Keychain |
| `npm run tauri -- --version` | 0 | PASS | `tauri-cli 2.11.4` |
| Earlier `npm run tauri build` attempt | 0 | HISTORICAL ONLY | Produced a local unsigned DMG, later independently hashed and inspected; this attempt is superseded and its DMG is not a current or approved release artifact |
| Later authoritative `npm run tauri build` rerun | 1 | FAIL | Built the release executable and app, then failed at DMG bundling: `failed to bundle project: error running bundle_dmg.sh: failed to run .../src-tauri/target/release/bundle/dmg/bundle_dmg.sh` |
| `git diff --check` | 0 | PASS | no whitespace errors |

The later independent verification is authoritative. The staging app remains at
`src-tauri/target/release/bundle/macos/Luno.app`; the old DMG was removed before
the failed rerun, and no current DMG exists. The failed packaging result is not
release readiness evidence.

### Artifacts And Hashes

- Historical first-attempt local DMG only: before the authoritative rerun it was
  independently measured at `49,500,186` bytes with SHA-256
  `5ab6e2d0eda37ecdfabf8811e2d0a53f38f16c927949b4dc4af8654242bbf73e`.
  `hdiutil imageinfo` exited `0` and identified an unencrypted compressed
  `UDZO` image. It was removed before the failed rerun and is not a current or
  approved release artifact.
- Current staging app: `src-tauri/target/release/bundle/macos/Luno.app`.
  `Contents/MacOS/luno-desktop` is Mach-O arm64, `60,940,400` bytes, SHA-256
  `61184c1fd0a543e23eaf901e60e36a31f6a06981b642de24e77a0b58aab2eafc`.
  `Info.plist` reports identifier `com.ty.luno`, version `0.1.0`, and
  `LSMinimumSystemVersion 13.0`.

### Bundle Verification And Managed Binaries

- `./scripts/verify-desktop-bundle.sh src-tauri/target/release/bundle/macos/Luno.app`
  exited `1`: `bundle manifest missing: .../Luno.app/Contents/Resources/desktop-binaries.json`.
- The app contains only `_up_/release/desktop-binaries.json` and
  `_up_/release/THIRD_PARTY_NOTICES.md` under `Contents/Resources`; it has no
  `Contents/Resources/bin/` managed executables.
- `release-input/bin/{ffmpeg,ffprobe,yt-dlp}` and
  `src-tauri/resources/bin/{ffmpeg,ffprobe,yt-dlp}` are absent. No provisioning
  was attempted and no developer-PATH binary was substituted.
- `release/desktop-binaries.json` remains valid JSON, but all three artifact
  hashes are the literal `REPLACE_WITH_REVIEWED_SHA256` placeholder.
  Developer PATH versions are FFmpeg/ffprobe `8.1` and yt-dlp `2026.03.17`;
  clean PATH cannot supply them.

### macOS Validation

- Staging-app `Info.plist`: bundle identifier `com.ty.luno`, version `0.1.0`,
  `LSMinimumSystemVersion 13.0`.
- `codesign --verify --deep --strict --verbose=2` exited `1`: `code has no
  resources but signature indicates they must be present`.
- `codesign -dv --verbose=4` exited `0` and reported an arm64 ad-hoc,
  linker-signed executable (`flags=0x20002(adhoc,linker-signed)`), no Team ID,
  and `Sealed Resources=none`. This is not a release signature or hardened
  runtime result; no `runtime` flag was reported.
- `codesign -d --entitlements :-` exited `0` with no entitlements payload.
- `spctl --assess --type execute --verbose=4` exited `1` with the same missing
  sealed-resource error. No acceptance was claimed.
- `xcrun stapler validate` exited `65`: `Luno.app does not have a ticket
  stapled to it`. `xcrun notarytool --version` exited `0` (`1.1.2 (41)`),
  proving tool availability only. No submission or stapling command was
  invoked.
- `security find-identity -v -p codesigning` reported `0 valid identities`.

### Isolated Launch And Installation

- Direct staging-executable launch used a process-created temporary `HOME`
  containing only empty `fixture-library` and `.luno_previews` directories.
  `timed_out=false`, child exit `-6`, no stdout, and stderr reported panic
  `Failed to setup app: ... unsafe legacy migration input`. The temporary HOME
  was removed and no matching process remained.
- No GUI window, clean-machine install, playback, accessibility, screenshot, or
  user-data rollback evidence was obtained. Classification:
  INCONCLUSIVE/BLOCKED.

### Rollback And Release Boundary

`docs/migration/rollback.md` remains the governing rollback record: migration
is additive, legacy files and audio are preserved, and the Python release must
be used if migration or packaging fails. This receipt distinguishes the local
unsigned DMG from a signed bundle, a notarized artifact, and a
release-approved artifact. None of the latter three exists here. The existing
Python/PyInstaller fallback and legacy user data must be retained.

### Explicit Blockers

1. No reviewed `release-input/bin/` executables are present.
2. The manifest contains placeholder checksums, and reviewed managed binaries
   are missing; clean-PATH smoke cannot pass.
3. The current DMG bundling failed in this environment at `bundle_dmg.sh`.
4. The app resource layout places release files under `_up_/release/`, omits
   managed binaries, and fails the bundle verifier.
5. The app has only an ad-hoc linker signature, fails strict codesign and
   `spctl` resource validation, has no hardened-runtime evidence, and there is
   no signing identity.
6. No notarization or stapling occurred; the app has no stapled ticket.
7. Isolated launch panicked on unsafe legacy migration input and terminated
   with child exit `-6`; no clean-machine evidence exists.

**Formal Gate 7 verdict: INCONCLUSIVE / BLOCKED. The historical local unsigned
DMG is not current or approved for release, signing, notarization, distribution,
or submission.**

The previous sign-off history follows.

Status: **BLOCKED**. Current evidence was refreshed on 2026-08-14 at source
HEAD `969a83319a96603a3608c041fc8abf28e2f8a3f7` after the first-parent range
`93915b329b35085358c8924017a93bbb1ee14fd7^..969a83319a96603a3608c041fc8abf28e2f8a3f7`.
The macOS Tauri release is not approved; retain the Python/PyInstaller fallback
and legacy user data.

## HISTORICAL — Evidence (2026-08-14 base, superseded)

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
- Cargo/unit tests pass, but physical macOS audio backend/device/codec
  target-runtime validation and packaged target playback were not exercised.

## HISTORICAL — Remaining Blockers (2026-08-14 view, superseded)

Release is blocked only by unresolved release prerequisites and target evidence:

- No reviewed `release-input/bin/` executables and no pinned reviewed checksums.
- No signing or notarization evidence.
- Physical macOS audio backend/device/codec target-runtime validation and
  packaged target playback were not exercised; this remains separate from the
  unavailable video backend.
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

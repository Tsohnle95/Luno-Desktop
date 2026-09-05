# Gate 4 Receipt: Downloader, Settings, And Transfer

Date: 2026-08-15
Verdict: **INCONCLUSIVE / BLOCKED**

## Scope And Safety

- This receipt covers the current Gate 4 implementation and its native smoke run.
- The native run was isolated under `/private/tmp/luno-gate4-live3` with synthetic `HOME` and `TMPDIR`.
- Its configured library root was `/private/tmp/luno-gate4-live3/library`.
- A local executable fake helper copied a valid 44.1 kHz stereo PCM WAV. No network, providers, credentials, Keychain, or real `HOME` were used.
- Startup migration was skipped because there was no preexisting legacy config. The probe then saved and reloaded settings.
- No fixture bytes or temporary paths were added to the repository.
- Gate 3 caveat remains: synthetic decode/fixture checks and target events are not evidence of human-audible output or a measured crossfade envelope.

## Independent Checks

| Exact command | Result |
|---|---|
| `cargo test --manifest-path src-tauri/Cargo.toml` | PASS: 85 passed, 1 ignored interactive Keychain test |
| `cargo build --manifest-path src-tauri/Cargo.toml` | PASS |
| `npm test` | PASS: 5 files, 30 tests |
| `npm run typecheck` | PASS |
| `npm run build` | PASS: 54 Vite modules |
| `git diff --check` | PASS |
| `HOME=<fixture-root>/home TMPDIR=<fixture-root>/tmp CARGO_NET_OFFLINE=true VITE_LUNO_GATE4_SMOKE=1 VITE_LUNO_GATE4_SMOKE_HOME=<fixture-root>/home VITE_LUNO_GATE4_SMOKE_ROOT=<fixture-root>/library VITE_LUNO_GATE4_SMOKE_HELPER=<fixture-root>/fake-helper VITE_LUNO_GATE4_SMOKE_DESTINATION=<fixture-root>/destination npm run tauri dev` | BLOCKED: exited 101 before native launch; offline Cargo could not resolve cached `crc32fast`, and the Vite child also reported port 1420 already in use |

Existing Rust `fmt` drift and warnings remain non-blocking and unrelated. They were not changed for this receipt.

## Native Fixture And Run

- Fixture root: `/private/tmp/luno-gate4-live3`.
- Library root: `/private/tmp/luno-gate4-live3/library`.
- The local executable fake helper copied a valid 44.1 kHz stereo PCM WAV into the download flow.
- Marker file: `.downloaded_vids`.
- The run used no network, providers, credentials, Keychain, or real `HOME`.
- No preexisting legacy config was present, so startup migration was skipped; the probe saved and reloaded settings.

Durable artifact: `docs/verification/artifacts/gate4-native-downloader-settings-transfer.png`.
It is a valid 2360x1600 PNG. The native `luno-desktop` window visibly displayed `Gate 4 smoke pass`.

The overlay observed these commands in order:

`subscribe`, `saveSettings`, `loadSettings`, `registerRoot`, `selectLibraryRoot`, `previewCsv`, `validateDirectUrl`, `queueUrls`, `syncMarkers`, `scanLibrary`, `getLibrary`, `transferExport`, `transferPreview`, `transferApply`, `startDownload`, `shutdown`.

Observed job states were `running`, `running`, and `succeeded`. No errors or missing observations were reported.

Unexercised disclosures: provider search/provider-network access, native file-picker UI, and real external provider credentials/services.

## Current Native Retry

The current isolated retry used the exact command recorded above and did not reach the compiled binary. Cargo offline resolution failed with `no matching package named crc32fast found`; the Vite child separately reported port 1420 already in use. No native result is claimed for this retry.

## Filesystem Corroboration

- Config exists under the synthetic fixture home.
- Promoted output exists at `/private/tmp/luno-gate4-live3/library/Unsorted/gate4-smoke.mp3` and is 352878 bytes.
- No `.part` files remain.
- The `.downloaded_vids` marker file exists.

## Evidence Matrix

| Boundary | Result |
|---|---|
| Local settings, root, downloader validation, CSV, queue, marker, transfer, export, preview, apply, and fake-helper atomic promotion | PASS at target bridge/runtime |
| Frontend view controls | PASS only by tests |
| Provider/network search | UNOBSERVED |
| Native picker | UNOBSERVED |
| Direct user click flows | UNOBSERVED |
| Cancel/retry | UNOBSERVED |
| Approval-bearing transfer import | UNOBSERVED |
| External provider handoff | UNOBSERVED |

## Formal Verdict

The formal verdict is **INCONCLUSIVE / BLOCKED**, not PASS, because the provider/network, native picker, direct user-flow, cancel/retry, approval-bearing transfer-import, and external-provider-handoff boundaries remain unobserved. The Gate 3 no-human-audible/crossfade caveat also remains.

## Diff Scope

File-only documentation diff: `docs/verification/receipts/gate4-downloader-settings-transfer.md`.
No code, tests, other docs, commit, push, or reset was performed.

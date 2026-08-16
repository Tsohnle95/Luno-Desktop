# Isolated Packaged Startup Receipt: migration panic fix

Run timestamp: `2026-08-16T02:15-02:19` (this environment's local clock).
This receipt records the reproduction, root cause, fix, regression tests, and
real packaged-launch evidence for the isolated startup panic. It is
development evidence, not release approval.

## Reported failure

An isolated packaged/direct launch with a fresh temporary HOME, empty fixture
library and preview directory, and `PATH=/usr/bin:/bin` aborted with a
non-unwinding panic:

- Child exit `134` (SIGABRT; the receipt at that time recorded `-6`)
- stderr:

```
thread 'main' panicked at .../tauri-2.11.5/src/app.rs:1425:11:
Failed to setup app: error encountered during setup hook: unsafe legacy migration input
```

Reproduced on HEAD `f9e81c78` binary
`src-tauri/target/release/bundle/macos/Luno.app/Contents/MacOS/luno-desktop`
with `HOME=/tmp/luno-repro.6hDI2O/home`.

## Root cause

Two defects in `src-tauri/src/migration/legacy_readers.rs::validate_existing_path`
(plus the startup assembly that turned the failure into an abort):

1. **Strict symlink-component rejection.** `validate_existing_path` walked
   every existing path component and rejected any symlink. On macOS the
   standard system redirects `/tmp -> /private/tmp` (and `/var -> /private/var`,
   which holds the default `TMPDIR=/var/folders/...`) are symlinks. Any
   isolated HOME below them failed validation, `has_legacy_data` returned
   `Err`, and `startup_migration` (main.rs) returned
   `Err("unsafe legacy migration input")`. Tauri runs the setup hook inside
   the AppKit `did_finish_launching` callback, where unwinding is forbidden
   (`panic in a function that cannot unwind`) -> SIGABRT. A HOME under
   `/private/tmp` (no symlink components) launched without this failure,
   confirming the diagnosis.

2. **Multi-level missing-suffix collapse.** The validation result was computed
   with `canonicalize_path`, which canonicalizes the deepest existing ancestor
   and re-appends only the final name. For a not-yet-created multi-level
   target (`HOME/Library/Application Support/com.ty.luno`), the intermediate
   components were dropped, so a genuine first run would write migration
   records to `HOME/com.ty.luno` instead of the Tauri app-data directory.
   The pre-existing unit test masked this because it pre-created the full
   target root before migrating.

## Fix

- `validate_existing_path` now canonicalizes the deepest existing ancestor
  and preserves **every** component below it, so system redirects resolve and
  not-yet-created targets keep their full shape. A symlinked **leaf** stays
  unresolved so the explicit leaf checks (preflight, `resolve_music_root`,
  `safe_directory`, `cleanup_previews`) still reject it; containment checks
  compare canonicalized paths; inventory traversal still skips symlinks.
- `startup_migration` (macOS) no longer aborts app setup on migration-input
  failure: it records `MigrationStatus::Failed { reason }` (with the
  underlying error) and continues, so the app always opens a native window
  with a truthful bounded migration state. Migration remains fail-closed —
  nothing is migrated or scanned when input is unsafe.

## Regression tests

Added to `src-tauri/src/migration/first_run.rs` (`mod tests`, macOS-only):

- `isolated_home_under_system_tmp_redirect_is_safe_first_run` — empty HOME
  below `/tmp` validates, `has_legacy_data` is `Ok(false)`, and a full
  migration with a legacy config succeeds with records at
  `HOME/Library/Application Support/com.ty.luno/`.
- `redirect_resolution_does_not_weaken_symlinked_leaf_rejection` — a
  symlinked leaf still survives ancestor resolution and is still rejected as
  a configured music root (`resolve_music_root`).

Both failed against the pre-fix code with
`path component must not be a symlink: /tmp` and pass after the fix.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib migration::first_run::tests::`:
  `3 passed` (2 regression + pre-existing envelope test).
- `CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml`:
  `91 passed, 0 failed, 1 ignored` (ignored test requires interactive login
  Keychain; pre-existing).
- `npm test`: `34 passed`; `npm run typecheck`: exit 0; `npm run build`:
  exit 0 (55 modules). `git diff --check`: exit 0.
- Pre-existing warnings unchanged (3 Cargo warnings, Vite outDir notice).

## Packaged launch evidence

Fresh `npm run tauri -- build --bundles app` (isolated HOME/TMPDIR,
`CARGO_HOME=/Users/ty/.cargo`, `CARGO_NET_OFFLINE=true`) exited 0 and produced
`src-tauri/target/release/bundle/macos/Luno.app`.

### Launch 1 — empty isolated HOME (the previously failing scenario)

`HOME=/tmp/luno-launch.r3v8Ox/home` (only `fixture-library/` and
`.luno_previews/`), `TMPDIR` isolated, `PATH=/usr/bin:/bin`.

- Process alive after 6 s, `STAT SN`, empty stderr (no panic).
- Native window: `CGWindowList` `owner=Luno id=74823 layer=0 name=Luno`
  `1180x800`. Screenshot:
  `docs/verification/artifacts/startup-isolated-empty-home.png`.
- AX tree (`docs/verification/artifacts/startup-isolated-empty-home.ax.txt`):
  `AXWindow/AXStandardWindow` -> WebArea with sidebar navigation (Home,
  Recent, Discover, Downloader, Settings, Song, Error Log) and application
  status region reading exactly `Migration skipped: no legacy data found`.
- Graceful close via `AXButton/AXCloseButton` press: ok; process exited;
  `pgrep` found no luno-desktop/vite/tauri orphan.
- Real `~/.luno_config.json` mtime unchanged; isolated HOME untouched.

### Launch 2 — isolated HOME with legacy config

`HOME=/tmp/luno-migrate-launch.8MoQr4/home` with `.luno_config.json`
(`{"music_root":"Music","volume":0.7}`), `PATH=/usr/bin:/bin`.

- Process alive after 8 s, empty stderr, no panic.
- Real packaged first-run migration completed at the correct app-data path:
  `HOME/Library/Application Support/com.ty.luno/` contains
  `migration-version.json` (`desktop-first-run`, version 1, completed),
  `settings-v1.json` (music_root `Music`, volume 0.7), and
  `migration-backup-*/` with the redacted config (`lastfm_key: [REDACTED]`).
- Legacy `.luno_config.json` retained byte-for-byte (additive migration).
- Graceful close via AX close button; no orphan processes.

Transcripts: `docs/verification/artifacts/startup-launch-transcripts.txt`.

## Scope notes

This receipt does not claim: signed/notarized artifacts, clean-machine install,
Gate 5-7 completion, or release approval. `docs/release/sign-off.md` remains
the governing sign-off and stays **INCONCLUSIVE / BLOCKED** until its required
gates are independently satisfied.
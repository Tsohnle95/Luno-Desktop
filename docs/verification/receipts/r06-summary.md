# R06 Verification Summary

Status: **INCONCLUSIVE RELEASE BLOCKER**. Run date: 2026-08-14. Source-run HEAD: `b76d9b5be9ee43b3a5a6576b0904d6b3858b3ebc`.

Receipt provenance: R06 was run from source-run HEAD `b76d9b5be9ee43b3a5a6576b0904d6b3858b3ebc`. The listed receipt-document SHAs are selected anchors, not a contiguous chain: receipt-generation anchor `c208950f34bbc3fcc053e1b5d2cd49d593715c57`, correction anchors `65cabe686f32125849c3d13b0ffce8b814396f27` and `cba3a17a1fce8cd4faed98f510da5039f1776702`. Exact first-parent verification command at the audited R07 HEAD: `git log --first-parent --format='%H %s' --reverse b76d9b5be9ee43b3a5a6576b0904d6b3858b3ebc..25e5ba7c0e627222ea50a4f5301162715a1a64ee`; its output is the first-parent commit history from the source-run anchor through the audited receipt commits. This summary does not claim the SHA of its containing commit.

## Results

- Python differential: PASS as reference-only, `6 passed in 0.64s`.
- Python full suite: PASS as reference-only, `109 passed in 13.16s`.
- JSON syntax and `git diff --check`: PASS.
- Rust Cargo: FAILED, exit `101`; compilation reached the Rust target and reported concrete errors (43 in the library and 48 in the library tests).
- TypeScript typecheck/build: INCONCLUSIVE, R06 exit `127`; `tsc` was unavailable (`sh: tsc: command not found`). Any exit-2 TypeScript result in `toolchains.md` is from a prior historical run, not R06.
- TypeScript tests: INCONCLUSIVE, no `test` script exists.
- Tauri smoke: INCONCLUSIVE, exit `127`; `tauri` was unavailable (`sh: tauri: command not found`).
- Visual screenshots, accessibility/focus, manual journeys, target differential, migration/rollback, process/download/video lifecycle, and shutdown/orphan checks: explicitly unavailable or not run; each receipt records its status/result/reason. No packaged release or managed media binaries were produced. Generated build artifacts from attempted toolchain work, if present, are not release artifacts.

All 37 feature IDs and 6 journey IDs are mapped in `r06-manifest.json`. Reference-only passes are not target parity passes. Existing dirty files `TODO.md` and `dispatch-plan.json` were preserved.

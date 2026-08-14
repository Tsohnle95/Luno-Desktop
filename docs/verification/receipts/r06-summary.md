# R06 Verification Summary

Status: **BLOCKED**. Run date: 2026-08-14. Source HEAD: `c208950f34bbc3fcc053e1b5d2cd49d593715c57`.

Receipt provenance: R06 was evaluated against source commit `c208950f34bbc3fcc053e1b5d2cd49d593715c57`. Because receipt corrections are committed separately, `metadata/final-head.txt` and `metadata/final-status.txt` are the exact `git rev-parse HEAD` and `git status --porcelain` outputs captured immediately after receipt generation, before the receipt-fix commit.

## Results

- Python differential: PASS as reference-only, `6 passed in 0.64s`.
- Python full suite: PASS as reference-only, `109 passed in 13.16s`.
- JSON syntax and `git diff --check`: PASS.
- Rust Cargo: FAILED, exit `101`; compilation reached the Rust target and reported concrete errors (43 in the library and 48 in the library tests).
- TypeScript typecheck/build: INCONCLUSIVE, exit `127`; `tsc` was unavailable (`sh: tsc: command not found`).
- TypeScript tests: INCONCLUSIVE, no `test` script exists.
- Tauri smoke: INCONCLUSIVE, exit `127`; `tauri` was unavailable (`sh: tauri: command not found`).
- Visual screenshots, accessibility/focus, manual journeys, target differential, migration/rollback, process/download/video lifecycle, and shutdown/orphan checks: explicitly unavailable or not run; each receipt records its status/result/reason. No screenshots, binaries, user data, or fabricated artifacts were produced.

All 37 feature IDs and 6 journey IDs are mapped in `r06-manifest.json`. Reference-only passes are not target parity passes. Existing dirty files `TODO.md` and `dispatch-plan.json` were preserved.

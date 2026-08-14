# Toolchain Verification Receipt

Receipt timestamp: `2026-08-14T09:19:38Z`

## Environment

- Host: macOS `26.5` (build `25F71`), Apple Silicon `arm64`
- Xcode Command Line Tools: `/Library/Developer/CommandLineTools`
- Node: `v22.23.2`
- npm: `10.9.8`
- Rust installed during this run with Homebrew: `rustc 1.97.1 (8bab26f4f 2026-07-14) (Homebrew)`
- Cargo installed during this run with Homebrew: `cargo 1.97.1 (c980f4866 2026-06-30) (Homebrew)`
- `rustup` is not installed; its required version check is inconclusive.
- Network was available for npm and crates.io/Homebrew downloads.

## Prior Historical Receipts (Not R06 Authoritative)

| Command | Exit | Status | Output |
|---|---:|---|---|
| `node --version` | `0` | PASS | `v22.23.2` |
| `npm --version` | `0` | PASS | `10.9.8` |
| `rustc --version` | `0` | PASS | `rustc 1.97.1 (8bab26f4f 2026-07-14) (Homebrew)` |
| `cargo --version` | `0` | PASS | `cargo 1.97.1 (c980f4866 2026-06-30) (Homebrew)` |
| `rustup show active-toolchain` | `127` | INCONCLUSIVE | `zsh:1: command not found: rustup` |
| `npm ci` | `0` | PASS | `added 72 packages, and audited 73 packages in 1s`; `found 0 vulnerabilities` |
| `npm run typecheck` | `2` | PRIOR HISTORICAL RUN: FAILED | TypeScript compilation failed. This is not the authoritative R06 result. |
| `npm run build` | `2` | PRIOR HISTORICAL RUN: FAILED | The command's `tsc --noEmit` phase failed; this is not the authoritative R06 result. |
| `cargo test --manifest-path src-tauri/Cargo.toml` | `101` | FAILED | Dependencies compiled/downloaded, then the `luno-desktop` build script failed: `resource path \`resources/bin\` doesn't exist`. |

## Installation Notes

`npm ci` installed the locked JavaScript dependencies successfully. Rust was installed with `brew install rust` because `rustc`, `cargo`, and `rustup` were initially unavailable. The Rust test gate reached project compilation; its failure is a missing repository resource, not an unavailable Rust toolchain. No application implementation was modified.

The exit-2 TypeScript rows above are prior historical runs. They must not override R06's recorded npm exit `127` results, which are INCONCLUSIVE because `tsc` was unavailable.

## R06 Authoritative npm Receipts

| Command | Exit | Status | Receipt |
|---|---:|---|---|
| `npm run typecheck` | `127` | INCONCLUSIVE | `docs/verification/receipts/r06/typescript/typecheck.txt` (`sh: tsc: command not found`) |
| `npm run build` | `127` | INCONCLUSIVE | `docs/verification/receipts/r06/typescript/build.txt` (`sh: tsc: command not found`) |

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

## Exact Receipts

| Command | Exit | Status | Output |
|---|---:|---|---|
| `node --version` | `0` | PASS | `v22.23.2` |
| `npm --version` | `0` | PASS | `10.9.8` |
| `rustc --version` | `0` | PASS | `rustc 1.97.1 (8bab26f4f 2026-07-14) (Homebrew)` |
| `cargo --version` | `0` | PASS | `cargo 1.97.1 (c980f4866 2026-06-30) (Homebrew)` |
| `rustup show active-toolchain` | `127` | INCONCLUSIVE | `zsh:1: command not found: rustup` |
| `npm ci` | `0` | PASS | `added 72 packages, and audited 73 packages in 1s`; `found 0 vulnerabilities` |
| `npm run typecheck` | `2` | INCONCLUSIVE | TypeScript compilation failed. Diagnostics include `src/app/bridge.ts` (TS2345, TS7006, TS2367), `src/shared/contracts.ts` (TS18046), `src/state/backend_projection.ts` (TS2366, TS2339), and missing `vitest` types in `src/test/*`. |
| `npm run build` | `2` | INCONCLUSIVE | The command's `tsc --noEmit` phase failed with the same TypeScript diagnostics; Vite did not run. |
| `cargo test --manifest-path src-tauri/Cargo.toml` | `101` | INCONCLUSIVE | Dependencies compiled/downloaded, then the `luno-desktop` build script failed: `resource path \`resources/bin\` doesn't exist`. |

## Installation Notes

`npm ci` installed the locked JavaScript dependencies successfully. Rust was installed with `brew install rust` because `rustc`, `cargo`, and `rustup` were initially unavailable. The Rust test gate reached project compilation; its failure is a missing repository resource, not an unavailable Rust toolchain. No application implementation was modified.

The final-parity receipt remains unchanged: the TypeScript and Rust gates are still `INCONCLUSIVE`, and no parity document update is warranted.

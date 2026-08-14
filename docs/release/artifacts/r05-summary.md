# R05 Packaging Receipt

Status: **BLOCKED**.

Executed in order on 2026-08-14 from reviewed commit `bd3a726edf4610bc0616a372bc449ee4dc387cab` on Darwin 25.5.0 (macOS, arm64). The workflow bundle path is `src-tauri/target/release/bundle/macos/Luno.app`.

Toolchain receipt: Node `v22.23.2`, npm `10.9.8`, Rust `rustc 1.97.1 (8bab26f4f 2026-07-14) (Homebrew)`, Cargo `1.97.1 (c980f4866 2026-06-30) (Homebrew)`. The TypeScript compiler was present and reported errors (exit `2`); its version was not separately recorded. The Tauri CLI was invoked successfully through `npm run tauri`; its DMG build reached the configured frontend prerequisite, where `npm run build` failed before Vite ran.

- `npm ci`: exit `0`, PASS; 72 packages installed, 0 vulnerabilities.
- `npm run typecheck`: exit `2`, FAILED; TypeScript and missing `vitest` diagnostics.
- `npm run build`: exit `2`, FAILED; typecheck phase stopped Vite.
- `cargo test --manifest-path src-tauri/Cargo.toml`: exit `101`, FAILED; `resources/bin` is absent.
- Provisioning: exit `1`, BLOCKED; `release-input/bin/ffmpeg` is absent.
- Tauri DMG build: exit `1`, BLOCKED; the Tauri CLI was invoked, but its required `npm run build` frontend prerequisite failed during typecheck before Vite ran. This is distinct from an unavailable Tauri command.
- Bundle verifier: exit `1`, BLOCKED; no `src-tauri/target/release/bundle/macos/Luno.app` bundle manifest exists.

No FFmpeg, ffprobe, yt-dlp binaries, SHA-256 values, license artifacts,
signing credentials, notarization credentials, DMG, or app bundle were created.
The release workflow now fails closed when reviewed inputs or Apple secrets are
missing. Provisioning and bundle verification inspect each binary's actual
architecture with macOS `file`, compare it with the manifest architecture
matrix, and then compare reported FFmpeg, ffprobe, and yt-dlp versions with the
manifest. Missing binaries or inspection tools are blocked/inconclusive and
never pass. No binaries, checksums, signatures, DMG, or app bundle were
fabricated.
Exact machine and command details are in `r05-manifest.json`.

Provenance receipt: `git rev-parse HEAD`: exit `0`, output `bd3a726edf4610bc0616a372bc449ee4dc387cab`. `git status --porcelain`: exit `0`, output ` M TODO.md` and ` M dispatch-plan.json`; these pre-existing worktree changes were outside the reviewed R05 artifacts. The manifest records each reviewed command and exit code against this commit.

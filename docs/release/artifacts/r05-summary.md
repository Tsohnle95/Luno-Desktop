# R05 Packaging Receipt

Status: **BLOCKED**.

Executed in order on 2026-08-14 from reviewed commit `7011d60777cb0d1b311d6d6f5acbc2d2a814a575` on Darwin 25.5.0 (macOS, arm64). The workflow bundle path is `src-tauri/target/release/bundle/macos/Luno.app`.

Toolchain receipt: Node `v22.23.2`, npm `10.9.8`, Rust `rustc 1.97.1 (8bab26f4f 2026-07-14) (Homebrew)`, Cargo `1.97.1 (c980f4866 2026-06-30) (Homebrew)`. TypeScript, Vite, and Tauri CLI versions were unavailable because their local executables were unavailable in the current workspace.

- `npm ci`: exit `0`, PASS; 72 packages installed, 0 vulnerabilities.
- `npm run typecheck`: exit `2`, FAILED; TypeScript and missing `vitest` diagnostics.
- `npm run build`: exit `2`, FAILED; typecheck phase stopped Vite.
- `cargo test --manifest-path src-tauri/Cargo.toml`: exit `101`, FAILED; `resources/bin` is absent.
- Provisioning: exit `1`, BLOCKED; `release-input/bin/ffmpeg` is absent.
- Tauri DMG build: exit `1`, BLOCKED; its `npm run build` prerequisite failed.
- Bundle verifier: exit `1`, BLOCKED; no `src-tauri/target/release/bundle/macos/Luno.app` bundle manifest exists.

No FFmpeg, ffprobe, yt-dlp binaries, SHA-256 values, license artifacts,
signing credentials, notarization credentials, DMG, or app bundle were created.
The release workflow now fails closed when reviewed inputs or Apple secrets are
missing, and provisioning/bundle verification compare reported FFmpeg, ffprobe,
and yt-dlp versions with `release/desktop-binaries.json` when those binaries
exist, with version-command failures and mismatches blocking the gate. No
binaries, checksums, signatures, DMG, or app bundle were fabricated.
Exact machine and command details are in `r05-manifest.json`.

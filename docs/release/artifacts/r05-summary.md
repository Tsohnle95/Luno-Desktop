# R05 Packaging Receipt

Status: **BLOCKED**.

Executed in order on 2026-08-14 from commit `6467d166231e3446b9edb44a9053aa88bc3d30a`:

- `npm ci`: exit `0`, PASS; 72 packages installed, 0 vulnerabilities.
- `npm run typecheck`: exit `2`, INCONCLUSIVE; existing TypeScript and missing `vitest` diagnostics.
- `npm run build`: exit `2`, INCONCLUSIVE; typecheck phase stopped Vite.
- `cargo test --manifest-path src-tauri/Cargo.toml`: exit `101`, INCONCLUSIVE; `resources/bin` is absent.
- Provisioning: exit `1`, BLOCKED; `release-input/bin/ffmpeg` is absent.
- Tauri DMG build: exit `1`, BLOCKED; its `npm run build` prerequisite failed.
- Bundle verifier: exit `1`, BLOCKED; no `dist/Luno.app` bundle manifest exists.

No FFmpeg, ffprobe, yt-dlp binaries, SHA-256 values, license artifacts,
signing credentials, notarization credentials, DMG, or app bundle were created.
The release workflow now fails closed when reviewed inputs or Apple secrets are
missing. Exact machine and command details are in `r05-manifest.json`.

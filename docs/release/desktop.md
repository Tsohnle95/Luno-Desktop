# macOS Desktop Release

Document role: canonical owner for release packaging — pinned managed
binaries, provisioning, bundle verification, signing/notarization hooks,
and rollback. Verification and CI procedure lives in
`docs/verification/desktop.md`.

Luno releases are macOS-only and retain the stable identifier
`com.ty.luno`. The Tauri bundle targets a DMG and requires macOS 13 or newer.

## Managed binaries

FFmpeg/ffprobe `7.1.1` and yt-dlp `2026.08.19` are pinned in
`release/desktop-binaries.json`. Release input must contain executable files in
`release-input/bin/`; run `LUNO_BINARY_SOURCE_DIR=release-input/bin
./scripts/provision-desktop-binaries.sh`. The script refuses placeholder or
mismatched SHA-256 values and checks each input's architecture with macOS `file`
against the manifest architecture matrix. Never download an unreviewed binary
during a build.

Each update requires a reviewed release commit containing the new version,
architecture-specific checksums, upstream license notices, and explicit human
licensing/signing approval. This is an update review, not runtime self-update.
Reviewed inputs were supplied under receipt R06: ffmpeg/ffprobe 7.1.1 built
locally from the PGP- and git-tag-verified ffmpeg.org source (LGPL-only, arm64)
and yt-dlp 2025.08.11 from the official GitHub release (checksum- and
signature-verified, universal2). On 2026-08-22 the yt-dlp pin was refreshed to
2026.08.19 (official GitHub release `yt-dlp_macos`, universal2) after the stale
2025.08.11 build was refused for nearly all current YouTube content ("The
following content is not available on this app"); the replacement's SHA-256
matches that release's published `SHA2-256SUMS`. The manifest carries the
reviewed SHA-256 values and `THIRD_PARTY_NOTICES.md` carries the exact upstream
license texts.
The inputs themselves are intentionally not committed; the release owner
re-supplies them per release, and provisioning and bundle verification fail
closed if they are absent or mismatched.

## App icon

The bundle icon set is generated from `src-tauri/icons/icon.png` with
`npx tauri icon src-tauri/icons/icon.png` and listed under `bundle.icon` in
`src-tauri/tauri.conf.json` (32/128/128@2x PNGs plus `icon.icns`/`icon.ico`).
Tauri's default is an empty icon list, which silently bundles its own logo;
`bundle.icon` must stay populated or the app ships with the wrong icon. macOS
reads `CFBundleIconFile` (`icon.icns`), which notarization also requires at
512pt@2x. Regenerate and commit the set whenever the source art changes.

## Gates

Run `./scripts/verify-desktop-bundle.sh '<Luno Desktop.app>'` after building. It checks
the bundle manifest, notices, executable bits, checksums, tool versions,
architecture, the presence of the bundle icon, and a clean-machine smoke with
developer `PATH` unavailable.
Missing or tampered tools, or missing inspection tools, are blocked/inconclusive
and never pass; application services must surface degraded media behavior.

Signing and notarization are CI hooks. The release workflow fails without
`APPLE_SIGNING_IDENTITY`; certificate, Apple ID, team, and notarization secrets
are supplied by the release owner. Signing/notarization cannot be verified on
non-macOS machines and is an inconclusive receipt until macOS CI completes.
The workflow also requires `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID`;
none of these credentials are stored in the repository.

## Rollback

Keep the last signed DMG and the known-good Python/PyInstaller release. If a
Tauri release fails migration or packaging gates, distribute the Python build,
preserve user data and legacy configuration, and do not overwrite the Tauri
bundle. Resume Tauri rollout only after a reviewed correction passes the clean
machine, signature, checksum, and user-data rollback gates.

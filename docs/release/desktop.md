# macOS Desktop Release

Luno releases are macOS-only and retain the stable identifier
`com.ty.luno`. The Tauri bundle targets a DMG and requires macOS 13 or newer.

## Managed binaries

FFmpeg/ffprobe `7.1.1` and yt-dlp `2025.08.11` are pinned in
`release/desktop-binaries.json`. Release input must contain executable files in
`release-input/bin/`; run `LUNO_BINARY_SOURCE_DIR=release-input/bin
./scripts/provision-desktop-binaries.sh`. The script refuses placeholder or
mismatched SHA-256 values. Never download an unreviewed binary during a build.

Each update requires a reviewed release commit containing the new version,
architecture-specific checksums, upstream license notices, and explicit human
licensing/signing approval. This is an update review, not runtime self-update.

## Gates

Run `./scripts/verify-desktop-bundle.sh <Luno.app>` after building. It checks
the bundle manifest, notices, executable bits, checksums, tool versions, and a
clean-machine smoke with developer `PATH` unavailable. Missing or tampered
tools fail closed; application services must surface degraded media behavior.

Signing and notarization are CI hooks. The release workflow fails without
`APPLE_SIGNING_IDENTITY`; certificate, Apple ID, team, and notarization secrets
are supplied by the release owner. Signing/notarization cannot be verified on
non-macOS machines and is an inconclusive receipt until macOS CI completes.

## Rollback

Keep the last signed DMG and the known-good Python/PyInstaller release. If a
Tauri release fails migration or packaging gates, distribute the Python build,
preserve user data and legacy configuration, and do not overwrite the Tauri
bundle. Resume Tauri rollout only after a reviewed correction passes the clean
machine, signature, checksum, and user-data rollback gates.

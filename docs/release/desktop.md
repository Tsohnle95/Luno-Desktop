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

The master artwork is `src-tauri/icons/app-icon.png` (1024×1024). Generate the
bundle set with `npx tauri icon src-tauri/icons/app-icon.png`. It writes the
32/128/128@2x PNGs, `icon.icns`, `icon.ico`, and a 512×512 `icon.png`
derivative; `bundle.icon` in `src-tauri/tauri.conf.json` lists the subset the
bundle uses. The generator also emits extra platform artifacts (64×64, the
Windows `Square*`/`StoreLogo` PNGs, and `android/`+`ios/` trees) into
`src-tauri/icons/`; this macOS-only repo commits only the entries above, so
prune the extras before committing. `icon.png` is a generated derivative, not the
master — always regenerate from `app-icon.png`, never from `icon.png`, or the
iconset silently loses resolution. Tauri's default is an empty icon list, which
bundles no icon at all (macOS shows its generic app icon); `bundle.icon` must
stay populated or `CFBundleIconFile` disappears from the bundle. macOS reads
`CFBundleIconFile` (`icon.icns`). Regenerate and commit the set whenever the
master art changes.

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

## Unsigned preview releases

The `v0.1.0` macOS Preview is an explicitly approved unsigned prerelease for
Apple Silicon (arm64), requiring macOS 13 or newer. It is not Developer ID signed
or notarized; an ad-hoc linker signature is not a trusted developer signature.
The signed CI workflow and stable-release requirements above remain in force.
Do not present this preview as a signed stable release or an Intel/universal app.

Build with Node 22 and Rust stable using
`npm run tauri -- build --bundles app,dmg`. Verify the application inside the
mounted DMG with `scripts/verify-desktop-bundle.sh`, check its application
identifier and version, and compare its executable with the verified build.
Publish the DMG as `Luno-Desktop-v0.1.0-macOS-arm64.dmg` with a `SHA256SUMS.txt`
asset on a GitHub prerelease whose tag points to the reviewed source commit.

The release page uses `docs/assets/releases/luno-desktop-banner.svg` and
`download-macos.svg`, with its durable body in `release/notes/v0.1.0.md`.
It must disclose architecture, minimum macOS, signature/notarization status,
installation steps, and the DMG checksum. Updates replace the application bundle;
they must never instruct users to delete their music or application data.

## Rollback

Keep the last signed DMG and the known-good Python/PyInstaller release. If a
Tauri release fails migration or packaging gates, distribute the Python build,
preserve user data and legacy configuration, and do not overwrite the Tauri
bundle. Resume Tauri rollout only after a reviewed correction passes the clean
machine, signature, checksum, and user-data rollback gates.

# R06 Media Binary Supply and Bundle Verification Receipt

Status: **PASS** for supply, provisioning, DMG bundle, and bundle verification;
signing/notarization remain BLOCKED (no Apple credentials).

Executed on 2026-08-16 from source review head `0fc27703af21839c326148734f6aeed8019c60a3`
on Darwin 25.5.0 (macOS, arm64). Scope: replace the placeholder release-binary
inputs with reviewed binaries, pin their SHA-256 values, attach upstream
license texts, provision, build and inspect a DMG, and verify the mounted
bundle, including the no-developer-PATH smoke.

## Supplied and reviewed artifacts

Pinned versions are unchanged: ffmpeg/ffprobe `7.1.1`, yt-dlp `2025.08.11`.

| name | SHA-256 | architecture | source |
| --- | --- | --- | --- |
| ffmpeg | d69335856dd48485f0343693734048fb7d89861720bf7b8255f947f11737274f | arm64 | ffmpeg.org release 7.1.1 tarball, locally built LGPL-only |
| ffprobe | a2c0abaa9744aec49f339d1f7a1afc981a9cc5d6fe48329ddd04dd400b6ac375 | arm64 | ffmpeg.org release 7.1.1 tarball, locally built LGPL-only |
| yt-dlp | 8001c730007495a26de24d7fcf6c72e76bc24c47bdde2cfd86d51e21626bd7e4 | universal2 | official GitHub release 2025.08.11 `yt-dlp_macos` |

### FFmpeg/ffprobe provenance (double-signed, locally built)

- Source: `https://ffmpeg.org/releases/ffmpeg-7.1.1.tar.xz` over TLS.
  Tarball SHA-256 `733984395e0dbbe5c046abda2dc49a5544e7e0e1e2366bba849222ae9e3a03b1`.
- PGP: `gpg --verify ffmpeg-7.1.1.tar.xz.asc ffmpeg-7.1.1.tar.xz` → `Good
  signature from "FFmpeg release signing key <ffmpeg-devel@ffmpeg.org>"` with
  `FCF986EA15E6E293A5644F10B4322F04D67658D8` (key fetched over TLS from
  `https://ffmpeg.org/ffmpeg-devel.asc`; the same fingerprint is published on
  `https://ffmpeg.org/download.html`).
- Git tag: shallow clone of tag `n7.1.1`; `git tag -v n7.1.1` → `Good
  signature` with `DD1EC9E8DE085C629B3E1846B18E8928B3948D64` (tagger
  "FFmpeg 7.1.1 release", Michael Niedermayer).
- Build: `./configure --disable-gpl --disable-doc --disable-debug
  --disable-ffplay --disable-sdl2 --disable-libxcb --disable-xlib` with Apple
  clang 21.0.0; configure reported `License: LGPL version 2.1 or later`.
  `otool -L` shows no non-system dylibs; `file` reports `Mach-O 64-bit
  executable arm64`. `ffmpeg -version` → `ffmpeg version 7.1.1`.

### yt-dlp provenance (checksum + signature)

- Source: `https://github.com/yt-dlp/yt-dlp/releases/download/2025.08.11/yt-dlp_macos`
  over TLS; `file` reports universal2 (x86_64 + arm64); `--version` →
  `2025.08.11`.
- Checksum: SHA-256 matches the value published in the same release's
  `SHA2-256SUMS` exactly.
- PGP: `gpg --verify SHA2-256SUMS.sig SHA2-256SUMS` → `Good signature from
  "Simon Sawicki (yt-dlp signing key) <contact@grub4k.xyz>"` with
  `AC0CBBE6848D6A873464AF4E57CF65933B5A7581`. The key was fetched from
  keys.openpgp.org and its fingerprint matches the official key distributed in
  the yt-dlp repository (`https://github.com/yt-dlp/yt-dlp/blob/master/public.key`),
  which is the verification path documented in the yt-dlp README.

## Checks executed in order

- `release/desktop-binaries.json`: all three `REPLACE_WITH_REVIEWED_SHA256`
  placeholders replaced with the reviewed values above; `source` fields
  updated to reflect actual provenance. PASS.
- `release/THIRD_PARTY_NOTICES.md`: rewritten with exact upstream license
  texts (LGPL-2.1 text from the verified tarball's `COPYING.LGPLv2.1`;
  Unlicense text from the official 2025.08.11 `LICENSE`), per-artifact SHA-256,
  versions, architectures, source URLs, and the exact build configuration.
  PASS.
- `LUNO_BINARY_SOURCE_DIR=release-input/bin ./scripts/provision-desktop-binaries.sh`:
  exit `0`; validated architecture matrix, reported versions, and SHA-256
  values against the manifest; installed executable copies to
  `src-tauri/resources/bin/`.
- DMG build in isolated environment: fresh `HOME` and `TMPDIR` under
  `/private/tmp/luno-build-*`, `CARGO_HOME=/Users/ty/.cargo`,
  `CARGO_NET_OFFLINE=true`, `PATH=/opt/homebrew/bin:/usr/bin:/bin`;
  `npm run tauri -- build` → exit `0`; artifact
  `src-tauri/target/release/bundle/dmg/Luno_0.1.0_aarch64.dmg` (108,050,744 B;
  SHA-256 `441c1d815854b234391ced66c26b8ee7f5bce9ee90f0fbbfac2b3014fa4992eb`).
- `hdiutil attach -nobrowse -readonly ...` mounted at `/Volumes/Luno`; then
  `./scripts/verify-desktop-bundle.sh /Volumes/Luno/Luno.app` → exit `0`,
  including bundle manifest, notices, executable bits, all three checksums,
  versions, architectures, and the no-developer-PATH smoke
  (`PATH=/usr/bin:/bin`).
- In-bundle inspection: `Contents/Resources/bin/{ffmpeg,ffprobe,yt-dlp}`
  present with executable bits; recorded SHA-256 values equal the manifest;
  `Contents/Resources/desktop-binaries.json` (1,065 B) and
  `Contents/Resources/THIRD_PARTY_NOTICES.md` (30,120 B) shipped as configured
  in `tauri.conf.json` resources.
- Launch smoke: packaged `luno-desktop` from the mounted DMG ran for 10 s with
  clean `PATH=/usr/bin:/bin` and isolated `HOME`/`TMPDIR`, then was
  terminated; no crash. `codesign -dv` reports ad-hoc signing only.
- Cleanup: DMG detached; provisioned copies removed from
  `src-tauri/resources/bin/` (only `.gitkeep` remains, as committed); a stale
  `dist/Luno.app` process from earlier packaging work was terminated.

## Still blocked (unchanged)

- Signing and notarization: no `APPLE_SIGNING_IDENTITY`/Apple credentials exist
  in the repository; the release workflow fails closed without them (R05
  posture). The bundle in this receipt is ad-hoc signed only.
- Clean-machine installation on another physical machine was not performed.

## Architecture note

ffmpeg/ffprobe are arm64-only (host-native builds), so on an Intel Mac the
bundle verifier's architecture check for those binaries would fail; a future
release needing Intel coverage must supply universal2 builds (yt-dlp already
is universal2). This is recorded as a release decision, not a defect of the
verifier.

Exact machine and command details are in `r06-manifest.json`.
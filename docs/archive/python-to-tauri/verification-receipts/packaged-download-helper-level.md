# Packaged Download Receipt: Real Network Download Through The Bundled Helper

Receipt date: `2026-08-16`. Verification performed from the repository root;
no release commands, no commit, no signing, no GUI/accessibility automation.

## Scope

This receipt proves the **packaged download pipeline at the helper level**:
the `luno-download-helper` script bundled inside the packaged app, executing
the bundled `yt-dlp`/`ffmpeg` binaries, performs a **real network download**
(a live YouTube search query) and lands a playable M4A at the production
destination (`Unsorted` under the library root), under a clean environment
(`PATH=/usr/bin:/bin`, isolated `HOME`/`TMPDIR`).

It does **not** claim the packaged-UI journey (search rows rendered in the
window, job terminal state, post-scan library pickup in the packaged app):
that remains unobserved (see sign-off prerequisite 3).

## Provenance

- Packaged app: `/private/tmp/luno-bundle-app/Luno.app` (release build from
  the 2026-08-16 source at `0fef32a6` + downstream fixups; binaries
  unchanged by the two later reconciliation commits).
- Bundled binaries hash-matched to the reviewed manifest
  `release/desktop-binaries.json` / `release-input/bin` staging:

  | Bundled binary | SHA-256 | vs manifest |
  |---|---:|---|
  | `yt-dlp` | `8001c730007495a26de24d7fcf6c72e76bc24c47bdde2cfd86d51e21626bd7e4` | MATCH |
  | `ffmpeg` | `d69335856dd48485f0343693734048fb7d89861720bf7b8255f947f11737274f` | MATCH |
  | `ffprobe` | `a2c0abaa9744aec49f339d1f7a1afc981a9cc5d6fe48329ddd04dd400b6ac375` | MATCH |

  (`luno-download-helper` is a tracked script shipped via Tauri resources, not
  a managed binary; it is correctly absent from the binary manifest.)

## Method (exact production invocation)

Fixture root `/private/tmp/luno-dl-proof` with `home/Music/Unsorted` created.
Execution environment: `PATH=/usr/bin:/bin` (no development PATH),
`HOME=$fixture/home`, `TMPDIR=$fixture/tmp`.

```
env PATH=/usr/bin:/bin HOME=$fixture/home TMPDIR=$fixture/tmp \
  /private/tmp/luno-bundle-app/Luno.app/Contents/Resources/bin/luno-download-helper \
  "$fixture/home/Music/Unsorted/dl-1786935244.m4a" \
  "ytsearch1:never gonna give you up"
```

The helper prepends its own directory to `PATH` (production wiring), so the
bundled `yt-dlp`/`ffmpeg` resolve without any development tools.

## Result

- Helper exit: `0`.
- Output: `dl-1786935244.m4a`, 3,446,220 bytes, SHA-256
  `079b09771e356f6a397e41674a1a9b489868de43838987566228446fd61afaa2`,
  promoted from `*.part` to the final target by the helper's `mv` (no
  `.part` leftovers).
- `file`: `ISO Media, Apple iTunes ALAC/AAC-LC (.M4A) Audio`.
- Bundled `ffprobe`:
  - Stream: `aac`, audio, 44100 Hz, 2 channels.
  - Format: `mov,mp4,m4a,3gp,3g2,mj2`, duration `213.043084` s (full track),
    size 3,446,220.
- The download source succeeded through YouTube's current client behavior;
  yt-dlp emitted non-fatal warnings (signature extraction fallback,
  SABR-only format skips) and still produced the complete file.

## Deterministic Gates

Not re-run: this receipt is documentation-only, and the deterministic gates
were re-run at HEAD `3430bf3a` (typecheck 0, 45 frontend tests, 98 Rust
tests, `git diff --check` clean) before this evidence was captured.

## Boundaries

No claim of release approval; the download happened in an isolated fixture
home on this machine, not on a clean machine. Signing/notarization,
clean-machine install, physical audio measurement, real provider journeys,
and the target video backend remain external (sign-off prerequisites 1-2,
4-6).
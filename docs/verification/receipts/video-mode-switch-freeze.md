# Verification Receipt — video-mode song-switch freeze (2026-08-25)

## Scope

Fixes for the 2026-08-25 report: switching songs with video mode ON froze the
window for ~20+ seconds. Root causes diagnosed and fixed in one change set:

1. **Double kick per switch** — `play_with_context` AND
   `start_selected_track` both called `kick_video_pipeline`, spawning two
   prefetch passes per switch (second superseding the first mid-search).
2. **Unbounded background footprint** — prefetch depth was FIVE queue tracks
   at a ≤1080p ladder (observed: 350 MB downloaded in 3 minutes; single files
   of 118 MB / 142 MB), and the quality-upgrade transcode ran concurrently
   with prefetch downloads.
3. **Whole-file blob delivery** — the stock asset protocol buffers the entire
   staged MP4 when the first media request has no `Range` header, and wry
   delivers it as ONE NSData blob; the revision-2 hot-swap forced a second
   full-size load mid-song.

## Changes

- `src-tauri/src/state.rs`: kick only from `start_selected_track` (one pass
  per actual track start); prefetch plan `.take(5)` → `.take(2)`; new
  `video_heavy_work` mutex serializing upgrade transcode ↔ prefetch downloads
  via polling `try_lock` with cancellation checkpoints; `video_root_dir()`
  accessor.
- `src-tauri/src/video/runtime.rs`: `FORMAT_LADDER_FULL` (≤1080p) replaced by
  `FORMAT_LADDER_HIGH` (≤720p) used by BOTH `prefetch_track` (via
  `download_video`) and `upgrade_staged_video`; quick tier stays ≤480p;
  docs updated.
- `src-tauri/src/video/stream.rs` (NEW): range-aware streaming protocol —
  percent-decode + store-containment resolution, `plan_response` decision
  matrix (HEAD / no-range → 206 prefix window / bounded / suffix /
  multi-range first-only / 416), and file-backed `response_for`. Every media
  response is a ≤2 MB window with `Accept-Ranges: bytes`.
- `src-tauri/src/main.rs`: registers asynchronous `luno-video` scheme;
  handler runs `response_for` on the async runtime (never the UI thread).
- `src-tauri/tauri.conf.json`: CSP `media-src` gains `luno-video:`.
- `src/app/Shell.tsx`: `safeVideoSource` builds staged srcs through
  `convertFileSrc(path, "luno-video")`.
- `src/test/bridge.test.ts`: pins the custom-scheme source building +
  traversal fallback (`core` mock now provides Tauri's macOS
  `convertFileSrc` implementation).

## Evidence

| Check | Command | Result |
| --- | --- | --- |
| Rust unit/integration (incl. 8 new stream tests, updated ladder test, all gate suites) | `cargo test --manifest-path src-tauri/Cargo.toml` | **134 passed; 0 failed** (3 pre-existing network-bound ignores) |
| TypeScript | `npm run typecheck` | clean |
| Frontend suite | `npx vitest run` | **105 passed; 0 failed** |
| Production bundle | `npm run build` | ✓ 63 modules transformed |
| Python desktop suite | `.venv/bin/python -m pytest -q` | **109 passed** |
| Launch smoke | `LUNO_INSTANCE_PORT=48714 ./src-tauri/target/debug/luno-desktop` | running after 8 s, `[luno-video] runtime ready`, no panic |

The named `desktop-playback` harness command remains unavailable in this
environment (as recorded in `gate3-playback-queue.md`); its configured
equivalent — the gate3 playback/queue/clock/recovery tests inside
`cargo test` — is included in the 134-pass run above. New warnings: none
(all reported warnings pre-exist in untouched code).

## Behavior notes

- Prefetch now stages the next TWO tracks at ≤720p; upgrades land at ≤720p
  (the historical pre-two-tier ceiling). Expected background footprint per
  switch drops from hundreds of MB to tens.
- `<video>` seeks/hot-swaps issue cheap ranged requests instead of
  whole-file blob loads; containment keeps the protocol locked to the video
  store (traversal/symlink/extension checks tested).

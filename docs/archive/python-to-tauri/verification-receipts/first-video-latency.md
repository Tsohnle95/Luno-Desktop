# Verification Receipt — first-video latency audit (2026-08-26)

## Symptom

"It doesn't seem like the app is downloading smaller sized videos first. It
takes a very long time to download the first low quality video."

## Evidence (user's own video store)

- `current-_gp51lt9kdA.mp4` = **1.13 GB**, `current-q01rExZcaS4.mp4` =
  **442 MB**, `current-Z0bSI_8o4BU.mp4` = **186 MB** — current-slot files at
  hundreds of MB to >1 GB.
- Full-size DUPLICATES across slots: `current-cFRcabr9Etg.mp4` (88 MB,
  03:24) AND `next-cFRcabr9Etg.mp4` (byte-identical 88,272,186 bytes, 03:32).

## Root causes & fixes

1. **Quick tier not bounded for long films** — a ≤480p pull of a multi-hour
   "music video" (full album film) is still hundreds of MB, so the FIRST
   frame took minutes. Fix: `FORMAT_LADDER_QUICK` lowered **480p → 360p**
   (typically single-digit-to-low-tens of MB, seconds-to-tens-of-seconds)
   and `--concurrent-fragments` 5 → 8 for faster segment pulls. HD upgrade
   target stays ≤720p.
2. **Prefetch re-downloaded videos that already existed as
   `current-<id>.mp4`** when the cache index lacked their key — duplicating
   full files and stealing bandwidth from the playing track's own traffic.
   Fix: `prefetch_track` treats an existing CURRENT-slot copy of the same id
   as a cache hit, recording the key at that file's existing index tier
   (`tier_for_video_id`) so pending-upgrade state is preserved.

## Tests / battery

- Ladder assertions updated (`height<=360`, never `height<=480`).
- `cargo test`: **148 passed; 0 failed** (three consecutive full-suite runs
  after hardening `previous_walks_back…` with task-quiescence waits — its
  intermittent parallel-load flake was reproduced, root-caused to asserting
  while async loads were in flight, and eliminated).
- Frontend typecheck clean + **107 passed**; Python suite **109 passed**.
- `npm run build` + `npx tauri build --bundles app` ✓; `dist/Luno.app`
  refreshed from the new bundle.

The named `desktop-playback` harness command remains unavailable in this
environment; its configured equivalent (gate3 playback tests) ran inside the
148-pass `cargo test`.

# Verification Receipt — video loader audit (2026-08-25)

## Scope

Audit of the video loader: (a) does it load the current + next queue songs,
(b) does it serve a low-quality version fast and replace it with an HD
version once available.

## Findings

1. **Queue coverage — was 2, now 3.** The pipeline plan covered the current
   track + TWO upcoming songs (an intentional reduction from five after the
   morning freeze diagnosis). Per product direction the depth is now THREE,
   extracted into the pure, unit-tested `prefetch_plan` +
   `VIDEO_PREFETCH_DEPTH = 3` (current-first ordering preserved; no-current
   case takes the first three).
2. **Two-tier serving — YES, with one real hole (fixed).** The current
   track's video downloads through the QUICK ≤480p ladder first and plays;
   a background `luno-video-upgrade` pass then replaces it IN PLACE with the
   ≤720p HIGH ladder (atomic rename; webview hot-swaps via revision 2).
   Prefetched upcoming tracks download at HIGH directly.
   **Hole:** the cache index marked EVERY staged file as done. If the HD
   upgrade was interrupted (app restart, or superseded by skipping tracks),
   the 480p file was re-adopted forever as final quality.
   **Fix:** tier-aware index entries (`{"id","tier":"quick"}` objects;
   legacy bare strings read as high). Quick passes record `quick`; a
   successful HD upgrade flips all keys for that video back to `high`
   (`set_tier_for_video_id`); adoption paths set `needs_upgrade` from the
   tier so interrupted upgrades re-run on the next play.
3. **HD ceiling note:** "HD" here means ≤720p by design (the former ≤1080p
   ceiling produced single files of 118–197 MB and was a major factor in the
   song-switch freeze). With range-streaming + heavy-work serialization in
   place the ceiling can be revisited if 1080p is explicitly wanted; it is a
   one-line ladder change plus brain/doc updates.

## Tests (red-green where applicable)

| Test | Neutered | With fix |
| --- | --- | --- |
| `index_hit_with_quick_tier_reports_needs_upgrade` | FAIL ("must schedule the HD upgrade") | PASS |
| `prefetch_plan_covers_current_plus_depth_successors` | new pure-fn coverage | PASS |
| `index_roundtrips_tiers_and_reads_legacy_strings_as_high` / `set_tier_for_video_id_flips_every_matching_key` | new coverage | PASS |

## Full battery

| Check | Result |
| --- | --- |
| `cargo test` | **142 passed; 0 failed** |
| `npm run typecheck` + `npx vitest run` | clean, **107 passed** |
| `.venv/bin/python -m pytest -q` | **109 passed** |
| `npm run build` + `npx tauri build --bundles app` | ✓ |
| `dist/Luno.app` refreshed from the new bundle | ✓ |

The named `desktop-playback` harness command remains unavailable in this
environment; its configured equivalent (gate3 playback tests) ran inside the
142-pass `cargo test`.

# Verification Receipt — search-click stale-context healing (2026-08-25)

## Symptom

Recurring report (multiple prior fixes in the same family: duplicate-id
collapse, atomic map swap, `heal_track_id`, virtual-window clamp, filter
normalization): searching in the top input, clicking a result row, and the
song "disappears" / the list ends on "No matches".

## Root cause analysis

- The user's running app (`dist/Luno.app`, built 2026-08-24 23:08) predates
  HEAD commit `4881dbb1` (2026-08-25 14:30), which already shipped
  `heal_track_id` + the atomic id→path swap for exactly this family.
- Residual hole at HEAD: `play_with_context` healed ONLY the clicked track
  id. The rest of the clicked `context` was stored RAW, so after a rescan
  that re-keyed ids (`#<path-hash>` flips when duplicates appear/disappear)
  the stored queue kept dangling entries — Now Playing resolved nothing,
  auto-advance skipped, repeat clicks kept failing.
- The zero-match empty state was a dead end (no recovery affordance).

## Changes

- `src-tauri/src/state.rs` `play_with_context`: resolve + heal the ENTIRE
  request under ONE lock — clicked track AND every context entry; duplicates
  collapse; effective (healed) track id flows into queue/ack/load task;
  containment validated post-heal.
- `src/views/LibraryView.tsx`: zero-match state now offers a one-click
  **Clear search** action.
- Tests:
  - `play_with_context_heals_a_stale_context_across_a_suffix_flip`
    (red-green verified: fails with context-healing neutered, passes with it).
  - `zero-match state` vitest block (Clear-search affordance + matching rows
    stay rendered).

## Evidence

| Check | Command | Result |
| --- | --- | --- |
| Rust suite (incl. new regression test) | `cargo test --manifest-path src-tauri/Cargo.toml` | **135 passed; 0 failed** |
| TypeScript | `npm run typecheck` | clean |
| Frontend suite | `npx vitest run` | **107 passed; 0 failed** |
| Python desktop suite | `.venv/bin/python -m pytest -q` | **109 passed** |
| Production bundle | `npm run build` | ✓ |
| Tauri release bundle | `npx tauri build --bundles app` | ✓ `target/release/bundle/macos/Luno.app` |
| Release smoke | `LUNO_INSTANCE_PORT=48715 target/release/luno-desktop` | running after 8 s, no panic |
| User-facing copy refreshed | `ditto …/bundle/macos/Luno.app dist/Luno.app` | ✓ (old build moved to `dist/Luno.app.old-20260825`) |

The named `desktop-playback` harness command remains unavailable in this
environment; its configured equivalent (gate3 playback tests) ran inside the
135-pass `cargo test`.

# Verification Receipt — shuffle randomness + back-navigation (2026-08-25)

## Symptoms

1. "The shuffler plays the same songs each time I load the app" — shuffle
   was not random.
2. Skipping ahead with Shuffle on, then trying to go back a couple songs,
   was refused — the skipped-to song became unrecoverable.

## Root causes

1. **`shuffle_current_first` contained no RNG.** It moved the current track
   to index 0 and `rotate_left(len - 1)`-ed the rest — a FIXED permutation,
   identical every launch (and every skip, see #2).
2. **`next_track`/`previous_track` re-entered `play_with_context`.** Under
   shuffle that calls `shuffle_current_first` again: every skip re-shuffled
   the whole queue with the new current first, so Previous always landed at
   index 0 where further backs saturate ("wouldn't let me"), the order
   mutated on each press, and toggling shuffle OFF restored the SHUFFLED
   sequence as if it were the original playlist.

## Fixes

- `queue.rs`: Fisher-Yates (`shuffled_order`, xorshift64*) seeded from OS
  entropy + a per-call counter; current stays first; pure and seed-stable
  for tests.
- `state.rs`: new `start_current_track` starts audio for the already-current
  queue entry (resolve+heal path identical to play) WITHOUT touching queue
  structure; both handlers now advance/retreat then call it. Auto-advance
  inherits the same fix (no more reshuffle on natural transitions).
- Toggle-off still restores the true original order.

## Tests

| Test | Covers |
| --- | --- |
| `shuffled_order_is_a_permutation_and_seed_stable` | permutation validity + determinism-by-seed |
| `shuffle_produces_different_orders_across_instances` | the "same songs every launch" complaint (8 instances must diverge) |
| `toggle_off_restores_true_original_even_after_many_shuffles` | original-order preservation |
| `previous_walks_back_through_shuffled_queue_without_rebuilding_it` | the exact report: two Nexts then two Previous walk indices 2→0 over an UNCHANGED queue; toggle-off restores original. Uses real PCM WAV fixtures (undecodable fixtures trip failure-recovery auto-skip and race assertions); 30 s length so no heartbeat can complete a track mid-test. Stress-run ×5 plus three full-suite runs green. |

## Full battery

| Check | Result |
| --- | --- |
| `cargo test` | **146 passed; 0 failed** (3 consecutive full runs) |
| `npm run typecheck` + `npx vitest run` | clean, **107 passed** |
| `.venv/bin/python -m pytest -q` | **109 passed** |
| `npx tauri build --bundles app` | ✓ |
| `dist/Luno.app` refreshed from the new bundle | ✓ |

Note: Home ▸ "Made for you" is intentionally a DETERMINISTIC shuffle keyed
to catalogue membership (mobile parity) — unrelated to this fix.

The named `desktop-playback` harness command remains unavailable in this
environment; its configured equivalent (gate3 playback tests) ran inside the
146-pass `cargo test`.

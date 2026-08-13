# Desktop Verification Receipts

Status: U44 harness committed on 2026-08-13. A receipt is `pass` only when the
command completed successfully without skips, unavailable dependencies,
timeouts, truncation, or collection errors. Anything else is `inconclusive`.

## Commands

| Gate | Command | Evidence policy |
|---|---|---|
| Python differential/fake tools | `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q` | Required automated receipt; `python` vs `python3` differences are recorded. |
| TypeScript compile | `npm run typecheck` | Required automated receipt; unavailable `npm`/dependencies is inconclusive. |
| Rust unit | `cargo test --manifest-path src-tauri/Cargo.toml` | Required automated receipt; timeout or missing toolchain is inconclusive. |
| Visual | Browser screenshot runner at 960x660, 1180x800, 1600x1000 | Geometry is exact; colors use the declared perceptual threshold. No screenshot is a pass without an artifact. |
| Accessibility | Browser accessibility runner with keyboard/focus checks | Source inspection is not a pass. |
| Manual | `tests/journeys/manifest.json` sequences | Record OS, window size, fixture IDs, artifact path, and terminal state. |

## U01 Acceptance Matrix

| ID | Receipt | Current U44 evidence |
|---|---|---|
| AC-SHELL-01 | automated + visual/manual | automated source contract; browser/manual receipt inconclusive until runner exists |
| AC-SHELL-02 | automated + visual/manual | `src/test/visual/u44-geometry.ts`; screenshot receipt inconclusive |
| AC-LIB-01 | differential | existing differential reference harness |
| AC-LIB-02 | differential | existing differential reference harness |
| AC-LIB-03 | differential | existing differential reference harness |
| AC-LIB-04 | differential | target generation receipt required; not available in current runner |
| AC-LIB-05 | differential + journey | target journey receipt required; not available in current runner |
| AC-LIB-06 | differential + journey | target journey receipt required; not available in current runner |
| AC-PLAY-01 | differential + journey | existing pure differential surface; shell journey pending |
| AC-PLAY-02 | differential + journey | existing pure differential surface; shell journey pending |
| AC-PLAY-03 | differential | target differential adapter receipt required |
| AC-PLAY-04 | differential | target differential adapter receipt required |
| AC-PLAY-05 | differential | target differential adapter receipt required |
| AC-PLAY-06 | manual/audible | approved removal decision; audible receipt is not applicable, migration rejection test remains target-owned |
| AC-DL-01 | fake-subprocess + journey | deterministic fake-tool suite; target integration receipt pending |
| AC-DL-02 | fake-subprocess | fake missing/partial/valid output cases; target promotion receipt pending |
| AC-DL-03 | fake-subprocess + journey | target marker workflow receipt pending |
| AC-DL-04 | fake-subprocess + journey | target workflow receipt pending |
| AC-DISC-01 | differential + journey | target provider/network adapter receipt pending |
| AC-DISC-02 | differential | existing reference filtering surface; target receipt pending |
| AC-DISC-03 | fake-subprocess + journey | target isolation/promotion receipt pending |
| AC-DISC-04 | fake-subprocess + journey | target deadline/cancellation receipt pending |
| AC-DISC-05 | journey + shutdown | target queue handoff receipt pending |
| AC-DISC-06 | journey | target explicit-save receipt pending |
| AC-ART-01 | differential + visual | target artwork/cache receipt pending |
| AC-VIDEO-01 | fake-subprocess + journey | target staging receipt pending |
| AC-VIDEO-02 | fake-subprocess + journey | target audio-clock receipt pending |
| AC-VIDEO-03 | fake-subprocess + shutdown/manual | target lifecycle receipt pending |
| AC-XFER-01 | differential | frozen contract tests are the automated receipt |
| AC-XFER-02 | differential | frozen rejection tests are the automated receipt |
| AC-XFER-03 | differential | target planner receipt pending |
| AC-XFER-04 | differential + journey | provider-index/handoff receipt pending |
| AC-PERSIST-01 | differential + shutdown | target crash/recovery receipt pending |
| AC-PERSIST-02 | differential + shutdown | target marker recovery receipt pending |
| AC-OPS-01 | accessibility/manual | target diagnostics/redaction receipt pending |
| AC-OPS-02 | manual/package | target packaged smoke receipt pending |
| AC-OPS-03 | shutdown + journey | `tests/differential/test_u44_receipts.py` timeout classification; target shutdown receipt pending |

## Non-Pass Receipts

The current workspace has no browser runner, screenshot artifact store, packaged
application, or configured target-vs-reference adapter. Those gates are
`inconclusive`, not pass. The configured historical command `python -m pytest`
is also inconclusive when `python` is unavailable; use the exact command output
from the current run below rather than inferring success from source presence.

## Exact Receipts

- `python3 -m pytest tests/differential/test_harness.py tests/differential/test_u44_receipts.py -q`: exit `0`, `6 passed in 0.64s`.
- `python3 -m pytest -q`: exit `0`, `109 passed in 7.15s`.
- `python -m pytest -q`: exit `127`, `zsh:1: command not found: python` (inconclusive).
- `npm run typecheck`: exit `127`, `sh: tsc: command not found` (inconclusive).
- `cargo test --manifest-path src-tauri/Cargo.toml`: exit `127`, `zsh:1: command not found: cargo` (inconclusive).
- `python3 -m json.tool tests/journeys/manifest.json >/dev/null`: exit `0`.
- `git diff --check`: exit `0`.
- Browser visual/accessibility, packaged smoke, manual journeys, and target
  differential/shutdown adapters: unavailable in this workspace (inconclusive).

## Ad-Hoc Legacy Tests

`test_tkvideo.py` is an obsolete Tk/video experiment. `test_dl.py`,
`test_focus.py`, and `test_label_ph.py` are ad-hoc legacy experiments. They are
not U44 release gates and must not be counted as parity, differential,
integration, visual, or accessibility receipts.

# Formal Gate 5 Receipt: Discover, Preview, Artwork, And Video

Receipt date: `2026-08-15`
Execution timestamp (UTC): `2026-08-16T03:08:11Z`
Real-runtime execution timestamp (UTC): `2026-08-16T09:12:49Z` (launch), `2026-08-16T09:15:48Z` (AX capture)
Environment: macOS Darwin 25.5.0 arm64; `rustc/cargo 1.97.1`; Node `22.23.2`; npm `10.9.8`; `/opt/homebrew/bin/python3.14` Python `3.14.7`.

## Formal Verdict

**Formal Gate 5: PASS for the opt-in synthetic seam through the real Tauri
runtime; target parity for real provider, FFmpeg/media, manual journeys, and
the packaged runtime remains BLOCKED.**

The full chain native command → IPC → React/WebView event → native shutdown
event was exercised in a real `luno-desktop` process with a real macOS window
on `2026-08-16T09:12:49Z`. The opt-in smoke probe reported discovery
(`1` item), preview-ready/hidden-before-save/visible-after-scan states,
video-ready state, native result-event observation, and shutdown-complete
event observation with no errors. Evidence is durable in
`docs/verification/artifacts/gate5-real-runtime-pass.png` and
`docs/verification/artifacts/gate5-real-runtime.ax.txt`.

Policy PASS is distinct from target parity BLOCKED: only synthetic loopback
provider data and synthetic ID3/MP4 bytes were used, and manual journeys,
APIC/Tk artwork decoding, FFmpeg/native media processes, wall-clock deadline
behavior, real providers, real video, and the packaged runtime remain
unobserved.

## Provenance And Safety

- Policy fixture source: `src-tauri/src/gate5_tests.rs`, registered in
  `src-tauri/src/lib.rs`; it calls policy modules directly.
- Native smoke source: `src-tauri/src/runtime/gate5_smoke.rs`, registered by
  `gate5_smoke` in `src-tauri/src/main.rs`; `src/app/gate5Smoke.tsx` is opt-in
  only (`VITE_LUNO_GATE5_SMOKE=1`) and requires three explicit fixture roots.
- `bridge.ts` preserves payloads for non-backend Tauri commands while retaining
  the backend wrapper and no-argument calls. It listens for the command result
  event before invoking shutdown; the React probe bounds the native result,
  result-event, shutdown command, and terminal shutdown-event waits with the
  configured timeout, and passes only when both lifecycle events are observed.
  The native command rejects
  relative/control-character roots, roots outside the system temporary
  directory (including symlink-resolved existing parents), and every pair of
  overlapping roots in either direction, and pre-existing roots. Loopback
  setup and empty video-source failures return errors rather than panic or
  index an empty list.
- Provider fixture: an in-process loopback `TcpListener` on `127.0.0.1` returns
  controlled Last.fm and Deezer JSON. It uses no external provider, account,
  external network, real API key, or real credentials; the existing fixture-key
  value is synthetic.
- Media fixture: synthetic ID3-like bytes, synthetic MP4 `ftyp` bytes, explicit
  process-scoped temp roots, and an in-memory fake decoder/factory. It uses no
  yt-dlp, FFmpeg, audio device, real video, or real library.
- `HOME` was not changed. No Keychain, secrets, credentials, or music library
  was accessed. Preview/video stores use explicit synthetic temp roots.
- The final fixture additions cover: direct non-empty discovery with largest
  usable image selection; a second requested limit proving separate cache
  behavior; empty-after-fallback; pre-cancelled preview download; preview
  next/previous/return-to-seed; DiscoverMode rolling-window `Prepare`, ready
  adoption, and `Restore`; scan invisibility before explicit promotion and
  visibility afterward; and VideoStage next-slot ready, cancellation, and
  stale-generation behavior.
- The native seam uses only synthetic bytes, an in-process loopback provider,
  fake/local video candidate callbacks, and caller-provided roots. No real
  provider, credentials, library, HOME, Keychain, FFmpeg, or audio device is
  touched. The unit test calls `run()` directly with three fresh process-scoped
  temp children; the real-runtime launch (`2026-08-16T09:12:49Z`) exercised the
  same seam through the actual Tauri process with the opt-in probe and is
  documented in `Real Tauri Runtime Launch`.

## Exact Verification Commands

| Exact command | Result | Classification |
|---|---:|---|
| `cargo test --manifest-path src-tauri/Cargo.toml gate5_smoke` | exit 0; 3 passed | PASS: root safety, empty-source, and direct synthetic `run()` native seam tests |
| `cargo test --manifest-path src-tauri/Cargo.toml gate5_tests` | exit 0; 8 passed | PASS: Gate 5 policy fixtures |
| `cargo test --manifest-path src-tauri/Cargo.toml gate2_tests` | exit 0; 24 passed | PASS: regression |
| `cargo test --manifest-path src-tauri/Cargo.toml gate3_tests` | exit 0; 10 passed | PASS: regression |
| `cargo test --manifest-path src-tauri/Cargo.toml gate4_tests` | exit 0; 11 passed | PASS: regression |
| `cargo test --manifest-path src-tauri/Cargo.toml` | exit 0; 91 passed, 1 ignored | PASS: full Rust suite; ignored test requires interactive Keychain |
| `/opt/homebrew/bin/python3.14 -m pytest test_discover_mode.py test_art_cache.py test_video_preview.py` | exit 0; 46 passed | PASS: focused Python reference suite |
| `/opt/homebrew/bin/python3.14 -m pytest -q` | exit 0; 109 passed | PASS: full Python reference suite |
| `/Library/Developer/CommandLineTools/usr/bin/python3 -m pytest` | exit 1; `No module named pytest` | INCONCLUSIVE: unavailable system Python dependency |
| `npm run typecheck` | exit 0; `tsc --noEmit` | PASS: typecheck |
| `npm test -- --run src/test/bridge.test.ts src/test/gate5_smoke.test.ts` | exit 0; 2 files, 21 tests | PASS: focused bridge argument and Gate 5 lifecycle/timeout tests |
| `npm test` | exit 0; 6 files, 34 tests | PASS: frontend tests including Gate 5 lifecycle negative cases |
| `npm run build` | exit 0; 55 Vite modules transformed | PASS: build only, not runtime parity |
| `rustfmt --edition 2021 --check src-tauri/src/discovery/artwork.rs src-tauri/src/preview/discover_mode.rs src-tauri/src/preview/save.rs src-tauri/src/gate5_tests.rs` | exit 0 | PASS: targeted intentional Gate 5 Rust files |
| `git diff --check` | exit 0 | PASS: whitespace |
| `rustfmt --edition 2021 --check src-tauri/src/runtime/gate5_smoke.rs` | exit 0 | PASS: new Rust smoke seam |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | exit 1 | INCONCLUSIVE: pre-existing formatting drift in unrelated migration/transfer/video files; no broad reformat applied |

## Real Tauri Runtime Launch (2026-08-16)

### Defect Found And Fixed

`gate5_smoke` is a direct Tauri command; `bridge.gate5Smoke` invokes it with
the snake_case keys `{ root, preview_root, video_root }`. Tauri v2 renames
command arguments to camelCase by default, so `preview_root`/`video_root` were
silently rejected at the IPC boundary (`invalid args `previewRoot` for command
`gate5_smoke`: command gate5_smoke missing required key previewRoot`) and the
invoke never reached the Rust handler; the probe's invocation timed out and
the smoke could not pass. The command is now declared
`#[tauri::command(rename_all = "snake_case")]` so the bridge's existing
snake_case contract matches the handler. The defect was isolated with Rust-side
entry logging plus step instrumentation in the opt-in probe; a no-argument
`invoke("gate5_smoke", {})` control produced the missing-key rejection above,
confirming the boundary. No other direct command is invoked from the
frontend with multi-word snake_case arguments; `downloader_search`,
`settings_load`, and `transfer_preview` are reached only through
`backend_command` envelopes, which carry opaque `Value` payloads and are
unaffected.

### Launch And Observation

- Fixture isolation: `HOME=<root>/home`, `TMPDIR=<root>/tmp` with
  `CARGO_HOME=/Users/ty/.cargo` and `CARGO_NET_OFFLINE=true`; three fixture
  roots under the canonical temp directory: `<root>/tmp/library`,
  `<root>/tmp/preview`, `<root>/tmp/video`; none pre-existed, none overlapped.
- Launch: `VITE_LUNO_GATE5_SMOKE=1 VITE_LUNO_GATE5_SMOKE_ROOT=<root>/tmp/library
  VITE_LUNO_GATE5_SMOKE_PREVIEW=<root>/tmp/preview
  VITE_LUNO_GATE5_SMOKE_VIDEO=<root>/tmp/video npm run tauri -- dev --no-watch`.
  Vite `7.3.6` on port 1420; no pre-existing port or process conflicts.
- Native window identity: one `luno-desktop` process after launch
  (`target/debug/luno-desktop`); macOS accessibility identified window
  `Luno` (CGWindowID `75132`, 1180x800) with the full shell AX tree
  (`Primary navigation` with Home/Recent/Discover/Downloader/Settings
  buttons, `Library` pane, `Queue`).
- Probe lifecycle observed in the WebView AX tree: the opt-in overlay
  (`role=status`, `data-gate5-smoke`) rendered from `Native Gate 5 smoke is
  running` through invocation and shutdown, and settled at:

  > `Discovery 1; preview ready; video ready. Native event observed; shutdown
  > complete. Unobserved: manual Discover/Song Page rendering; APIC/Tk artwork
  > decoding; FFmpeg/process/native audio clock; external provider/video and
  > wall-clock deadlines.`

  That text is the probe's pass state: result present, result event observed,
  shutdown complete, zero errors. The screenshot
  `docs/verification/artifacts/gate5-real-runtime-pass.png` and the AX capture
  `docs/verification/artifacts/gate5-real-runtime.ax.txt` are durable evidence.
- Post-run fixture verification (all under the fixture root, then removed):
  - `tmp/library/Fixture Playlist/fixture-artist - fixture-track.mp3` — the
    promoted track (21 synthetic ID3-like bytes), proving save-after-scan
    visibility state `saved_visible_after_scan=true`;
  - `tmp/preview/fixture-artist-fixture-track.mp3` — the preview asset,
    proving `preview_ready=true` and hidden-before-save handling;
  - `tmp/video/current-fixture-video.mp4` — the staged video source
    (18 synthetic MP4 `ftyp` bytes), proving `video_ready=true`.
- Shutdown and cleanup: the window was closed through macOS AX after
  evidence capture; both `luno-desktop` and the Vite child exited, leaving no
  orphan processes. The fixture root was deleted afterward.
- Evidence separation: provider data came only from the in-process loopback
  `TcpListener`; media bytes are synthetic. No real provider, credentials,
  Keychain, real HOME, background FFmpeg/yt-dlp process, audio device, or
  real library was touched.

### Runtime Verification Recap

| Exact command | Result | Classification |
|---|---:|---|
| Real-runtime launch above (`npm run tauri -- dev --no-watch`) | exit 0; probe overlay reached pass state | PASS: command → result event → React → shutdown event end-to-end (synthetic fixtures) |
| AX pass-state capture `docs/verification/artifacts/gate5-real-runtime.ax.txt` | quotes above | PASS |
| Screenshot `docs/verification/artifacts/gate5-real-runtime-pass.png` | 230 891 bytes, 1180x800 | PASS: window recaptured after WebView render, not an early blank frame |
| Post-run fixture inspection | three files above | PASS: library/preview/video fixture states on disk |
| Window close via AX; `pgrep -fl` | app and Vite exited; no orphans | PASS |
| Fixture root removal | removed after evidence | PASS |

## Corpus Matrix

Policy PASS means the target policy function ran with controlled fixtures.
Native seam PASS means the direct synthetic `run()` function test, the
command/React probe contract tests, and the opt-in probe running end-to-end in
the real Tauri dev process (see `Real Tauri Runtime Launch` above) all pass.
Target parity BLOCKED means real provider/FFmpeg/manual/packaged evidence is
still absent.

| Corpus reference | Policy evidence | Target parity |
|---|---|---|
| `REF-DISCOVERY` | Loopback direct success, largest usable image, limit-sensitive cache, empty-after-fallback, filtering, dedupe, error, empty, and missing-key states | BLOCKED |
| `REF-PREVIEW-SESSION` | Isolated atomic asset, pre-cancel, generation guard, five-ahead window, next/previous/return-to-seed, and stale completion | BLOCKED; deadline timing not runtime-proven |
| `REF-DISCOVER-HANDOFF` | Rolling-window prepare/ready adoption, queue capture, restore, natural handoff, and caller-owned intervention | BLOCKED |
| `REF-PREVIEW-SAVE` | Destination/traversal validation, scan before/after promotion, verified duplicate, and staged retention | BLOCKED |
| `REF-ARTWORK` | Loopback fallback URL, bounded cache/eviction, and deterministic design | BLOCKED; APIC/Tk path not exercised |
| `REF-VIDEO` | Relevance/canonicalization, isolated current/next staging, next-slot cancellation/stale generation, unavailable state, fake player clock/lifecycle/fallback | BLOCKED; no FFmpeg/process/UI run |

## Acceptance Matrix

| Acceptance | Classification | Evidence / limitation |
|---|---|---|
| `AC-DISC-01` | Policy PASS; target parity BLOCKED | Provider success, fallback, empty-after-fallback, error, and missing-key fixtures |
| `AC-DISC-02` | Policy PASS; target parity BLOCKED | Filtering, dedupe, direct usable artwork, and limit-sensitive cache |
| `AC-DISC-03` | Policy PASS; target parity BLOCKED | Temporary-root isolation and scan invisibility until explicit promotion |
| `AC-DISC-04` | Policy PASS; runtime timing INCONCLUSIVE | Cancellation, generation, window, navigation, and retry-state fixtures; no native deadline run |
| `AC-DISC-05` | Policy PASS; target parity BLOCKED | Rolling adoption, physical queue capture, restore, and intervention handling |
| `AC-DISC-06` | Policy PASS; target parity BLOCKED | Explicit destination, traversal rejection, promotion scan visibility, and duplicate guard |
| `AC-ART-01` | Policy PASS; target parity BLOCKED | Loopback/cache/design policy; no embedded APIC or Tk stale-completion run |
| `AC-VIDEO-01` | Policy PASS; target parity BLOCKED | Relevance, canonical IDs, temporary current/next staging, cancellation, stale generation, and unavailable state |
| `AC-VIDEO-02` | Policy PASS; target parity BLOCKED | Fake audio-clock pause/resume/seek/resize/EOF/fullscreen events |
| `AC-VIDEO-03` | Policy PASS; target parity BLOCKED | Fake decoder failure, fallback, shutdown, next-slot cancellation, and stale-generation policy |

## Journey Matrix

| Journey | Classification | Relevant policy/unit evidence | Target journey limitation |
|---|---|---|---|
| `JRN-DISCOVER` | Policy/unit PASS; native seam PASS in real Tauri dev runtime; real-provider journey BLOCKED | Policy fixtures; `src/app/gate5Smoke.tsx` probe; runtime pass-state overlay and artifacts | Real provider/UI journey not executed; discovery used the loopback fixture only. |
| `JRN-VIDEO` | Policy/unit PASS; native seam PASS in real Tauri dev runtime; real FFmpeg journey BLOCKED | Policy fixtures; `gate5_smoke` synthetic staging result; probe video-ready state | No FFmpeg/real video/UI playback journey was executed. |

## Manual, Screenshot, And Runtime Evidence

| Evidence | Classification | Limitation |
|---|---|---|
| `CAP-DISCOVER-STATES` | BLOCKED | No manual target Discover UI capture. |
| `CAP-SONG-PAGE-STATES` | BLOCKED | No manual artwork/preview/video page capture. |
| `CAP-RESIZE-FULLSCREEN` | BLOCKED | No native resize/fullscreen media run. |
| `CAP-GATE5-RUNTIME-SEAM` | PASS (synthetic) | `gate5-real-runtime-pass.png` + `gate5-real-runtime.ax.txt`; probe pass-state overlay observed over the live shell in the real Tauri window. |
| `JRN-DISCOVER` | BLOCKED for target parity | Real provider/UI journey not executed; loopback fixture only. |
| `JRN-VIDEO` | BLOCKED for target parity | Fake source/staging/player fixtures and probe video-ready state exist; no FFmpeg/real-video run. |

The remaining required manual/runtime checks are still unexercised; no
PASS/FAIL beyond the synthetic seam is claimed for them.

## Blockers

- The opt-in native seam now runs end-to-end in the real Tauri dev process
  (command invocation, emitted result-event delivery, native shutdown, and UI
  overlay all observed; see `Real Tauri Runtime Launch`). Still unobserved:
  real provider/network results, FFmpeg or yt-dlp subprocess behavior, APIC/Tk
  artwork decoding, a manual Discover/Song Page UI journey, wall-clock
  deadline behavior, and the packaged runtime.
- The probe's result is a window-state capture; the app window itself was not
  driven through a manual discover→save→video page journey. This run is dev
  mode, not the packaged artifact, and used only synthetic loopback/IPC
  fixtures with an isolated HOME/TMPDIR.
- Target parity for real media remains **BLOCKED** by design; no real-media
  PASS is claimed.

## Fixture Invocation And Rollback

- Executed isolated invocation: `VITE_LUNO_GATE5_SMOKE=1
  VITE_LUNO_GATE5_SMOKE_ROOT=<tmp>/library
  VITE_LUNO_GATE5_SMOKE_PREVIEW=<tmp>/preview
  VITE_LUNO_GATE5_SMOKE_VIDEO=<tmp>/video npm run tauri -- dev --no-watch`,
  with an isolated `HOME`/`TMPDIR` and no provider network. The probe passed
  (see `Real Tauri Runtime Launch`) and the fixture root was removed after
  evidence capture.
- Root safety unchanged: the command rejects preview/video roots inside the
  library and the library root inside either preview/video root, and requires
  all roots to be absolute, temporary-directory-contained, pairwise
  non-overlapping, and freshly created. `rename_all = "snake_case"` on
  `gate5_smoke` aligns the Tauri v2 IPC argument contract with the bridge's
  snake_case keys.
  Temporary files are atomically staged and remain outside the library until explicit save;
  the seam is removable by deleting `gate5_smoke`, `gate5Smoke`, and their
  registration lines without touching user data or production queue behavior.

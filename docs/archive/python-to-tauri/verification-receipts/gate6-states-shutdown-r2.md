# Gate 6 Receipt Supp R2: Populated-State, Keyboard, Downloads, And Active-Work Shutdown QA

Receipt date: `2026-08-16`
Execution timestamp (UTC): `2026-08-16T06:20:00Z`
Environment: macOS `26.5`, Darwin `25.5.0`, arm64; Node `v22.23.2`; npm
`10.9.8`; Rust/Cargo `1.97.1`; Tauri CLI `2.11.4`.
Source head at execution: `7ed2d52c` plus the four commits listed under
"Defects Found And Fixed".

## Formal Verdict

**Gate 6: PASS with reservations.** This supplement executes the matrix rows
that the prior receipt (`gate6-visual-accessibility.md`, INCONCLUSIVE) left
BLOCKED: populated library/queue/player states, all seven application routes
rendered with controlled fixture content, keyboard traversal and activation
(tab/arrow/enter/space/escape), command-rejected error state, the download
lifecycle (search → select → running → cancel → cancelled), and close-while-
work-active shutdown (playback, active download, and boot scan) with process
cleanup. Every row below marked native-observed used only the real Tauri
window plus macOS Accessibility tools; no source inspection, HTML, Vite
server, unit test, or build result is promoted as visual or accessibility
proof. The loading row is documented as **unreachable by any user action** in
the deployed boot order (native evidence below); the video row remains
unavailable in the dev stage and is documented from its live runtime status
and Song Page fallback text.

## Governing Records And Safety

Same bounds as the prior receipt: fresh `/private/tmp/luno-gate6-*` fixture
homes with isolated `HOME`, `TMPDIR`, no real keychain/credentials/library or
network; `CARGO_HOME=/Users/ty/.cargo` as a separate cache only;
`CARGO_NET_OFFLINE=true`; `npm run tauri -- dev --no-watch` for every launch.

Fixture environments:

- `fixtures-populated.py` — 12 controlled tracks (6 WAV plus 6 ID3 stub MP3s)
  across three artists, config `music_root: Music`. Used for run1 series,
  run2b, run2c, settings, and downloader runs.
- `fixtures-big.py` — 3,600 stub MP3s across 30 top-level albums. Used for the
  boot-scan timing runs. A 12,000-file variant documented the migration
  inspection cap (see Defects/notes).
- `slow-helper.sh` — download helper that sleeps 40s then writes
  `ID3synthetic-audio` to its destination argument, exercised through the UI
  via `VITE_LUNO_GATE4_HELPER`/`VITE_LUNO_GATE4_DESTINATION` (directory path).

## Defects Found And Fixed (all fixed and committed)

| Commit | Defect | Native observation before/after |
|---|---|---|
| `846c2559` | `TrackStarted` was published with the library generation while the frontend's stale-event filter tracks the playback-domain generation (which advances every heartbeat). Every `track_started` was discarded, so the Recent route stayed at `0 of 100 session tracks` with no rows even though the backend recorded history. | Before: `value  of 100 session tracks`, no Recently Played rows. After: `value 4` + ` of 100 session tracks`, rows `Metric/Epilogue/Cascade/Aurora ... Recently Played`. |
| `cf010c14` | The Downloader source tablist had no roving tabindex: all seven `role=tab` buttons stayed in the Tab order, so Tab cycled `Search → ... → Allowed Dups → Search` forever and the panel content (Audio URL field, submit) was unreachable by keyboard. | Before: 40+ tabs never left the tablist ring; `NOT_FOUND_Audio URL` for the field. After: 4 tabs reach the `Search` tab, ArrowRight ×2 activates `Direct URL`, Tab exits the tablist to the panel, and the field/submit flow works. |
| `d3bfa418` | Shell never passed `loading` to LibraryView, so the generation-0 pre-scan snapshot painted as `No songs in All Music.` during the first scan. | Loading is now wired for null or generation-0 snapshots (`role="status"`, `Loading library...`, unit-covered); boot polls document the state is unreachable in practice (below). |
| `7ed2d52c` | Test-only: PlayerBar fixture used a nonexistent `track_id` field instead of the `queue`/`current_index` snapshot shape. | Typecheck surfaced the mismatch; fixture aligned. |

Final checks at execution head: `npm test` → 41 passed; `npm run typecheck`
→ clean; `cargo test --manifest-path src-tauri/Cargo.toml --lib` → 91 passed,
0 failed, 1 ignored. No promotion of these source checks as visual evidence.

## Native Evidence By Matrix Row

Runs: `run2c.M4VR` (full populated journey), `run2b` (position progress),
`run1.*` (empty library + Recent empty states, route placeholders),
`run4b.2Ikh`/`run4c.vl91` (download lifecycle + active-download close),
`run3.*` (boot-scan timing), settings run (Settings route). All under
`/private/tmp/luno-gate6-*`; key screenshots are promoted to
`docs/verification/artifacts/`.

| Area | Native observation | Classification |
|---|---|---|
| Populated Home/Library | `Migration completed.` runtime status; `12 songs`; rows `Aurora artwork AuroraUnknown artist Add to favorites All Music` (etc.); `Scan`, `Shuffle`, `All Music (12)`, `Unsorted`, `Favorites (0)` scope buttons; screenshot `gate6-run2c-home.png` | PASS: native-observed |
| Player bar while playing | `Pause`, `Previous track`, `Next track` buttons; position pair rendered `0:00/0:01`; elapsed advanced `0:00 → 0:01` between run2b captures with a 1.5s track; queue auto-advanced to later tracks (`button Aurora ... Play Aurora next` queue items); screenshots `gate6-run2c-playing.png` | PASS: native-observed |
| Auto-advance into subsequent tracks | Playing-1 vs playing-2 captures show the current queue head changed (Aurora → Cascade) with fresh positions, and Recent gained `Metric/Epilogue/Cascade/Aurora` most-recent-first | PASS: native-observed |
| Recent populated | `value 4` + ` of 100 session tracks`; rows with `Recently Played` playlist context; `Clear History` button; empty-state row absent; screenshot `gate6-run2c-recent.png` | PASS: native-observed |
| Recent empty | 4/4 empty-library runs: `value  of 100 session tracks` + `value No recently played songs.` (run1.3EH5 `text-Recent.txt`, etc.) | PASS: native-observed |
| Empty library | `value Library`, `value No songs in `, `button All Music (0)` (run1.3EH5 `text-Home.txt`; run3.ThVc after-cap row) | PASS: native-observed |
| Song Page | Current track `Stub 6` + `Unknown artist` + `Add to favorites`; `Recommended for this song` / `0 similar songs - tap to preview` / `Refresh recommendations`; `↓ Scroll for recommendations`; `Enter fullscreen` button; video fallback `Video preview unavailable; showing artwork.`; screenshot `gate6-run2c-songpage.png` | PASS: native-observed |
| Downloader route (search tab) | `Download music to your library` heading; `Search`/`Save to` (+ destination value), Progress/Download log/Downloaded songs accordions | PASS: native-observed |
| Downloader tabs keyboard a11y | Tab reaches active tab; ArrowLeft/Right activate adjacent tabs; Tab exits tablist to panel; focus sequence `Search tab → Direct URL → Audio URL AXTextField` verified by `findfocused` | PASS: native-observed + fix commit |
| Downloader error state | Keystrokes typed `http://x` (`role=AXTextField title=Audio URL value=http://x`); submit produced the command-rejected banner `{"code":"invalid_request","message":"destination must be an absolute path","retryable":false}`; screenshot `gate6-run2c-error-state.png` | PASS: native-observed |
| Downloader search results | `scsearch1:artist track` + `ytsearch1:artist track` rows with provider labels; checkbox `Select scsearch1:artist track value=false`; space toggled to `Download selected (1)` | PASS: native-observed |
| Download running | Runtime status `Downloading` + `Cancel` button; Progress accordion `single` / `running`; persisted across captures; screenshot `gate6-run4b-downloading.png` | PASS: native-observed |
| Download cancel | Pressing `Cancel` drove the job to `cancelled`; helper child process killed; destination dir left empty | PASS: native-observed |
| Download retry gating | Dev shell surfaces the explicit policy when helpers are absent (`External download helpers are unavailable...`) | PASS: source-stated dev affordance (no native helper run) |
| Discover | `Discovery` heading; `Recommendations will appear here when the discovery backend is connected.` | PASS: native-observed (run1.SzWZ `text-Discover.txt`) |
| Settings | `Settings` heading; `Connect to Last.fm`; keychain note (`stored in the macOS Keychain and is never written to settings files`); `Last.fm API key` field; `Automatic recommendations`; `Music folder`; screenshot `gate6-run-Settings-route.png` | PASS: native-observed |
| Error Log | Route heading `Error & Debug Logs` | PASS: native-observed |
| Command-rejected banner | Full backend error JSON rendered in the runtime-status region | PASS: native-observed |
| Video | Dev stage: `Video preview unavailable; artwork fallback is active.` runtime status across every run; Song Page fallback text; no native video stream exists in the dev stage (gate5 seam only) | DOCUMENTED: unavailable in dev stage; fallback states native-observed |
| Loading (first scan) | Unreachable: with a 3,600-file tree the boot scan always completes before first paint — poll series (`run3.ElLQ poll-0..15`) shows the first painted frame already at `3600 songs`. The generation-0 window exists only before paint. Affordance now wired (defensive) and unit-covered | DOCUMENTED: unreachable by user action in current boot order; wiring + unit proof only |
| Close while playing | `AX close` → `CLOSED`; 6s later `pgrep` → `no luno/vite processes` | PASS: active-work close proof (playback active) |
| Close during active download | App closed while job `running` (helper mid-sleep); no luno/vite/helper processes after | PASS: active-work close proof (download active) |
| Close during boot scan | Closed during the 3,600-file boot scan window; no luno/vite processes after | PASS: bounded boot-scan close proof |

## Native Attempt Notes And Tooling

Keyboard automation used only real `osascript` key events (tab `48`; arrows
`124/123`; enter `36`; space `49`) against the focused native window, verified
per step by `findfocused` (`role=AXButton | title=...`, `role=AXTextField`,
`role=AXCheckBox`). Text entry used `osascript keystroke` into the focused
field; AX `setValue` was found to be reset by React's controlled inputs
(WebKit set succeeds but the AX value never changes), and `axctl`'s matcher
was corrected to compare case-insensitively and to target only text-field
roles — both tooling fixes, no app change.

The 12,000-file fixture documented the migration inspection cap: the legacy
marker inventory rejects trees exceeding 4096 entries
(`Migration failed: legacy marker inspection exceeded 4096 entries`), a
deliberate safety bound; the 3,600-file tree was used instead.

## Deterministic Checks At Execution Head `7ed2d52c`

| Exact command | Result | Classification |
|---|---|---|
| `npm run typecheck` | exit `0` | PASS: source check only |
| `npm test` | exit `0`; 8 files, 41 tests passed | PASS: automated check only |
| `CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --lib` | exit `0`; 91 passed, 0 failed, 1 ignored | PASS: automated check only |
| `git diff --check` | exit `0` | PASS: hygiene |
| `npm run tauri -- --version` | exit `0`; `tauri-cli 2.11.4` | PASS: tooling only |

The same three pre-existing Cargo warnings persisted (unnecessary `mut`
`first_run.rs:777`, unused `error` `sync.rs:157`, unused
`SecKeychainItemModifyAttributesAndData` `keychain.rs:74`); none was promoted
to a visual/accessibility defect.

## Evidence Paths

Promoted screenshots (all verified PNGs captured with `screencapture -x`
while the native window was on screen):

- `docs/verification/artifacts/gate6-run2c-home.png` ← `/private/tmp/luno-gate6-run2c.M4VR/1-shot-home.png`
- `docs/verification/artifacts/gate6-run2c-playing.png` ← `.../2-shot-playing.png`
- `docs/verification/artifacts/gate6-run2c-recent.png` ← `.../3-shot-recent.png`
- `docs/verification/artifacts/gate6-run2c-songpage.png` ← `.../4-shot-songpage.png`
- `docs/verification/artifacts/gate6-run2c-error-state.png` ← `.../5-shot-error-state.png`
- `docs/verification/artifacts/gate6-run4b-downloading.png` ← `/private/tmp/luno-gate6-run4b.2Ikh/shot-job-running.png`
- `docs/verification/artifacts/gate6-run4c-downloading-close.png` ← `/private/tmp/luno-gate6-run4c.vl91/shot-job-running.png`
- `docs/verification/artifacts/gate6-run-Settings-route.png` ← `/private/tmp/luno-gate6-settings2.*/shot-settings.png`

Primary AX text/tree dumps (ephemeral, provenance only):

- `run2c.M4VR`: `1-home.txt`, `2-playing-1.txt`, `2-playing-2.txt`,
  `3-recent.txt`, `4-songpage.txt`, `5-error-state.txt`, `5-fields.txt`
  (`value=http://x`), `6-close.txt`, `6-orphans.txt` (`no luno/vite processes`)
- `run2b.Hobr`: `text-playing-1/2.txt` (position `0:01` + advance), `text-recent.txt`
- `run1.3EH5`: `text-Home.txt` (empty library), `text-Recent.txt` (`No recently played songs.`)
- `run1.SzWZ`: `text-Discover.txt` (Discovery placeholder)
- `run4b.2Ikh`: `job-running-1.txt` (`Downloading`/`Cancel`/`running`),
  `job-running-2.txt`, `job-cancelling.txt`, `job-after-cancel.txt` (`cancelled`)
- `run4c.vl91`: `job-running-1.txt`, `close-active.txt` (`CLOSED`), `orphans.txt`
- `run3.ElLQ`: `poll-0..15.txt` (first paint already `3600 songs`),
  `close-scan.txt`, `orphans.txt`
- `run3.9mCs`: `poll-0.txt` (4096-entry cap banner), `orphans.txt`
- `dlfix3.*`: `focus-after-tab1.txt`/`focus-after-tab2.txt` (tabpanel → `AXTextField Audio URL`)
- settings run: `text-settings.txt`

## Worktree Hygiene And Final Classification

- Four fix commits: `846c2559`, `cf010c14`, `d3bfa418`, `7ed2d52c`; worktree
  clean after receipt commit (`.hermes/` untracked pre-dates this session).
- No production fix beyond the three defect fixes above; no signing, secret,
  or user-data change.

**PASS with reservations.** All previously BLOCKED matrix rows are now
native-observed or explicitly documented as unavailable with evidence,
including active-work shutdown for playback, download, and boot-scan windows.
Reservations: the loading state and the live video stage (the dev stage has no
video pipeline — the gate5 provider seam is the only video path; the runtime
status and Song Page fallback states are native-observed) remain
documented-unavailable rather than native-observed, and the wide-window
resize row from the prior receipt stayed display-constrained.

Supplementary to `gate6-visual-accessibility.md` (idle shell, geometry, and
idle-close receipt), which remains valid for its scope.
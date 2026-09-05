# Gate 6 Receipt: Native Visual Accessibility And Shutdown QA

Receipt date: `2026-08-16`
Execution timestamp (UTC): `2026-08-16T03:25:22Z`
Environment: macOS `26.5`, Darwin `25.5.0`, arm64; Node `v22.23.2`; npm
`10.9.8`; Rust/Cargo `1.97.1`; Tauri CLI `2.11.4`.

## Formal Verdict

**Gate 6: INCONCLUSIVE / BLOCKED overall.** A real native Tauri window was
observed and captured at the default size, and macOS Accessibility automation
exposed the native window and top-level controls. The empty synthetic fixture
rendered correctly enough for that limited observation. The complete visual and
accessibility gate remains unproven: narrow/wide rendered state coverage,
keyboard traversal, focus visibility/order, all routes, populated/loading/error
states, and a complete accessibility tree audit were not executed. No source
inspection, HTML, Vite server, unit test, or build result is promoted as visual
or accessibility proof.

## Governing Records And Safety

Read before execution:

- `AGENTS.md`, `project_brain.md`
- `docs/migration/acceptance-catalog.md`, `docs/migration/reference-corpus.md`
- `migration-audit/01-shell-navigation.md`, `migration-audit/02-visual-system.md`, `migration-audit/12-operations-release.md`
- `migration-audit/14-audit-review.md`, `migration-audit/15-synthesis.md`, `migration-audit/README.md`
- `docs/migration/rollback.md`, `contracts/library-export/v1/README.md`
- Prior Gate 2, 3, 4, and 5 receipts, `docs/verification/receipts/r06-summary.md`, and R06 visual/accessibility/shutdown unavailable records.

Every native attempt used a fresh `/private/tmp/luno-gate6-*` directory with an
isolated `HOME`, `TMPDIR`, empty `home/fixture-library`, and empty
`home/.luno_previews`. `CARGO_HOME=/Users/ty/.cargo` was used only as a separate
dependency cache; `CARGO_NET_OFFLINE=true` was set. npm used a separate
process-local cache under each fixture directory. No real HOME, Keychain,
credentials, music library, downloads, provider data, network provider, or
uncontrolled media was used. The fixture roots remained empty in every run.

Initial `git status --short` showed pre-existing modified source/configuration
files and untracked Gate 2-5 work, reports, artifacts, and receipts, including
this receipt. Those changes were preserved. No production fix, reset, clean,
commit, push, deploy, signing, secret change, or user-data change was made.

## Geometry And Native Measurements

The configuration contract is `src-tauri/tauri.conf.json`: default `1180x800`,
minimum `960x660`, resizable. The controlled wide target was `1600x1000`.

| Target | Native observation | Classification |
|---|---|---|
| Default `1180x800` | AX query returned `Luno, 166, 56, 1180, 800`; screenshot was created. | PASS: directly observed native shell only |
| Minimum `960x660` | macOS automation set and read `960, 660`. | PASS: size measurement only; no screenshot/state matrix |
| Wide `1600x1000` | Automation request succeeded, but the display constrained the observed size to `1346, 866`. | INCONCLUSIVE: requested wide size not attainable on this display |

The default screenshot shows the empty synthetic Library, Queue, shell routes,
player footer, and fallback/status banner. It does not establish populated,
loading, error, downloader, Discover, Song Page, video, settings, or keyboard
behavior.

## Evidence Paths

The durable screenshot artifact is accompanied by its original ephemeral source
path for provenance:

- Screenshot artifact: `docs/verification/artifacts/gate6-native-default-shell.png`
  (verified `3024x1964` full-screen PNG containing the native Luno window
  observed at AX geometry `1180x800`)
- Screenshot source provenance: `/private/tmp/luno-gate6-t4m0g0t5/native-default-screen.png`
- Native AX top-level query: `/private/tmp/luno-gate6-ax-b89ra6nf/ax-window.txt`
- Native launch/close output: `/private/tmp/luno-gate6-close-vwyoddbz/native-close.out`
- Geometry output: `/private/tmp/luno-gate6-geometry-38lziroq/native-geometry.out`

The AX query returned roles/names including `AXGroup`, `AXButton`, `AXStaticText`,
`Luno`, `close button`, `full screen button`, `minimize button`, and enabled
state values. This is direct native AX evidence for the observed shell window,
not a complete route-by-route accessibility audit.

## Native Attempts

All attempts used this exact application command with only the isolated
environment values varying:

```text
npm run tauri -- dev --no-watch
```

1. `/private/tmp/luno-gate6-t4m0g0t5`: wrapper ran 45.76 seconds, timed out,
   returned `-15` after process-group termination. The binary reached
   `target/debug/luno-desktop`; `screencapture -x .../native-default-screen.png`
   returned `0`, and the AX window query returned `0` with `Luno, 166, 56,
   1180, 800`. Initial group termination left descendants, which were then
   explicitly terminated; a follow-up process scan was empty.
2. `/private/tmp/luno-gate6-close-vwyoddbz`: AX geometry query returned `0`,
   `Luno, 166, 56, 1180, 800`; AX click on the native close button returned
   `0`; wrapper returned `0` without timeout. The post-run process scan found no
   matching Luno, Vite, or Tauri process.
3. `/private/tmp/luno-gate6-ax-b89ra6nf`: top-level AX query returned `0` and
   wrote 287 bytes; native close-button click returned `0`; wrapper returned
   `0` without timeout. The post-run process scan found no matching process.
4. `/private/tmp/luno-gate6-geometry-38lziroq`: default AX size was `1180,
   800`; wide request/read was `1346, 866`; minimum request/read was `960, 660`;
   close-button click and wrapper both returned `0` without timeout. The
   post-run process scan was empty.

The close runs are direct launched close-event/process-cleanup observations for
the idle native window. They do not prove cancellation/join behavior while
scan, download, preview, artwork, or video work is active. Rust shutdown policy
and unit tests are recorded separately from this launched close proof.

## Visual, Accessibility, And Shutdown Matrix

`Policy/source PASS` is not rendered evidence. Only the rows explicitly marked
native-observed use the screenshot or AX paths above.

| Area | Evidence | Classification |
|---|---|---|
| Default empty shell at `1180x800` | Real native window and screenshot | PASS: limited native visual observation |
| Native window identity/geometry | macOS AX returned `Luno`, position, and `1180x800` | PASS: limited native AX observation |
| Top-level native roles/names/enabled values | Real AX query returned buttons, static text, close/full-screen/minimize controls | PASS: limited AX observation |
| Minimum resize `960x660` | Real AX size read after resize | PASS: geometry only |
| Wide resize `1600x1000` | Display constrained result `1346x866` | INCONCLUSIVE |
| Three-column proportions, row height, truncation, clamping | Source/configuration only; no full measurement | BLOCKED |
| Populated library, queue/player, loading/error/unavailable states | Not rendered with controlled content/state matrix | BLOCKED |
| Downloader, Discover, Song Page/video, Settings, Error Log | Not exercised in native run | BLOCKED |
| Keyboard traversal, focus order/visibility, activation | No keyboard automation run | BLOCKED |
| Complete accessible names/roles/values/announcements | Top-level AX only; no complete tree or state traversal | BLOCKED |
| Idle native close and process cleanup | Real close-button automation; wrapper exit 0; post-run process scan empty | PASS: bounded idle close proof |
| Active-work cancellation/join and temporary cleanup | Rust policy/unit evidence only; not launched during active work | BLOCKED |

## Required Deterministic Checks

| Exact command | Result | Classification |
|---|---|---|
| `npm run typecheck` | exit `0`; `tsc --noEmit` | PASS: source check only |
| `npm test` | exit `0`; 6 files, 34 tests passed | PASS: frontend automated check only |
| `npm run build` | exit `0`; 55 Vite modules transformed; warning that `dist` is outside project root and was not emptied | PASS: build only, not runtime evidence |
| `CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml` | exit `0`; 89 passed, 0 failed, 1 ignored; 0 doc tests | PASS: Rust automated check only; ignored test is interactive macOS Keychain |
| `npm run tauri -- --version` | exit `0`; `tauri-cli 2.11.4` | PASS: tooling only |
| `git diff --check` | exit `0`; no output | PASS: whitespace hygiene |

Cargo emitted three existing warnings: unnecessary `mut` in
`src-tauri/src/migration/first_run.rs:777`, unused `error` in
`src-tauri/src/download/sync.rs:157`, and unused Keychain function
`SecKeychainItemModifyAttributesAndData` in `src-tauri/src/migration/keychain.rs:74`.
The native runs repeated the same three warnings. No warning was promoted to a
visual/accessibility defect.

## Tool Availability

Exact probe:

```text
which screencapture; which osascript; which cliclick; which xdotool; which chromium; which google-chrome; which playwright
```

The probe returned exit `1` because some commands were unavailable:

```text
/usr/sbin/screencapture
/usr/bin/osascript
cliclick not found
xdotool not found
chromium not found
google-chrome not found
playwright not found
```

macOS `screencapture` and `osascript` were usable. No Chromium/Chrome,
Playwright CLI, cliclick, or xdotool runner was available. The native visual
and AX observations used only the real Tauri window plus macOS tools; no Vite
HTML probe was treated as native evidence.

## Final Worktree Hygiene

- Final `git diff --check`: exit `0`.
- Existing Gate 2-5 modified/untracked files remain present; no unrelated file
  was reverted or cleaned.
- Generated `dist/` output is ignored; Gate 6 fixture, cache, AX, launch, and
  source screenshot files were outside the repository. The durable screenshot
  artifact is `docs/verification/artifacts/gate6-native-default-shell.png`, a
  verified `3024x1964` full-screen PNG containing the native Luno window
  observed at AX geometry `1180x800`.
- The artifact's source provenance remains `/private/tmp/luno-gate6-t4m0g0t5/native-default-screen.png`.
- No commit was made, as requested.

## Final Classification

**INCONCLUSIVE / BLOCKED, not a formal visual/accessibility parity PASS.**
This run provides genuine, bounded native evidence for the empty default shell,
top-level AX exposure, minimum-size measurement, and idle close cleanup. The
missing boundary is complete visual/accessibility coverage across required
routes/states and keyboard/focus behavior, plus active-work shutdown evidence;
those gaps prevent a formal Gate 6 PASS.

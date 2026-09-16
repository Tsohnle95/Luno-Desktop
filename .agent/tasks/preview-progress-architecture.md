# Task: Truthful responsive preview progress

Status: REVIEWED

## Goal

Make recommendation preview preparation responsive and report truthful transfer progress without competing speculative yt-dlp workloads.

## Acceptance criteria

- [ ] Loading a song does not launch six concurrent video-search yt-dlp processes alongside preview preparation.
- [ ] Preview source resolution gets priority over speculative video discovery.
- [ ] The UI distinguishes resolving/preparing from byte-transfer percentage; it never presents 0% as a stalled byte download.
- [ ] During transfer, helper percentages are displayed unchanged.
- [ ] A completed preview becomes ready/playable and queued successors continue.
- [ ] Normal video staging, durable downloads, cancellation, and temporary-file cleanup remain functional.

## Relevant context

- `docs/modules/downloads-discovery.md`
- `docs/modules/playback.md`
- `src-tauri/src/video/runtime.rs::find_sources_cancellable`
- `src-tauri/src/runtime/preview_worker.rs::prepare_preview_with_progress`
- `src-tauri/src/state.rs` preview/video task orchestration
- `src/components/Recommendations.tsx`
- `src/components/VideoStage.tsx::DownloadRing`

## Invariants / constraints

- Rust owns preview/video scheduling and lifecycle policy.
- Percent means media bytes transferred, not startup, search, or encoding phases.
- Heavy processes remain off the UI thread and cancellable.
- Preview assets remain disposable and outside the library.
- Do not change the pinned managed binaries or transfer contract.

## Affected surfaces

- Video source search concurrency and cancellation.
- Preview progress event shape/state semantics.
- Recommendation status and ring rendering.
- Integration tests for process admission and phase transitions.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Remove six-process video search fan-out; establish bounded/cancellable lookup | complete | video runtime tests + live process count | pending |
| 2 | Separate preview phase from transfer percentage | complete | parser/fast-helper tests + frontend tests | pending |
| 3 | End-to-end live preview and cumulative validation | active | canonical gates and bundle verification passed; live song interaction pending | pending |

## Validation plan

- Targeted checks: video runtime lookup tests, preview worker/session tests, bridge and recommendation component tests.
- Integration/platform/manual checks: fresh app, one song, process tree shows bounded yt-dlp workload; preview status advances resolving → downloading percentage → ready.
- Canonical/final gate: `npm run verify`, `cargo test --manifest-path src-tauri/Cargo.toml`, `npm run build`.

## Decisions

- The current 0→100 behavior is truthful for byte transfer but misleading for the long source-resolution phase.
- Six concurrent video searches are speculative workload and must not contend with interactive preview preparation.
- The managed yt-dlp cold start emits no output for roughly 18 seconds.
  Source resolution is indeterminate; numeric progress begins only when
  yt-dlp reports transferred bytes. The user explicitly chose speed over
  throttling, so small files may legitimately jump directly to 100%.
- Real yt-dlp `_percent_str` output is left-padded; parsing must trim both sides before numeric conversion.
- Preview preparation cancels and gates speculative video staging; video work resumes only after the serialized preview window settles.
- Plan reviewed against concurrency, cancellation, existing backend ownership, and live observability requirements.

## Open risks / blockers

- YouTube may complete small audio transfers between progress samples; the UI must still show phase text rather than invent intermediate percentages.
- Live song selection is the remaining direct validation; it requires user interaction in the installed app.

## Completion

- [ ] Acceptance criteria satisfied
- [ ] Cumulative validation passed
- [ ] Canonical durable truth updated
- [ ] Future work transferred to backlog/issues
- [ ] Final Git state reviewed

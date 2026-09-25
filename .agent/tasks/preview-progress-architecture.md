# Task: Reliable recommendation playback and Discovery policy

Status: REVIEWED / ACTIVE

## Goal

Make recommended songs reliably ready before selection, keep recommendation
playback uninterrupted when Discovery is toggled, and make queue navigation
follow the selected policy: recommendations while Discovery is on, physical
playlist while it is off.

## Evidence

- The live helper can exit successfully without the expected file when user
  yt-dlp configuration changes download behavior; the shipped helper reports
  `download produced no output file`.
- The preview worker bypasses the durable downloader's isolated `HOME`, HTTP
  403 HLS retry, and Unix process-group cancellation.
- `discover_enable` invalidates the focused preview and stops/clears its audio;
  it also cancels the library heartbeat during prefetch. `discover_disable`
  stops and reloads an audible recommendation from the physical seed.
- Preview audio is a temporary overlay: `current_track` can be a recommendation
  while the authoritative physical queue remains on its seed. Discovery must
  anchor its projection to both identities.

## Acceptance criteria

- [ ] Preview yt-dlp behavior is independent of user config and stale archive
  entries; first-window failures retry supported 403/no-output cases once and
  report a bounded actionable error otherwise.
- [ ] Cancelling preview preparation terminates its helper process tree and
  cannot publish a stale asset or leave readers waiting on an orphan child.
- [ ] The initial five recommendation assets are prefetched serially; tapping a
  ready one starts without a second source lookup/download.
- [ ] Enabling or disabling Discovery does not stop, reload, or seek the
  currently audible song.
- [ ] With Discovery on, natural completion and Next/Previous walk the ordered
  recommendation projection from the currently audible recommendation.
- [ ] With Discovery off, Next/Previous and natural completion use the physical
  playlist even while a recommendation is audible.
- [ ] Queue snapshots, recommendation readiness, explicit selection/retry, and
  existing prepared-asset reuse remain truthful through toggles.

## Invariants / constraints

- Rust owns acquisition, queue policy, audio state, and cancellation.
- The physical queue remains the restoration source; a recommendation may be
  the audible track while the physical current item remains its seed.
- Discovery is a transport-policy projection, not an audio command. Toggling it
  preserves the backend stream, playback clock, and audible identity.
- Preview assets remain temporary and outside the library until explicitly
  saved. Store/session generations reject stale completions.
- Do not change the transfer contract or pinned managed binaries.
- Percentages continue to represent media bytes; source lookup stays
  indeterminate and a file is playable only after atomic publication.

## Affected surfaces

- `src-tauri/resources/bin/luno-download-helper`
- `src-tauri/src/runtime/preview_worker.rs`
- `src-tauri/src/download/workflows.rs` or shared process supervision
- `src-tauri/src/preview/discover_mode.rs`
- `src-tauri/src/state.rs` preview/discovery handoff and queue policy
- `src-tauri/src/gate4_tests.rs`, `gate5_tests.rs`, and focused unit tests
- `docs/modules/downloads-discovery.md`, `docs/modules/playback.md`

## Adversarial plan review

- **Can queue.current identify the song that is sounding?** No. Focused
  recommendation playback leaves the physical queue at the library seed, so
  enable must read `current_track` separately and pass it into the projection.
- **Can a toggle cancel the audio heartbeat indirectly?** Yes. Keep the active
  heartbeat alive across a policy switch; once it completes, the ordinary
  `next_track` path observes the current Discovery policy and moves accordingly.
- **Can a projection rebuild reset Previous/Next?** Yes. Preserve the live track
  id when recalculating its index, with the audible recommendation placed
  between the captured physical prefix and suffix.
- **Could both modes race for an in-flight download?** Yes. Invalidate and
  cancel only preview preparation on enable, retain published assets, then let
  Discovery adopt them. Do not leave two active preparation drivers.
- **Will off mode accidentally autoplay recommendations?** It must invalidate
  Discover and restore the physical queue without stopping the overlay; the
  existing physical Next path handles keys and natural completion.
- **Can 403 fallback overwrite a usable first output?** Only retry when the
  first attempt failed or produced no nonempty target; retain monotonic progress
  and clean failed temporary output before retry.
- **Are packaged runtimes required for this repair?** No binary changes are in
  scope. Enable Node only when already discoverable, keep yt-dlp's default
  supported runtime behavior otherwise, and retain the HLS retry.
- **Can errors strand the UI?** Failure still becomes an unavailable row and
  releases the serialized driver to continue through the rest of the first
  window; test the reported missing-output path directly.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Deterministic, retryable, process-tree-safe preview helper path | active | helper argv/no-output/403/cancellation tests | pending |
| 2 | Discovery projection anchored to audible recommendation; non-interrupting toggles | pending | DiscoverMode and state policy tests | pending |
| 3 | Integration, canonical docs, cumulative validation, live interaction | pending | `npm run verify`; `cargo test --manifest-path src-tauri/Cargo.toml`; `npm run build`; dev app smoke | pending |

## Validation plan

- Test helper argv contains `--ignore-config`, no ambient archive/config flags
  can suppress output, 403/no-output fallback works once, empty output fails,
  and cancellation kills a spawned descendant.
- Test queue projection/current index for a focused recommendation and key
  policy after enable/disable, including preservation of physical restoration.
- Run the canonical desktop gates under Node 22 and Rust stable.
- Exercise one recommended tap, enable/disable during playback, natural end,
  and Next/Previous in the dev app when live media access is available.

## Completion

- [ ] All acceptance criteria satisfied
- [ ] Cumulative validation passed
- [ ] Canonical durable truth updated
- [ ] Final Git state reviewed

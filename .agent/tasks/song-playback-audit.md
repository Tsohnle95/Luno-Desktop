# Task: Song download and temporary playback audit

Status: IMPLEMENTING — validation pending

## Goal and observed failures

Repair the Spaceboy download/playback path and the song-page recommendation,
video, artwork, queue, history, and discovery interactions. The user observed
a long pause at 95%, a microphone permission prompt, no video progress,
recommendations that did not work reliably, a prepared song downloading again
when clicked, wrong fallback art, wrong Next Up/history context, and prepared
songs disappearing when discovery mode was enabled.

## Confirmed starting facts

- `run_download_helper` maps yt-dlp transfer progress to 0–95%, then reports
  97% after the helper exits; extraction, validation, promotion and rescan may
  occupy the apparent 95% pause. Diagnose actual elapsed phases before changing
  the percentage contract.
- Playback uses `cpal::default_host().default_output_device()` and
  `build_output_stream`; no microphone request appears in app source. Audit the
  packaged app, macOS/Tauri/WebKit permission path, and any playback side effect
  before assigning a cause or requesting any capture permission.
- `Shell` auto-fetches song recommendations and calls `previewPrefetch`; the
  backend prefetch window is five. `preview_tap` currently cancels preview tasks
  and creates a new session; verify whether it preserves ready assets.
- `Shell` suppresses video staging while a focused preview is active and passes
  recommendation artwork to the video slot. `VideoStage` falls back to generic
  `Artwork`, whereas recommendation rows use `RecommendationArtworkImage`.
- Discover mode can adopt ready files from `PreviewAssetStore`, but enabling it
  hands off the preview session and changes the queue projection.

## Plans by issue

| Issue | Implementation plan | Acceptance / focused check |
|---|---|
| Download lingers at 95% | Time helper transfer, postprocess, signature validation, atomic promotion, and rescan for a real Spaceboy download. Keep percentages truthful; expose a phase or indeterminate finalizing state during long postprocess and ensure terminal success follows file availability. | UI explains finalization; no false 100%, and a completed file is playable. Workflow/UI tests plus one live download when available. |
| Microphone prompt on playback | Reproduce on the release bundle with macOS permission logs and inspect bundle plist, WebKit/Tauri capabilities, and audio initialization. Remove the exact input/capture trigger or erroneous entitlement while preserving output playback. | Playing a local song after denying microphone access works and raises no new mic prompt; no input stream is opened. Direct macOS check required. |
| Video lacks download/spinner | Trace `videoStage`, admission pressure, `video_state` event/response, song-page status mapping, and focused-preview suppression. Ensure a visible searching/downloading state while eligible staging runs; correlate state to audible identity and resume after preview contention. | On song page, video mode yields progress/phase or an explicit unavailable/error result, never a silent blank panel. Backend/frontend tests and live UI check. |
| First five recommendations / temporary playlist | Verify discovery fetch, key mapping, `previewPrefetch`, session window and snapshot publication. Make first five ready assets visible/clickable in a stable temporary list; keep failed/pending rows honest. | Five initial recommendations prepare without a tap; each ready row plays from its prepared asset. Session and song-page tests. |
| Clicking ready song downloads again | Follow `previewTap` through task cancellation, session generation, `PreviewAssetStore` lookup/adoption and source identity. Reuse valid staged media; only download when missing/stale, without interrupting playback. | A ready recommendation starts from the existing file with no second helper invocation; test prefetch→tap and tap between prepared rows. |
| Wrong giant fallback art | Use the same custom recommendation fallback in the large song-page stage as recommendation rows, including null/broken image URLs; preserve real track artwork when available. | A recommendation without art shows custom fallback in row, backdrop/stage, and player; no gray star rectangle. Component/visual check. |
| Wrong Next Up / recently played context | Audit authoritative queue/index and preview/discover projections plus recent-history ownership. Resolve recommendation IDs without shifting row indices; make current, next, and history reflect the audible session and origin context. | Queue and history follow library, focused preview, and discover mode through next/previous/tap/exit. Rust projection and shell tests. |
| Discovery hides prepared songs | Trace preview→discover handoff, ready-asset adoption, recommendation seeds/identity, and snapshot replacement. Preserve visibility and playability of already staged entries across toggle, and resume preparing the rolling window. | Toggle on after prefetch: ready rows remain present and play without redownload; toggle off restores physical queue. Integration test. |

## Execution sequence

1. Reproduce and instrument only the affected paths; record the actual cause
   of each symptom. Confirm which issues share a session/identity root cause.
2. Fix backend temporary-asset/session/discover handoff and queue/history
   invariants first. Add focused regression tests and commit a sound phase.
3. Fix frontend artwork, video/progress, recommendation and queue projections.
   Validate component behavior and commit.
4. Fix download finalization messaging and microphone trigger based on direct
   evidence. Validate on macOS where possible and commit.
5. Run `npm run verify`, `cargo test --manifest-path src-tauri/Cargo.toml`, and
   `npm run build` under Node 22/Rust stable. Review the entire diff, update
   only changed canonical truth, and report any live behavior not verified.

## Invariants and review

- Rust remains the owner of playback order, temporary assets, and video work.
- Temporary media stays outside the durable library until explicitly saved.
- Keep asset generations/cancellation safe; no stale worker may publish into a
  newer session. Keep heavy processes off the UI thread.
- Preserve library queue restoration, stable track keys, transfer v1, app ID,
  and managed binaries.
- Before implementation, challenge the plan against ready-asset lifetime,
  key collisions, focused-preview versus discover ownership, event ordering,
  macOS permission provenance, and whether recent history includes temporary
  tracks by design. Revise this file when evidence resolves an assumption.

## Adversarial review and resolved implementation decisions (2026-09-18)

- The task is a `PROJECT`/`BATCH`: preview/discover handoff, queue identity,
  video staging, and platform playback cross subsystem boundaries. Work is
  split into independently testable backend session/queue fixes, frontend
  projection/artwork/video fixes, and download/platform evidence.
- Preview files are generation guarded during writes, but final `.aac` files
  are intentionally retained across a session handoff. A tap must adopt a
  valid existing file before scheduling a worker; cancellation alone must not
  be treated as proof that a file is missing. Regression coverage will model a
  prefetch completion followed by a tap and assert no second preparation.
- Discover projects pending recommendation ids into the playback queue, but
  its disable path clears the queued projection before the handoff can
  preserve ready assets. The fix will retain ready assets through disable or
  handoff and make the next session adopt them by stable `previewKey` identity.
  Pending entries remain visible and are prepared when selected.
- Queue rows resolve recommendation ids in `Shell`. Focused preview playback
  now records its temporary key in the in-memory recent event stream, and the
  frontend resolves it from the active recommendation metadata map. Recent
  history is not persisted or restored by this app, so staged recommendation
  files and their keys cannot become restart ghosts; rows disappear when their
  metadata leaves the active recommendation set.
- `VideoStage` has a loading ring and `video_state` carries progress, but the
  song-page effect returns while a focused preview is active and loading with
  no source falls through to generic artwork. The fix will keep an explicit
  loading state correlated to the audible identity, show the spinner while
  source resolution/download is active, and avoid staging a library video
  underneath focused preview audio.
- Recommendation rows use a deterministic custom fallback, while
  `VideoStage` uses `Artwork`'s generic fallback. The stage will receive the
  same recommendation fallback metadata/component for null or failed URLs.
- The 95% download plateau is intentional helper transfer mapping. The
  coordinator emits no progress while validation/promotion/rescan runs, so
  the UI cannot distinguish finalization from a stalled transfer. Preserve
  truthful percentages and add an explicit finalizing state only when the
  Rust event and TypeScript consumer can change together; otherwise improve
  finalizing labeling and terminal ordering with focused tests.
- Source inspection found only cpal output construction (`default_output_device`
  plus `build_output_stream`) and no input stream. Microphone behavior cannot
  be safely fixed from source inspection alone; inspect packaged entitlements
  and perform a direct macOS reproduction before changing capabilities. If no
  app trigger is found, record it as an external WebKit/macOS observation.
- No transfer v1, app identifier, managed binary pin, or library persistence
  shape will change. Any new event field must be added to both Rust and the
  TypeScript validator with compatibility coverage.

## Implementation status (2026-09-18)

- Download finalization is explicit in the job contract and UI, with an
  indeterminate handoff state after helper transfer.
- Preview/discover handoff adopts existing ready assets, publishes prepared
  identities, and has a helper-call-count regression test covering the
  prefetch → discover → focused tap race.
- Focused/discover queue and recent projections preserve recommendation
  identity and unresolved row positions; the video surface remains in loading
  mode while a source is being staged; recommendation artwork is the shared
  fallback for the large stage.
- Microphone provenance remains unverified on a live macOS bundle because the
  source audit found no input stream or capture request.

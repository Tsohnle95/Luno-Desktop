# Task: Make recommendation playback one coherent session

Status: REVIEWED

## Goal

Replace the conflicting focused-preview and discovery lifecycles with a
coherent recommendation-session architecture whose assets, work, navigation,
errors, queue projection, and playlist saves remain truthful across mode
changes.

## Acceptance criteria

- [ ] The first five song recommendations prepare serially and remain cached
      for the active seed until the seed changes or the app shuts down.
- [ ] Clicking a prepared recommendation plays its existing asset without a
      helper invocation or a resolving spinner.
- [ ] With discovery off, Previous/Next from a focused recommendation exits to
      the captured physical queue and applies the requested physical movement.
- [ ] With discovery on, Previous/Next walk the recommendation projection in
      both directions without resetting to the seed when readiness changes.
- [ ] A preparation failure reaches a terminal per-row state, records a useful
      bounded error, releases its task slot, and can be retried.
- [ ] Discovery continues past an unavailable recommendation and never parks
      indefinitely on a failed row.
- [ ] Enabling/disabling/tapping across modes preserves valid prepared assets
      and restores the exact physical queue and index.
- [ ] Any recommendation can be added to an existing or newly created playlist;
      prepared audio is reused and unstaged audio uses the durable download path.
- [ ] Queue, footer, Recent, song-page art/video, and recommendation rows agree
      on the audible identity throughout these transitions.

## Relevant context

- `docs/modules/downloads-discovery.md`
- `docs/modules/playback.md`
- `docs/modules/contracts.md`
- `src-tauri/src/preview/session.rs`
- `src-tauri/src/preview/discover_mode.rs`
- `src-tauri/src/preview/assets.rs`
- `src-tauri/src/runtime/preview_worker.rs`
- `src-tauri/src/state.rs`
- `src/app/Shell.tsx`
- `src/components/Recommendations.tsx`
- `src-tauri/src/gate2_tests.rs`
- `src/test/views/song_page.test.tsx`

## Invariants / constraints

- Rust owns recommendation session state, physical queue capture, navigation,
  asset readiness, and worker admission. The frontend renders snapshots.
- The asset store owns completed temporary files independently from a focused
  or discovery presentation mode. Routine navigation never deletes valid
  assets for the same seed.
- Exactly one serialized preparation pipeline writes the shared asset store.
  Every accepted job has a terminal completion and releases its in-flight key.
- Queue projection changes preserve the currently selected track by identity;
  restoration uses the captured physical queue and index.
- A failed or cancelled worker cannot publish into a newer generation.
- Temporary assets remain outside the library; only explicit Save promotes or
  durably downloads into the chosen playlist.
- Heavy helper/ffmpeg work stays off the UI thread and is cancellable.

## Affected surfaces

- Recommendation session state and worker orchestration in `AppState`.
- Focused and discovery navigation and physical-queue restoration.
- Preview helper diagnostics, fallback source resolution, timeouts, and retry.
- Preview/discovery snapshot truth and frontend row projection.
- Existing/new playlist save flow from song-page recommendations.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Reproduce lifecycle, navigation, failure, and save regressions in backend tests | complete | targeted `gate2_tests` + state-machine tests | pending |
| 2 | Consolidate asset/work ownership and repair navigation/projection invariants | complete | focused Rust integration tests | pending |
| 3 | Make helper failures terminal, diagnostic, retryable, and able to continue | complete | preview-worker + failure-path tests | pending |
| 4 | Align frontend status/actions and verify playlist picker/create/save flow | complete | song-page/shell/bridge tests | pending |
| 5 | Live/helper smoke, canonical gates, docs, cumulative review | active | direct helper, `npm run verify`, Cargo, build | pending |

## Validation plan

- Targeted: prefetch → play → Previous → replay prepared; discovery Next and
  Previous before/after rolling-window expansion; failed helper → terminal row
  → retry; prepared and unstaged Save into existing/new playlist.
- Integration: run the shipped helper contract from a space-containing bundle
  path and, where network is available, a real recommendation preparation.
- UI: component tests for terminal errors, ready reuse, picker availability,
  save state, and coherent current/queue labels.
- Canonical: `npm run verify`, `cargo test --manifest-path
  src-tauri/Cargo.toml`, `npm run build` under the pinned toolchains.

## Decisions

- Prepared files are session assets, not mode assets. Focused playback and
  discovery are presentation/navigation policies over the same cache.
- Physical transport commands exit focused playback atomically. Previous and
  Next apply to the captured physical index; if no previous exists, Previous
  restarts/restores the seed instead of stopping silently.
- Discovery queue updates locate the audible/current id in the new projection;
  they never reuse the original physical index after playback enters the block.
- The preparation registry removes terminal keys. Retry is explicit and cannot
  be suppressed by a stale `raised` entry.
- Production helper lookup uses an explicit yt-dlp search source and reports a
  sanitized tail of stderr. Generic `Failed("preview download helper failed")`
  is insufficient for diagnosis or recovery.
- Save remains explicit per playlist and is available regardless of preview
  readiness; ready files remux, missing files enqueue the durable downloader.

## Adversarial plan review

- A full wire-version change is unnecessary: current additive snapshot fields
  can express readiness/failure. Change contracts only if tests prove a phase
  cannot be represented truthfully.
- Replacing both public modes with a brand-new wire command would expand risk
  without improving ownership. Consolidate backend asset/work ownership while
  retaining command compatibility.
- Cache retention must stay bounded. Files are cleared when a genuinely new
  recommendation seed replaces the session and at shutdown/startup, not on a
  transport button or mode handoff.
- Queue restoration and worker cancellation must be tested together; testing
  state machines alone would miss AppState lock/order/event races.
- Network behavior is not deterministic. Production-source fallback and stderr
  tests use controlled helpers; a live smoke is supporting evidence only.

## Open risks / blockers

- A live provider can reject an otherwise valid recommendation; the required
  behavior is terminal, actionable, retryable failure rather than guaranteed
  provider availability.
- Direct audible playback checks are macOS-only; backend queue and helper flows
  remain testable in the canonical suite.

## Completion

- [ ] Acceptance criteria satisfied
- [ ] Cumulative validation passed
- [ ] Canonical durable truth updated
- [ ] Future work transferred to backlog/issues
- [ ] Final Git state reviewed

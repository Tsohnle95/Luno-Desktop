# Task: Deliver viewable video tiers, true app fullscreen, and dual recommendation prefetch

Status: REVIEWED

## Goal

Make current and recommended songs arrive with useful video quality on time: publish a viewable first tier, prioritize a 1080p replacement, cover the full app in fullscreen, and prefetch both audio and video for the five recommendation items.

## Acceptance criteria

- [ ] The interactive first video tier targets a genuinely viewable resolution instead of 180/240p, while preserving fast first-frame behavior and H.264/AAC compatibility.
- [ ] The final tier requests the best available H.264 video up to 1080p and replaces the initial tier in place.
- [ ] The current track's final-quality upgrade has priority over speculative successor video work, so five background prefetches cannot delay it for minutes.
- [ ] Fullscreen covers the complete Luno app viewport, including the topbar and footer regions; only video and hover controls remain visible, and Escape exits reliably.
- [ ] The initial five recommendation preparations schedule both temporary audio and quick video assets. Clicking a fully prepared recommendation does not begin its first video download.
- [ ] Audio remains the playback readiness requirement, while video preparation progresses independently and failures do not make playable audio unavailable.
- [ ] Focused recommendation and Discover contexts keep their existing queue, cancellation, generation, and five-successor ownership rules.
- [ ] Existing truthful progress, quick-to-high revision swapping, and bounded temporary storage remain intact.

## Relevant context

- `docs/modules/playback.md`: video staging, quality tiers, prefetch ownership, fullscreen.
- `docs/modules/downloads-discovery.md`: five-item temporary preview window and task scheduling.
- `src-tauri/src/video/runtime.rs`: format ladders, `prefetch_track`, `upgrade_staged_video`.
- `src-tauri/src/state.rs`: current staging, upgrade priority, preview completion, `kick_video_pipeline`.
- `src/components/VideoStage.tsx`, `src/styles/layout.css`, `src/app/Shell.tsx`: fullscreen containment and shell layers.
- `src-tauri/src/state.rs`, `src-tauri/src/gate5_tests.rs`, `src/test/views/video_stage.test.tsx`: regression coverage.

## Invariants / constraints

- Rust remains authoritative for staging, prioritization, temporary asset ownership, and cancellation.
- Current-track interactive work wins over speculative work; no heavy download blocks command/UI threads.
- The first tier must remain materially smaller/faster than 1080p and use a WebKit-playable codec path.
- Final quality is best available up to 1080p; lower source quality degrades naturally without fabricating resolution.
- Fullscreen stays inside the app window and must not invoke the prior OS fullscreen path that could black out WKWebView.
- Audio and video have independent failure states; a video miss cannot invalidate a prepared audio preview.

## Affected surfaces

- Video format selection, cache tier semantics, and current-upgrade scheduling.
- Recommendation preparation orchestration and video prefetch admission.
- VideoStage fullscreen state/CSS and shell layout visibility.
- Focused Rust/React tests, live format probes, and canonical module documentation.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Audit quality ladders, upgrade lock ordering, fullscreen selectors, and recommendation preparation handoff | complete | source/doc trace and adversarial plan review | pending |
| 2 | Raise quick/final quality and guarantee current 1080p upgrade priority | pending | ladder/unit tests plus live selected-format probe | pending |
| 3 | Make fullscreen cover the whole app with hover-only controls | pending | component/CSS tests and rendered/browser inspection | pending |
| 4 | Couple each recommendation audio preparation with quick-video prefetch without coupling failure | pending | state/session integration tests covering initial five and click reuse | pending |
| 5 | Integrate, update docs, run live/cumulative validation and canonical gates, review diff | pending | `npm run verify`; `cargo test --manifest-path src-tauri/Cargo.toml`; `npm run build` | pending |

## Validation plan

- Targeted checks: format ladder selection and tier persistence; upgrade-versus-prefetch ordering; recommendation audio/video scheduling and reuse; fullscreen DOM/CSS behavior.
- Integration/platform checks: bundled yt-dlp format probe for the reported title or a stable fixture; live quick/high staging where practical; browser/rendered fullscreen inspection.
- Canonical gate: `npm run verify`, `cargo test --manifest-path src-tauri/Cargo.toml`, `npm run build`.

## Decisions

- Use 480p as the interactive target and 1080p as the final ceiling. This keeps a meaningful size gap while removing the visibly unusable 240p tier.
- Give the audible track's high-quality replacement priority immediately after the first playable tier. Successor quick downloads resume after the current upgrade releases heavy-work ownership.
- Drive recommendation video preparation from the recommendation window rather than the physical playback queue, because focused mode intentionally keeps recommendations outside that queue.
- Treat audio readiness and video readiness as parallel facets. A row can become playable when audio publishes; its video can continue preparing, and a video failure stays local to video.
- Fullscreen uses a shell-level CSS state/class so grid rows cannot reserve header/footer space behind a fixed descendant.

## Open risks / blockers

- Some sources do not offer H.264 at 1080p; the final ladder must choose the best compatible lower resolution.
- Five audio plus five video preparations can create network and task pressure. Scheduling must remain bounded and prioritize current playback.
- CSS `:has()` behavior in WKWebView needs direct inspection; a React-owned shell class is the fallback if selector containment is unreliable.

## Completion

- [ ] Acceptance criteria satisfied
- [ ] Cumulative validation passed
- [ ] Canonical durable truth updated
- [ ] Future work transferred to backlog/issues
- [ ] Final Git state reviewed

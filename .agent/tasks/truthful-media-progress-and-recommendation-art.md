# Task: Make media progress truthful and recommendation artwork reliable

Status: ACTIVE

## Goal

Show byte-transfer progress only while bytes are actually downloading, keep source preparation visibly indeterminate, and ensure every missing or provider-placeholder recommendation image uses Luno's generated artwork on every playback surface.

## Acceptance criteria

- [ ] Video lookup/extractor/setup time does not appear as a frozen 0%; the UI remains indeterminate with accurate preparation copy until yt-dlp reports measurable transfer bytes.
- [ ] Video transfer progress is derived from downloaded and total bytes, remains bounded and monotonic across separate video/audio streams, and reaches 100 only when the quick playable artifact is published.
- [ ] Recommendation audio preparation uses the same phase rule: no fabricated 0% during helper startup/source resolution, and visible percentage updates reflect yt-dlp media bytes when a total is available.
- [ ] A provider placeholder URL is treated like missing artwork. Recommendation rows, queue/player projections, song backdrop, and giant media stage render the same deterministic Luno fallback.
- [ ] No generated recommendation design resembles the deprecated neutral gray field with a white star/cross.
- [ ] Existing cancellation, timeout, quick-first/high-quality video replacement, temporary asset publication, and five-item recommendation window behavior remain intact.
- [ ] A live managed-tool probe records where the delay occurs and confirms progress output during a real transfer, or the unverified platform boundary is reported explicitly.

## Relevant context

- `docs/modules/playback.md`: video staging and progress semantics.
- `docs/modules/downloads-discovery.md`: preview progress and recommendation artwork.
- `src-tauri/src/video/runtime.rs`: `VideoProgress`, `download_ladder`, staging tests.
- `src-tauri/src/runtime/preview_worker.rs`: helper output parsing and preview progress.
- `src/components/VideoStage.tsx`: video phase/progress presentation.
- `src/components/RecommendationArtwork.tsx`: placeholder detection and generated designs.
- `src/app/Shell.tsx`, `src/views/SongPageView.tsx`, queue/player components: recommendation track projections and artwork consumers.

## Invariants / constraints

- Rust remains authoritative for external process phases and media publication; React renders snapshots without reconstructing downloader policy.
- Heavy yt-dlp/FFmpeg work remains off the UI thread, with cancellation and timeout draining both output pipes.
- A numeric percentage represents a known byte ratio. Unknown totals and preparation phases remain indeterminate.
- Preview assets remain temporary and become playable only after atomic publication.
- Quick video remains the first playable tier; the high-quality pass must not reset current quick-tier progress or delay first frame.
- Placeholder recognition is centralized and deterministic so all UI surfaces choose the same image for the same recommendation.

## Affected surfaces

- Video downloader progress protocol/parser and its state publication.
- Preview helper progress protocol/parser and recommendation row state.
- Video and recommendation progress UI labels/rings.
- Recommendation artwork normalization and all recommendation playback projections.
- Focused Rust and React tests plus canonical module documentation.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Audit exact yt-dlp/helper output timing, write reviewed plan, identify every placeholder-art consumer | complete | source/doc trace and plan review | `7b83a632` |
| 2 | Implement truthful video byte progress and phase presentation | complete | 10 runtime tests, 19 VideoStage tests, live plan + 27 byte-event probe | `1208b33a`, `5b9529d0`, `7b6fcf92`, `63981c3b` |
| 3 | Implement truthful recommendation preview byte progress | complete | preview worker/session/helper tests and live bundled preview | `0f344ffe` |
| 4 | Normalize provider placeholder artwork and remove gray/star-like generated designs across playback surfaces | complete | 62 focused frontend tests including exhaustive artwork policy | `315335bb` |
| 5 | Integrate, update durable docs, run cumulative and canonical validation, review all diffs | active | `npm run verify`; `cargo test --manifest-path src-tauri/Cargo.toml`; `npm run build` | pending |

## Validation plan

- Targeted checks: Rust tests around byte protocol parsing, multi-stream monotonicity, publication completion, preview worker failures/cancellation; React tests for indeterminate/determinate copy and placeholder fallback on all song surfaces.
- Integration/platform/manual checks: timestamp raw managed yt-dlp output for a quick video and managed preview download; inspect rendered fallback markup for a known Last.fm placeholder.
- Canonical/final gate: `npm run verify`, `cargo test --manifest-path src-tauri/Cargo.toml`, `npm run build` under the pinned toolchains.

## Decisions

- Treat this as a PROJECT because progress truth spans two external-process protocols and frontend state, while artwork spans provider data and several projections.
- Keep lookup/preparation indeterminate. A process having spawned is not evidence that media bytes have started, so it cannot truthfully establish 0%.
- Use machine-readable byte fields rather than parsing formatted percent strings. Separate yt-dlp streams must be combined without backwards jumps; final 100 belongs to successful playable publication.
- Normalize recommendation placeholder URLs at the shared recommendation-art boundary and at track projection boundaries, so valid library artwork is unaffected.
- Run the three implementation audits in separate file ownership domains, then integrate and review centrally.

## Open risks / blockers

- Live provider timing varies; the probe validates event ordering and semantic truth, not a fixed performance threshold.

## Completion

- [ ] Acceptance criteria satisfied
- [ ] Cumulative validation passed
- [ ] Canonical durable truth updated
- [ ] Future work transferred to backlog/issues
- [ ] Final Git state reviewed

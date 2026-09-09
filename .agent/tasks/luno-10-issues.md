# Luno 10-issues batch — triage + plan

## Triage (one pass, 2026-09-09)
Clusters:
- A Playback queue/history: #1 shuffle-back, #9 progress stuck, #11 next/autoplay dead after rec click
- B Artwork/search: #2 search no image, #4b queue art missing, #10 placeholder text
- C Mutations: #3 delete via right-click, #5 favourites all views, #8 remove large fav button
- D Video: #4a video-mode off spinner
- E Discovery: #6 discover mode missing, #7 rec preload/click-to-play (+#9/#11 overlap)

Classification:
- PATCH: #1, #2, #3, #4a, #4b, #5, #8, #9, #10, #11
- FEATURE: #6, #7 (new UI + contract additions for discover_enable/disable, snapshot handling)

Ordered execution (one unit at a time):
1. U10 placeholder (trivial PATCH)
2. U8 remove large fav button (trivial PATCH)
3. U5 favourites publish+persist (PATCH, backend)
4. U3 delete audit (PATCH)
5. U4a video-off spinner (PATCH, frontend)
6. U2+U4b artwork (PATCH, frontend LRU/window)
7. U1 shuffle audit (PATCH, verify + test)
8. U6+U7 discovery mode + rec preload (FEATURE, contracts+bridge+projection+UI)
9. U9+U11 preview progress/next restore (PATCH, backend heartbeat/restore)

Evidence so far:
- queue.rs previous() walks physical shuffled order; state.rs start_current_track avoids re-shuffle (comment 5443ff). Test at state.rs:640 covers skip-ahead-then-back.
- toggle_favorite (state.rs:5101) mutates in-memory library but never publishes library_snapshot nor persists to settings — frontend never sees change.
- delete_tracks (state.rs:5015) does remove + schedule_mutation_scan; frontend context menu delete path exists (Shell.tsx:737) with window.confirm.
- Shell.tsx:480 status logic keeps "loading" when mode off → spinner stays (VideoStage.tsx:289). Should hide when mode off.
- track_artwork hook CAP 80 + eviction listener deletes from requested but effect only re-runs on idsKey change → evicted visible ids stay blank.
- contracts.ts + bridge.ts lack discover_enable/disable/advance; backend has DiscoverMode (preview/discover_mode.rs) + dispatch arms (state.rs:4523-25); backend_projection.ts has no DiscoverSnapshot case → would throw "unhandled".
- preview audio (state.rs:2918 play_preview_audio) cancels playback-heartbeat, sets clock/current_track/status but publishes no playback_snapshot → footer position frozen; next/previous routing in Shell.tsx:724 only checks preview.active, not discover.active.
- SongPageView.tsx:101 large fav button to remove per #8; Shell.tsx:681 placeholder to change per #10.
- SongPage onPreviewRecommendation wiring truncated in earlier read — need to confirm Shell song-page instantiation passes previewTap for songRecs.

Validation per slice: npm run typecheck + relevant vitest + cargo test subset; final: npm run verify, cargo test, npm run build.

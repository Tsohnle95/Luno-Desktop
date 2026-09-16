# Task: Queue, discovery, preview identity, playlists, icon, error surface

Status: active
Mode: PROJECT (playback queue policy + preview/discovery state + release config)

## Outcome

Fix the reported desktop defects as independently verifiable units:

1. App bundle uses the Luno icon, not Tauri's default.
2. Video/art identity follows the AUDIBLE track, including recommended-song
   playback (no stale previous-song video/art).
3. Discovery mode's toggle is honest and its Next/Previous walk the temporary
   recommendation list, not the physical library queue.
4. Adding a track/recommended song to a playlist is visible immediately and
   does not silently no-op.
5. Playlist sorting/filtering works for every option (incl. Recently added).
6. Queue panel's Now playing / Next up reflect the authoritative backend index.
7. Preview preparation failures no longer spawn an undismissable global banner.

## Units

- U-A Icon: generate full Tauri iconset from `src-tauri/icons/icon.png`, wire
  `bundle.icon`, document in release doc, verify bundle check.
- U-B Error surface: backend `error` events dismissible; preview-preparation
  failure stops publishing a global error event (per-row state is authoritative).
- U-C Playlists: rescan after staged save; optimistic add + surface failures;
  "Recently added" sort + per-scope reset.
- U-D Queue authority: `track_started` must not override the authoritative
  `current_index`; queue snapshots preserve position/duration; remove-current
  audio consistency; `user_queue_count` on previous.
- U-E Recommendation identity: Shell passes the preview display track to the
  song page + video correlation; queue rows resolve recommendation keys so the
  index stays aligned.
- U-F Discovery transport: project pending recommendations immediately, prepare
  on select, honest toggle affordance.

## Validation

- `npm run typecheck`, `npm test`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `npm run build`
- Bundle icon present after `npx tauri icon` (manual/scripted check).

## Risks

- U-D/U-E/U-F touch authoritative queue order and preview/discover state; keep
  changes additive and covered by tests. Do not change the transfer v1 contract
  or `com.ty.luno`.

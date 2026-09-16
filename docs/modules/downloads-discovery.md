# Downloads, Discovery, Previews, and Managed Toolchain

Document role: canonical owner for the downloader, recommendations and
preview sessions, the bundled FFmpeg/yt-dlp toolchain, process
supervision, and the background task registry. It owns everything that
fetches from outside the local library.

## What this owns

- Download jobs: search, staging, validation, atomic promotion, retry,
  sync, CSV ingestion, URL queueing.
- Discovery (Last.fm) and recommendation artwork.
- Preview sessions: tap/next/previous/cancel, temporary assets isolated
  from the library until explicit save.
- Managed binaries (FFmpeg/ffprobe/yt-dlp) discovery and fail-closed
  behavior; process supervision.
- `TaskRegistry` capacity, cancellation, and shutdown behavior.

What it does not own: library snapshots and promotion targets
(`docs/modules/library.md`); release provisioning of the binaries
(`docs/release/desktop.md`).

## Entry points

- `src-tauri/src/download/` — `coordinator.rs` (`DownloadCoordinator`),
  `workflows.rs`, `promotion.rs` (`promote_output`, `CollisionPolicy`,
  audio-signature validation), `search.rs`, `sync.rs`, `csv.rs`,
  `markers.rs`.
- `src-tauri/src/discovery/` — `service.rs` (`DiscoveryService`,
  validated recommendation metadata), `artwork.rs`.
- `src-tauri/src/preview/` — `session.rs` (`PreviewSession`),
  `discover_mode.rs`, `assets.rs` (`PreviewAssetStore`), `save.rs`
  (`remux_and_promote`, `PlaylistDestination`).
- `src-tauri/src/process/` — `supervisor.rs` (`Supervisor`,
  `ProcessRegistry`), `executables.rs` (managed-binary discovery by kind).
- `src-tauri/src/runtime/` — `task_registry.rs` (`TaskRegistry`,
  `TaskSpec`, cancellation tokens), `preview_worker.rs`
  (`prepare_preview`), `shutdown.rs`, `gate5_smoke.rs`.
- `src-tauri/src/state.rs` — job registry, `start_download` containment,
  discovery-key state, preview/video toolchain holders.
- `src/views/DownloaderView.tsx`, `src/views/DiscoverView.tsx`,
  `src/components/DownloadTabs.tsx`, `src/components/Recommendations.tsx`.

## State ownership

- `AppState` owns download jobs, the process registry, the task registry,
  discovery key presence, preview sessions, and preview/video toolchain
  paths. Preview assets live in a temporary store outside the music
  library.

## Invariants

- Production downloads are M4A/AAC (`--audio-format m4a`; the reviewed
  FFmpeg has no MP3 encoder) — see
  `docs/decisions/download-format-m4a.md`. Promotion accepts MP4 `ftyp`
  plus ID3/MPEG signatures; destinations are `*.m4a` in production.
- Jobs publish truthful progress, validate staged media, promote
  atomically inside the configured library root, and trigger a coalesced
  rescan. Missing parents outside the root are rejected, never created.
- Download helper invocations receive the source (`scsearch1:`-style or
  URL); the bundled helper resolves via `resolve_bundle_helper`
  (resource dir → manifest dir). Retry reuses the stored job.
- The helper builds its yt-dlp argv without word-splitting so bundle paths
  containing spaces (e.g. `/Applications/Luno Desktop.app/…`) survive as a
  single `--ffmpeg-location` value; a split flag once starved yt-dlp of
  ffmpeg, failing every preview/download postprocess step (2026-09-09).
- Discovery metadata is validated before preview/download work; preview
  assets stay temporary and hidden from the library until an explicit
  save promotes them.
- Preview percentages are raw yt-dlp media-transfer percentages. Helper
  startup and source resolution remain indeterminate because no byte total
  exists yet; fast preview files may complete between progress samples and
  jump directly to 100%. Only atomic publication makes an item playable.
- Serialized preview preparation has admission priority over speculative
  video staging; pending preview work cancels/gates video helpers, and video
  staging resumes after the preview window settles.
- Missing or mismatched managed tools fail closed with degraded media
  behavior surfaced — never silent success, never an unreviewed download
  at build time.
- Blocking network/process work runs off the UI thread through the task
  registry (bounded slots) or `spawn_blocking`; cancellation and shutdown
  drain are first-class.

## Core flows

```text
search_downloader → results → start_download → helper spawn → staged output
→ signature validation → atomic promote → coalesced rescan → job succeeded
discover → validated recommendations → preview_tap → session → preview_next/
previous/cancel → save → remux_and_promote → library
```

## Tests

- `src-tauri/src/gate4_tests.rs` — downloader, promotion, transfer codec,
  settings/transfer; `gate4_real_dl_test.rs` — real-download path.
- `src-tauri/src/gate5_tests.rs`, `runtime/gate5_smoke.rs` — discovery,
  preview, video readiness smoke.
- `src/test/gate4_smoke.test.ts`, `src/test/gate5_smoke.test.ts`,
  `src/test/views/downloader.test.tsx`.

## Related canonical docs

- `docs/decisions/download-format-m4a.md` — why production output is M4A.
- `docs/release/desktop.md` — pinning, provisioning, bundle verification.
- `docs/modules/library.md` — scan/rescan and promotion targets.
- `docs/modules/contracts.md` — job/download command envelopes.

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
  rescan. The post-transfer validation/promotion interval is published as
  `finalizing` so the UI stays live while the file becomes playable. Missing
  parents outside the root are rejected, never created.
- Download helper invocations receive the source (`scsearch1:`-style or
  URL); the bundled helper resolves via `resolve_bundle_helper`
  (resource dir → manifest dir). Retry reuses the stored job.
- The helper builds its yt-dlp argv without word-splitting so bundle paths
  containing spaces (e.g. `/Applications/Luno Desktop.app/…`) survive as a
  single `--ffmpeg-location` value; a split flag once starved yt-dlp of
  ffmpeg, failing every preview/download postprocess step (2026-09-09).
- A direct-media HTTP 403 gets one automatic retry through YouTube's HLS audio
  rendition, which avoids the selectively enforced GVS proof-of-origin token.
  Other failures remain terminal, and the normal provider-neutral quality
  selector remains the first attempt.
- Discovery metadata is validated before preview/download work; preview
  assets stay temporary and hidden from the library until an explicit
  save promotes them.
- Prepared recommendation files belong to the active recommendation seed,
  independently of whether focused preview or Discover currently presents
  them. Previous/Next, a mode handoff, and disabling Discover retain valid
  files; a different seed or app cleanup replaces the bounded cache.
- Outside Discover mode, each audible library seed owns an automatic five-item
  preparation window. Enabling Discover invalidates that prefetch session;
  disabling it rebuilds the window even when the library seed did not change.
  A focused tap requests the selected item plus its next five successors.
- Preview preparation/playback failures surface only through the preview
  snapshot's per-row `unavailable` state; the retrying prefetch pipeline does
  not publish global error events (which stacked an undismissable banner).
- Discover mode projects its rolling recommendation window into the queue
  immediately — pending items are prepared on selection — so Next/Previous walk
  the temporary list instead of falling through to the physical library. It
  publishes the stable keys whose assets are ready, takes ownership from an
  active preview/prefetch session rather than racing it, and adopts an
  already published asset before starting another helper.
- Each presentation mode has one active serialized preparation driver. A
  terminal success or failure releases its in-flight key, failed rows leave
  the transport projection, and explicit selection retries them. Worker
  deadlines cancel the helper/encoder child; provider failures include a
  bounded diagnostic tail instead of an opaque helper error. Plain metadata
  queries go directly to the managed helper as `ytsearch1:` sources.
- Preview percentages are computed from yt-dlp's machine-readable downloaded
  and exact/estimated total bytes. Helper startup, source resolution, zero-byte
  samples, and unknown totals remain indeterminate; transfer values stay
  monotonic and below 100. Only atomic publication reports 100 and makes an
  item playable.
- Last.fm placeholder URLs are treated as missing artwork. Recommendation
  rows and every playback projection use the same deterministic generated
  Luno artwork; the generated palette set excludes the retired neutral gray
  and white cross/star treatment.
- Serialized preview audio and bounded quick-video preparation start together
  for the same five recommendation items. They publish independently, while
  the audible track's quick and final-quality video retains admission priority
  over speculative recommendation video work.
- URL and URL-queue downloads resolve title/artist through a best-effort
  yt-dlp metadata probe (`validate_direct_url` → `probe_media`) and name the
  output `Artist - Title.m4a`, because the scanner derives metadata from the
  filename; without it those downloads surfaced as the opaque job id +
  "Unknown artist". A failed probe degrades the filename only, never the
  download. `src/state/download_naming.ts` owns the shared naming/display
  policy.
- A physical playlist may link to one canonical YouTube playlist URL. A sync
  enumerates stable video IDs through the supervised yt-dlp boundary, skips
  tracks already found anywhere under the library root, and also skips IDs in
  that playlist's append-only `.downloaded_vids` success markers. Each new item
  downloads sequentially as M4A into the selected playlist folder; its success
  marker is persisted before the sync advances. Sync and single downloads share
  one admission gate, publish normal `sync` job events, support process-group
  cancellation, and trigger a coalesced scan after new files arrive.
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
playlist sync → supervised flat enumeration → library/marker dedupe
→ sequential helper downloads → per-item success marker → coalesced rescan
discover → validated recommendations → shared temporary assets → focused tap
or Discover projection → select/next/previous → explicit save →
remux_and_promote or durable download → chosen playlist → library
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

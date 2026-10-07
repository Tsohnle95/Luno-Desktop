# Library, Persistence, and Migration

Document role: canonical owner for library scanning, track identity,
mutations, artwork extraction, persistence, and user-data compatibility.
It owns the guarantees behind every library snapshot the UI renders.

## What this owns

- Filesystem scanning, playlist derivation, and snapshot publication.
- Stable media-key derivation and collision handling.
- Library mutations (playlists, favorites, deletes, markers) with
  safe-root containment and atomic writes.
- Embedded-artwork extraction, the bounded process cache, and the bounded
  persistent thumbnail cache.
- Settings/config persistence, first-run migration, keychain and legacy
  readers, recovery.

What it does not own: playback/queue policy (`docs/modules/playback.md`);
transfer manifests (`docs/modules/contracts.md`).

## Entry points

- `src-tauri/src/library/scanner.rs` — off-UI-thread scan, root
  validation/canonicalization, supported extensions (incl. `m4a`).
- `src-tauri/src/library/index.rs` — `LibraryIndex`, refresh requests.
- `src-tauri/src/library/mutations.rs` — `PlaylistService`, playlist/track
  mutations.
- `src-tauri/src/library/duplicates.rs` — duplicate detection, allowlists.
- `src-tauri/src/library/reconcile.rs`,
  `reference_repair.rs` — rescan reconciliation, `heal_track_id`.
- `src-tauri/src/library/favorites.rs`, `watch.rs` — favorites store,
  filesystem watcher.
- `src-tauri/src/library/metadata.rs` — versioned portable per-track metadata
  in `.luno_track_metadata.json` at the music root.
- `src-tauri/src/library/artwork.rs` — bounded embedded-artwork extraction
  served as data URLs over the `track_artwork` command, background warmup,
  cache invalidation, and staged metadata writes. Downscaled results and
  definitive no-art results are stored in the versioned app-cache directory;
  source path, size, and modification time select each entry.
- `src-tauri/src/library/artwork_search.rs` — user-triggered MusicBrainz
  recording search, linked release-group lookup, and Cover Art Archive candidate images. It applies an
  identified User-Agent, spaces provider requests by at least one second, and
  keeps a bounded, expiring set of review candidates in memory.
- `src-tauri/src/playback/backend_adapter.rs::probe_duration` — container
  duration probing served through the blocking `track_duration` Tauri command
  for visible library rows.
- `src-tauri/src/domain/identity.rs` — `TrackMetadata`, filename-derived
  identity, provider identity.
- `src-tauri/src/domain/normalization.rs` — title/artist normalization and
  title-splitting behavior.
- `src-tauri/src/media_keys.rs` — filename-derived stable media keys.
- `src-tauri/src/persistence/` — `atomic_write.rs`, `recovery.rs`,
  `legacy.rs` (active compat reader for existing user data).
- `src-tauri/src/migration/` — `first_run.rs`, `keychain.rs`,
  `legacy_readers.rs`. `legacy` means user-data compatibility, not dead
  code — never remove in cleanup.
- `src-tauri/src/logging/` — `redaction.rs`, `service.rs` (migration
  status and diagnostics stay sanitized: no paths leak into status payloads).

## State ownership

- `AppState` owns scan roots, the published library snapshot + generation,
  `track_paths` (id→path map), provider metadata, favorites, settings
  home, and migration status.
- Filesystem truth stays on disk; snapshots are derived and
  generation-guarded. A failed/incomplete root scan must never replace a
  usable non-empty library with an empty snapshot.

## Invariants

- Track IDs are filename-metadata-derived. Existing paths keep their prior
  collision ID across rescans while their filename metadata identity remains
  the same, even when duplicate siblings are added or removed. IDs still held
  by the live queue are not reused for a different path during that rebuild.
  Same-stem collisions gain a `#<path-hash>` discriminator; cross-folder
  same-stem files stay distinct tracks.
- Clicks racing a rescan carry stale IDs: `PlayWithContext` heals every
  context entry onto the current map under one lock; ambiguous siblings
  must fail rather than guess.
- Playlist additions are real `fs::copy` files, not playlist references. A
  copy can be removed while an identical file remains in another playlist.
  Track deletion, empty-playlist, and delete-playlist commands compare every
  same-size regular audio candidate byte by byte across scanned configured
  roots, regardless of filename. If no identical copy remains, or any root or
  candidate cannot be checked, they return a single-use review ID without
  deleting; the confirmed command recomputes the result under the library
  file-mutation lock and checks a SHA-256 stamp of each requested last copy.
  Playlist creation/copies, staged recommendation promotions, transfer
  imports, direct promotions, deletes, and metadata renames serialize through
  the Luno file-mutation lock. Download and sync jobs also block deletion
  through media admission and take the same lock when promoting their final
  file. A staged recommendation save
  requires its existing playlist directory and cannot recreate a playlist
  deleted before its worker begins. A separate external process can still
  change files between Luno's final check and unlink.
- Mutations are safe-root-contained and atomic; downloads promote only
  inside the configured library root and trigger a coalesced rescan.
- The scanner excludes `.luno_previews`, `.luno-download-staging`, the current
  temporary worker-home folder name, and the legacy `.luno-gate4-worker-home`
  folder from playlist discovery. It also ignores legacy `.part.<format>`
  download outputs. Every discovered folder retains its destination path,
  including empty playlists, so the downloader can save into an empty folder.
  The legacy worker folder is hidden but left intact for user review.
- Favorite paths in backend state are authoritative over stale preferences
  snapshots. When a track is copied into a playlist, a favorite source marks
  the successfully copied track as a favorite too; the original remains
  favorited.
- A favorite toggle publishes a targeted `track_favorite_changed` event at
  the current library scan generation. It does not advance the scan epoch;
  scans reconcile favorite flags from the latest path-based store before
  replacing the snapshot.
- Physical playlist-to-YouTube links are stored by `PlaylistService` in the
  versioned root-level `.luno_playlist_sync_sources.json` sidecar, keyed by
  playlist name. `.luno_playlist_urls.json` remains an atomic first-link mirror
  for older builds. Reads fall back to that legacy map for playlists not yet
  present in the new sidecar; delete mutations clear both stores, and every
  scan projects the first link onto `Playlist.url`. URLs never determine
  filesystem destinations.
- Track dates and persisted song metadata are stored in the root-level
  `.luno_track_metadata.json` sidecar, keyed by normalized paths relative to
  the music root. The first scan seeds `added_ms` from file mtime; later scans,
  renames, and playlist copies retain that timestamp. The atomic sidecar moves
  with the music folder and is not part of the app bundle. A corrupt or
  unsupported sidecar is left untouched while the snapshot falls back to
  filename metadata and file mtime.
- Duplicate allow decisions remain in the root-level
  `.luno_allowed_dups.json` sidecar, so the existing allowlist moves with the
  music folder and is independent of app rebuilds.
- Editing a track's artist/title (`edit_track_metadata`) renames its library
  file in place: same folder, extension preserved, collision-rejecting,
  path-keyed favorites moved to the new path, then a coalesced rescan. Because
  identity is filename-derived, the track id changes after the rescan.
- Artwork is lazy and bounded on both sides; the frontend LRU evicts by
  count and byte budget and retries evicted entries lazily on the next
  visible pass — never eagerly from the eviction event (fetch-evict-refetch
  loop, 2026-09-09).
- Artwork maintenance is initiated from the playlist cover menu or Settings.
  Local scans check embedded tags without building data URLs; provider search
  only runs for a selected missing-art track and sends its title and artist to
  MusicBrainz. If the artist is missing, it searches by title alone. It searches
  recordings, filters low-scoring matches, then checks linked release groups in
  Cover Art Archive. A separate user-triggered YouTube search offers closely
  matching video thumbnails as a fallback. YouTube searches the title and
  combined artist/title every time, then ranks and deduplicates up to twelve
  close matches. It uses the managed yt-dlp search if the results page cannot
  be parsed or the automatic results do not align with the saved artist.
  Exact and strong title matches remain eligible when the channel/uploader
  differs, while weaker matches must also match the saved artist. The artwork
  picker also provides a manual YouTube search that preserves YouTube's result
  order and shows up to twelve thumbnail candidates in a scrollable list. Those
  thumbnails may not be album covers. Results from both providers are displayed
  for review, and no match is written until the user applies one. YouTube thumbnail fetches are restricted
  to `i.ytimg.com`, bounded, and decoded through the same image normalizer.
  MusicBrainz permits free API use for
  non-commercial applications; a commercial Luno distribution needs an
  appropriate plan or arrangement.
  Candidate images are byte- and dimension-bounded, decoded, resized to 512 px,
  and written only when the format's primary tag is writable. Luno stages a
  same-directory copy, preserves file permissions, verifies the source stamp
  and reads the resulting artwork back before atomic replacement under the
  library file-mutation lock. WebM is playable when its audio codec is
  supported, but embedded artwork writing through `lofty` is unsupported; the
  artwork command returns a format-specific refusal before staging, leaving
  the music file unchanged. A writable text tag alone does not guarantee
  artwork support, so the staged read-back check is required for formats that
  can be safely tagged.
- Frontend artwork requests have view-scoped demand leases: queued offscreen
  work is removed when no mounted view needs it, duplicate requests share one
  job, and the audible track and visible library rows receive higher queue
  priority than static sidebar mosaics. A low-priority library-wide warmup
  submits bounded batches, with at most two background requests active. The
  process cache remains bounded at 32 MiB and the disk cache at 512 MiB.
- Missing table durations are probed lazily for virtualized visible rows
  through `track_duration`, on a blocking worker. Probed values stay in the
  bounded frontend session cache across route changes and snapshot generations;
  they do not change filename-derived IDs or the portable metadata sidecar, and
  library scans do not probe every audio file. Duration sorting reorders known
  results as they arrive and keeps unresolved tracks after the known group.
- Library track rows, home song and playlist cards, and playlist cover mosaics use the
  deterministic generated Luno artwork when a file has no usable embedded
  cover, including recommendations saved to a physical playlist.
- Served artwork is downscaled to a 512 px display size (`MAX_ARTWORK_DIM`):
  embedded covers are routinely far larger than anything the UI shows, and
  serving them raw put gigabytes of decoded bitmaps into the renderer.
- `com.ty.luno` identity, support/config paths, migration markers,
  favorites, settings, and queue semantics are stable. Deliberate
  compatibility changes need their own migration design + tests.
- Last.fm key: 0600 file under app data; macOS Keychain is read at most
  once per install (silent, no-prompt) purely to seed that file. Startup
  never raises a Keychain dialog.

## Core flows

```text
scan_async → validate/canonicalize roots → off-thread scan → stable keys
→ generation-guarded snapshot → library event (or keep-previous + error
event when all roots unreadable)
play_with_context → heal ids vs current map → queue → playback
library snapshot → low-priority artwork warmup → bounded disk/process cache
visible row → prioritized track_artwork command → cached/extracted data URL
visible library row → track_duration command → blocking container probe
playlist artwork menu/Settings → local missing-art scan → selected song
→ MusicBrainz/CAA candidate review → staged tag write → atomic replace
set_playlist_url → validated YouTube URL → atomic URL map → rescan → snapshot
startup → has_legacy_data → validated_music_root → migrate → status
```

## Tests

- `src-tauri/src/gate2_tests.rs` — scan lifecycle, persistence sync,
  startup IPC, scan events, healing/stability regressions.
- `state.rs` unit tests — duplicate-stem stability, stale-context healing,
  unreadable-root preservation, settings resolution.
- `src/test/artwork_eviction.test.ts` — frontend artwork LRU behavior.

## Related canonical docs

- `docs/modules/contracts.md` — snapshot envelope shape.
- `docs/modules/frontend.md` — list rendering and artwork consumption.
- `docs/modules/playback.md` — queue entries referencing track IDs.

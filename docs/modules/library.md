# Library, Persistence, and Migration

Document role: canonical owner for library scanning, track identity,
mutations, artwork extraction, persistence, and user-data compatibility.
It owns the guarantees behind every library snapshot the UI renders.

## What this owns

- Filesystem scanning, playlist derivation, and snapshot publication.
- Stable media-key derivation and collision handling.
- Library mutations (playlists, favorites, deletes, markers) with
  safe-root containment and atomic writes.
- Lazy artwork extraction and the bounded backend cache.
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
- `src-tauri/src/library/artwork.rs` — bounded embedded-artwork extraction
  served as data URLs over the `track_artwork` command.
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

- Track IDs are filename-metadata-derived and stable across rescans when
  no files are added/removed. Same-stem collisions gain a `#<path-hash>`
  discriminator; cross-folder same-stem files stay distinct tracks.
- Clicks racing a rescan carry stale IDs: `PlayWithContext` heals every
  context entry onto the current map under one lock; ambiguous siblings
  must fail rather than guess.
- Mutations are safe-root-contained and atomic; downloads promote only
  inside the configured library root and trigger a coalesced rescan.
- Physical playlist-to-YouTube links are stored by `PlaylistService` in the
  root-level `.luno_playlist_urls.json` map. Keys follow playlist names, writes
  are atomic, rename/delete mutations keep the map aligned, and every scan
  projects a stored link onto `Playlist.url`; URLs never determine filesystem
  destinations.
- Editing a track's artist/title (`edit_track_metadata`) renames its library
  file in place: same folder, extension preserved, collision-rejecting,
  path-keyed favorites moved to the new path, then a coalesced rescan. Because
  identity is filename-derived, the track id changes after the rescan.
- Artwork is lazy and bounded on both sides; the frontend LRU evicts by
  count and byte budget and retries evicted entries lazily on the next
  visible pass — never eagerly from the eviction event (fetch-evict-refetch
  loop, 2026-09-09).
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
visible row → track_artwork command → bounded extract → data URL
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

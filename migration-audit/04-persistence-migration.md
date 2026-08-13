# Persistence and Migration Audit

This dossier audits state that must survive a desktop migration, plus Android
parity and compatibility evidence. Source and tests are authoritative; the
project brains are used only to identify documented expectations.

## Desktop Configuration
- Status: implemented
- Evidence:
  - `music_player.py:70-75` defines `~/.luno_config.json`, the music root, and the external preview directory.
  - `music_player.py:4621-4689` loads and writes `music_root`, Last.fm key, discovery/playback preferences, global media-key preference, and `favorite_songs`.
  - `music_player.py:831-852` loads config before constructing the engine and installs variable-change autosave traces.
- User behavior: The selected library root, API/discovery settings, volume, crossfade selection, sleep-timer selection, global media keys, and favorites are restored across desktop launches. A configured music root is accepted only when it currently exists (`music_player.py:4631-4636`); otherwise the default remains active.
- State and data flow: Config is decoded into Tk variables and `_favorite_paths`; `_save_config()` serializes the current values and normalized favorite keys. It contains no queue, history, download marker, or preview state.
- Persistence and compatibility: JSON is rewritten directly with `open(..., "w")` (`music_player.py:4685-4689`). Malformed/unreadable config is caught and printed, but there is no backup, schema version, migration, or atomic temp-file replacement.
- Async, lifecycle, and failure behavior: Autosave is triggered by Tk variable traces; save failures do not surface to the user. Config load occurs during startup before the first library refresh.
- UI states and visual requirements: Settings owns the preferences UI (`views/settings.py`); persistence itself has no durable error state.
- Migration implementation requirements: Preserve the exact existing keys and tolerant missing-key behavior. Read malformed/legacy files without destroying the original, validate types and paths, and write versioned config through temp-file plus flush/fsync/atomic replace. Decide how secrets map to the target keychain; the desktop currently stores the Last.fm key as plaintext JSON.
- Verification requirements: Round-trip each key, missing keys, invalid JSON, nonexistent configured root, malformed favorites, and interrupted writes. Ensure secret material is never included in portable manifests.
- Confidence: high
- Open questions: No desktop config schema/version or migration utility is implemented.

## Favorites And History
- Status: implemented (favorites); documented-only for persistent desktop history
- Evidence:
  - `music_player.py:736-737` initializes `_recently_played` and `_favorite_paths` in memory.
  - `music_player.py:1705-1722` normalizes favorite paths; `music_player.py:462-468` loads favorites from config.
  - `engine.py:168-174` maintains most-recent-first history and bounds it to 100 entries.
  - `music_player.py:4204-4212` plays or clears history, but `_clear_recent()` only mutates memory and redraws.
  - `project_brain.md:159-164,239-250` documents favorites in config and describes recent history as controller state.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/entity/Track.kt:8-18` stores Android favorites durably in Room; `TrackDao.kt:37-38` toggles them.
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/RecentlyPlayedStore.kt:15-99` implements bounded JSON SharedPreferences history; `MusicControllerTest.kt:471-495` verifies reopen/ordering.
- User behavior: Desktop favorites are path-based, projected as a virtual Favorites playlist, and pruned against completed scans (`project_brain.md:159-163`; `music_player.py:1709-1719`). Desktop history is newest-first, de-duplicated, capped at 100, playable as a context, and clearable, but is lost on process exit. Android retains 100 metadata snapshots and durable per-track favorite flags.
- State and data flow: Desktop file rename updates live queues, history, and favorite keys (`music_player.py:636-661`); preview cleanup removes staged files from in-memory history (`music_player.py:2834-2841`). Android history stores URI/title/artist/album/duration/artwork and playback-origin metadata, while Room tracks are keyed by URI.
- Persistence and compatibility: Desktop favorite paths are absolute normalized/case-folded paths and are intentionally excluded from the shared manifest (`project_brain.md:249-253`). Android history parser skips malformed array entries and returns an empty list for malformed JSON (`RecentlyPlayedStore.kt:25-57`), which is corruption-tolerant but loses the whole history. Android Room’s `playlist_tracks` composite key prevents duplicate membership (`PlaylistTrack.kt:7-32`).
- Async, lifecycle, and failure behavior: Desktop mutations are UI-thread state changes with no history persistence write. Android store writes use `apply()` (`RecentlyPlayedStore.kt:79-83`), so commit timing is asynchronous; Room updates are repository/DAO operations.
- UI states and visual requirements: Desktop Recent is the `views/recent.py` virtual list; Android Queue/Recents uses the same persisted history (`mobile-app/app/src/main/java/com/luno/mobile/ui/player/QueueRecentsTabs.kt:181-191`).
- Migration implementation requirements: Do not claim to migrate desktop history that does not exist on disk. Preserve favorites only as local metadata, remap paths when the library root changes, prune missing files, and never put favorites/history in v1 manifests. If the target adds durable history, make that a deliberate product extension rather than an inferred compatibility requirement.
- Verification requirements: Test favorite normalization, rename, scan pruning, duplicate playlist membership, 100-entry ordering, clear behavior, malformed Android history, and Room favorite/foreign-key behavior.
- Confidence: high
- Open questions: Whether migration should introduce persistent desktop history is unresolved.

## Playlist URLs And Duplicate Exclusions
- Status: implemented
- Evidence:
  - `music_player.py:4691-4720` stores playlist URL metadata in `<MUSIC_ROOT>/.luno_playlist_urls.json`, keyed by playlist name, with URL validation in `music_player.py:326-347`.
  - `engine.py:416-431` loads/saves allowed duplicate normalized keys in `<MUSIC_ROOT>/.luno_allowed_dups.json`.
  - `downloader.py:334-359` reads `.downloaded_vids`, `.sync_failed_vids`, and legacy `.downloaded.txt`; `downloader.py:377-405` appends successful/permanent-failure markers.
  - `test_downloader_sync.py:79-156` verifies marker creation, legacy handling, and permanent-failure markers.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/entity/Playlist.kt:7-14` persists `playlistUrl`; `PlaylistDao.kt:91-95` queries URL-bearing playlists.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/AppDatabase.kt:51-55` migrates playlist URLs in Room v2->v3.
- User behavior: Desktop playlist sync URL follows a physical playlist folder and can be removed by saving empty input. Allowed duplicate keys suppress duplicate warnings and can be backed up through the downloader UI. Sync markers prevent repeat downloads and record permanent failures. Android stores the URL on the playlist entity and queues WorkManager sync.
- State and data flow: Desktop URL metadata is separate from folder contents; changing `MUSIC_ROOT` reloads the metadata file (`music_player.py:4607-4618`). Allowed keys are loaded by the engine and written from the master root. Sync compares the whole library and uses normalized artist/title keys (`downloader.py:371-435`). Android playlist membership is a Room relation with ordered `sortOrder` and a composite uniqueness key.
- Persistence and compatibility: Desktop URL and allowed-duplicate JSON have no versioning, type validation, or atomic writes; malformed files reset URL metadata or leave allowed keys unchanged. Marker reads are line-oriented and tolerate read errors. `.downloaded.txt` is a legacy format whose final whitespace token is imported as a video ID. Android migrations are append-only through schema version 9 (`AppDatabase.kt:17-110`), and Room schema snapshots are tracked per the census.
- Async, lifecycle, and failure behavior: Desktop marker appends can occur before/independent of the resulting audio write (`downloader.py:377-387`), and all marker write errors are swallowed. This can lose retryability or leave a false-success marker after interruption. Android WorkManager jobs and Room job state are durable; worker writes generated unique names and routes completed downloads transactionally into tracks/memberships (`DownloadWorker.kt:253-279`).
- UI states and visual requirements: Desktop URL, Sync, Duplicates, and Allowed Dups workflows live in `views/library.py` and `views/downloader.py`; Android playlist cards expose URL/sync actions (`mobile-app/app/src/main/java/com/luno/mobile/ui/components/PlaylistCard.kt:141-174`).
- Migration implementation requirements: Import all three desktop marker formats, preserve failed IDs and reasons where useful, canonicalize IDs, and bind marker success to a verified completed destination. Retain playlist URLs by stable playlist identity or an explicit name mapping, not silently by a changed folder name. Version and atomically replace URL/exclusion state; keep allowed duplicate keys local and out of manifests.
- Verification requirements: Test legacy marker conversion, malformed lines, interrupted marker writes, URL metadata corruption, root relocation, duplicate-key normalization, and no duplicate download after an existing file is found.
- Confidence: high
- Open questions: Desktop URL metadata has no playlist ID, so rename/merge behavior requires a migration policy.

## Previews And Downloaded-Artwork State
- Status: implemented
- Evidence:
  - `music_player.py:71-75,820-825` places previews at `~/.luno_previews`, deletes the prior directory at startup, and recreates it outside the library.
  - `music_player.py:2834-2841` deletes staged preview files and removes them from history; `project_brain.md:280-284` states previews are session-only unless explicitly promoted.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/DownloadRepository.kt:188-245` validates preview containment and promotes by copying into app-private downloads plus an explicit playlist.
  - `mobile-app/app/src/main/java/com/luno/mobile/playback/RecommendationPreviewManager.kt:2180-2249` downloads to `.part`, renames/copies to the final cache file, validates duration, and rejects oversized/invalid audio.
  - `mobile-app/app/src/test/java/com/luno/mobile/data/repository/DownloadRepositoryTest.kt:190-297` verifies explicit playlist promotion, distinct generated URIs, and rollback cleanup.
- User behavior: Desktop previews never enter `_library` or the physical queue permanently; Add to Library explicitly copies them to a selected destination. Android temporary previews live in cache, while promotion creates a durable Track, DownloadJob, and playlist membership; no implicit Unsorted destination is used by promotion.
- State and data flow: Preview files are disposable audio plus cached artwork. Android durable downloads retain content URIs when SAF is configured, or app-private file URIs otherwise; thumbnail/artwork paths are persisted on jobs/tracks (`DownloadJob.kt:14-30`, `Track.kt:8-22`).
- Persistence and compatibility: Startup cleanup intentionally discards abandoned preview files. Android `.part` files are not treated as ready; final rename is attempted, with copy fallback (`RecommendationPreviewManager.kt:2220-2223`). There is no desktop preview manifest or recovery ledger.
- Async, lifecycle, and failure behavior: Desktop cleanup defers deletion of the currently playing preview. Android promotion uses `NonCancellable` and a Room transaction; failures remove durable files and leave the cache preview (`DownloadRepositoryTest.kt:260-297`). SAF copy cleanup deletes a partially created destination (`MusicFolderRepository.kt:68-88`).
- UI states and visual requirements: Preview rows expose preparing/ready/failed/save states in the desktop Song Page and Android full player; these states are transient and must not be reconstructed from library scan results.
- Migration implementation requirements: Never import `.luno_previews` as library audio. Clean stale target previews at startup/session end, ignore `.part` files, and migrate only explicitly promoted durable files. Preserve artwork sidecars/cache rules where the target supports them.
- Verification requirements: Test crash leftovers, path traversal outside preview root, partial files, failed promotion rollback, current-preview deferred deletion, and explicit destination enforcement.
- Confidence: high
- Open questions: No desktop test fixture covers preview cleanup or promotion failure.

## Manifest Writes And Transfer Compatibility
- Status: implemented, with atomicity gap
- Evidence:
  - `library_transfer.py:22-35,73-228` defines and strictly validates bounded v1 manifests, rejecting unknown fields, unsafe references/names, invalid versions, and malformed JSON.
  - `library_transfer.py:231-237` validates then directly truncates and writes JSON; no temporary file, fsync, or atomic replace is used.
  - `library_transfer.py:290-369` builds path-free metadata/playlist manifests and marks metadata-only tracks as ambiguous.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/export/LibraryManifestCodec.kt:48-80` and `LibraryManifestCodecTest.kt:40-90` establish matching Android strict codec behavior and shared fixtures.
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/export/ExportImportScreen.kt:66-110` and `MainShell.kt:345-375` write exports directly through a SAF output stream and read imports boundedly.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/LibraryTransferRepository.kt:70-159` implements additive matching/import and queues unresolved tracks.
- User behavior: Users export selected tracks/playlists/library as metadata-only JSON. Imports validate untrusted input, match local tracks, require ambiguity confirmation for metadata-only records, retain existing memberships, and queue missing tracks without automatic deletion.
- State and data flow: The manifest contains ordered playlist refs and optional provider IDs, never local paths, secrets, audio, favorites, history, or queue. Desktop copies matched files into sanitized playlist folders; Android updates Room memberships and download destinations.
- Persistence and compatibility: v1 and checked-in `contracts/library-export/v1` fixtures are the cross-platform boundary. Desktop `write_manifest()` and Android SAF export can leave a truncated/partial destination if interrupted. Android import rejects malformed/unknown/path-like data and uses bounded reads (`ExportImportScreen.kt:94-110`).
- Async, lifecycle, and failure behavior: Desktop export build/probing is worker-driven per `project_brain.md:186-190`; writes themselves are not atomic. Android encode/build and file IO are dispatched off the UI thread, with UI error states but no destination rollback protocol.
- UI states and visual requirements: Desktop settings/library transfer actions and Android SAF Export/Import screen expose preview, ambiguity, success, and failure states. The wire contract does not prescribe visual layout.
- Migration implementation requirements: Preserve exact v1 schema, fixtures, ordering, additive/no-delete semantics, no-path rule, and ambiguity flag. Replace direct manifest writes with atomic same-directory temp writes where filesystem semantics allow; for SAF, write to a provider-created temp document and replace/clean up only after complete validation, or document provider limitations. Never use a manifest as a backup of local config/history/favorites.
- Verification requirements: Run desktop fixture/transfer tests and Android codec/transfer tests; add interrupted-write, oversized input, invalid Unicode/control data, duplicate refs, and SAF write-failure coverage.
- Confidence: high
- Open questions: SAF providers do not guarantee atomic rename, so the target needs an explicit provider-safe export strategy.

## Android Room, Preferences, And SAF Persistence
- Status: implemented
- Evidence:
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/AppDatabase.kt:17-110` declares Room v9 and migrations 1->9 for downloads, playlist URLs, artwork, play counts, multi-playlist destinations, and favorites.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/entity/PlaylistTrack.kt:7-32` defines composite membership identity, foreign keys, cascade deletion, and sort order.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/MusicFolderRepository.kt:9-24` persists the selected tree URI in app-private SharedPreferences; `LibraryRepository.kt:1036-1053` persists SAF read grants via `takePersistableUriPermission`.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/LibraryRepository.kt:139-169` treats grant failure as a counted condition while continuing import; `LibraryRepositoryTest.kt:29-65` provides fake-provider coverage.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/discovery/LastfmKeyStore.kt:13-41` uses encrypted preferences with a plaintext fallback when Keystore encryption is unavailable.
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/MusicFolderRepository.kt:50-88` reuses/rejects collisions and deletes failed SAF documents.
- User behavior: Android library, playlists, memberships, favorites, artwork, play counts, and download jobs survive process restarts through Room. The selected SAF folder URI is restored from preferences; a lost grant/provider is reported as unavailable rather than silently treated as an empty library. Imported descendants use the persisted tree grant rather than per-file grant calls (`LibraryRepository.kt:110-129`).
- State and data flow: Room is the durable catalogue; SAF URIs are Track primary keys and DownloadJob local URIs. Preferences hold small process-independent settings/history/URI data. App-private files hold downloads/artwork/cache and are reconciled into Room.
- Persistence and compatibility: Room migrations are explicit and schema snapshots are tracked. There is no destructive fallback in the inspected database builder (`AppDatabase.kt:104-110`). Preferences use `apply()` and have no schema migration/versioning; Last.fm key fallback weakens secret-at-rest guarantees by design. SAF grants can be lost on reinstall, as documented by `mobile-app/project_brain.md:282-297`.
- Async, lifecycle, and failure behavior: Folder import is off-main-thread and per-file failures are isolated; permission persistence is timeout-bounded and failures are counted. SAF writes clean up created documents on failure. Database transactions protect track/membership/job commits in workers and preview promotion.
- UI states and visual requirements: Settings owns folder selection/reconciliation and import progress; downloads expose durable queued/downloading/completed/failed/cancelled states from Room.
- Migration implementation requirements: Preserve Room version history when importing an Android catalogue, reconcile unavailable SAF URIs distinctly from empty folders, re-request grants after reinstall, retain collision policy, and migrate app-private downloads before deleting old locations. Do not copy Android-only Room IDs into desktop path-based state except through v1 manifest semantics.
- Verification requirements: Run `cd mobile-app && ./gradlew testDebugUnitTest lintDebug`; include schema migration tests, lost-grant/ejected-storage tests, preference corruption tests, transaction rollback, duplicate document IDs, and SAF partial-write cleanup.
- Confidence: high
- Open questions: The inspected unit tests use in-memory Room databases; persistent on-device upgrade and real provider behavior remain manual/device coverage gaps.

## Migration Summary
- Desktop durable inputs: `~/.luno_config.json`, `<MUSIC_ROOT>/.luno_playlist_urls.json`, `<MUSIC_ROOT>/.luno_allowed_dups.json`, `.downloaded_vids`, `.sync_failed_vids`, and legacy `.downloaded.txt`.
- Desktop non-durable state: `_recently_played`, engine queue/shuffle/repeat, error logs, and preview staging. Do not invent migration data for these.
- Android durable inputs: Room database, SharedPreferences history/folder/recent-save settings, persisted SAF grants, app-private downloads/artwork, and playlist URL/job fields.
- Shared compatibility input: only the strict `luno.library.export` v1 manifest. It excludes paths, secrets, favorites, history, queue, and audio bytes.
- Highest migration risks: direct non-atomic desktop JSON/marker writes, playlist-name-keyed URL metadata, plaintext desktop API key, desktop history expectation mismatch, corrupted preference handling, and SAF export atomicity.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`, `project_brain.md`, `mobile-app/project_brain.md`, `music_player.py`, `engine.py`, `downloader.py`, `library_transfer.py`, `views/recent.py`, `views/settings.py`, `views/downloader.py`, Android `AppDatabase.kt`, Room entities/DAOs, `RecentlyPlayedStore.kt`, `MusicFolderRepository.kt`, `LibraryRepository.kt`, `DownloadRepository.kt`, `DownloadWorker.kt`, `RecommendationPreviewManager.kt`, `LibraryTransferRepository.kt`, `LibraryManifestCodec.kt`, `LastfmKeyStore.kt`, `MainShell.kt`, and `ExportImportScreen.kt`.
- Tests/fixtures inspected: `test_downloader_sync.py`, `test_library_transfer.py`, `test_contract_fixtures.py`, Android `LibraryManifestCodecTest.kt`, `MusicControllerTest.kt`, `DownloadRepositoryTest.kt`, `LibraryRepositoryTest.kt`, `AppDatabaseTest.kt`, and tracked contract v1 fixtures/schema.
- Claims requiring review: Whether future migration should add persistent desktop history; stable identity/name mapping for desktop playlist URLs; provider-safe atomic export replacement; desktop API-key secret migration; real-device Room/SAF upgrade behavior.
- Coverage gaps: No desktop persistence test suite covers config/URL/allowed-duplicate corruption or atomicity; no desktop preview persistence tests; Android tests are primarily Robolectric/in-memory and do not prove all real SAF provider or interrupted-write behavior.
- Status: complete

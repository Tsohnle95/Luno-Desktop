# Library And Filesystem Audit

This dossier audits implemented desktop and Android behavior. The desktop is
the migration reference; Android is an independently implemented parity
surface, not shared runtime code.

## Library Scanning
- Status: implemented
- Evidence:
  - `library_tools.py:8-43`
  - `music_player.py:3334-3383,3385-3418`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/LibraryRepository.kt:192-225,251-320`
  - `mobile-app/app/src/test/java/com/luno/mobile/data/repository/LibraryRepositoryTest.kt:102-126,313-350`
- User behavior: Desktop scans configured roots and treats each immediate child directory as a playlist; audio files directly in the root are `Unsorted`. Supported desktop extensions are `.mp3`, `.m4a`, `.wav`, `.flac`, `.mp4`, and `.webm`. Android imports user-selected SAF files/folders/trees, recursively creates folder-named playlists, and puts root files in `Unsorted` for a library-tree import.
- State and data flow: Desktop `scan_library()` returns `dict[playlist, list[path]]`; the controller replaces `_library` only for the current scan generation, then repopulates sidebar and track list. Android stores `Track` rows keyed by URI and `PlaylistTrack` memberships in Room; a folder import can therefore preserve one file in the library while assigning it to a folder playlist.
- Persistence and compatibility: Desktop paths are local filesystem state and favorites/config reference normalized absolute paths. Android persists SAF grants and content URIs; the v1 manifest deliberately excludes local paths and audio bytes (`contracts/library-export/v1/schema.json:1-53`, `library_transfer.py:73-110`).
- Async, lifecycle, and failure behavior: Desktop scanning runs on a daemon thread and delivers completion through `after(0)`; overlapping requests coalesce and stale generations are ignored. Android folder traversal is bounded to four file workers, uses document-id dedupe, per-file/listing/Room timeouts, retries listings, and counts unlistable folders as errors rather than silently treating them as empty. Tests cover AOSP and Samsung-style providers, empty/self-child folders, 4,047 files, duplicate rescans, broken listings, and slow listings (`LibraryRepositoryTest.kt:181-389`).
- UI states and visual requirements: Desktop preserves the 56 px virtual-list row and scroll clamping contract (`views/library.py:768-781`). Android exposes import progress/results through the shell import manager, but Android import is an explicit user action rather than a passive filesystem scan (`mobile-app/project_brain.md:1-9`).
- Migration implementation requirements: Preserve immediate-child desktop playlist semantics, exact extension policy, `Unsorted`, newest-first root ordering, generation/coalescing behavior, and non-blocking UI. If the target supports Android-style recursive import, make it an explicit compatibility mode rather than silently changing desktop scan semantics.
- Verification requirements: Add deterministic target tests for roots missing, empty roots, nested directories, unsupported extensions, mtime ordering/ties, scan coalescing, stale generation rejection, and UI-thread delivery. Retain Android SAF provider regression coverage.
- Confidence: high
- Open questions: Desktop has no checked-in scan-library unit tests; exact behavior for permission errors and symlink directories is source-defined only.

## Playlists, Sorting, And Filtering
- Status: implemented
- Evidence:
  - `music_player.py:679-695,3472-3503`
  - `views/library.py:711-739`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/PlaylistRepository.kt:27-62,97-123`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/components/SortChip.kt:33-63`
  - `mobile-app/app/src/test/java/com/luno/mobile/ui/components/SortChipTest.kt:1-120`
- User behavior: Desktop creates sanitized physical folders under `MUSIC_ROOT`; sidebar order is All Music, Favorites, then alphabetical physical playlists. All Music displays Unsorted first, then alphabetical playlists. In-playlist filtering matches title or artist case-insensitively. Desktop sort modes and their default selection are implemented in the library view, including A-Z, Z-A, Recently added, and Duration.
- State and data flow: Desktop playlist membership is inferred from folder paths on every scan. Android playlist membership is explicit Room junction data with `sortOrder`; adding/removing/clearing a playlist does not remove the underlying track (`PlaylistRepository.kt:97-123`). Android’s sort helpers sort tracks by title, `addedAt`, or descending duration and playlists by name, creation time, or total duration.
- Persistence and compatibility: Desktop playlist identity is a sanitized folder name, with no separate membership database. Android names/descriptions/URLs and ordered memberships are Room-persisted. Export/import carries ordered playlist references, not paths.
- Async, lifecycle, and failure behavior: Desktop folder creation immediately schedules a refresh; Android repository operations are suspend/Room operations and validate blank names, nonexistent playlists, and non-library tracks.
- UI states and visual requirements: Desktop uses 56 px rows and clamped virtual scrolling; Android uses a shared sort chip and playlist detail search/filter UI (`mobile-app/project_brain.md:157-160`).
- Migration implementation requirements: Preserve playlist ordering, Unsorted placement, title/artist filter semantics, sort labels and direction, and the distinction between physical desktop folders and Android metadata memberships. Do not assume Android’s explicit membership model can be projected into desktop without defining folder materialization.
- Verification requirements: Test sort direction, recent/duration ties, filtering artist/title, All Music deduplication behavior, playlist creation sanitization, and add/remove/clear membership without deleting tracks. Android `PlaylistRepositoryTest` and `SortChipTest` are existing parity evidence.
- Confidence: high
- Open questions: Desktop’s current `sort` implementation and persisted sort preference location should be confirmed before target schema design; the view state is visible, but no dedicated desktop sort test is checked in.

## Favorites
- Status: implemented
- Evidence:
  - `music_player.py:1705-1764,3415-3418,4621-4684`
  - `views/library.py:676-739,862-869`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/entity/Track.kt:8-18`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/db/dao/TrackDao.kt:37-38`
  - `project_brain.md:159-164,249-250`
- User behavior: Desktop supports favorite toggles from library rows, context menus, queue/player surfaces, and Song Page. Favorites appear in a reserved virtual `Favorites` playlist and can be moved ahead of other rows. Android stores `isFavorite` on each Room track and exposes the same local favorite concept.
- State and data flow: Desktop stores normalized absolute path keys in `_favorite_paths`, intersects them with every completed scan, and removes dead entries. Renames update the old key to the new path. Android favorite state is a field on the track row and survives URI-based library persistence.
- Persistence and compatibility: Desktop uses `favorite_songs` in `~/.luno_config.json`; Android uses Room schema state. Favorites are intentionally absent from the shared manifest, so export/import must not transfer them (`project_brain.md:249-253`).
- Async, lifecycle, and failure behavior: Desktop favorite UI refreshes locally without a filesystem scan; scan completion prunes missing paths and saves config. Android favorite updates are DAO writes and naturally flow to Compose observers.
- UI states and visual requirements: Desktop uses outlined/filled stars and a Favorites sidebar card; `Favorites` is never a physical scanned folder. Android’s library/home/player favorite projection is database-backed.
- Migration implementation requirements: Preserve local-only favorites, virtual Favorites scope, stable path normalization, rename migration, deletion pruning, and favorite-first ordering. Never serialize favorites in the v1 transfer manifest.
- Verification requirements: Test persistence load/save, missing-file pruning, rename preservation, duplicate playlist membership deduplication in Favorites, bulk toggle semantics, and import/export exclusion.
- Confidence: high
- Open questions: Android Room migration history should be checked when targeting older databases; the current entity/schema establishes the field but not a cross-device favorite migration contract.

## Duplicates
- Status: implemented
- Evidence:
  - `library_tools.py:64-105`
  - `music_player.py:3915-3917`
  - `views/downloader.py:913-1007,1120-1455`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/DuplicateFinder.kt:5-42`
  - `mobile-app/app/src/test/java/com/luno/mobile/data/repository/DuplicateFinderTest.kt:7-41`
- User behavior: Desktop groups files by normalized artist/title derived from filename splitting, excluding common release tags, and permits normalized keys through `.luno_allowed_dups.json` in the master library root. The duplicate UI can delete selected files and export/import portable allowed-duplicate backups. Android shows normalized duplicate groups and supports metadata-only track removal from its duplicate screen.
- State and data flow: Desktop scans first, then duplicate detection derives groups from every playlist path; allowed keys suppress groups. Android groups all Room tracks with the same normalization and sorts groups by first title.
- Persistence and compatibility: The normalization algorithm is intentionally aligned across source, including parenthetical/bracket tags, featuring variants, extensions, punctuation, and case. Allowed-duplicate state is desktop-local and is not in the v1 manifest.
- Async, lifecycle, and failure behavior: Desktop duplicate scanning is invoked from the downloader workflow and deletion is followed by a library refresh; Android duplicate grouping is an in-memory projection of Room tracks.
- UI states and visual requirements: Desktop exposes Playlist and Filename columns and explicit allowed-duplicate backup controls (`project_brain.md:214`). Android duplicate groups are a separate drawer/library screen (`mobile-app/project_brain.md:157-159`).
- Migration implementation requirements: Port the exact normalization regex and distinguish true duplicate groups from user-allowed groups. Preserve destructive-delete confirmation and portable allowed-key format if existing desktop data must migrate. Do not equate Android’s metadata-only removal with desktop file deletion.
- Verification requirements: Keep common-tag, featuring, punctuation, blank-key, allowed-key, cross-playlist, and deletion tests. Existing Android normalization tests cover only the core grouping cases; desktop allowed-key and UI deletion need target tests.
- Confidence: high
- Open questions: Whether target duplicate review should retain the desktop allowed-key backup format or replace it with a versioned format is unresolved.

## Rename, Edit, And Delete
- Status: implemented
- Evidence:
  - `music_player.py:489-598,636-676`
  - `music_player.py:4095-4172`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/LibraryRepository.kt:929-955`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/PlaylistRepository.kt:113-123`
  - `mobile-app/app/src/test/java/com/luno/mobile/data/repository/LibraryRepositoryTest.kt:82-100`
- User behavior: Desktop edits one selected local file by sanitizing artist/title and renaming it to `Artist - Title.ext` or `Title.ext`; it rejects empty titles and collisions. It can permanently delete selected songs, clear playlist contents while retaining the folder, or delete a playlist folder with root/protected-name safety checks. Android edits title/artist metadata in Room without renaming the source URI, removes a track row via `deleteTrack`, and removes playlist membership separately without deleting audio.
- State and data flow: Desktop rename updates queue, original queue, recent history, favorites, now-playing labels, queue UI, recent UI, config, and then rescans. Desktop file deletes prune favorites on explicit song deletion and through the next scan. Android metadata edits update the Room row and search immediately; track deletion is a metadata/database operation unless a separate file-management flow exists.
- Persistence and compatibility: Desktop filename is the metadata identity used by search/playback/discovery/duplicate fallback. Android has independent title/artist fields and URI identity. This is a fundamental migration difference: filename edits cannot be represented as Android metadata updates without choosing a target ownership model.
- Async, lifecycle, and failure behavior: Desktop `os.rename`, `os.remove`, and `shutil.rmtree` are synchronous controller actions and can report errors through dialogs; refresh work is asynchronous. Android metadata/playlist changes are suspend DAO calls, while SAF file deletion is not part of `LibraryRepository.deleteTrack`.
- UI states and visual requirements: Desktop edit is single-selection modal UI; delete actions require confirmation. Android uses metadata editor/action sheets and separate playlist removal actions (`mobile-app/project_brain.md:136,141-143`).
- Migration implementation requirements: Decide and document whether target edit mutates physical filenames, metadata records, or both. Preserve collision checks, sanitization, queue/history/favorite reference repair, protected root deletion, confirmation copy, and explicit distinction between remove-from-playlist and delete-from-library.
- Verification requirements: Test rename of current/favorited/recent/queued tracks, case-only rename, collision, invalid names, missing files, folder safety, partial delete failures, and Android metadata search/update plus playlist-only removal.
- Confidence: high
- Open questions: Desktop deletion’s handling of failed individual files is only printed, and Android’s `deleteTrack` file cleanup policy is not established by the repository source.

## Watchers And Desktop/Android Differences
- Status: implemented
- Evidence:
  - `music_player.py:3334-3383,4588-4601`
  - `mobile-app/app/src/main/java/com/luno/mobile/data/repository/LibraryRepository.kt:201-225`
  - `mobile-app/app/src/main/java/com/luno/mobile/ui/shell/MusicFolderImportManager.kt`
  - `mobile-app/project_brain.md:1-9,282-299`
- User behavior: Desktop automatically polls every 30 seconds and refreshes when no download, sync, transfer, discovery download, or scan is active. Android has no filesystem watcher or periodic rescan implementation in the inspected library surface; it imports through explicit SAF flows and persists grants for future playback.
- State and data flow: Desktop watchdog calls the same generation-tagged scan and all UI mutations return through Tk’s event loop. Android import is a process-lifetime coroutine owned by `MusicFolderImportManager`, with progress/final status in a `StateFlow`; Room flows drive library UI after import.
- Persistence and compatibility: Desktop watches the configured local root and stores that path in config. Android watches no root; content URIs and persisted SAF grants are the storage identity. Android’s storage contract distinguishes unavailable storage from genuinely empty folders in product requirements, but this audit found no watcher/rescan equivalent.
- Async, lifecycle, and failure behavior: Desktop watchdog skips active operations and reschedules itself unconditionally. Android import has bounded listings, DAO calls, file processing, grant persistence, retries, and explicit failure/stall status; the watchdog described in the shell manager is an import-stall watchdog, not a filesystem change watcher.
- UI states and visual requirements: Desktop silently refreshes the library snapshot when the watchdog detects changes. Android surfaces import progress and completion/failure in a persistent shell strip and requires the user to launch import.
- Migration implementation requirements: Implement a target watcher/polling policy only after deciding whether it should mirror desktop polling or use OS filesystem notifications. Preserve operation gating, refresh coalescing, stale-result rejection, and UI-thread/event delivery. Do not claim Android’s import stall watchdog provides library change detection.
- Verification requirements: Test watchdog interval/gating/reschedule, concurrent manual refresh, scan failure recovery, and filesystem changes during downloads/transfers. Android tests should continue to cover import completion, timeout/error accounting, and persisted grant failures; no Android watcher test exists because no watcher is implemented.
- Confidence: high
- Open questions: Whether Android should reconcile external changes via MediaStore/SAF notification or remain explicit-import-only is a product decision, not settled implementation behavior.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`, `project_brain.md`, `mobile-app/project_brain.md`, `library_tools.py`, `music_player.py`, `views/library.py`, `views/downloader.py`, `mobile-app/app/src/main/java/com/luno/mobile/data/repository/LibraryRepository.kt`, `DuplicateFinder.kt`, `PlaylistRepository.kt`, `TrackDao.kt`, `Track.kt`, `SortChip.kt`, and `MusicFolderImportManager.kt`.
- Tests/fixtures inspected: `mobile-app/app/src/test/java/com/luno/mobile/data/repository/LibraryRepositoryTest.kt`, `DuplicateFinderTest.kt`, `SortChipTest.kt`, `LibraryTransferRepositoryTest.kt`, and desktop `test_library_transfer.py`.
- Claims requiring review: Desktop sort persistence and exact target duplicate-backup compatibility; Android external-file deletion policy; whether Android will add change reconciliation.
- Coverage gaps: No desktop scan/favorites/rename/delete/watchdog unit suite; no Android filesystem watcher exists to test; no real SAF/device permission corpus beyond the fake provider.
- Status: complete

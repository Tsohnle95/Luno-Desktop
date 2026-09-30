# Task: Improve playlist selection and concurrent downloads

Status: ACTIVE

## Goal

Make playlist membership and artwork visible, protect favorite persistence,
remove the generated worker directory from playlist discovery, and let
Downloader search results start bounded overlapping downloads into searchable
playlist destinations.

## Acceptance criteria

- [ ] Playlist picking indicates which selected library tracks are already in each destination.
- [ ] Playlist destinations in Downloader can be searched by playlist name and still resolve to the correct folder; Unsorted remains the default.
- [ ] Search can continue while downloads run, with multiple downloads active and routed to the destination chosen for each click.
- [ ] Concurrent download jobs are bounded, independently cancellable, and all drain at shutdown; playlist sync remains exclusive with media downloads.
- [ ] Sidebar, Unsorted, popular-playlist cards, and playlist selection artwork use up to four member-track images in a quadrant mosaic.
- [ ] Download workers no longer create a folder under the music root; the old helper folder is excluded from playlist discovery without deleting its contents.
- [ ] A stale general settings save cannot overwrite current favorite paths.
- [ ] Explain the origin limits for the physical “YouTube playlist” folder without inventing track provenance.

## Relevant context

- `docs/modules/frontend.md`: playlist and Downloader rendering ownership.
- `docs/modules/library.md`: scan, playlist, and favorite persistence guarantees.
- `docs/modules/downloads-discovery.md`: download jobs, sync exclusion, and task capacity.
- `src/components/PlaylistPicker.tsx`, `src/views/DownloaderView.tsx`, `src/app/Shell.tsx`.
- `src-tauri/src/download/coordinator.rs`, `src-tauri/src/state.rs`, `src-tauri/src/library/scanner.rs`.
- `src/test/views/downloader.test.tsx`, frontend view tests, `src-tauri/src/gate2_tests.rs`, and state/coordinator tests.

## Invariants / constraints

- The library root remains the only promotion destination; concurrent jobs must use separate staged files and atomic collision-rejecting promotion.
- Playlist sync remains sequential and mutually exclusive with normal media downloads.
- A failed task admission must cancel its already-started coordinator job; no worker may be orphaned.
- Job cancellation, terminal events, retry requests, and shutdown must identify each job independently.
- Favorite paths held by backend state are authoritative; partial settings updates preserve them.
- The legacy helper directory is hidden from scans but is not deleted automatically.

## Affected surfaces

- Rust download coordinator and AppState start/cancel/sync/shutdown lifecycle.
- Rust scanner filtering for the legacy worker-home directory.
- Rust settings merge semantics for favorites.
- React Downloader job queue, destination picker, playlist picker, artwork projection, and sidebar/home cards.
- Focused regression coverage for concurrency, worker-home placement, favorites, playlist membership, and destination searching.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Worker-home placement/filtering and authoritative favorite persistence | complete | Scanner filtering, settings merge, favorite copy/persistence, and worker-home cleanup tests passed | |
| 2 | Playlist membership indication, searchable destinations, and shared mosaics | pending | Targeted frontend checks | |
| 3 | Bounded concurrent coordinator and shell job queue, including cancellation/recovery | pending | Focused coordinator/state/UI checks | |
| 4 | Cumulative review and canonical validation | pending | `npm run verify`, Rust tests, `npm run build` | |

## Validation plan

- Targeted Rust tests for scanner filtering, download worker-home placement, favorite merge behavior, concurrent jobs, cancellation, and sync exclusion.
- Targeted frontend checks for playlist membership, filtered destination selection, mosaic art, and starting another search while jobs remain active.
- Canonical gates under the supported toolchain: `npm run verify`, `cargo test --manifest-path src-tauri/Cargo.toml`, and `npm run build`.
- Direct live UI verification if the running app is available; otherwise report the UI appearance check as unverified.

## Decisions

- Cap normal downloader jobs at three active jobs. This permits useful overlap while bounding helper processes, network activity, and staging IO.
- Keep the coordinator as owner of helper worker handles and tokens; represent active jobs by ID in a map so cancellation and shutdown can address each one.
- Place each worker home under an app-specific system temporary directory keyed by canonical library-root identity, job ID, and generation, then remove it after that helper exits. Exclude both the current and legacy worker-home names from playlist discovery.
- Use the backend favorite set when writing settings, because toggles already persist it and UI settings objects can be stale.
- Use a shared playlist artwork component with at most four track covers and the library hero's existing sparse-playlist layout.
- The “YouTube playlist” name is not created by a hard-coded application flow; scanner playlist entries are physical library folders. The current repository has no per-file origin ledger, so source history cannot be identified from the folder name alone.

Plan review found two lifecycle edges to account for: each active download also needs a TaskRegistry event watcher, and a watcher admission failure must cancel the helper after coordinator start; startup sync admission must test whether *any* coordinator job is active once jobs are stored by ID. The UI queue must preserve each row's resolved source, destination, and metadata before a later search replaces visible results.

## Open risks / blockers

- Concurrent work increases pressure on the task registry; coordinator capacity must stay below production task capacity and failed watcher admission must cancel the job.
- The app's live library is outside the workspace, so the exact existing contents and origin of the user's “YouTube playlist” folder cannot be inspected here.

## Completion

- [ ] Acceptance criteria satisfied
- [ ] Cumulative validation passed
- [ ] Canonical durable truth updated
- [ ] Future work transferred to backlog/issues
- [ ] Final Git state reviewed

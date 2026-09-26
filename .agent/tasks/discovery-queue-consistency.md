# Task: Keep discovery, recommendations, and queue in sync

Status: COMPLETE

## Goal

Keep audible playback, the selected queue row, the song page, and the footer consistent through focused previews and Discover transitions.

## Acceptance criteria

- [x] Recommendation taps and Discover Next/Previous show the selected/audible item in the song page, footer, and queue.
- [x] Enabling/disabling Discover projects/restores the queue without stale preview metadata or restarting audible audio.
- [x] Queue operations have defined behavior while Discover owns the temporary projection and preserve user intent after disable.
- [x] Failed or pending recommendations do not leave misleading playback state; retry remains possible.

## Relevant context

- `docs/modules/playback.md`, `docs/modules/downloads-discovery.md`, `docs/modules/frontend.md`, `docs/modules/contracts.md`
- `src-tauri/src/state.rs`: preview and Discover transitions, playback snapshots, queue mutations
- `src-tauri/src/preview/discover_mode.rs`; `src/app/Shell.tsx`; `src/state/backend_projection.ts`
- State tests, `src/test/bridge.test.ts`, view tests

## Invariants / constraints

- The backend owns queue order/index and physical restoration.
- A focused recommendation may sound while its ID is absent from the physical queue.
- Discover may preserve a focused preview session for in-flight preparation.
- Keep heavy helper and audio work off the UI thread; preserve live playback on policy toggles.

## Affected surfaces

- Playback snapshot contract, backend queue policy, frontend song/footer/queue projection, regression coverage.

## Phases

| Phase | Scope | Status | Validation | Commit |
|---|---|---|---|---|
| 1 | Repair the coupled playback identity, preview transitions, and queue policy | complete | focused Rust/TS checks and full gate | implementation checkpoint |
| 2 | Review cumulative behavior, document durable truth, and close the task | complete | diff review and full gate | closure checkpoint |

## Validation plan

- Targeted: backend state transitions, frontend projection tests and typecheck.
- Integration: exercise Discover toggle/skip and focused preview paths in a live app if available; report any unverified external provider path.
- Canonical: `npm run verify`, `cargo test --manifest-path src-tauri/Cargo.toml`, `npm run build` under pinned toolchains.

## Decisions

- Playback snapshots need an explicit audible identity because restoring a physical queue can leave a recommendation sounding outside that queue. The queue index remains authoritative for transport position.
- Discover's active snapshot takes presentation precedence over a preserved focused preview snapshot.
- A stopped selected recommendation must route the footer Play control back through its owning mode; the library-only fallback left an enabled button with no action.
- Review result: queue edits must either update the captured physical queue or clearly exit Discover; silent loss of edits on disable is unacceptable. Keep changes scoped to existing backend policy rather than duplicating it in React.

## Open risks / blockers

- External Last.fm and yt-dlp outcomes vary by provider and cannot be proven from fixtures alone.
- Live UI playback and provider behavior were not exercised on an installed app in this workspace.

## Completion

- [x] Acceptance criteria satisfied through focused and integration coverage
- [x] Cumulative validation passed: 191 frontend tests, 230 Rust tests, production build
- [x] Canonical durable truth updated
- [x] Final diff scope reviewed

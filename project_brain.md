# Luno Desktop Project Brain

This is the architectural map for the active Luno Desktop repository. Source
and tests are authoritative when this document is stale. The former mixed
Python/Android brain is preserved at
`docs/archive/python-to-tauri/project_brain-monorepo.md` for historical detail.

## Product and technical invariants

- Human-facing product name: `Luno Desktop`.
- Tauri identifier: `com.ty.luno`; do not change it during ordinary work.
- Stack: React, TypeScript, Vite, Tauri 2, and Rust.
- Audio output is normalized to 44.1 kHz through the reviewed FFmpeg path.
- Heavy filesystem scanning, yt-dlp/process work, discovery, downloads, and
  artwork extraction stay off the UI thread. UI mutations are scheduled via
  the established callback/event boundary.
- Preserve the documented 56 px virtual-list row height and scroll-region
  clamping behavior where the legacy-compatible library UI still applies.
- Preserve existing user data, first-run migration, keychain/file migration,
  Room-independent transfer behavior, and all Tauri compatibility readers.

## Repository map

```text
├── src/                         # React application and typed frontend bridge
│   ├── app/                     # shell, bridge, and smoke probes
│   ├── components/              # reusable player/library/download UI
│   ├── state/                   # frontend projections and bounded caches
│   ├── views/                   # route-level desktop views
│   ├── shared/                  # shared TypeScript contracts
│   └── test/                    # Vitest component, bridge, and smoke tests
├── src-tauri/
│   ├── src/                     # Rust command boundary and domain runtime
│   ├── resources/bin/           # managed helper and provisioned binaries
│   └── tauri.conf.json          # product, identifier, bundle resources
├── contracts/library-export/v1/ # language-neutral transfer schema and fixtures
├── release/                     # pinned binary manifest and notices
├── release-input/               # reviewed local binary input for provisioning
├── scripts/                     # provisioning and bundle verification
├── docs/                        # active architecture, decisions, product, release, verification
├── build-test.command           # local Rust/frontend/bundle harness
├── dev.command                  # local development launcher
├── package.json                 # Node scripts and desktop package metadata
├── AGENTS.md                    # Desktop-specific agent constraints
└── project_brain.md             # this document
```

## Runtime boundaries and data flow

`src-tauri/src/main.rs` creates the Tauri command boundary and `AppState` owns
the session state, configured library roots, event publication, playback queue,
task registry, settings, migration status, download jobs, discovery, preview,
artwork, and video state. Synchronous work is dispatched through the existing
blocking-worker boundary so the Tauri/UI thread remains responsive.

Library scans validate and canonicalize configured roots, scan audio files off
the UI thread, derive stable media keys from filename metadata, and publish a
generation-guarded library snapshot. Mutations use safe-root containment and
atomic writes. A failed or incomplete root scan must not replace a previously
usable non-empty library with an empty snapshot.

Playback owns an authoritative queue and playback clock. Shuffle, repeat,
next/previous, queue mutations, media keys, recent-track state, and failure
recovery operate through the Rust state boundary. The frontend renders
snapshots and sends typed commands; it must not recreate backend queue policy.

Downloads use the bundled `luno-download-helper` and reviewed FFmpeg. Jobs
publish truthful progress, validate staged media, promote atomically inside the
configured library root, and trigger a coalesced rescan after success.

Artwork is lazy: scans do not load every embedded image. Visible frontend rows
request artwork through the Rust command boundary, which extracts bounded
embedded artwork and serves data URLs through a bounded cache. The frontend
cache is bounded and must retry entries evicted from its LRU.

Discovery and previews are visibility-scoped. Recommendation metadata is
validated before preview/download work; preview assets are temporary and
isolated from the persistent library until an explicit save operation.

The transfer codec implements `luno.library.export` v1. Manifests are
versioned, path-free, strictly validated, and merged additively. Fixtures in
`contracts/library-export/v1/` are part of the compatibility contract with
Luno Mobile and must remain identical in both repositories.

## Persistence and migration

Keep `src-tauri/src/migration/first_run.rs`, `keychain.rs`,
`legacy_readers.rs`, and `src-tauri/src/persistence/legacy.rs` active. Their
legacy naming describes compatibility with existing Luno user data, not the
archived Python runtime. Do not remove or rename them as part of cleanup.

Stable identity and data locations include `com.ty.luno`, existing application
support/configuration paths, migration markers, favorites, settings, queue
semantics, and transfer manifests. Any deliberate compatibility change needs
its own migration design and tests.

## Verification

```bash
npm ci
npm run typecheck
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npx tauri build
```

Use `./build-test.command` for the local end-to-end harness when a macOS GUI
launch is appropriate. Use `scripts/verify-desktop-bundle.sh` against the
generated `Luno Desktop.app` to verify resources, checksums, architectures,
versions, and no-developer-PATH behavior.

## Ownership and change discipline

This repository is Desktop-only. Mobile development is at
https://github.com/Tsohnle95/Luno-Mobile. The final Python/Tkinter/Pygame
generation is frozen under the `legacy-python-desktop-final` tag/branch and is
not an active dependency. Do not bulk rename `Luno`, change identifiers, alter
the transfer format, modernize compatibility code, or refactor unrelated
modules as part of repository maintenance.

Update this file when the Desktop file registry, architecture, state/data flow,
technical constraints, release path, or compatibility boundary changes.

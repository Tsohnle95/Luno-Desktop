# Luno Desktop

Luno Desktop is the desktop edition of Luno, built with React, TypeScript,
Vite, Tauri 2, and Rust. The stable application identifier is
`com.ty.luno`.

## Development

```bash
npm ci
npm run dev
```

The Tauri runtime and its compatibility/migration readers live under
`src-tauri/`. The desktop app keeps existing Luno user data and first-run
migration behavior intact.

## Verification

```bash
npm run typecheck
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

For a local production bundle, use `npx tauri build`. The release workflow
provisions the reviewed desktop binaries from `release-input/bin/`, packages
the resources listed in `src-tauri/tauri.conf.json`, and verifies the resulting
bundle with `scripts/verify-desktop-bundle.sh`.

## Architecture

- `src/` — React UI, typed bridge, state projections, and frontend tests.
- `src-tauri/` — Rust commands, persistence, migration, scanning, playback,
  downloads, discovery, artwork, video, and release runtime.
- `contracts/library-export/v1/` — path-free shared library-transfer schema and
  fixtures.
- `release/`, `release-input/`, `scripts/` — reviewed binary provisioning,
  third-party notices, and bundle verification.
- `docs/` — architecture, decisions, product, release, and verification docs.

## Luno Mobile and legacy desktop

Luno Mobile is now an independent Android repository:
https://github.com/Tsohnle95/Luno-Mobile

The final Python/Tkinter/Pygame desktop generation is frozen under the
`archive/python-desktop-final` branch and `legacy-python-desktop-final` tag in
this repository. It is retained for historical reference and compatibility
research; active desktop development is Tauri/Rust only.

The Desktop and Mobile repositories intentionally duplicate the small v1
transfer fixture set. Neither active application requires cloning the other
repository at build or runtime.

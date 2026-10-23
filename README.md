<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" height="96" alt="Luno app icon">
</p>

# Luno Desktop

A macOS music player for your local library, playlists, and music discovery.
Built with React, TypeScript, Tauri 2, and Rust.

[![Desktop CI](https://github.com/Tsohnle95/Luno-Desktop/actions/workflows/desktop-tauri-foundation.yml/badge.svg?branch=main)](https://github.com/Tsohnle95/Luno-Desktop/actions/workflows/desktop-tauri-foundation.yml)

[Project overview](https://tylonsohnle.com/project-studies/luno.html) ·
[Luno Mobile](https://github.com/Tsohnle95/Luno-Mobile)

## Features

- Browse your music by playlist or album, search your library, and save favorites.
- Control playback with a queue, shuffle, repeat, and macOS media keys.
- Discover recommendations, preview songs, and save them to your library.
- Download music and sync linked YouTube playlists, with a review flow for unavailable songs.
- Manage track metadata and artwork, and watch available music videos.
- Transfer your library with the shared Luno Desktop and Luno Mobile export format.

## Availability

[![Download Luno Desktop for macOS](docs/assets/releases/download-macos.svg)](https://github.com/Tsohnle95/Luno-Desktop/releases/tag/v0.1.0)

The **v0.1.0 macOS Preview** is available for **Apple Silicon Macs running
macOS 13 or newer**. [Download and installation instructions](https://github.com/Tsohnle95/Luno-Desktop/releases/tag/v0.1.0).

This preview is not Developer ID signed or notarized. macOS may block its first
launch; review the release page before installing. Intel builds are not included.

For Android, see [Luno Mobile](https://github.com/Tsohnle95/Luno-Mobile).

## Development

Requires Node 22, Rust stable, and the macOS Xcode Command Line Tools.
See the [setup and verification guide](docs/verification/desktop.md) for
toolchain details and the [release guide](docs/release/desktop.md) for
managed media binaries required by production bundles.

```bash
npm ci
npm run tauri -- dev
```

`npm run dev` starts the frontend alone; use the Tauri command above for
the desktop runtime and native playback.

## Verification

```bash
npm run verify
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

Production packaging requires the reviewed media binaries described in the
[release guide](docs/release/desktop.md). Signing and notarization require
the release owner's Apple credentials.

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

Luno Mobile is maintained in its own
[Android repository](https://github.com/Tsohnle95/Luno-Mobile).

The final Python/Tkinter/Pygame desktop generation is frozen under the
`archive/python-desktop-final` branch and `legacy-python-desktop-final` tag in
this repository. It is retained for historical reference and compatibility
research; active desktop development is Tauri/Rust only.

The Desktop and Mobile repositories intentionally duplicate the small v1
transfer fixture set. Neither active application requires cloning the other
repository at build or runtime.

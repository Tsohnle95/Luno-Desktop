# Desktop Migration Implementation Dispatch

## Wave 0: Reference And Contracts
- [x] U01 Freeze Python reference corpus and acceptance IDs
- [x] U02 Resolve migration decisions and record ADRs
- [x] U03 Define versioned Rust/TypeScript domain and IPC contracts
- [x] U04 Establish Python-versus-target differential harness
- [x] U05 Bootstrap Tauri 2, React, TypeScript, and Rust workspace

## Wave 1: Foundation Services
- [x] U06 Implement runtime ownership, bounded tasks, cancellation, and shutdown
- [x] U07 Implement atomic persistence and recovery primitives
- [x] U08 Implement structured redacted logging
- [x] U09 Implement subprocess supervision and executable discovery

## Wave 2: Compatibility And Library
- [x] U10 Implement legacy settings, marker, favorite, and preview readers
- [x] U11 Implement Track/Playlist identity and normalization policies
- [x] U12 Implement library index and exact scan semantics
- [x] U13 Implement filesystem watching and reconciliation
- [x] U14 Implement playlist CRUD and protected-root safeguards
- [x] U15 Implement favorites and path-reference repair
- [x] U16 Implement duplicate detection and allowed-duplicate backups
- [x] U17 Implement transfer codec and strict v1 validation
- [x] U18 Implement provider index, matching, and import planning

## Wave 3: Playback And Platform Controls
- [x] U19 Select and implement the Rust audio backend
- [x] U20 Implement playback queue and Up Next semantics
- [x] U21 Implement clock, seeking, completion, and failure recovery
- [x] U22 Resolve and implement crossfade semantics
- [x] U23 Implement sleep timer and desktop media-session adapters

## Wave 4: React Shell And Core UI
- [x] U24 Implement typed IPC bridge and frontend projection store
- [x] U25 Implement design tokens, responsive shell, routing, and scrolling
- [x] U26 Implement shared artwork, row, queue, and player components
- [x] U27 Implement Library and Recent views
- [x] U28 Implement Settings and Error Log views
- [x] U29 Implement transfer dialogs and library mutation dialogs

## Wave 5: Downloads And Discovery Services
- [x] U30 Implement validated media output and atomic download promotion
- [x] U31 Implement normal download job coordination and cancellation
- [x] U32 Implement search, direct URL, CSV, and URL Queue workflows
- [x] U33 Implement playlist sync, markers, and duplicate destination rules
- [x] U34 Implement Downloader workspace UI
- [x] U35 Implement discovery provider, cache, filtering, and recommendation artwork
- [x] U36 Implement preview staging and asset lifecycle
- [x] U37 Implement tap-preview sessions and queue handoff
- [x] U38 Implement Discover mode and explicit save promotion

## Wave 6: Song Page And Video
- [x] U39 Implement Song Page layout, recommendations, and UI states
- [x] U40 Implement video source lookup and temporary staging
- [x] U41 Implement audio-clock video playback, resize, seek, and fullscreen
- [x] U46 Integrate the video stage into the Song Page presentation

## Wave 7: Migration And Release
- [x] U42 Implement first-run data migration and rollback
- [x] U43 Implement Tauri packaging and dependency provisioning
- [x] U44 Add differential, visual, accessibility, and journey verification suites
- [x] U45 Complete final parity, shutdown, migration, and release sign-off

## Release Blockers
- [x] R01 Implement real macOS Keychain migration adapter
- [x] R02 Implement functional Tauri IPC domain commands and events
- [x] R03 Wire frontend projections to functional backend and video state
- [x] R04 Install target toolchains and pass Rust/TypeScript gates
- [x] R05 Produce signed macOS package with reviewed managed dependencies
- [x] R06 Execute target visual, accessibility, journey, migration, runtime/process/video, and shutdown evidence with receipts and artifacts
- [x] R07 Re-run final parity and release sign-off

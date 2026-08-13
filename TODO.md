# Desktop Migration Implementation Dispatch

## Wave 0: Reference And Contracts
- [ ] U01 Freeze Python reference corpus and acceptance IDs
- [ ] U02 Resolve migration decisions and record ADRs
- [ ] U03 Define versioned Rust/TypeScript domain and IPC contracts
- [ ] U04 Establish Python-versus-target differential harness
- [ ] U05 Bootstrap Tauri 2, React, TypeScript, and Rust workspace

## Wave 1: Foundation Services
- [ ] U06 Implement runtime ownership, bounded tasks, cancellation, and shutdown
- [ ] U07 Implement atomic persistence and recovery primitives
- [ ] U08 Implement structured redacted logging
- [ ] U09 Implement subprocess supervision and executable discovery

## Wave 2: Compatibility And Library
- [ ] U10 Implement legacy settings, marker, favorite, and preview readers
- [ ] U11 Implement Track/Playlist identity and normalization policies
- [ ] U12 Implement library index and exact scan semantics
- [ ] U13 Implement filesystem watching and reconciliation
- [ ] U14 Implement playlist CRUD and protected-root safeguards
- [ ] U15 Implement favorites and path-reference repair
- [ ] U16 Implement duplicate detection and allowed-duplicate backups
- [ ] U17 Implement transfer codec and strict v1 validation
- [ ] U18 Implement provider index, matching, and import planning

## Wave 3: Playback And Platform Controls
- [ ] U19 Select and implement the Rust audio backend
- [ ] U20 Implement playback queue and Up Next semantics
- [ ] U21 Implement clock, seeking, completion, and failure recovery
- [ ] U22 Resolve and implement crossfade semantics
- [ ] U23 Implement sleep timer and desktop media-session adapters

## Wave 4: React Shell And Core UI
- [ ] U24 Implement typed IPC bridge and frontend projection store
- [ ] U25 Implement design tokens, responsive shell, routing, and scrolling
- [ ] U26 Implement shared artwork, row, queue, and player components
- [ ] U27 Implement Library and Recent views
- [ ] U28 Implement Settings and Error Log views
- [ ] U29 Implement transfer dialogs and library mutation dialogs

## Wave 5: Downloads And Discovery Services
- [ ] U30 Implement validated media output and atomic download promotion
- [ ] U31 Implement normal download job coordination and cancellation
- [ ] U32 Implement search, direct URL, CSV, and URL Queue workflows
- [ ] U33 Implement playlist sync, markers, and duplicate destination rules
- [ ] U34 Implement Downloader workspace UI
- [ ] U35 Implement discovery provider, cache, filtering, and recommendation artwork
- [ ] U36 Implement preview staging and asset lifecycle
- [ ] U37 Implement tap-preview sessions and queue handoff
- [ ] U38 Implement Discover mode and explicit save promotion

## Wave 6: Song Page And Video
- [ ] U39 Implement Song Page layout, recommendations, and UI states
- [ ] U40 Implement video source lookup and temporary staging
- [ ] U41 Implement audio-clock video playback, resize, seek, and fullscreen
- [ ] U46 Integrate the video stage into the Song Page presentation

## Wave 7: Migration And Release
- [ ] U42 Implement first-run data migration and rollback
- [ ] U43 Implement Tauri packaging and dependency provisioning
- [ ] U44 Add differential, visual, accessibility, and journey verification suites
- [ ] U45 Complete final parity, shutdown, migration, and release sign-off

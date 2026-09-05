# Desktop Architecture

Luno Desktop is the active React/TypeScript/Vite/Tauri 2/Rust application in
this repository. The historical Python/Tkinter/Pygame implementation is
archived under `legacy-python-desktop-final`; it is not an active runtime
dependency.

## Runtime boundary

The React frontend in `src/` talks to the Rust runtime in `src-tauri/` through
the typed command/event bridge. Rust owns persistence, migration, library
scanning, playback, queue policy, downloads, discovery, artwork, previews, and
video state. Blocking filesystem, process, and network work stays off the UI
thread.

Compatibility readers under `src-tauri/src/migration/` and
`src-tauri/src/persistence/legacy.rs` remain active because they read existing
Luno user data. Their `legacy` names do not refer to the archived Python
source.

## Shared boundary

The only intentional cross-repository contract is the versioned library export
manifest. It contains metadata, playlist membership, and optional provider
identifiers; it never contains local paths, secrets, audio bytes, queue state,
or history. The schema and fixtures are in
`contracts/library-export/v1/` and are duplicated in Luno Mobile.

## Release boundary

`release/`, `release-input/`, `src-tauri/resources/`, and `scripts/` are active
Desktop release infrastructure. Provisioning and bundle verification fail
closed for missing or unreviewed FFmpeg, ffprobe, and yt-dlp inputs. The stable
application identifier is `com.ty.luno`; the human-facing product is
`Luno Desktop`.

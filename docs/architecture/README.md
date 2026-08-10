# Architecture

## Applications

The repository contains two product implementations:

### Desktop

`music_player.py` is the Tkinter controller. Views live in `views/`, playback
is wrapped by `LunoEngine` in `engine.py`, and downloads are handled by
`downloader.py`. Heavy filesystem, network, and media work must remain off the
Tk thread; UI mutations return through Tk's event loop.

### Android

`mobile-app/` is a native Kotlin application. `data/` owns persistence,
storage, discovery, artwork, and transfer operations. `playback/` owns Media3,
download workers, and source extraction. `ui/` owns Compose screens and shared
components.

## Shared Boundary

The only intentional cross-platform runtime contract is the versioned library
export manifest. It contains metadata, playlist membership, and optional
provider identifiers. It never contains local paths, secrets, audio bytes,
queue state, or history.

The contract is defined in
`contracts/library-export/v1/README.md` and implemented independently by
`library_transfer.py` and Android's `LibraryManifestCodec`.

## Current Refactoring Constraints

- Preserve desktop Pygame lifecycle and 44.1 kHz download output.
- Preserve the desktop 56 px virtual-list row contract.
- Keep Android Room migrations append-only.
- Do not introduce shared runtime code between Python and Kotlin merely to
  avoid duplication; shared fixtures and wire compatibility are the priority.

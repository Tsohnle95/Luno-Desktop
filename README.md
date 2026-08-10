# Luno

Luno is a local-first music player implemented as two applications in one
repository:

- **Desktop:** Python, Tkinter, Pygame, yt-dlp, and FFmpeg.
- **Android:** Kotlin, Jetpack Compose, Media3, Room, SAF, and WorkManager.

The applications share a versioned, path-free library transfer format. They do
not share playback or UI runtime code.

## Repository Guide

- `music_player.py`, `engine.py`, `views/`: desktop application
- `mobile-app/`: native Android application
- `library_transfer.py`: desktop implementation of the transfer contract
- `contracts/library-export/v1/`: language-neutral contract and fixtures
- `docs/`: architecture, decisions, and product documentation
- `project_brain.md`: desktop operational index
- `mobile-app/project_brain.md`: Android operational index

Source code and tests are authoritative when documentation is stale.

## Verification

Desktop tests:

```bash
python -m pytest
```

Android unit tests and lint:

```bash
cd mobile-app
./gradlew testDebugUnitTest lintDebug
```

Desktop packaging requires the local `.venv` and external FFmpeg binary. The
Android debug helper is `./build-mobile.command`.

## Documentation

- [Architecture overview](docs/architecture/README.md)
- [Transfer contract](contracts/library-export/v1/README.md)
- [Architecture decisions](docs/decisions/README.md)
- [Product roadmap](docs/product/roadmap.md)

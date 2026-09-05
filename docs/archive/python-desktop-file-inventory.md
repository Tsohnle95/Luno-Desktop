# Final Python Desktop File Inventory

This inventory defines the exact Python/Tkinter/Pygame boundary preserved by
`archive/python-desktop-final` and `legacy-python-desktop-final`. The archive
branch/tag is the source of truth; this file makes the dependency boundary
reviewable before removal from active Desktop `main`.

## Runtime and packaging

- `music_player.py` — Tkinter controller and application entry point.
- `engine.py` — Pygame playback engine and compatibility exports.
- `downloader.py` — yt-dlp/FFmpeg download and playlist synchronization.
- `discovery.py` — Last.fm discovery service.
- `previews.py` — recommendation preview resolution and staging.
- `rec_artwork.py` — recommendation artwork fallback generation.
- `video_preview.py` — temporary video probing and preview helpers.
- `source_lookup.py` — source lookup/probing helpers.
- `library_tools.py` — filesystem scanning, CSV parsing, and duplicate logic.
- `library_transfer.py` — v1 path-free transfer codec and merge planner.
- `theme.py` — Tk palette and playlist gradient helpers.
- `utils.py` — resource paths, formatting, track-name splitting, and shared helpers.
- `widgets.py` — custom Tk canvas widgets.
- `views/` — all Tk frames: `library.py`, `downloader.py`, `recent.py`,
  `discover.py`, `settings.py`, `song_page.py`, `error_log.py`, and
  `__init__.py` for PyInstaller package recognition.
- `icon.icns` — macOS application icon used by PyInstaller.
- `Luno.spec` — PyInstaller analysis, hidden imports, resources, and bundle
  identifier `com.ty.luno`.
- `build.command` — supported legacy macOS build helper.
- `requirements.txt` — Python runtime, test, PyInstaller, PyObjC, Pygame,
  Pillow, mutagen, requests, and yt-dlp dependencies.
- `pytest.ini` — legacy test collection configuration.
- `.github/workflows/desktop.yml` — Python desktop verification workflow.

## Tests and differential harness

- `test_art_cache.py`
- `test_contract_fixtures.py`
- `test_discover_mode.py`
- `test_dl.py`
- `test_downloader_sync.py`
- `test_engine_playback.py`
- `test_focus.py`
- `test_label_ph.py`
- `test_library_transfer.py`
- `test_media_keys.py`
- `test_tkvideo.py` — ad-hoc Tk/video experiment retained as historical test material.
- `test_video_preview.py`
- `tests/differential/` — U44 receipt and harness tests for comparing legacy
  behavior with the migration target.
- `tools/differential/` — reference, comparator, and runner helpers used by
  the differential harness.

## Boundary audit

The runtime import graph begins at `music_player.py` and reaches the runtime
modules and `views/` listed above. `requirements.txt`, `Luno.spec`,
`build.command`, `pytest.ini`, the Python tests, the differential harness, the
icon, and the Python CI workflow are included because they are required to
run, test, package, or understand the final generation. Tauri `src/`,
`src-tauri/`, `scripts/`, `release/`, `release-input/`, and the shared transfer
fixtures are not legacy Python files and remain active in Luno Desktop.

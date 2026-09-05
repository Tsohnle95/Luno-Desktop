# Luno Legacy Python Desktop

This branch and tag preserve the final Python/Tkinter/Pygame desktop generation
of Luno before active desktop development moved to React/TypeScript/Tauri/Rust.
It is frozen and unsupported, but retained for historical reference,
compatibility research, and rollback/debugging.

Active desktop development: https://github.com/Tsohnle95/Luno-Desktop

Mobile development: https://github.com/Tsohnle95/Luno-Mobile

The exact preserved boundary is recorded in
`docs/archive/python-desktop-file-inventory.md`. For local development, use
Python 3.12, create `.venv`, install `requirements.txt`, and run `python -m
pytest`. The PyInstaller entry point is `Luno.spec`; `build.command` is the
legacy macOS helper. Do not treat this archive as an active release target.

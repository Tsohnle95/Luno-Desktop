# AGENTS.md - Luno Operating Rules

## Start Here

1. Read `project_brain.md` before changing the desktop application.
2. Read `mobile-app/project_brain.md` before changing the Android application.
3. Treat source and tests as the highest authority when a brain document is stale.
4. Inspect only the files needed for the current task and preserve unrelated user changes.

## Desktop Constraints

- Audio output is normalized to 44.1 kHz through FFmpeg.
- Preserve the pygame engine lifecycle.
- Keep heavy yt-dlp and filesystem scanning work off the UI thread.
- Schedule UI mutations with `.after(0, callback)`.
- Preserve the documented 56 px virtual-list row height and scroll-region clamping behavior.

## Documentation

- Synchronize the applicable project brain when a code change alters behavior, architecture, interfaces, data flow, state, technical constraints, or the documented file registry.
- Update the file tree when files are added, removed, or moved.
- Update desktop data-flow and state sections when library scanning, playback, downloading, artwork, `_library`, `engine.queue`, normalization keys, or title-splitting behavior changes.
- Update the `views/` registry when a view is added or its unique responsibility changes.
- Do not rewrite brain documents for style-only or test-only changes that leave documented facts unchanged.

## Verification

- Use the configured harness checks when available.
- For desktop playback changes, run `desktop-playback`.
- For Android behavior, run `android-unit`.
- A timeout, skipped required check, or truncated command without a receipt is inconclusive, not a pass.

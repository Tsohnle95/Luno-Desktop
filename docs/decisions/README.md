# Architecture Decisions

Document role: index of durable desktop decisions; each linked decision
doc owns its own truth. Current-state architecture lives in
`project_brain.md`.

Each decision should record its context, chosen approach, consequences, and
status. Historical implementation details belong in the changelog rather than
in the architecture overview.

## Existing Decisions

- Production downloads produce M4A/AAC, not MP3
  (`download-format-m4a.md`): the reviewed FFmpeg build has no MP3
  encoder, so the bundled helper requests `--audio-format m4a` and
  promotion accepts the MP4 `ftyp` signature.
- The library export format is versioned and path-free
  (`contracts/library-export/v1/README.md`, implemented in
  `src-tauri/src/transfer/`): metadata-only manifests, additive imports,
  fixtures duplicated with Luno Mobile.
- This repository is Desktop-only: Luno Mobile lives at
  `https://github.com/Tsohnle95/Luno-Mobile`; the final Python desktop
  generation is frozen under the `legacy-python-desktop-final` tag and
  `archive/python-desktop-final` branch.

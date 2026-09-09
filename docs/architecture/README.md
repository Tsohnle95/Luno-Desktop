# Desktop Architecture

Document role: routing pointer to the canonical owners; holds no
architectural truth itself. Canonical cross-system architecture and
invariants live in `project_brain.md` (repository root).
If this file ever conflicts with it, this file is wrong.

- Runtime boundary, data flow, persistence, verification, ownership:
  `project_brain.md`.
- Bridge, envelopes, transfer contract: `docs/modules/contracts.md`.
- Frontend (shell, views, projections): `docs/modules/frontend.md`.
- Library, persistence, migration: `docs/modules/library.md`.
- Playback, video, media keys: `docs/modules/playback.md`.
- Downloads, discovery, previews, toolchain:
  `docs/modules/downloads-discovery.md`.
- Setup, verification, CI: `docs/verification/desktop.md`.
- Release, bundle, signing: `docs/release/desktop.md`.
- Agent execution process: `docs/agent-execution.md`.

# Task: Repair playlist and recommendation artwork

Status: ACTIVE

## Goal / acceptance
- Playlist tiles retain four equal quadrants for 1–4 songs, regardless of image dimensions or fallback state.
- Recently downloaded uses that tile in sidebar and collection hero.
- Recommendations preserve usable provider art and resolve missing art through the existing ArtworkService.
- Regression tests reproduce failures; targeted, canonical and direct layout validation pass.

## Context / affected surfaces
`docs/modules/frontend.md`, `docs/modules/downloads-discovery.md`; PlaylistArtwork, sidebar/hero CSS, DiscoveryService and ArtworkService.

## Invariants / review decisions
- No bridge changes. Backend owns recommendation metadata and artwork resolution.
- Preserve valid Last.fm covers. Filter known library entries before external lookups.
- Bound missing-cover resolution to four workers and a six-second batch deadline; individual HTTP requests time out. Do not cache transient failures as definitive misses.
- Reuse the existing bounded artwork cache; no new runtime workers survive command completion.
- Tests use injected local provider URLs, never production network.
- Repeat available covers to fill four equal cells; empty playlists keep their icon.
- Review: sequential network lookups would stall recommendation readiness; bounded scoped workers and request deadlines limit that risk. Artwork failures must never fail discovery.

## Phases
1. Complete: mosaic reproduction and fix; 49 targeted tests passed; checkpoint a8628a51.
2. Complete: regression failed with None before resolver integration; 7 discovery tests pass. Canonical verify: 227 frontend tests/docs pass; Rust: 260 pass, 6 ignored; production frontend build passes.
3. Active: Firefox confirms 36 cells each occupy exactly 25% of their tile. Release build/install and native UI check underway; cumulative review and final checkpoint remain.

## Risks
Provider outages still require generated art; packaged installation is separate from source validation.

# Task: Repair playlist and recommendation artwork

Status: REVIEWED

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
1. Mosaic reproduction, layout fix, targeted validation and checkpoint.
2. Recommendation lookup reproduction, bounded resolver integration and targeted validation.
3. Canonical gates, direct visual check, cumulative review, final checkpoint and remove task file.

## Risks
Provider outages still require generated art; packaged installation is separate from source validation.

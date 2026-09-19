# Task: Repair recommendation context and progressive video staging

Status: REVIEWED

## Goal

Make focused recommendation playback preserve the physical queue, keep a
rolling five-item temporary audio window, render deterministic custom fallback
art everywhere, and restore the video pipeline as a fast quick-tier publish
followed by a background high-tier replacement with truthful progress.

## Acceptance criteria

- [ ] Missing or rejected recommendation artwork uses `RecommendationArtwork`
      in the song-page media stage and backdrop; the gray/star fallback is
      unreachable for a recommendation identity.
- [ ] Focused recommendation playback never projects recommendation successors
      into the authoritative queue. Global Next returns to the next physical
      item captured from the active library/playlist context.
- [ ] The initial recommendation prefetch prepares five items without changing
      playback. Tapping item N keeps N plus its next five items in the desired
      rolling window and serially prepares every missing item.
- [ ] Discover mode alone projects recommendations into transport/autoplay;
      its rolling window remains five.
- [ ] Video mode stages the audible track plus five successors from the
      authoritative context: physical successors during focused preview and
      projected recommendation successors during Discover mode.
- [ ] Current video staging publishes quick-tier progress, serves the quick
      file immediately, then upgrades and republishes the high tier.
- [ ] Search, store admission, cancellation, and spawn failures always resolve
      Loading; no spinner can survive without a live worker.
- [ ] The real bundled yt-dlp/FFmpeg path stages and probes a playable quick
      MP4, and the custom `luno-video` stream serves it in bounded ranges.

## Invariants

- Rust owns physical queue context, temporary asset windows, worker admission,
  and video quality state. React renders snapshots.
- Focused preview is an audible overlay over a captured physical queue, not a
  transport queue. Discover is the only temporary queue projection.
- Audio and video heavy work remains serialized and cancellable; interactive
  current work has priority over speculative successors and quality upgrades.
- A worker publishes only into its own generation/track slot.
- Quick and high files are validated as H.264/AAC MP4 before publication.

## Plan review

- The existing custom artwork generator already owns 75 stable designs. Reuse
  it at every fallback surface instead of adding another asset system.
- Keep focused and Discover wire commands compatible; fix backend window and
  queue ownership rather than adding frontend queue policy.
- Progress must include the search/resolution phase as indeterminate and the
  media transfer as determinate. A lack of percentage during search is valid;
  a lack of percentage after yt-dlp starts is a failure to publish progress.
- Five audio previews and five video successors have different resource cost.
  Audio stays serialized; video uses quick-tier current first and sequential
  background prefetch with cancellation.
- Live provider timing is supporting evidence. Controlled process fixtures own
  deterministic failure, progress, tier, and handoff assertions.
- Adversarial review found four lifecycle traps that must be tested directly:
  a store-pressure or wait-cap return can orphan `Loading`; a focused preview
  key has no library metadata; React currently suppresses staging while that
  preview owns audio; and eager high-tier successor downloads can block the
  current quick tier. The repair must remove every silent exit, resolve
  metadata from recommendation seeds, follow audible identity, and prefetch
  successors at the quick tier.
- Source lookup currently launches every fallback query before it ranks the
  first result. Stop once enough relevant candidates exist and reject generic
  long-form/live-session matches for ordinary song requests.

## Phases

| Phase | Scope | Status |
|---|---|---|
| 1 | Reproduce artwork, focused queue, rolling audio, and video failures | complete |
| 2 | Repair recommendation artwork and focused session ownership | pending |
| 3 | Repair rolling five-item audio preparation | pending |
| 4 | Repair progressive video staging and five-successor prefetch | pending |
| 5 | Live smoke, canonical gates, docs, cumulative review | pending |

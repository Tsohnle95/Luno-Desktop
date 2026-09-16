import { renderToStaticMarkup } from "react-dom/server";
import type { Track } from "../../shared/contracts";
import { Recommendations, recommendationPreviewState, type Recommendation } from "../../components/Recommendations";
import { VideoStage } from "../../components/VideoStage";
import { DiscoverView } from "../../views/DiscoverView";
import { SongPageView, VideoModePill } from "../../views/SongPageView";

const track: Track = { id: "seed", path: null, title: "Seed", artist: "Artist", album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null };
const recommendation: Recommendation = { id: "rec", title: "Similar", artist: "Other" };

export const songPageChecks = {
  page: Boolean(SongPageView) && Boolean(Recommendations),
  mediaRatio: "16 / 9",
  pageSize: 25,
  cap: 1000,
  seedIdentity: track.id === "seed" && recommendation.id !== track.id,
  // Favorites toggle lives in the footer PlayerBar, not the song page body.
  favoriteUnavailable: !renderToStaticMarkup(<SongPageView track={track} backendAvailable={false} />).includes("Add to Favorites") && !renderToStaticMarkup(<SongPageView track={track} backendAvailable={false} />).includes("song-page-favorite-help"),
};

if (!songPageChecks.page || songPageChecks.pageSize !== 25 || songPageChecks.cap !== 1000 || !songPageChecks.seedIdentity || !songPageChecks.favoriteUnavailable) throw new Error("Song page contract failed");

import { describe, expect, it } from "vitest";

describe("Song page video mode", () => {
  it("keeps the options menu retired in favor of stage hover controls", () => {
    // The legacy ⋯ menu was removed entirely; video mode now lives in the
    // hover controls rendered by VideoStage on the media slot itself.
    const html = renderToStaticMarkup(<SongPageView track={track} backendAvailable />);
    expect(html).not.toContain('aria-label="Song page options"');
    const wired = renderToStaticMarkup(<SongPageView track={track} backendAvailable onVideoModeChange={() => undefined} videoMode />);
    expect(wired).not.toContain('aria-label="Song page options"');
    expect(wired).toContain('data-testid="video-stage"');
  });

  it("renders the pill toggle with its current state", () => {
    const off = renderToStaticMarkup(<VideoModePill enabled={false} />);
    expect(off).toContain('data-testid="video-mode-toggle"');
    expect(off).toContain(">Off</span>");
    expect(off).toContain('aria-checked="false"');
    const on = renderToStaticMarkup(<VideoModePill enabled />);
    expect(on).toContain("is-on");
    expect(on).toContain('aria-checked="true"');
    expect(on).toContain(">On</span>");
  });

  it("describes the prefetch behavior inside the menu copy", () => {
    // The legacy ⋯ options menu was replaced by the hover controls on the
    // media stage; the song page no longer renders any menu anchor.
    const wired = renderToStaticMarkup(<SongPageView track={track} backendAvailable onVideoModeChange={() => undefined} />);
    expect(wired).not.toContain("luno-song-menu-anchor");
  });
});

describe("Music-mode hover controls", () => {
  it("renders the mode toggle over ARTWORK (mode off, no video loaded)", () => {
    // Regression: controls used to be gated on showVideo, which made music
    // mode impossible to re-enable once turned off — the exact reported bug.
    const html = renderToStaticMarkup(
      <SongPageView track={track} backendAvailable videoMode={false} onVideoModeChange={() => undefined} />
    );
    expect(html).toContain('aria-label="Music mode off"');
    expect(html).toContain('data-testid="video-hover-controls"');
  });

  it("shows fullscreen only when a video is actually playing", () => {
    const artworkOnly = renderToStaticMarkup(<VideoStage artworkUrl="art.png" videoModeEnabled={false} onVideoModeChange={() => undefined} />);
    expect(artworkOnly).not.toContain("luno-video-fullscreen");
    expect(artworkOnly).not.toContain("<video");
    const withVideo = renderToStaticMarkup(<VideoStage src="clip.mp4" status="playing" videoModeEnabled onVideoModeChange={() => undefined} />);
    expect(withVideo).toContain("luno-video-fullscreen");
  });

  it("never shows the ghost Load-music-video button while mode is off", () => {
    // Regression: with videoMode off the button rendered whenever no src was
    // set, resurrecting a stale control after the user turned mode off.
    const html = renderToStaticMarkup(
      <SongPageView track={track} backendAvailable videoConnection="connected" onLoadVideo={() => undefined} videoMode={false} onVideoModeChange={() => undefined} />
    );
    expect(html).not.toContain("Load music video");
    expect(html).not.toContain("Retry music video");
    // Mode ON + no source + a failed/absent attempt => the only allowed
    // manual control, phrased as a retry (never the old ghost button).
    const retryHtml = renderToStaticMarkup(
      <SongPageView track={track} backendAvailable videoConnection="connected" onLoadVideo={() => undefined} videoMode onVideoModeChange={() => undefined} video={{ status: "unavailable" }} />
    );
    expect(retryHtml).toContain("Video unavailable");
    const idleHtml = renderToStaticMarkup(
      <SongPageView track={track} backendAvailable videoConnection="connected" onLoadVideo={() => undefined} videoMode onVideoModeChange={() => undefined} />
    );
    // No failure recorded => nothing to retry => no manual button at all.
    // (Class-name greps hit the embedded stylesheet; assert on labels.)
    expect(idleHtml).not.toContain("Retry music video");
    expect(idleHtml).not.toContain(">Load music video<");
  });
});

describe("Audible recommendation identity", () => {
  const recommendationTrack: Track = { id: "Other-Similar", path: null, title: "Similar", artist: "Other", album: null, duration_ms: null, artwork_url: "rec.png", favorite: false, provider: null };

  it("renders the recommendation's title/art while its preview owns the output", () => {
    const html = renderToStaticMarkup(<SongPageView track={track} nowPlaying={recommendationTrack} backendAvailable />);
    expect(html).toContain("Similar");
    expect(html).toContain("rec.png");
    expect(html).not.toContain(">Seed</h1>");
  });
});

describe("Discovery mode toggle", () => {
  const recs: Recommendation[] = [{ artist: "Other", title: "Similar", match_score: 0.9, artwork_url: null }];

  it("offers an enable toggle when recommendations are loaded", () => {
    const html = renderToStaticMarkup(
      <SongPageView track={track} backendAvailable recommendations={recs} recommendationState="ready" onToggleDiscoverMode={() => undefined} />
    );
    expect(html).toContain('aria-label="Enable discovery mode"');
    expect(html).toContain("Discovery mode: Off");
  });

  it("announces the on state and restores library wording", () => {
    const html = renderToStaticMarkup(
      <SongPageView track={track} backendAvailable recommendations={recs} recommendationState="ready" discoverMode onToggleDiscoverMode={() => undefined} discoverHint="Pre-loaded 2 of 5 recommendations." />
    );
    expect(html).toContain('aria-label="Disable discovery mode"');
    expect(html).toContain("Discovery mode: On");
    expect(html).toContain("Pre-loaded 2 of 5 recommendations.");
  });

  it("hides the toggle when no handler is wired", () => {
    const html = renderToStaticMarkup(<SongPageView track={track} backendAvailable />);
    expect(html).not.toContain("Discovery mode:");
  });
});

describe("Artistless seed honesty", () => {
  // Regression: files without "Artist - Title" metadata (e.g. "[ spaceboy ]",
  // which has no separator and no ID3 artist tag) publish `artist: None`, so
  // the Last.fm seed — which requires an artist — is null. Every downstream
  // surface used to degrade silently or misleadingly ("Play a song to see
  // recommendations" while one was playing; a dead disabled toggle), which
  // read as "discovery is completely broken".
  const artistless: Track = { id: "spaceboy", path: null, title: "[ spaceboy ]", artist: null, album: null, duration_ms: null, artwork_url: null, favorite: true, provider: null };

  it("explains the missing artist instead of asking the user to play a song", () => {
    const html = renderToStaticMarkup(
      <Recommendations seed={artistless} items={[]} state="idle" />
    );
    expect(html).toContain("has no artist info");
    expect(html).not.toContain("Play a song to see recommendations");
  });

  it("keeps the refresh control disabled when there is no artist to look up", () => {
    const html = renderToStaticMarkup(
      <Recommendations seed={artistless} items={[]} state="idle" onRefresh={() => undefined} />
    );
    expect(html).toContain('aria-label="Refresh recommendations"');
    expect(html).toContain("disabled");
  });

  it("still invites a genuinely trackless state to play something", () => {
    const html = renderToStaticMarkup(<Recommendations seed={null} items={[]} state="idle" />);
    expect(html).toContain("Play a song to see recommendations");
  });

  it("names the artistless track on the Discover route instead of asking for a seed", () => {
    const base = {
      recommendations: [], session: null, saved: {}, playlists: [], loading: false, error: null,
      onRefresh: () => undefined, onTap: () => undefined, onNext: () => undefined,
      onPrevious: () => undefined, onCancel: () => undefined, onSave: () => undefined,
    };
    const honest = renderToStaticMarkup(<DiscoverView seed={null} artistMissingTitle="[ spaceboy ]" {...base} />);
    expect(honest).toContain("has no artist info");
    expect(honest).not.toContain("Select or play a track to seed recommendations.");
    const trackless = renderToStaticMarkup(<DiscoverView seed={null} {...base} />);
    expect(trackless).toContain("Select or play a track to seed recommendations.");
  });
});

describe("Recommendation preview progress", () => {
  const first: Recommendation = { title: "First", artist: "Artist" };
  const second: Recommendation = { title: "Second", artist: "Artist" };
  const firstKey = "Artist-First";
  const secondKey = "Artist-Second";

  it("does not make transfer completion playable before the asset is published", () => {
    const state = recommendationPreviewState(first, {
      generation: 1,
      active: true,
      current: null,
      window: [firstKey],
      prepared: [],
      unavailable: [],
      cancelled: false,
      progress: [{ key: firstKey, percent: 100 }],
    });
    expect(state.progress).toBe(100);
    expect(state.prepared).toBe(false);
  });

  it("shows source resolution only for the serialized active item", () => {
    const html = renderToStaticMarkup(
      <Recommendations
        seed={track}
        items={[first, second]}
        state="ready"
        preview={{ generation: 1, active: true, current: null, window: [firstKey, secondKey], prepared: [], unavailable: [], cancelled: false, progress: [] }}
      />
    );
    expect(html).toContain("Finding a playable source");
    expect(html).toContain("Queued for temporary preview");
    expect(html.match(/<span class="luno-video-progress/g)).toHaveLength(1);
  });
});

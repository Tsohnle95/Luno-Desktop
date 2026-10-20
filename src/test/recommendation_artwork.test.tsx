import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  RECOMMENDATION_ART_DESIGN_COUNT,
  RecommendationArtwork,
  RecommendationArtworkImage,
  isPlaceholderArtwork,
  normalizedRecommendationKey,
  recommendationArtworkDesignIndex,
} from "../components/RecommendationArtwork";
import { PlayerBar } from "../components/PlayerBar";
import { Queue } from "../components/Queue";
import { VideoStage } from "../components/VideoStage";
import { SongPageView } from "../views/SongPageView";
import type { Track } from "../shared/contracts";

const providerPlaceholder = "https://lastfm.freetls.fastly.net/i/u/300x300/default.jpg";
const recommendationTrack: Track = {
  id: "provider-rec",
  path: null,
  title: "Track",
  artist: "Artist",
  album: null,
  duration_ms: 120_000,
  artwork_url: providerPlaceholder,
  favorite: false,
  provider: null,
};

describe("recommendation artwork policy", () => {
  it.each([
    ["https://lastfm-img.freetls.fastly.net/i/u/300x300/2a96cbd8b46e442fc41c2b86b821562f.png", "current Last.fm sentinel host"],
    ["https://lastfm-img.freetls.fastly.net/i/u/300x300/default.png", "current Last.fm default host"],
    [undefined, "missing"],
    [null, "null"],
    ["", "empty"],
    ["   ", "whitespace"],
    ["https://lastfm.freetls.fastly.net/i/u/300x300/2a96cbd8b46e442fc41c2b86b821562f.jpg", "Last.fm sentinel"],
    ["https://lastfm.freetls.fastly.net/i/u/300x300/default.jpg", "Last.fm default"],
    ["https://last.fm/images/no_image.png", "Last.fm no image"],
    ["https://last.fm/images/placeholder.jpg", "Last.fm placeholder"],
  ])("recognizes %s as %s", (url, _description) => {
    expect(isPlaceholderArtwork(url)).toBe(true);
  });

  it.each([
    "https://lastfm.freetls.fastly.net/i/u/300x300/abc123/cover.jpg",
    "https://cdn.example.test/placeholder.jpg",
    "data:image/png;base64,AAAA",
    "/Users/me/Music/last.fm-live.m4a",
  ])("keeps valid non-provider artwork usable: %s", (url) => {
    expect(isPlaceholderArtwork(url)).toBe(false);
  });

  it("uses one identity key and design for equivalent artist/title spelling", () => {
    expect(normalizedRecommendationKey("Beyoncé", "Déjà Vu")).toBe("beyonce|deja vu");
    expect(recommendationArtworkDesignIndex("Beyoncé", "Déjà Vu")).toBe(
      recommendationArtworkDesignIndex("beyonce", "deja vu"),
    );
  });

  it("keeps every deterministic design colorful and free of the retired cross glyph", () => {
    const seen = new Set<number>();
    for (let target = 0; target < RECOMMENDATION_ART_DESIGN_COUNT; target += 1) {
      let artist = "Artist 0";
      let title = "Track 0";
      for (let i = 0; i < RECOMMENDATION_ART_DESIGN_COUNT * 100; i += 1) {
        const candidateArtist = `Artist ${i}`;
        const candidateTitle = `Track ${i * 7 + 3}`;
        if (recommendationArtworkDesignIndex(candidateArtist, candidateTitle) === target) {
          artist = candidateArtist;
          title = candidateTitle;
          break;
        }
      }
      const html = renderToStaticMarkup(<RecommendationArtwork artist={artist} title={title} />);
      seen.add(Number(html.match(/data-design="(\d+)"/)?.[1]));
      expect(html).not.toContain("border-left");
      expect(html).not.toContain("border-right");
      expect(html).not.toContain("width:0");
      expect(html).not.toContain("height:0");
      expect(html).not.toContain("#1C1917");
      expect(html).not.toContain("#171717");
      expect(html).not.toContain("#292524");
    }
    expect(seen.size).toBe(RECOMMENDATION_ART_DESIGN_COUNT);
  });

  it("turns a provider placeholder into the same custom art used by missing artwork", () => {
    const placeholderHtml = renderToStaticMarkup(
      <RecommendationArtworkImage artist="Artist" title="Track" artworkUrl={providerPlaceholder} />,
    );
    const missingHtml = renderToStaticMarkup(<RecommendationArtworkImage artist="Artist" title="Track" />);
    expect(placeholderHtml).toContain('class="luno-rec-art"');
    expect(placeholderHtml).not.toContain(providerPlaceholder);
    expect(placeholderHtml).toContain('data-design="');
    expect(placeholderHtml).toContain(placeholderHtml.match(/data-design="\d+"/)?.[0] ?? "");
    expect(placeholderHtml).toBe(missingHtml);
  });

  it("routes provider placeholders through the same fallback on every playback surface", () => {
    const player = renderToStaticMarkup(<PlayerBar track={recommendationTrack} />);
    const queue = renderToStaticMarkup(<Queue tracks={[recommendationTrack]} />);
    const stage = renderToStaticMarkup(<VideoStage artworkUrl={providerPlaceholder} artworkArtist="Artist" artworkTitle="Track" />);
    const songPage = renderToStaticMarkup(<SongPageView track={recommendationTrack} backendAvailable />);

    for (const html of [player, queue, stage, songPage]) {
      expect(html).not.toContain(providerPlaceholder);
      expect(html).toContain('data-design="');
    }
    expect(songPage.match(/data-design="/g)).toHaveLength(2);
  });
});

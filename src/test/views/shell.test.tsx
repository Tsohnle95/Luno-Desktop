import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PlayerBar } from "../../components/PlayerBar";
import { Shell } from "../../app/Shell";
import type { BackendProjection } from "../../state/backend_projection";

const projectionTrack = { id: "t1", path: null, title: "Aurora", artist: "Gate Six", album: "Chaos 93", duration_ms: 3000, artwork_url: null, favorite: false, provider: null };
const projection = {
  getSnapshot: (): BackendProjection => ({ library: { version: 1, generation: 1, tracks: [projectionTrack], playlists: [{ id: "p1", name: "Rap", path: null, url: null, ordered_track_ids: ["t1"] }] }, playback: { queue: ["t1"], current_index: 0, shuffle: false, repeat: "off", status: "playing", position_ms: 1250, duration_ms: 3000, generation: 1 }, preview: null, video: null, jobs: {}, saved: {}, errors: [], recentHistory: [], shutdown: null, generations: { library: 1, playback: 1, preview: -1, video: -1, jobs: -1, errors: -1 }, lastEventIds: {} }),
  subscribe: () => () => {},
};

describe("Gate 6 player bar", () => {
  it("exposes labeled transport, scrubber, and volume controls", () => {
    const html = renderToStaticMarkup(<PlayerBar />);
    expect(html).toContain("Nothing playing");
    expect(html).toContain('aria-label="Previous track"');
    expect(html).toContain('aria-label="Next track"');
    expect(html).toContain('aria-label="Playback position"');
    expect(html).toContain('aria-label="Volume"');
  });

  it("announces playing state through the toggle label and position values", () => {
    const html = renderToStaticMarkup(<PlayerBar track={{ id: "t1", title: "Aurora", artist: "Gate Six", album: null, path: null, duration_ms: 3000, artwork_url: null, favorite: false, provider: null }} playback={{ queue: ["t1"], current_index: 0, shuffle: false, repeat: "off", status: "playing", position_ms: 1250, duration_ms: 3000, generation: 1 }} />);
    expect(html).toContain('aria-label="Pause"');
    expect(html).toContain("Aurora");
    expect(html).toContain("0:01");
    expect(html).toContain("0:03");
  });

  it("renders the Spotify-style frame: top bar nav, search, Your Library sidebar, and queue panel", () => {
    const html = renderToStaticMarkup(<Shell backendProjection={projection} backendAvailable={false} />);
    expect(html).toContain('aria-label="Primary navigation"');
    expect(html).toContain('aria-label="Home" aria-current="page"');
    expect(html).toContain("What do you want to play?");
    // All other destinations live inside the settings rail — no icons in the bar.
    expect(html).not.toContain('title="Discover"');
    expect(html).not.toContain('title="Downloader"');
    expect(html).not.toContain('title="Recent"');
    expect(html).toContain("Your Library");
    expect(html).toContain('aria-label="Show playlists"');
    expect(html).toContain('aria-label="Show albums"');
    expect(html).toContain("All Music");
    expect(html).toContain("Favorites");
    expect(html).toContain('data-slot="queue"');
    expect(html).toContain('aria-label="Close queue panel"');
    expect(html).toContain('aria-label="Queue tab"');
    expect(html).toContain(">Recently played</button>");
    expect(html).toContain("Now playing");
    expect(html).toContain("Next from: Rap");
  });

  it("lands on the Home dashboard, keeps migration silent, and reaches settings via the top-left profile", () => {
    const html = renderToStaticMarkup(<Shell backendProjection={projection} backendAvailable={false} />);
    expect(html).not.toContain("Migration completed");
    expect(html).toContain("Made for you");
    expect(html).toContain("Favorited songs");
    expect(html).toContain("Recently played");
    expect(html).toContain("Most popular songs");
    expect(html).toContain("Most popular playlists");
    expect(html).toContain("Recently downloaded");
    // Profile avatar sits at the FAR top-right, after the primary nav, and
    // opens the dedicated settings page (no dropdown).
    expect(html.indexOf('aria-label="Primary navigation"')).toBeLessThan(html.indexOf('aria-label="Open settings"'));
    expect(html).not.toContain("aria-haspopup");
  });

  it("exposes exactly one current navigation entry across the primary nav", () => {
    const html = renderToStaticMarkup(<Shell backendProjection={projection} backendAvailable={false} />);
    const region = html.split('aria-label="Primary navigation"')[1]?.split("</nav>")[0] ?? "";
    expect((region.match(/aria-current="page"/g) ?? []).length).toBe(1);
  });

  it("wires the player transport to shuffle, repeat, and the queue toggle", () => {
    const html = renderToStaticMarkup(<Shell backendProjection={projection} backendAvailable={false} />);
    expect(html).toContain('aria-label="Enable shuffle"');
    expect(html).toContain('aria-label="Repeat off"');
    expect(html).toContain('aria-label="Close queue panel"');
  });
});
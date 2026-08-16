import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PlayerBar } from "../../components/PlayerBar";

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
    const html = renderToStaticMarkup(<PlayerBar track={{ id: "t1", title: "Aurora", artist: "Gate Six", album: null, path: null, duration_ms: 3000, artwork_url: null, favorite: false, provider: null }} playback={{ status: "playing", track_id: "t1", position_ms: 1250, duration_ms: 3000, shuffle: false, repeat: "off", volume: 1, generation: 1 }} />);
    expect(html).toContain('aria-label="Pause"');
    expect(html).toContain("Aurora");
    expect(html).toContain("0:01");
    expect(html).toContain("0:03");
  });
});
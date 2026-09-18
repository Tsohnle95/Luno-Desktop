import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DownloaderView } from "../../views/DownloaderView";
import { LibraryView } from "../../views/LibraryView";

const runningJob = { id: "job-1", kind: "single" as const, state: "running" as const, track_id: null, progress: 0.5, generation: 1 };
const libraryTrack = (id: string, title: string, album: string | null, favorite = false) => ({ id, path: null, title, artist: "Artist", album, duration_ms: 90_000, artwork_url: null, favorite, provider: null });
const snapshot = { version: 1 as const, generation: 1, tracks: [libraryTrack("a", "Zed", "Chaos 93"), libraryTrack("b", "Alpha", null, true)], playlists: [] };
const playback = { queue: ["a"], current_index: 0, shuffle: false, repeat: "off" as const, status: "playing" as const, position_ms: 1000, duration_ms: 90_000, generation: 1 };

describe("Gate 6 downloader view", () => {
  it("renders disclosure buttons with aria-expanded instead of native details", () => {
    const html = renderToStaticMarkup(<DownloaderView />);
    expect(html).not.toContain("<details");
    expect(html).not.toContain("<summary");
    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("Progress");
    expect(html).toContain("Download log");
    expect(html).toContain("Downloaded songs");
  });

  it("renders tablist with seven wired tabs and an explicit panel", () => {
    const html = renderToStaticMarkup(<DownloaderView />);
    expect(html).toContain('role="tablist"');
    expect(html).toContain('role="tabpanel"');
    expect((html.match(/role="tab"/g) ?? []).length).toBe(7);
    expect(html).toContain('aria-controls="download-panel-search"');
    expect(html).toContain('aria-labelledby="download-tab-search"');
  });

  it("keeps only the active tab in the tab order (roving tabindex)", () => {
    const html = renderToStaticMarkup(<DownloaderView />);
    expect((html.match(/id="download-tab-[a-z-]+" tabindex="0"/g) ?? [])).toHaveLength(1);
    expect((html.match(/id="download-tab-[a-z-]+" tabindex="-1"/g) ?? [])).toHaveLength(6);
    expect(html).toContain('id="download-tab-search" tabindex="0"');
  });

  it("exposes progress, cancel, and live-region affordances for active jobs", () => {
    const html = renderToStaticMarkup(<DownloaderView jobs={{ "job-1": runningJob }} />);
    expect(html).toContain("Downloading — 50%");
    expect(html).toContain("luno-video-progress");
    expect(html).toContain("Cancel");
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain('aria-label="Download progress"');
  });

  it("retains an explicit completion status after the active job finishes", () => {
    const completed = { ...runningJob, state: "succeeded" as const, progress: 1 };
    const html = renderToStaticMarkup(<DownloaderView jobs={{ "job-1": completed }} />);
    expect(html).toContain("Download complete — added to your library.");
    expect(html).toContain('role="status"');
  });

  it("shows the resolved song name beside active progress", () => {
    const html = renderToStaticMarkup(<DownloaderView jobs={{ "job-1": runningJob }} activeLabel="Rick Astley — Never Gonna Give You Up" />);
    expect(html).toContain("Downloading — Rick Astley — Never Gonna Give You Up — 50%");
  });

  it("renders an animated search indicator without the obsolete 30-second warning", () => {
    const html = renderToStaticMarkup(<DownloaderView searching={true} />);
    expect(html).toContain("luno-download-spinner");
    expect(html).toContain("Searching YouTube…");
    expect(html).not.toContain("30 seconds");
  });

  it("disables OS autocorrect and spellcheck on the downloader search field", () => {
    // Autocorrect silently rewrote pasted queries/URLs; every text entry point
    // opts out. Only the active tab's fields are server-rendered here.
    const html = renderToStaticMarkup(<DownloaderView />);
    expect(html).toMatch(/<input[^>]*aria-label="Search for music"[^>]*autocorrect="off"[^>]*spellcheck="false"/i);
  });
});
describe("Gate 6 library view", () => {
  it("announces the loading state while the first snapshot is pending", () => {
    const html = renderToStaticMarkup(<LibraryView loading={true} />);
    expect(html).toContain('role="status"');
    expect(html).toContain("Loading library...");
  });

  it("renders the Spotify-style hero with scope title, meta, and action row", () => {
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} playback={playback} />);
    expect(html).toContain("All Music");
    expect(html).toContain("2 songs");
    expect(html).toContain('aria-label="Play All Music"');
    expect(html).toContain('aria-label="Enable shuffle"');
    expect(html).toContain('aria-label="Scan library for changes"');
    expect(html).toContain('aria-label="Sort tracks"');
    expect(html).toContain("luno-table-head");
  });

  it("keeps the virtualized 56px row list with column cells and favorite toggles", () => {
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} playback={playback} />);
    expect((html.match(/luno-track-row/g) ?? []).length).toBeGreaterThanOrEqual(2);
    expect(html).toContain('aria-label="Add Zed to favorites"');
    expect(html).toContain("Alpha");
  });

  it("resolves album scopes from client-side album groups", () => {
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} playback={playback} selectedScope="album:Chaos 93" />);
    expect(html).toContain("Album");
    expect(html).toContain('aria-label="Play Chaos 93"');
  });

  it("scrolls the hero away and pins only the column head", () => {
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} playback={playback} />);
    expect(html).toContain("luno-hero");
    expect(html).toContain("luno-table-head");
    expect(html).toContain("luno-track-scroller");
  });
});

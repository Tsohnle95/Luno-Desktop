import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DownloaderView, destinationName, filterDownloadDestinations, linkedPlaylistForDestination } from "../../views/DownloaderView";
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

  it("keeps search and result actions available while multiple transfers run", () => {
    const html = renderToStaticMarkup(<DownloaderView
      jobs={{
        "job-1": runningJob,
        "job-2": { ...runningJob, id: "job-2", progress: 0.25 },
      }}
      activeLabels={{ "job-1": "First song", "job-2": "Second song" }}
      searchResults={[{ id: "next-song", title: "Next song", artist: "Artist" }]}
    />);
    expect(html).toContain("2 downloads active");
    expect(html).toContain("First song");
    expect(html).toContain("Second song");
    expect(html).toMatch(/<input[^>]*aria-label="Search for music"(?![^>]*disabled)/);
    expect(html).toContain('aria-label="Download Next song"');
  });

  it("shows an indeterminate finalization phase after transfer reaches the handoff", () => {
    const finalizing = { ...runningJob, progress: 0.97, phase: "finalizing" as const };
    const html = renderToStaticMarkup(<DownloaderView jobs={{ "job-1": finalizing }} />);
    expect(html).toContain("Finalizing download — checking and adding the file");
    expect(html).not.toContain(">97%</b>");
  });

  it("keeps the transfer boundary truthful until the backend announces finalization", () => {
    const html = renderToStaticMarkup(<DownloaderView jobs={{ "job-1": { ...runningJob, progress: 0.95 } }} />);
    expect(html).toContain("Downloading — 95%");
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

  it("offers a searchable destination field and filters playlist names", () => {
    const html = renderToStaticMarkup(<DownloaderView
      destinations={["/music/Unsorted", "/music/Live Shows", "/music/Favorites"]}
      defaultDestination="/music/Unsorted"
    />);
    expect(html).toContain('role="combobox"');
    expect(html).toContain('aria-label="Save to"');
    expect(html).toContain('placeholder="Search playlists"');
    expect(destinationName("/music/Live Shows")).toBe("Live Shows");
    expect(filterDownloadDestinations(["/music/Unsorted", "/music/Live Shows", "/music/Favorites"], "live")).toEqual(["/music/Live Shows"]);
  });

  it("renders the Shell-owned destination after the Downloader view remounts", () => {
    const html = renderToStaticMarkup(<DownloaderView
      destinations={["/music/Unsorted", "/music/Live Shows"]}
      defaultDestination="/music/Unsorted"
      destination="/music/Live Shows"
    />);
    expect(html).toContain('aria-label="Save to"');
    expect(html).toContain('value="Live Shows"');
  });

  it("resolves Save to to the linked physical playlist used by sync", () => {
    const roadTrip = { id: "p1", name: "Road trip", path: "/music/Road trip", url: "https://www.youtube.com/playlist?list=PL123", ordered_track_ids: [] };
    const unlinked = { id: "p2", name: "Unlinked", path: "/music/Unlinked", url: null, ordered_track_ids: [] };
    expect(linkedPlaylistForDestination([roadTrip, unlinked], "/music/Road trip")).toBe(roadTrip);
    expect(linkedPlaylistForDestination([roadTrip, unlinked], "/music/Unlinked")).toBeUndefined();
  });

  it("does not offer the single-download retry action for failed playlist sync jobs", () => {
    const failedSync = { ...runningJob, id: "sync-1", kind: "sync" as const, state: "failed" as const };
    const html = renderToStaticMarkup(<DownloaderView jobs={{ "sync-1": failedSync }} />);
    expect(html).toContain("playlist sync: failed");
    expect(html).not.toContain(">Retry</button>");
  });

  it("labels active playlist work as a sync", () => {
    const html = renderToStaticMarkup(<DownloaderView jobs={{ sync: { ...runningJob, kind: "sync" } }} />);
    expect(html).toContain("Syncing playlist — 50%");
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
    expect(html).not.toContain('aria-label="Scan library for changes"');
    expect(html).toContain('aria-label="Sort title Z to A"');
    expect(html).toContain('aria-label="Sort latest added first"');
    expect(html).toContain('aria-label="Sort shortest duration first"');
    expect(html).not.toContain('aria-label="Filter tracks"');
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

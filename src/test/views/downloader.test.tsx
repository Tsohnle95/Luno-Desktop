import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DownloaderView, DownloadSyncPanel, DownloadSongRow, destinationName, filterDownloadDestinations, linkedPlaylistForDestination } from "../../views/DownloaderView";
import { LibraryView } from "../../views/LibraryView";

const runningJob = { id: "job-1", kind: "single" as const, state: "running" as const, track_id: null, progress: 0.5, generation: 1 };
const libraryTrack = (id: string, title: string, album: string | null, favorite = false) => ({ id, path: null, title, artist: "Artist", album, duration_ms: 90_000, artwork_url: null, favorite, provider: null });
const snapshot = { version: 1 as const, generation: 1, tracks: [libraryTrack("a", "Zed", "Chaos 93"), libraryTrack("b", "Alpha", null, true)], playlists: [] };
const playback = { queue: ["a"], current_index: 0, shuffle: false, repeat: "off" as const, status: "playing" as const, position_ms: 1000, duration_ms: 90_000, generation: 1 };

describe("complete download results", () => {
  it("leaves the idle results area quiet", () => {
    const html = renderToStaticMarkup(<DownloaderView />);
    expect(html).not.toContain("Find your next favorite");
    expect(html).not.toContain("Search by song or artist, paste a YouTube link");
    expect(html).not.toContain('width="25"');
  });
  it("identifies a playlist and accounts for every source entry against its destination", () => {
    const item = { provider: "youtube" as const, query: "song", title: "Missing song", artist: null, duration: null, thumbnail_url: null, url: "https://youtube.com/watch?v=missing" };
    const html = renderToStaticMarkup(<DownloaderView destination="/music/Live Shows" searchResults={[{ id: item.url, title: item.title }]} playlistPreview={{ url: "https://www.youtube.com/playlist?list=PL1", title: "My live playlist", total: 5, downloaded: 2, unavailable: 1, skipped: 1, items: [item] }} />);
    expect(html).toContain("My live playlist");
    expect(html).toContain("YOUTUBE PLAYLIST");
    expect(html).toContain("5 songs · 2 already in Live Shows");
    expect(html).toContain("1 missing");
    expect(html).toContain("1 unavailable");
    expect(html).toContain("1 skipped after review");
    expect(html).toContain('aria-label="Download Missing song"');
  });
  it("always exposes Sync all, a URL field, and Sync playlist even for an unlinked destination", () => {
    const linked = { id: "p1", name: "Rap", path: "/music/Rap", url: "https://youtube.com/playlist?list=one", ordered_track_ids: [] };
    const html = renderToStaticMarkup(<DownloadSyncPanel destinations={["/music/Unsorted", "/music/Rap"]} destination="/music/Unsorted" playlists={[linked]} onDestinationChange={() => {}} />);
    expect(html).toContain("Sync all (1)");
    expect(html).toContain('placeholder="Paste a YouTube playlist URL"');
    expect(html).toContain("Sync playlist");
    expect(html).toContain("Linked playlists");
    expect(html).toContain('aria-label="Sync Rap"');
    expect(html).not.toContain("Use Link playlist in Your Library");
    expect(html).not.toMatch(/disabled=""[^>]*>Sync all/);
  });
  it("paginates a complete playlist without dropping later songs from Download all", () => {
    const songs = Array.from({ length: 61 }, (_, index) => ({ id: `song-${index}`, title: `Song ${index}` }));
    const html = renderToStaticMarkup(<DownloaderView searchResults={songs} />);
    expect((html.match(/role="listitem"/g) ?? [])).toHaveLength(25);
    expect(html).toContain('aria-label="Results pages"');
    expect(html).toContain("1–25 of 61");
    expect(html).toContain("Next");
    expect(html).toContain("Download all (61)");
  });
});

describe("one-click download rows", () => {
  it("downloads search and CSV songs without selecting them or staging a queue", () => {
    const html = renderToStaticMarkup(<DownloaderView searchResults={[{ id: "video1", title: "First song" }]} previewTracks={[{ id: "csv:1", title: "Imported song" }]} />);
    expect(html).not.toContain('type="checkbox"');
    expect(html).not.toContain("+ Queue");
    expect(html).not.toContain("Download queue (");
    expect(html).toContain('aria-label="Download First song"');
    expect(html).toContain('aria-label="Download Imported song"');
    expect(html).not.toContain("Download log");
    expect(html).not.toContain("Downloaded songs");
  });
});

describe("automatic download presentation", () => {
  const item = { id: "one", title: "First song", artist: "Artist", source: "https://youtube.com/watch?v=one", destination: "/music/Rap" };
  it("keeps exactly Search, Sync and Duplicates with one selected tab", () => {
    const html = renderToStaticMarkup(<DownloaderView />);
    expect((html.match(/role="tab"/g) ?? [])).toHaveLength(3);
    expect((html.match(/aria-selected="true"/g) ?? [])).toHaveLength(1);
    expect(html).toContain('role="tabpanel"');
    expect(html).toContain("Import CSV");
    expect(html).not.toContain("Direct URL");
  });
  it("shows progress and cancellation on the downloading song row", () => {
    const html = renderToStaticMarkup(<DownloadSongRow item={{ ...item, activity: { ...item, status: "running", jobId: "job1", progress: 0.42 } }} />);
    expect(html).toContain("Downloading 42%");
    expect(html).toContain('aria-label="Cancel First song"');
    expect(html).toContain('aria-valuenow="42"');
    expect(html).not.toContain('aria-label="Download First song"');
  });
  it("shows next and numbered waiting positions directly beside each song", () => {
    const html = renderToStaticMarkup(<DownloaderView activity={[{ ...item, status: "queued", position: 1 }, { ...item, id: "two", source: "second", title: "Second song", status: "queued", position: 2 }]} />);
    expect(html).toContain("Next in queue");
    expect(html).toContain("#2 in queue");
    expect(html).toContain('aria-label="Cancel Second song"');
    expect(html).not.toContain("Download queue (");
  });
  it("keeps finalization indeterminate until actual completion", () => {
    const html = renderToStaticMarkup(<DownloadSongRow item={{ ...item, activity: { ...item, status: "running", finalizing: true, progress: 0.97 } }} />);
    expect(html).toContain("Adding to library…");
    expect(html).not.toContain('aria-valuenow=');
    expect(html).not.toContain("Downloaded");
  });
  it("places actionable errors and retry on the failed row", () => {
    const html = renderToStaticMarkup(<DownloadSongRow item={{ ...item, activity: { ...item, status: "failed", error: "YouTube refused access. Try another source." } }} onRetry={() => {}} />);
    expect(html).toContain("YouTube refused access. Try another source.");
    expect(html).toContain('aria-label="Retry First song"');
    expect(html).not.toContain('role="alert"');
  });
  it("keeps downloads visible and new song actions enabled across searches", () => {
    const html = renderToStaticMarkup(<DownloaderView activity={[{ ...item, status: "running", progress: .5 }]} searchResults={[{ id: "new", title: "New result" }]} />);
    expect(html).toContain("First song");
    expect(html).toContain('aria-label="Download New result"');
    expect(html).not.toMatch(/aria-label="Download New result"[^>]*disabled/);
  });
  it("updates a result in place rather than moving it into a new progress section", () => {
    const html = renderToStaticMarkup(<DownloaderView destination="/music/Rap" searchResults={[{ ...item, id: item.source }]} activity={[{ ...item, status: "running", progress: .42 }]} />);
    expect((html.match(/role="listitem"/g) ?? [])).toHaveLength(1);
    expect(html).toContain("Downloading 42%");
    expect(html).not.toContain('aria-label="Your downloads"');
  });
  it("shows completion and cancellation on their songs", () => {
    const html = renderToStaticMarkup(<DownloaderView activity={[{ ...item, status: "succeeded" }, { ...item, id: "cancelled", source: "other", title: "Other song", status: "cancelled" }]} />);
    expect(html).toContain("Downloaded");
    expect(html).toContain("Cancelled");
    expect(html).toContain('aria-label="Retry Other song"');
  });
  it("disables lookup inputs during search but leaves active Cancel available", () => {
    const html = renderToStaticMarkup(<DownloaderView searching activity={[{ ...item, status: "running" }]} />);
    expect(html).toContain('aria-label="Import CSV"');
    expect(html).toMatch(/aria-label="Search for music"[^>]*disabled=""/);
    expect(html).toContain('aria-label="Cancel First song"');
    expect(html).toContain('autoCorrect="off"');
    expect(html).toContain('spellCheck="false"');
  });
  it("preserves destination selection and typed filtering", () => {
    expect(destinationName("/music/Live Shows")).toBe("Live Shows");
    expect(filterDownloadDestinations(["/music/Unsorted", "/music/Live Shows"], "live")).toEqual(["/music/Live Shows"]);
    const html = renderToStaticMarkup(<DownloaderView destinations={["/music/Unsorted", "/music/Live Shows"]} destination="/music/Live Shows" />);
    expect(html).toContain('value="Live Shows"');
  });
  it("resolves the linked physical destination for Sync", () => {
    const playlist = { id: "p1", name: "Rap", path: "/music/Rap", url: "https://youtube.com/playlist?list=one", ordered_track_ids: [] };
    expect(linkedPlaylistForDestination([playlist], "/music/Rap")).toBe(playlist);
    expect(linkedPlaylistForDestination([playlist], "/music/Other")).toBeUndefined();
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

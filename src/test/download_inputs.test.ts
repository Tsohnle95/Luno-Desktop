import { describe, expect, it } from "vitest";
import { downloadDuplicateGroups, parseAllowedDownloadDuplicates } from "../state/download_duplicates";
import { captureQueuedDownload, csvDownloadSource } from "../state/download_inputs";
describe("unified download sources", () => {
  it("resolves CSV metadata to a real provider query rather than passing csv:1 to yt-dlp", () => {
    expect(csvDownloadSource({ artist: "Artist", title: "Song" })).toBe("ytsearch1:Artist Song");
    expect(csvDownloadSource({ artist: "Artist", title: "Song", url: "https://open.spotify.com/track/123" })).toBe("ytsearch1:Artist Song");
    expect(csvDownloadSource({ title: "Song", url: "https://youtube.com/watch?v=123" })).toBe("https://youtube.com/watch?v=123");
  });
  it("captures source, metadata and destination before a later search or CSV replaces results", () => {
    const row = { source: "ytsearch1:Artist Song", title: "Song", artist: "Artist" };
    const queued = captureQueuedDownload("csv:1", "/music/Playlist", row);
    row.source = "ytsearch1:Other song"; row.title = "Other song";
    expect(queued).toEqual({ id: "csv:1", source: "ytsearch1:Artist Song", destination: "/music/Playlist", title: "Song", artist: "Artist" });
  });
  it("keeps duplicate review preferences reversible without changing source tracks", () => {
    const track = { id: "one", title: "Song", artist: "Artist", path: "/music/a.m4a", album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null };
    const tracks = [track, { ...track, id: "two", path: "/music/other/a.m4a" }];
    const groups = downloadDuplicateGroups(tracks, []);
    expect(groups).toHaveLength(1);
    const kept = parseAllowedDownloadDuplicates(JSON.stringify([groups[0].key, groups[0].key]));
    expect(kept).toHaveLength(1);
    expect(downloadDuplicateGroups(tracks, kept)).toEqual([]);
    expect(downloadDuplicateGroups(tracks, [])).toEqual(groups);
    expect(() => parseAllowedDownloadDuplicates('{"replace":true}')).toThrow();
    expect(track.path).toBe("/music/a.m4a");
  });

});

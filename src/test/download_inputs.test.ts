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

import { DownloadLookupGate, requeueDownloadAfterBusy } from "../state/download_inputs";
describe("download request ownership", () => {
  it("keeps the newest lookup authoritative across a slow file read and a newer search", async () => {
    const gate = new DownloadLookupGate();
    let resolveOld!: (value: string) => void;
    let resolveNew!: (value: string) => void;
    let busy = false;
    let result = "old visible rows";
    const errors: unknown[] = [];
    const handlers = { start: () => { busy = true; result = ""; }, success: (value: string) => { result = value; }, error: (error: unknown) => errors.push(error), finish: () => { busy = false; } };
    const old = gate.run(() => new Promise<string>((resolve) => { resolveOld = resolve; }), handlers);
    expect(busy).toBe(true);
    expect(result).toBe("");
    const latest = gate.run(() => new Promise<string>((resolve) => { resolveNew = resolve; }), handlers);
    resolveOld("stale file rows"); await old;
    expect(result).toBe("");
    expect(busy).toBe(true);
    resolveNew("new search rows"); await latest;
    expect(result).toBe("new search rows");
    expect(busy).toBe(false);
    expect(errors).toEqual([]);
  });

  it("clears old CSV rows immediately and reports file-read failures before preview work", async () => {
    const gate = new DownloadLookupGate();
    let busy = false;
    let rows = ["previous file song"];
    let error: unknown = null;
    const unreadable = new Error("File could not be read");
    const lookup = gate.run(async () => { throw unreadable; }, {
      start: () => { busy = true; rows = []; }, success: () => { rows = ["unexpected"]; },
      error: (value) => { error = value; }, finish: () => { busy = false; },
    });
    expect(busy).toBe(true);
    expect(rows).toEqual([]);
    await lookup;
    expect(busy).toBe(false);
    expect(rows).toEqual([]);
    expect(error).toBe(unreadable);
  });

  it("does not restore cancelled waiting work after its outstanding admission reports Busy", () => {
    const queue = ["cancelled pending song"];
    const oldEpoch = 0;
    queue.length = 0;
    const cancellationEpoch = 1;
    queue.push("newly requested song");
    expect(requeueDownloadAfterBusy(queue, "old admission", oldEpoch, cancellationEpoch)).toBe(false);
    expect(queue).toEqual(["newly requested song"]);
    expect(requeueDownloadAfterBusy(queue, "current admission", cancellationEpoch, cancellationEpoch)).toBe(true);
    expect(queue).toEqual(["current admission", "newly requested song"]);
  });
});

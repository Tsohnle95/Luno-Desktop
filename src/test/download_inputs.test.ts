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
  it("retains each duplicate copy ID and its actual playlist membership", () => {
    const track = { id: "one", title: "Song", artist: "Artist", path: "/music/A/song.m4a", album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null };
    const groups = downloadDuplicateGroups([track, { ...track, id: "two", path: "/music/B/song.m4a" }], [], [
      { id: "a", name: "Chill", path: "/music/A", url: null, ordered_track_ids: ["one"] },
      { id: "b", name: "Favorites mix", path: "/music/B", url: null, ordered_track_ids: ["two"] },
    ]);
    expect(groups[0].tracks).toEqual([
      { id: "one", title: "Song", artist: "Artist", path: "/music/A/song.m4a", playlists: ["Chill"] },
      { id: "two", title: "Song", artist: "Artist", path: "/music/B/song.m4a", playlists: ["Favorites mix"] },
    ]);
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

import { takeNextDownload } from "../state/download_inputs";
describe("automatic sequential downloads", () => {
  it("finishes one request before taking the next, including later playlist and CSV batches", () => {
    const pending = ["song1", "song2", "song3", "song4"];
    expect(takeNextDownload(pending, 0)).toBe("song1");
    pending.push("playlist1-a", "playlist1-b", "playlist2-a", "csv1-a", "csv2-a");
    expect(takeNextDownload(pending, 1)).toBeUndefined();
    const remaining: string[] = [];
    while (pending.length) remaining.push(takeNextDownload(pending, 0)!);
    expect(remaining).toEqual(["song2", "song3", "song4", "playlist1-a", "playlist1-b", "playlist2-a", "csv1-a", "csv2-a"]);
  });
});

import { downloadActivityRows, appendDownloadItems } from "../state/download_inputs";
it("preserves captured song, playlist and CSV rows after searches are replaced, with positions and errors", () => {
  const song = { id: "song", source: "https://youtube.com/watch?v=one", destination: "/music/A", title: "Song", artist: "Artist" };
  const playlist = { id: "playlist-one", source: "https://youtube.com/watch?v=two", destination: "/music/B", title: "Playlist song" };
  const csv = captureQueuedDownload("csv:1", "/music/C", { source: "ytsearch1:CSV song", title: "CSV song" });
  const rows = downloadActivityRows({ active: song }, [playlist, csv], { active: { id: "active", kind: "single", state: "running", track_id: null, progress: .4, generation: 1 } });
  expect(rows.map((r) => [r.title, r.status, r.position, r.destination])).toEqual([["Song", "running", undefined, "/music/A"], ["Playlist song", "queued", 1, "/music/B"], ["CSV song", "queued", 2, "/music/C"]]);
  const pending = [playlist, csv];
  appendDownloadItems(pending, [song, playlist, csv], rows);
  expect(pending).toEqual([playlist, csv]);
  const failed = downloadActivityRows({ active: song }, [], { active: { id: "active", kind: "single", state: "failed", track_id: null, progress: null, generation: 1 } }, { active: "YouTube refused access." });
  expect(failed[0].error).toBe("YouTube refused access.");
  appendDownloadItems(pending, [song], failed);
  expect(pending.at(-1)).toEqual(song);
});

import { awaitDownloadAdmission } from "../state/download_inputs";
it("retains an outstanding start and delivers cancellation after a delayed acknowledgement", async () => {
  let acknowledge!: () => void;
  let cancelled = false;
  const calls: string[] = [];
  const pending = ["next song"];
  const admission = awaitDownloadAdmission(new Promise<void>((resolve) => { acknowledge = resolve; }), () => cancelled, async () => { calls.push("cancel"); });
  expect(takeNextDownload(pending, 1)).toBeUndefined();
  cancelled = true;
  expect(calls).toEqual([]);
  acknowledge();
  await admission;
  expect(calls).toEqual(["cancel"]);
  expect(takeNextDownload(pending, 0)).toBe("next song");
});

it("shows retried songs at the end of the waiting rows, matching their actual FIFO position", () => {
  const retry = { id: "retry", source: "retry", destination: "/music", title: "Retry song" };
  const next = { id: "next", source: "next", destination: "/music", title: "Next song" };
  const rows = downloadActivityRows({ oldFailure: retry }, [next, retry], { oldFailure: { id: "oldFailure", kind: "single", state: "failed", track_id: null, progress: null, generation: 1 } });
  expect(rows.map((row) => [row.id, row.position])).toEqual([["next", 1], ["retry", 2]]);
});

it("does not mistake a cancellation failure after admission for a failed start", async () => {
  const cancellationError = new Error("job not found");
  const errors: unknown[] = [];
  await expect(awaitDownloadAdmission(Promise.resolve(), () => true, async () => { throw cancellationError; }, (error) => errors.push(error))).resolves.toBeUndefined();
  expect(errors).toEqual([cancellationError]);
});

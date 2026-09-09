import { describe, expect, it, vi } from "vitest";
import { backendCommandArgs, createBridge, isValidLibraryResponse, parseMigrationStatus, tauriTransport } from "../app/bridge";
import { createBackendProjection, mergeVideoSnapshot } from "../state/backend_projection";
import { currentVideoForTrack, playRecentTrack, runBridgeCommand, safeVideoSource, videoCommandsEnabled } from "../app/Shell";
import type { BridgeTransport } from "../app/bridge";
import type { Event, EventEnvelope } from "../shared/contracts";
import type { SaveState } from "../shared/contracts";
import { isCommandEnvelope, isEventEnvelope, isPreviewSnapshot, isDiscoverSnapshot, isSearchResults } from "../shared/contracts";

const nativeInvoke = vi.hoisted(() => vi.fn());
// convertFileSrc mirrors Tauri's injected macOS runtime implementation
// (`<protocol>://localhost/<encoded path>`) so Shell source-building is
// testable without a live webview.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: nativeInvoke,
  convertFileSrc: (filePath: string, protocol = "asset") => `${protocol}://localhost/${encodeURIComponent(filePath)}`,
}));

const event = (event: Event, generation = 1): EventEnvelope => ({ version: 1, event_id: `event-${generation}`, generation, job_id: null, session_id: null, event });

describe("typed IPC bridge and projection", () => {
  it("passes Gate 5 fixture roots to the native command without changing no-arg calls", async () => {
    nativeInvoke.mockResolvedValue({});

    await tauriTransport.invoke("gate5_smoke", {
      root: "/tmp/library",
      preview_root: "/tmp/previews",
      video_root: "/tmp/video",
    });
    await tauriTransport.invoke("migration_status", null);
    await tauriTransport.invoke("backend_command", { version: 1 });

    expect(nativeInvoke.mock.calls).toEqual([
      ["gate5_smoke", { root: "/tmp/library", preview_root: "/tmp/previews", video_root: "/tmp/video" }],
      ["migration_status"],
      ["backend_command", { payload: { version: 1 } }],
    ]);
  });

  it("rejects a start_download envelope without a source and accepts one with it", () => {
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "start_download", payload: { job_id: "j", destination: "/d/j.mp3", helper: "/h" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "start_download", payload: { job_id: "j", destination: "/d/j.mp3", helper: "/h", source: "ytsearch1:query" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "bundle_helper_path" } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover", payload: { artist: "Artist", title: "Track", limit: 5 } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover", payload: { artist: "", title: "Track", limit: 5 } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover", payload: { artist: "Artist", title: "Track", limit: 0 } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover", payload: { artist: "Artist", title: "Track", limit: 21 } } })).toBe(false);
    // search_downloader now carries an explicit result limit (Load More).
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "search_downloader", payload: { query: "artist track", try_soundcloud: true, limit: 25 } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "search_downloader", payload: { query: "artist track", try_soundcloud: true } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "search_downloader", payload: { query: "artist track", try_soundcloud: true, limit: 0 } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "search_downloader", payload: { query: "artist track", try_soundcloud: true, limit: 51 } } })).toBe(false);
  });

  it("accepts rich downloader search results and rejects the legacy provider-query shape", () => {
    const rich = [{ provider: "youtube", query: "ytsearch1:artist track", title: "Spaceboy", artist: "Smashing Pumpkins", duration: "4:28", thumbnail_url: "https://i.ytimg.com/vi/x/hqdefault.jpg", url: "https://www.youtube.com/watch?v=x" }];
    expect(isSearchResults(rich)).toBe(true);
    // Legacy provider-query-only results (no title) are no longer accepted.
    expect(isSearchResults([{ provider: "youtube", query: "ytsearch1:artist track" }])).toBe(false);
    expect(isSearchResults([{ ...rich[0], title: "" }])).toBe(false);
  });

  it("validates preview command envelopes", () => {
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "preview_tap", payload: { seeds: [{ artist: "Artist", title: "Track" }], key: "Artist-Track" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "preview_tap", payload: { seeds: [], key: "Artist-Track" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "preview_tap", payload: { seeds: [{ artist: "", title: "Track" }], key: "Artist-Track" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "preview_tap", payload: { seeds: [{ artist: "Artist", title: "Track" }, { artist: "B", title: "C" }], key: "Artist-Track" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "preview_tap", payload: { seeds: [{ artist: "Artist", title: "Track" }], key: "" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "preview_next" } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "preview_next", payload: {} } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "preview_previous" } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "preview_cancel" } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover_enable", payload: { seeds: [{ artist: "Artist", title: "Track" }] } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover_disable" } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover_advance" } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover_enable", payload: { seeds: "x" } } })).toBe(false);
  });

  it("validates save command envelopes", () => {
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "save", payload: { key: "Artist0-Track0", playlist_id: "playlist:playlist-a" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "save", payload: { key: "", playlist_id: "playlist:playlist-a" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "save", payload: { key: "Artist0-Track0", playlist_id: "" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "save", payload: { key: "Artist0-Track0" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "save" } })).toBe(false);
  });

  it("validates playlist and track mutation envelopes", () => {
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "create_playlist", payload: { name: "Road trip" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "create_playlist", payload: { name: "" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "add_tracks_to_playlist", payload: { track_ids: ["track-1"], playlist_id: "playlist:Road trip" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "add_tracks_to_playlist", payload: { track_ids: [], playlist_id: "playlist:Road trip" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "delete_tracks", payload: { track_ids: ["track-1"] } } })).toBe(true);
  });

  it("validates save_state event envelopes", () => {
    const base: SaveState = { key: "Artist0-Track0", playlist_id: "playlist:playlist-a", state: "saved", already_saved: false, path: "/library/playlist-a/Artist0 - Track0.m4a", job_id: null, error: null };
    const saveEvent = (payload: unknown) => event({ type: "save_state", payload: payload as { state: SaveState } });
    expect(isEventEnvelope(saveEvent({ state: base }))).toBe(true);
    expect(isEventEnvelope(saveEvent({ state: { ...base, state: "saving" } }))).toBe(true);
    expect(isEventEnvelope(saveEvent({ state: { ...base, state: "queued" } }))).toBe(true);
    expect(isEventEnvelope(saveEvent({ state: { ...base, state: "failed", error: "no staged audio" } }))).toBe(true);
    expect(isEventEnvelope(saveEvent({ state: { ...base, state: "bogus" } }))).toBe(false);
    expect(isEventEnvelope(saveEvent({ state: { ...base, already_saved: "yes" } }))).toBe(false);
    expect(isEventEnvelope(saveEvent({ state: { ...base, key: "" } }))).toBe(false);
    expect(isEventEnvelope(saveEvent({ state: { ...base, extra: 1 } }))).toBe(false);
  });

  it("validates preview snapshots and preview_snapshot event envelopes", () => {
    const snapshot = { generation: 4, active: true, current: "Artist-Track", window: ["Artist-Track"], prepared: ["Artist-Track"], unavailable: [], cancelled: false };
    expect(isPreviewSnapshot(snapshot)).toBe(true);
    expect(isPreviewSnapshot({ ...snapshot, active: "yes" })).toBe(false);
    expect(isPreviewSnapshot({ ...snapshot, current: 3 })).toBe(false);
    expect(isPreviewSnapshot({ ...snapshot, window: ["", "x"] })).toBe(false);
    expect(isPreviewSnapshot({ ...snapshot, extra: 1 })).toBe(false);
    expect(isEventEnvelope(event({ type: "preview_snapshot", payload: { snapshot } }))).toBe(true);
    expect(isEventEnvelope(event({ type: "preview_snapshot", payload: { snapshot: { ...snapshot, unavailable: "x" } as unknown as typeof snapshot } }))).toBe(false);
    const discover = { generation: 2, enabled: true, window: ["Artist-Track"], ready: 1, queued: 1 };
    expect(isDiscoverSnapshot(discover)).toBe(true);
    expect(isDiscoverSnapshot({ ...discover, enabled: "yes" })).toBe(false);
    expect(isDiscoverSnapshot({ ...discover, window: [""] })).toBe(false);
    expect(isDiscoverSnapshot({ ...discover, extra: 1 })).toBe(false);
    expect(isEventEnvelope(event({ type: "discover_snapshot", payload: { snapshot: discover } }, 2))).toBe(true);
  });

  it("uses the required native backend command payload argument shape", () => {
    const envelope = { version: 1, request_id: "request-1", command: { type: "get_library" } };
    expect(backendCommandArgs(envelope)).toEqual({ payload: envelope });
  });

  it("wraps commands and validates subscriptions", async () => {
    const calls: unknown[] = [];
    let callback: (payload: unknown) => void = () => {};
    const transport: BridgeTransport = { invoke: async <T>(_name: string, payload: unknown) => { calls.push(payload); return null as T; }, listen: async <T>(_name: string, handler: (payload: T) => void) => { callback = (payload) => handler(payload as T); return () => {}; } };
    const bridge = createBridge(transport);
    await bridge.promoteAudioOutput("/tmp/synthetic.part", "/tmp/library/song.mp3");
    await bridge.play("track-1");
    expect(calls).toHaveLength(2);
    expect((calls[0] as { version: number }).version).toBe(1);
    await bridge.resolveRuntimeHome();
    await bridge.videoStage("track-1");
    await bridge.videoSeek(1200);
    await bridge.videoFullscreen(true);
    expect(calls).toHaveLength(6);
    await bridge.subscribe(() => {});
    expect(calls).toHaveLength(6);
    expect(() => callback(event({ type: "error", payload: { error: { code: "internal", message: "x", retryable: false } } }))).not.toThrow();
    expect(() => callback({})).toThrow();
    expect(() => callback(event({ type: "playback_snapshot", payload: { snapshot: { queue: [], current_index: null, shuffle: false, repeat: "off", position_ms: -1, duration_ms: null, status: "stopped", generation: 1 } } }))).toThrow();
    expect(() => callback(event({ type: "job_updated", payload: { job: { id: "j", kind: "single", state: "running", track_id: null, progress: 2, generation: 1 } } }))).toThrow();
  });

  it("sends preview commands and validates session responses", async () => {
    const calls: unknown[] = [];
    const snapshot = { generation: 1, active: true, current: null, window: ["Artist-Track"], prepared: [], unavailable: [], cancelled: false };
    const bridge = createBridge({
      invoke: async <T>(_name: string, payload: unknown) => { calls.push(payload); return snapshot as T; },
      listen: async () => () => {},
    });
    await bridge.previewTap([{ artist: "Artist", title: "Track" }], "Artist-Track");
    await bridge.previewNext();
    await bridge.previewPrevious();
    await bridge.previewCancel();
    expect(calls).toHaveLength(4);
    const tapped = (calls[0] as { command: { type: string; payload: { seeds: unknown[]; key: string } } }).command;
    expect(tapped.type).toBe("preview_tap");
    expect(tapped.payload).toEqual({ seeds: [{ artist: "Artist", title: "Track" }], key: "Artist-Track" });
    expect((calls[1] as { command: { type: string } }).command.type).toBe("preview_next");
    expect((calls[2] as { command: { type: string } }).command.type).toBe("preview_previous");
    expect((calls[3] as { command: { type: string } }).command.type).toBe("preview_cancel");
    await expect(bridge.previewTap([{ artist: "Artist", title: "Track" }], "Artist-Track")).resolves.toMatchObject({ active: true });
    const rejecting = createBridge({ invoke: async () => null as never, listen: async () => () => {} });
    await expect(rejecting.previewNext()).rejects.toThrow("invalid preview session response");
  });

  it("sends context playback and explicit queue commands", async () => {
    const calls: unknown[] = [];
    const bridge = createBridge({
      invoke: async <T>(_name: string, payload: unknown) => { calls.push(payload); return null as T; },
      listen: async () => () => {},
    });
    await bridge.playWithContext("track-b", ["track-a", "track-b"]);    await bridge.next();
    await bridge.previous();
    await bridge.playNext(["track-x"]);
    await bridge.addToQueue(["track-y"]);
    await bridge.removeFromQueue(2);
    await bridge.reorderQueue(1, 2);
    await bridge.toggleShuffle();
    await bridge.setRepeat("all");
    expect(calls.map((value) => (value as { command: { type: string } }).command.type)).toEqual([
      "play_with_context", "next", "previous", "play_next", "add_to_queue",
      "remove_from_queue", "reorder_queue", "toggle_shuffle", "set_repeat",
    ]);
  });

  it("exposes Gate 4 acquisition, settings, and transfer commands through the typed bridge", async () => {
    const calls: unknown[] = [];
    const bridge = createBridge({
      invoke: async <T>(_name: string, payload: unknown) => { calls.push(payload); return [] as T; },
      listen: async () => () => {},
    });
    await bridge.validateDirectUrl("https://example.test/audio", "/fixture/library");
    await bridge.previewCsv("Track Name,Artist Name(s)\nSong,Artist");
    await bridge.queueUrls(["https://example.test/a"]);
    await bridge.selectLibraryRoot("/fixture/library");
    await bridge.loadSettings("/fixture/home");
    await bridge.saveSettings("/fixture/home", { music_root: "/fixture/library", auto_discovery: false, volume: 0.5, sleep_timer: null, global_keys: true, favorite_songs: [] });
    await bridge.transferApply("{}", "/fixture/library", {});
    await bridge.startDownload("job-1", "/fixture/library/Unsorted/job-1.mp3", "/fixture/bin/luno-download-helper", "scsearch1:fixture query");
    await bridge.retryDownload("job-1");
    await bridge.bundleHelper();
    await bridge.resolveRuntimeHome();
    expect(calls.map((value) => (value as { command: { type: string } }).command.type)).toEqual([
      "validate_direct_url", "preview_csv", "queue_urls", "select_library_root",
      "load_settings", "save_settings", "transfer_apply", "start_download", "retry_download", "bundle_helper_path", "resolve_runtime_home",
    ]);
    const downloadPayload = (calls.at(-4) as { command: { payload: { source: string } } }).command.payload;
    expect(downloadPayload.source).toBe("scsearch1:fixture query");
  });

  it("accepts only a valid library response for startup readiness", () => {
    const response = { version: 1, generation: 0, tracks: [], playlists: [] };
    expect(isValidLibraryResponse(response)).toBe(true);
    expect(isValidLibraryResponse(null)).toBe(false);
    expect(isValidLibraryResponse({ ...response, generation: -1 })).toBe(false);
    expect(isValidLibraryResponse({ ...response, tracks: null })).toBe(false);
    expect(isValidLibraryResponse({ ...response, tracks: [{ id: "broken" }] })).toBe(false);
  });

  it("projects lifecycle events and rejects stale generations", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "error", payload: { error: { code: "internal", message: "new", retryable: false } } }, 2));
    store.apply(event({ type: "error", payload: { error: { code: "internal", message: "old", retryable: false } } }, 1));
    expect(store.getSnapshot().errors).toHaveLength(1);
    store.apply({ ...event({ type: "shutdown", payload: { envelope: { version: 1, state: "complete", reason: "system" } } }, 2), event_id: "3" });
    expect(store.getSnapshot().shutdown?.state).toBe("complete");
  });

  it("accepts library root status events without throwing", () => {
    const store = createBackendProjection();
    expect(() => store.apply(event({ type: "library_root_status", payload: { root_index: 0, status: "scanned" } }))).not.toThrow();
  });

  it("keeps generation watermarks independent and projects history separately from queue", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "library_snapshot", payload: { snapshot: { version: 1, generation: 9, tracks: [], playlists: [] } } }, 9));
    store.apply({ ...event({ type: "playback_snapshot", payload: { snapshot: { queue: ["track-1"], current_index: 0, shuffle: false, repeat: "off", position_ms: 0, duration_ms: null, status: "playing", generation: 1 } } }, 1), event_id: "event-1" });
    store.apply({ ...event({ type: "track_started", payload: { track_id: "track-1" } }, 1), event_id: "event-2" });
    expect(store.getSnapshot().recentHistory).toEqual(["track-1"]);
    expect(store.getSnapshot().playback?.queue).toEqual(["track-1"]);
  });

  it("projects authoritative queue snapshots and keeps queue controls on the playback boundary", () => {
    const store = createBackendProjection();
    store.apply({ ...event({ type: "playback_snapshot", payload: { snapshot: { queue: ["a", "b"], current_index: 1, shuffle: false, repeat: "off", position_ms: 12, duration_ms: 90, status: "playing", generation: 3 } } }, 3), event_id: "event-10" });
    store.apply({ ...event({ type: "queue_changed", payload: { queue: ["b", "a", "x"] } }, 3), event_id: "event-11" });
    expect(store.getSnapshot().playback).toMatchObject({ queue: ["b", "a", "x"], current_index: 1, position_ms: 12, duration_ms: 90 });
  });

  it("projects preview snapshots and tracks their own generation watermark", () => {
    const store = createBackendProjection();
    store.apply({ ...event({ type: "preview_snapshot", payload: { snapshot: { generation: 7, active: true, current: "Artist-Track", window: ["Artist-Track"], prepared: ["Artist-Track"], unavailable: [], cancelled: false } } }, 7), event_id: "event-p1" });
    store.apply({ ...event({ type: "preview_snapshot", payload: { snapshot: { generation: 8, active: false, current: null, window: [], prepared: [], unavailable: ["Artist-Track"], cancelled: true } } }, 8), event_id: "event-p2" });
    expect(store.getSnapshot().preview).toMatchObject({ generation: 8, active: false, cancelled: true, unavailable: ["Artist-Track"] });
    expect(store.getSnapshot().generations.preview).toBe(8);
  });

  it("projects discover snapshots on their own watermark without disturbing preview", () => {
    const store = createBackendProjection();
    store.apply({ ...event({ type: "discover_snapshot", payload: { snapshot: { generation: 3, enabled: true, window: ["A-T"], ready: 0, queued: 0 } } }, 3), event_id: "event-d1" });
    expect(store.getSnapshot().discover).toMatchObject({ enabled: true, ready: 0 });
    expect(store.getSnapshot().generations.discover).toBe(3);
    expect(store.getSnapshot().preview).toBeNull();
  });

  it("projects save_state events per recommendation key and keeps the latest state", () => {
    const store = createBackendProjection();
    const saveEvent = (payload: { state: SaveState }, n: number) => ({ ...event({ type: "save_state", payload }, n), event_id: `event-s${n}` });
    store.apply(saveEvent({ state: { key: "Artist0-Track0", playlist_id: "playlist:playlist-a", state: "saving", already_saved: false, path: null, job_id: null, error: null } }, 1));
    expect(store.getSnapshot().saved["Artist0-Track0"]?.state).toBe("saving");
    store.apply(saveEvent({ state: { key: "Artist0-Track0", playlist_id: "playlist:playlist-a", state: "saved", already_saved: false, path: "/library/playlist-a/Artist0 - Track0.m4a", job_id: null, error: null } }, 2));
    store.apply(saveEvent({ state: { key: "Artist1-Track1", playlist_id: "playlist:playlist-a", state: "queued", already_saved: false, path: "/library/playlist-a/Artist1 - Track1.m4a", job_id: "save-Artist1-Track1", error: null } }, 3));
    expect(store.getSnapshot().saved["Artist0-Track0"]).toMatchObject({ state: "saved" });
    expect(store.getSnapshot().saved["Artist1-Track1"]).toMatchObject({ state: "queued", job_id: "save-Artist1-Track1" });
    expect(Object.keys(store.getSnapshot().saved)).toEqual(["Artist0-Track0", "Artist1-Track1"]);
  });

  it("rejects snapshots whose payload generation disagrees with the envelope", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "library_snapshot", payload: { snapshot: { version: 1, generation: 2, tracks: [], playlists: [] } } }, 1));
    expect(store.getSnapshot().library).toBeNull();
  });

  it("ignores video state belonging to another current track at the shell boundary", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "video_state", payload: { state: { track_id: "old", source_url: "https://example.test/old.mp4", staged_path: null, status: "ready", position_ms: 10, fullscreen: false, generation: 8 } } }, 8));
    store.apply({ ...event({ type: "playback_snapshot", payload: { snapshot: { queue: ["new"], current_index: 0, shuffle: false, repeat: "off", position_ms: 0, duration_ms: null, status: "playing", generation: 1 } } }, 1), event_id: "event-1" });
    expect(store.getSnapshot().video?.track_id).toBe("old");
    expect(store.getSnapshot().playback?.queue[0]).toBe("new");
    expect(safeVideoSource("javascript:alert(1)", null)).toBeNull();
    expect(safeVideoSource("https://example.test/video.mp4", "/tmp/../outside.mp4")).toBe("https://example.test/video.mp4");
  });

  it("streams staged videos through the range-aware luno-video scheme", () => {
    // The stock asset protocol buffers whole staged MP4s into one response
    // blob; the custom scheme answers ranged requests with ≤2 MB windows
    // (src-tauri video::stream). The src must carry the custom scheme and the
    // percent-encoded absolute path.
    const source = safeVideoSource(null, "/Users/ty/App Support/videos/current-abc.mp4");
    expect(source).toContain("luno-video://localhost/");
    expect(source).toContain("%2F"); // absolute path is encoded into the URL path
    expect(source).toContain("current-abc.mp4");
    // Traversal attempts never reach the scheme; they fall back to source_url.
    expect(safeVideoSource("https://example.test/video.mp4", "/videos/../secrets/current-x.mp4")).toBe("https://example.test/video.mp4");
  });

  it("keeps the latest video state for the current track regardless of playback generation", () => {
    // Video and playback generations are INDEPENDENT counters on the backend;
    // correlation is by track identity, with event ordering guaranteed upstream.
    const staged = { track_id: "track-1", source_url: "https://example.test/old.mp4", staged_path: null, status: "ready" as const, position_ms: 10, fullscreen: false, generation: 1 };
    expect(currentVideoForTrack({ playback: { queue: ["track-1"], current_index: 0, shuffle: false, repeat: "off", position_ms: 0, duration_ms: null, status: "playing", generation: 2 }, video: staged }, "track-1")?.source_url).toContain("old.mp4");
    expect(currentVideoForTrack({ playback: { queue: ["track-1"], current_index: 0, shuffle: false, repeat: "off", position_ms: 0, duration_ms: null, status: "playing", generation: 2 }, video: { ...staged, track_id: "track-2" } }, "track-1")).toBeNull();
    expect(currentVideoForTrack({ playback: null, video: staged }, "track-1")?.source_url).toContain("old.mp4");
  });

  it("does not enable video commands for artwork fallback", () => {
    expect(videoCommandsEnabled(true, true, "https://example.test/video.mp4", "unavailable")).toBe(false);
    expect(videoCommandsEnabled(true, true, null, "ready")).toBe(false);
  });

  it("validates migration counters and catches rejected runtime commands", async () => {
    expect(() => parseMigrationStatus({ completed: { warnings: -1, snapshot_created: false, removed_previews: 0 } })).toThrow();
    expect(() => parseMigrationStatus({ completed: { warnings: 1.5, snapshot_created: false, removed_previews: 0 } })).toThrow();
    expect(() => parseMigrationStatus({ completed: { warnings: Number.MAX_SAFE_INTEGER + 1, snapshot_created: false, removed_previews: 0 } })).toThrow();
    expect(() => parseMigrationStatus({ completed: { warnings: 0, snapshot_created: false, removed_previews: -1 } })).toThrow();
    const messages: string[] = [];
    runBridgeCommand(Promise.reject(new Error("backend unavailable")), (message) => messages.push(message));
    runBridgeCommand(Promise.reject({ code: "Unavailable", message: "audio output device unavailable: no default device" }), (message) => messages.push(message));
    await Promise.resolve();
    expect(messages).toEqual(["backend unavailable", "audio output device unavailable: no default device"]);
    const played: string[] = [];
    playRecentTrack({ play: (id) => { played.push(id); return Promise.resolve(null); } }, { id: "recent-1" } as never, () => {});
    expect(played).toEqual(["recent-1"]);
  });

  it("projects video state and safely selects the current track", async () => {
    const store = createBackendProjection();
    store.apply(event({ type: "library_snapshot", payload: { snapshot: { version: 1, generation: 1, tracks: [{ id: "track-1", path: null, title: "Song", artist: "Artist", album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null }], playlists: [] } } }, 1));
    store.apply({ ...event({ type: "playback_snapshot", payload: { snapshot: { queue: ["track-1"], current_index: 0, shuffle: false, repeat: "off", position_ms: 10, duration_ms: null, status: "playing", generation: 1 } } }, 1), event_id: "event-2" });
    store.apply({ ...event({ type: "video_state", payload: { state: { track_id: "track-1", source_url: "https://example.test/video.mp4", staged_path: null, status: "ready", position_ms: 10, fullscreen: false, generation: 1 } } }, 1), event_id: "event-3" });
    expect(store.getSnapshot().video?.status).toBe("ready");
    expect(store.getSnapshot().library?.tracks.find((track) => track.id === store.getSnapshot().playback?.queue[0])?.title).toBe("Song");
    store.apply({ ...event({ type: "playback_snapshot", payload: { snapshot: { queue: [], current_index: 3, shuffle: false, repeat: "off", position_ms: 0, duration_ms: null, status: "stopped", generation: 2 } } }, 2), event_id: "event-4" });
    expect(store.getSnapshot().playback?.current_index).toBe(3);
  });

  it("rejects duplicate and older event IDs even when generations are newer", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "error", payload: { error: { code: "internal", message: "two", retryable: false } } }, 2));
    store.apply({ ...event({ type: "error", payload: { error: { code: "internal", message: "duplicate", retryable: false } } }, 3), event_id: "2" });
    store.apply({ ...event({ type: "error", payload: { error: { code: "internal", message: "old", retryable: false } } }, 4), event_id: "1" });
    expect(store.getSnapshot().errors.map((error) => error.message)).toEqual(["two"]);
  });

  it("keeps repeated delivery errors out of domain ID ordering", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "error", payload: { error: { code: "internal", message: "domain", retryable: false } } }, 1));
    const deliveryError = { ...event({ type: "error", payload: { error: { code: "internal", message: "delivery", retryable: true } } }, 0), event_id: "delivery-error-1" };
    store.apply(deliveryError);
    store.apply({ ...deliveryError, event_id: "delivery-error-2" });
    store.apply({ ...event({ type: "error", payload: { error: { code: "internal", message: "next", retryable: false } } }, 2), event_id: "event-2" });
    expect(store.getSnapshot().errors.map((error) => error.message)).toEqual(["domain", "delivery", "delivery", "next"]);
        expect(store.getSnapshot().lastEventIds.errors).toBe("event-2");
  });

  it("mergeVideoSnapshot absorbs a Ready response after a lost Loading event", () => {
    const store = createBackendProjection();
    // Simulates the stranded case: the pushed video_state event never
    // arrived, so projection.video is still null when the song page polls.
    const ready = { track_id: "track-1", source_url: "https://example.test/v.mp4", staged_path: "/tmp/v.mp4", status: "ready" as const, position_ms: 0, fullscreen: false, generation: 4 };
    const next = mergeVideoSnapshot(store.getSnapshot(), ready);
    expect(next.video?.status).toBe("ready");
  });

  it("mergeVideoSnapshot does not walk a live session backwards to ready/loading", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "video_state", payload: { state: { track_id: "track-1", source_url: "https://example.test/v.mp4", staged_path: "/tmp/v.mp4", status: "playing", position_ms: 5000, fullscreen: false, generation: 5 } } }, 5));
    const stale = mergeVideoSnapshot(store.getSnapshot(), { track_id: "track-1", source_url: "https://example.test/v.mp4", staged_path: "/tmp/v.mp4", status: "ready", position_ms: 0, fullscreen: false, generation: 4 });
    expect(stale).toBe(store.getSnapshot());
  });

  it("mergeVideoSnapshot is a no-op when nothing changed", () => {
    const store = createBackendProjection();
    const state = { ...store.getSnapshot(), video: { track_id: "track-1", source_url: null, staged_path: null, status: "loading" as const, position_ms: 0, fullscreen: false, generation: 2 } };
    expect(mergeVideoSnapshot(state, { track_id: "track-1", source_url: null, staged_path: null, status: "loading", position_ms: 0, fullscreen: false, generation: 2 })).toBe(state);
  });

  it("accepts video_state events carrying download_progress and revision", () => {
    // REGRESSION: the validator required EXACTLY the seven original keys, so
    // every pushed video_state event (which serializes download_progress)
    // was silently discarded — the UI only survived via command responses
    // and polling.
    const store = createBackendProjection();
    store.apply({ ...event({ type: "video_state", payload: { state: { track_id: "track-1", source_url: null, staged_path: "/tmp/v.mp4", status: "loading", position_ms: 0, download_progress: 42, fullscreen: false, generation: 3 } } }, 3), event_id: "event-9" });
    expect(store.getSnapshot().video?.status).toBe("loading");
    expect(store.getSnapshot().video?.download_progress).toBe(42);
  });

  it("mergeVideoSnapshot applies a revision-only upgrade publication", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "video_state", payload: { state: { track_id: "track-1", source_url: "https://example.test/v.mp4", staged_path: "/tmp/v.mp4", status: "playing", position_ms: 5000, revision: 1, fullscreen: false, generation: 5 } } }, 5));
    // The background upgrade replaces the file IN PLACE — same path, same
    // status; only the revision changes. That must not be folded into
    // "unchanged" or the webview would never hot-swap to full quality.
    const next = mergeVideoSnapshot(store.getSnapshot(), { track_id: "track-1", source_url: "https://example.test/v.mp4", staged_path: "/tmp/v.mp4", status: "playing", position_ms: 5000, revision: 2, fullscreen: false, generation: 6 });
    expect(next.video?.revision).toBe(2);
  });
});

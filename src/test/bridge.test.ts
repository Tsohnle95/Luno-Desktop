import { describe, expect, it, vi } from "vitest";
import { backendCommandArgs, createBridge, isValidLibraryResponse, parseMigrationStatus, tauriTransport } from "../app/bridge";
import { createBackendProjection, initialBackendProjection, reduceBackendProjection, mergeVideoSnapshot } from "../state/backend_projection";
import { currentVideoForTrack, focusedQueueProjection, libraryFavoriteTarget, playbackDisplayTrack, playRecentTrack, recommendationTrackMap, runBridgeCommand, safeVideoSource, stoppedPlaybackCommand, videoCommandsEnabled, videoSurfaceActive } from "../app/Shell";
import type { Bridge, BridgeTransport } from "../app/bridge";
import type { Event, EventEnvelope, PlaybackSnapshot } from "../shared/contracts";
import type { SaveState } from "../shared/contracts";
import { isCommandEnvelope, isEventEnvelope, isPreviewSnapshot, isDiscoverSnapshot, isSearchResults, isPlaylistSyncIssues, isPlaylistSyncCandidates, isPlaylistSyncSelection, previewKey } from "../shared/contracts";
import type { Track } from "../shared/contracts";

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
  it("keeps progress, cancellation and completion independent across simultaneous jobs", () => {
    const update = (id: string, generation: number, sequence: number, state: "running" | "cancelling" | "cancelled" | "succeeded", progress: number | null) => ({
      ...event({ type: "job_updated", payload: { job: { id, kind: "single", state, track_id: null, generation, progress } } }, generation), event_id: `event-${sequence}`,
    });
    let projection = reduceBackendProjection(initialBackendProjection(), update("first", 1, 1, "running", 0));
    projection = reduceBackendProjection(projection, update("second", 2, 2, "running", 0));
    projection = reduceBackendProjection(projection, update("first", 1, 3, "running", 0.5));
    expect(projection.jobs.first.progress).toBe(0.5);
    projection = reduceBackendProjection(projection, update("first", 1, 4, "cancelling", null));
    expect(projection.jobs.first.state).toBe("cancelling");
    projection = reduceBackendProjection(projection, update("first", 1, 5, "cancelled", null));
    expect(projection.jobs.first.state).toBe("cancelled");
    // Interleaved deliveries may arrive out of order across different jobs.
    projection = reduceBackendProjection(projection, update("second", 2, 4, "succeeded", 1));
    expect(projection.jobs.second.state).toBe("succeeded");
    const retried = reduceBackendProjection(projection, update("first", 3, 6, "running", 0));
    expect(reduceBackendProjection(retried, update("first", 1, 7, "cancelled", null))).toBe(retried);
    expect(reduceBackendProjection(retried, update("first", 3, 5, "running", 0.8))).toBe(retried);
  });

  it("keeps physical successors visible during a focused recommendation", () => {
    const physical = ["a", "b", "c"].map((id): Track => ({ id, path: null, title: id, artist: "Artist", album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null }));
    const preview: Track = { ...physical[1], id: "rec", title: "Recommendation" };
    const projected = focusedQueueProjection(physical, 1, preview);
    expect(projected.map((item) => item.id)).toEqual(["a", "rec", "c"]);
    expect(projected[2]).toBe(physical[2]);
  });

  it("uses the projected selection in Discover and the audible recommendation after restoring the physical queue", () => {
    const library: Track = { id: "seed", path: null, title: "Seed", artist: "Artist", album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null };
    const oldPreview = { ...library, id: "old", title: "Old preview" };
    const selected = { ...library, id: "selected", title: "Selected" };
    expect(playbackDisplayTrack(true, selected, oldPreview, oldPreview, library)).toBe(selected);
    expect(focusedQueueProjection([library, selected], 1, null)[1]).toBe(selected);
    expect(playbackDisplayTrack(false, null, selected, oldPreview, library)).toBe(selected);
    expect(focusedQueueProjection([library], 0, selected)[0]).toBe(selected);
  });

  it("accepts an audible ID in playback events and rejects unrelated fields", () => {
    const snapshot: PlaybackSnapshot = { queue: ["seed"], current_index: 0, audible_track_id: "recommendation", shuffle: false, repeat: "off", position_ms: 10, duration_ms: null, status: "playing", generation: 1 };
    expect(isEventEnvelope(event({ type: "playback_snapshot", payload: { snapshot: { ...snapshot } } }))).toBe(true);
    expect(isEventEnvelope(event({ type: "playback_snapshot", payload: { snapshot: { ...snapshot, unexpected: true } } } as unknown as Event))).toBe(false);
  });

  it("retries a stopped recommendation through its active mode", async () => {
    const discoverSelect = vi.fn(async () => ({}));
    const previewTap = vi.fn(async () => ({}));
    const playWithContext = vi.fn(async () => ({}));
    const bridge = { discoverSelect, previewTap, playWithContext } as unknown as Pick<Bridge, "discoverSelect" | "previewTap" | "playWithContext">;
    const recommendations = Array.from({ length: 60 }, (_, index) => ({ artist: "Artist", title: `Track ${index}`, match_score: 1, artwork_url: null }));
    const library: Track = { id: "seed", path: null, title: "Seed", artist: "Artist", album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null };
    const selected = { ...library, id: previewKey("Artist", "Track 55") };
    await stoppedPlaybackCommand(bridge, selected, library, true, recommendations, [library]);
    expect(discoverSelect).toHaveBeenCalledWith(selected.id);
    await stoppedPlaybackCommand(bridge, selected, null, true, [], []);
    expect(discoverSelect).toHaveBeenCalledTimes(2);
    await stoppedPlaybackCommand(bridge, selected, library, false, recommendations, [library]);
    expect(previewTap).toHaveBeenCalledWith(recommendations.slice(50).map(({ artist, title }) => ({ artist, title })), selected.id);
    await stoppedPlaybackCommand(bridge, library, library, false, recommendations, [library]);
    expect(playWithContext).toHaveBeenCalledWith(library.id, [library.id]);
  });

  it("keeps Rust and TypeScript recommendation IDs aligned for non-BMP characters", () => {
    const recommendation = { artist: "Björk", title: "Track 🎵", match_score: 1, artwork_url: null };
    const key = previewKey(recommendation.artist, recommendation.title);
    expect(key).toBe("Bj_rk-Track__");
    expect(recommendationTrackMap([recommendation]).get(key)).toMatchObject({
      id: key,
      artist: "Björk",
      title: "Track 🎵",
    });
  });

  it("only targets the matching library snapshot track for footer favorites", () => {
    const saved: Track = { id: "saved-track", path: null, title: "Saved", artist: "Artist", album: null, duration_ms: null, artwork_url: null, favorite: true, provider: null };
    const preview: Track = { ...saved, id: previewKey("Artist", "Preview 🎵"), path: null, title: "Preview 🎵", favorite: false };
    const libraryTracks = new Map([[saved.id, saved]]);
    expect(libraryFavoriteTarget(saved, libraryTracks)).toBe(saved);
    expect(libraryFavoriteTarget(preview, libraryTracks)).toBeNull();
    expect(libraryFavoriteTarget({ ...preview, id: saved.id }, libraryTracks)).toBe(saved);
  });

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
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover_select", payload: { key: "Artist-Track" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover_select", payload: { key: "" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "discover_enable", payload: { seeds: "x" } } })).toBe(false);
  });

  it("selects a recommendation without leaving discover mode", async () => {
    const calls: unknown[] = [];
    const snapshot = { generation: 2, enabled: true, window: ["Artist-Track"], ready: 1, queued: 1, prepared: ["Artist-Track"], unavailable: [] };
    const bridge = createBridge({
      invoke: async <T>(_name: string, payload: unknown) => { calls.push(payload); return snapshot as T; },
      listen: async () => () => {},
    });
    await expect(bridge.discoverSelect("Artist-Track")).resolves.toMatchObject({ enabled: true });
    expect((calls[0] as { command: { type: string; payload: { key: string } } }).command).toEqual({ type: "discover_select", payload: { key: "Artist-Track" } });
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
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "set_playlist_url", payload: { playlist_id: "playlist:Road trip", url: "https://www.youtube.com/playlist?list=PL123" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "set_playlist_url", payload: { playlist_id: "playlist:Road trip", url: null } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "set_playlist_url", payload: { playlist_id: "", url: null } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "sync_playlist", payload: { playlist_id: "playlist:Road trip" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "sync_playlist", payload: { playlist_id: "" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "playlist_sync_issues", payload: { playlist_id: "playlist:Road trip" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "playlist_sync_candidates", payload: { playlist_id: "playlist:Road trip", video_id: "aPSOTX5AkiY" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "playlist_sync_select_source", payload: { playlist_id: "playlist:Road trip", video_id: "aPSOTX5AkiY", source_video_id: "6U7_Qy5c_uc" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "add_tracks_to_playlist", payload: { track_ids: ["track-1"], playlist_id: "playlist:Road trip" } } })).toBe(true);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "add_tracks_to_playlist", payload: { track_ids: [], playlist_id: "playlist:Road trip" } } })).toBe(false);
    expect(isCommandEnvelope({ version: 1, request_id: "r", command: { type: "delete_tracks", payload: { track_ids: ["track-1"] } } })).toBe(true);
  });

  it("validates unresolved playlist entries, YouTube candidates, and explicit source selections", () => {
    const issue = { video_id: "aPSOTX5AkiY", title: "Get What You Give", channel: "Boogie T - Topic", duration_secs: 292, reason: "Video unavailable", attempted_source_id: "aPSOTX5AkiY", selected_source_id: null, retry_pending: false, updated_at_ms: 1, local_matches: [] };
    expect(isPlaylistSyncIssues({ entries: [issue] })).toBe(true);
    expect(isPlaylistSyncIssues({ entries: [{ ...issue, extra: true }] })).toBe(false);
    const candidate = { video_id: "6U7_Qy5c_uc", title: "Get What You Give", channel: "Boogie T - Topic", duration: "4:51", thumbnail_url: null, url: "https://www.youtube.com/watch?v=6U7_Qy5c_uc" };
    expect(isPlaylistSyncCandidates([candidate])).toBe(true);
    expect(isPlaylistSyncCandidates([{ ...candidate, url: "https://www.youtube.com.evil/watch?v=6U7_Qy5c_uc" }])).toBe(false);
    expect(isPlaylistSyncSelection({ video_id: "aPSOTX5AkiY", source_video_id: candidate.video_id, url: candidate.url })).toBe(true);
    expect(isPlaylistSyncSelection({ video_id: "aPSOTX5AkiY", source_video_id: candidate.video_id, url: "https://example.com/watch?v=6U7_Qy5c_uc" })).toBe(false);
  });

  it("uses the backend source review commands and validates their responses", async () => {
    const calls: unknown[] = [];
    const issue = { video_id: "aPSOTX5AkiY", title: "Get What You Give", channel: "Boogie T - Topic", duration_secs: 292, reason: "Video unavailable", attempted_source_id: "aPSOTX5AkiY", selected_source_id: null, retry_pending: false, updated_at_ms: 1, local_matches: [] };
    const candidate = { video_id: "6U7_Qy5c_uc", title: "Get What You Give", channel: "Boogie T - Topic", duration: "4:51", thumbnail_url: null, url: "https://www.youtube.com/watch?v=6U7_Qy5c_uc" };
    const selection = { video_id: issue.video_id, source_video_id: candidate.video_id, url: candidate.url };
    const transport: BridgeTransport = {
      invoke: async <T>(_name: string, payload: unknown) => {
        calls.push(payload);
        const type = (payload as { command: { type: string } }).command.type;
        const result = type === "playlist_sync_issues" ? { entries: [issue] } : type === "playlist_sync_candidates" ? [candidate] : selection;
        return result as T;
      },
      listen: async () => () => {},
    };
    const bridge = createBridge(transport);
    await expect(bridge.playlistSyncIssues("playlist:Road trip")).resolves.toMatchObject([{ video_id: issue.video_id }]);
    await expect(bridge.playlistSyncCandidates("playlist:Road trip", issue.video_id)).resolves.toMatchObject([{ video_id: candidate.video_id }]);
    await expect(bridge.playlistSyncSelectSource("playlist:Road trip", issue.video_id, candidate.video_id)).resolves.toMatchObject(selection);
    expect(calls.map((payload) => (payload as { command: { type: string } }).command.type)).toEqual([
      "playlist_sync_issues", "playlist_sync_candidates", "playlist_sync_select_source",
    ]);
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
    expect(isDiscoverSnapshot({ ...discover, prepared: ["Artist-Track"] })).toBe(true);
    expect(isDiscoverSnapshot({ ...discover, enabled: "yes" })).toBe(false);
    expect(isDiscoverSnapshot({ ...discover, window: [""] })).toBe(false);
    expect(isDiscoverSnapshot({ ...discover, extra: 1 })).toBe(false);
    expect(isEventEnvelope(event({ type: "discover_snapshot", payload: { snapshot: discover } }, 2))).toBe(true);
    expect(isEventEnvelope(event({ type: "job_updated", payload: { job: { id: "j", kind: "single", state: "running", track_id: null, progress: 0.97, phase: "finalizing", generation: 2 } } }, 2))).toBe(true);
    expect(isEventEnvelope(event({ type: "job_updated", payload: { job: { id: "sync-j", kind: "sync", state: "running", track_id: null, progress: 0.25, current_item: "Artist - Song", generation: 2 } } }, 2))).toBe(true);
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

  it("sends the edit-track-metadata command with artist and title", async () => {
    const calls: unknown[] = [];
    const bridge = createBridge({
      invoke: async <T>(_name: string, payload: unknown) => { calls.push(payload); return null as T; },
      listen: async () => () => {},
    });
    await bridge.editTrackMetadata("track-1", "Rick Astley", "Never Gonna Give You Up");
    expect(calls).toHaveLength(1);
    const envelope = calls[0] as { command: { type: string; payload: Record<string, unknown> } };
    expect(envelope.command.type).toBe("edit_track_metadata");
    expect(envelope.command.payload).toEqual({ track_id: "track-1", artist: "Rick Astley", title: "Never Gonna Give You Up" });
  });

  it("accepts only a valid library response for startup readiness", () => {
    const response = { version: 1, generation: 0, tracks: [], playlists: [] };
    expect(isValidLibraryResponse(response)).toBe(true);
    expect(isValidLibraryResponse(null)).toBe(false);
    expect(isValidLibraryResponse({ ...response, generation: -1 })).toBe(false);
    expect(isValidLibraryResponse({ ...response, tracks: null })).toBe(false);
    expect(isValidLibraryResponse({ ...response, tracks: [{ id: "broken" }] })).toBe(false);
    const validTrack = { id: "t1", path: null, title: "Song", artist: null, album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null };
    expect(isValidLibraryResponse({ ...response, tracks: [validTrack] })).toBe(true);
    // Optional mtime for "Recently added"; unknown fields stay rejected.
    expect(isValidLibraryResponse({ ...response, tracks: [{ ...validTrack, added_ms: 1_700_000_000_000 }] })).toBe(true);
    expect(isValidLibraryResponse({ ...response, tracks: [{ ...validTrack, added_ms: "soon" }] })).toBe(false);
    expect(isValidLibraryResponse({ ...response, tracks: [{ ...validTrack, bogus: 1 }] })).toBe(false);
  });

  it("projects lifecycle events and rejects stale generations", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "error", payload: { error: { code: "internal", message: "new", retryable: false } } }, 2));
    store.apply(event({ type: "error", payload: { error: { code: "internal", message: "old", retryable: false } } }, 1));
    expect(store.getSnapshot().errors).toHaveLength(1);
    store.apply({ ...event({ type: "shutdown", payload: { envelope: { version: 1, state: "complete", reason: "system" } } }, 2), event_id: "3" });
    expect(store.getSnapshot().shutdown?.state).toBe("complete");
  });

  it("clears projected errors on demand so a transient failure is dismissable", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "error", payload: { error: { code: "unavailable", message: "preview could not be prepared", retryable: true } } }, 2));
    expect(store.getSnapshot().errors).toHaveLength(1);
    store.clearErrors();
    expect(store.getSnapshot().errors).toHaveLength(0);
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

  it("keeps the authoritative queue index instead of re-deriving it from track_started", () => {
    const store = createBackendProjection();
    // Duplicate ids are legal in a queue; the backend index is authoritative.
    store.apply({ ...event({ type: "playback_snapshot", payload: { snapshot: { queue: ["a", "b", "a"], current_index: 2, shuffle: false, repeat: "off", position_ms: 0, duration_ms: null, status: "playing", generation: 5 } } }, 5), event_id: "event-20" });
    store.apply({ ...event({ type: "track_started", payload: { track_id: "a" } }, 5), event_id: "event-21" });
    expect(store.getSnapshot().playback?.current_index).toBe(2);
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
    expect(videoSurfaceActive(true, true, true, true)).toBe(true);
    expect(videoSurfaceActive(true, true, false, true)).toBe(false);
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

it("keeps a download failure on its own song row and clears it on retry", () => {
  const store = createBackendProjection();
  store.apply({ ...event({ type: "error", payload: { error: { code: "unavailable", message: "This song is already in the destination playlist.", retryable: false } } }, 1), job_id: "download-one" });
  expect(store.getSnapshot().jobErrors?.["download-one"]).toBe("This song is already in the destination playlist.");
  expect(store.getSnapshot().errors).toEqual([]);
  store.apply({ ...event({ type: "job_updated", payload: { job: { id: "download-one", kind: "single", state: "running", track_id: null, progress: null, generation: 2 } } }, 2), event_id: "event-2" });
  expect(store.getSnapshot().jobErrors?.["download-one"]).toBeUndefined();
  store.apply({ ...event({ type: "error", payload: { error: { code: "unavailable", message: "Old failure", retryable: false } } }, 1), event_id: "event-3", job_id: "download-one" });
  expect(store.getSnapshot().jobErrors?.["download-one"]).toBeUndefined();
});

it("preserves playlist Sync error prompts while also projecting their inline reason", () => {
  const store = createBackendProjection();
  store.apply(event({ type: "job_updated", payload: { job: { id: "sync", kind: "sync", state: "failed", track_id: null, progress: null, generation: 1 } } }));
  const error = { code: "unavailable" as const, message: "YouTube could not access this playlist. Choose a browser.", retryable: true };
  store.apply({ ...event({ type: "error", payload: { error } }), event_id: "event-2", job_id: "sync" });
  expect(store.getSnapshot().errors).toEqual([error]);
  expect(store.getSnapshot().jobErrors?.sync).toBe(error.message);
});

import { describe, expect, it } from "vitest";
import { createBridge, isValidLibraryResponse, parseMigrationStatus } from "../app/bridge";
import { createBackendProjection } from "../state/backend_projection";
import { currentVideoForTrack, playRecentTrack, runBridgeCommand, safeVideoSource, videoCommandsEnabled } from "../app/Shell";
import type { BridgeTransport } from "../app/bridge";
import type { Event, EventEnvelope } from "../shared/contracts";

const event = (event: Event, generation = 1): EventEnvelope => ({ version: 1, event_id: `event-${generation}`, generation, job_id: null, session_id: null, event });

describe("typed IPC bridge and projection", () => {
  it("wraps commands and validates subscriptions", async () => {
    const calls: unknown[] = [];
    let callback: (payload: unknown) => void = () => {};
    const transport: BridgeTransport = { invoke: async <T>(_name: string, payload: unknown) => { calls.push(payload); return null as T; }, listen: async <T>(_name: string, handler: (payload: T) => void) => { callback = (payload) => handler(payload as T); return () => {}; } };
    const bridge = createBridge(transport);
    await bridge.play("track-1");
    expect(calls).toHaveLength(1);
    expect((calls[0] as { version: number }).version).toBe(1);
    await bridge.videoStage("track-1");
    await bridge.videoSeek(1200);
    await bridge.videoFullscreen(true);
    expect(calls).toHaveLength(4);
    await bridge.subscribe(() => {});
    expect(calls).toHaveLength(4);
    expect(() => callback(event({ type: "error", payload: { error: { code: "internal", message: "x", retryable: false } } }))).not.toThrow();
    expect(() => callback({})).toThrow();
    expect(() => callback(event({ type: "playback_snapshot", payload: { snapshot: { queue: [], current_index: null, position_ms: -1, duration_ms: null, status: "stopped", generation: 1 } } }))).toThrow();
    expect(() => callback(event({ type: "job_updated", payload: { job: { id: "j", kind: "single", state: "running", track_id: null, progress: 2, generation: 1 } } }))).toThrow();
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

  it("keeps generation watermarks independent and projects history separately from queue", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "library_snapshot", payload: { snapshot: { version: 1, generation: 9, tracks: [], playlists: [] } } }, 9));
    store.apply({ ...event({ type: "playback_snapshot", payload: { snapshot: { queue: ["track-1"], current_index: 0, position_ms: 0, duration_ms: null, status: "playing", generation: 1 } } }, 1), event_id: "event-1" });
    store.apply({ ...event({ type: "track_started", payload: { track_id: "track-1" } }, 1), event_id: "event-2" });
    expect(store.getSnapshot().recentHistory).toEqual(["track-1"]);
    expect(store.getSnapshot().playback?.queue).toEqual(["track-1"]);
  });

  it("rejects snapshots whose payload generation disagrees with the envelope", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "library_snapshot", payload: { snapshot: { version: 1, generation: 2, tracks: [], playlists: [] } } }, 1));
    expect(store.getSnapshot().library).toBeNull();
  });

  it("ignores video state belonging to another current track at the shell boundary", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "video_state", payload: { state: { track_id: "old", source_url: "https://example.test/old.mp4", staged_path: null, status: "ready", position_ms: 10, fullscreen: false, generation: 8 } } }, 8));
    store.apply({ ...event({ type: "playback_snapshot", payload: { snapshot: { queue: ["new"], current_index: 0, position_ms: 0, duration_ms: null, status: "playing", generation: 1 } } }, 1), event_id: "event-1" });
    expect(store.getSnapshot().video?.track_id).toBe("old");
    expect(store.getSnapshot().playback?.queue[0]).toBe("new");
    expect(safeVideoSource("javascript:alert(1)", null)).toBeNull();
    expect(safeVideoSource("https://example.test/video.mp4", "/tmp/../outside.mp4")).toBe("https://example.test/video.mp4");
  });

  it("ignores video state from an older playback generation when returning to a track", () => {
    const stale = { track_id: "track-1", source_url: "https://example.test/old.mp4", staged_path: null, status: "ready" as const, position_ms: 10, fullscreen: false, generation: 1 };
    expect(currentVideoForTrack({ playback: { queue: ["track-1"], current_index: 0, position_ms: 0, duration_ms: null, status: "playing", generation: 2 }, video: stale }, "track-1")).toBeNull();
    expect(currentVideoForTrack({ playback: { queue: ["track-1"], current_index: 0, position_ms: 0, duration_ms: null, status: "playing", generation: 2 }, video: { ...stale, generation: 2 } }, "track-1")?.source_url).toContain("old.mp4");
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
    await Promise.resolve();
    expect(messages).toEqual(["backend unavailable"]);
    const played: string[] = [];
    playRecentTrack({ play: (id) => { played.push(id); return Promise.resolve(null); } }, { id: "recent-1" } as never, () => {});
    expect(played).toEqual(["recent-1"]);
  });

  it("projects video state and safely selects the current track", async () => {
    const store = createBackendProjection();
    store.apply(event({ type: "library_snapshot", payload: { snapshot: { version: 1, generation: 1, tracks: [{ id: "track-1", path: null, title: "Song", artist: "Artist", album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null }], playlists: [] } } }, 1));
    store.apply({ ...event({ type: "playback_snapshot", payload: { snapshot: { queue: ["track-1"], current_index: 0, position_ms: 10, duration_ms: null, status: "playing", generation: 1 } } }, 1), event_id: "event-2" });
    store.apply({ ...event({ type: "video_state", payload: { state: { track_id: "track-1", source_url: "https://example.test/video.mp4", staged_path: null, status: "ready", position_ms: 10, fullscreen: false, generation: 1 } } }, 1), event_id: "event-3" });
    expect(store.getSnapshot().video?.status).toBe("ready");
    expect(store.getSnapshot().library?.tracks.find((track) => track.id === store.getSnapshot().playback?.queue[0])?.title).toBe("Song");
    store.apply({ ...event({ type: "playback_snapshot", payload: { snapshot: { queue: [], current_index: 3, position_ms: 0, duration_ms: null, status: "stopped", generation: 2 } } }, 2), event_id: "event-4" });
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
});

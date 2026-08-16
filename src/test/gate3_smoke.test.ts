import { describe, expect, it } from "vitest";
import { runGate3SmokeProbe, type Gate3SmokeBridge } from "../app/gate3Smoke";
import type { EventEnvelope } from "../shared/contracts";

const track = { id: "fixture-track", path: "/fixture/song.wav", title: "Fixture", artist: null, album: null, duration_ms: 1000, artwork_url: null, favorite: false, provider: null };
let eventSequence = 0;
const envelope = (event: EventEnvelope["event"]): EventEnvelope => ({ version: 1, event_id: `fixture-event-${eventSequence++}`, generation: 1, job_id: null, session_id: null, event });

function bridgeFor(playbackFailure = false, commandFailures: string[] = [], emitShutdown = true): Gate3SmokeBridge & { calls: string[]; emit: (event: EventEnvelope) => void; cleaned: boolean } {
  const calls: string[] = [];
  let handler: ((event: EventEnvelope) => void) | undefined;
  let cleaned = false;
  return {
    calls,
    emit: (event) => handler?.(event),
    registerRoot: async () => { calls.push("registerRoot"); },
    promoteAudioOutput: async () => { calls.push("promoteAudioOutput"); },
    scanLibrary: async () => { calls.push("scanLibrary"); queueMicrotask(() => handler?.(envelope({ type: "library_snapshot", payload: { snapshot: { version: 1, generation: 1, tracks: [track], playlists: [] } } }))); },
    getLibrary: async () => { calls.push("getLibrary"); return { version: 1, generation: 1, tracks: [track], playlists: [] }; },
    playWithContext: async () => { calls.push("playWithContext"); queueMicrotask(() => playbackFailure
      ? handler?.(envelope({ type: "error", payload: { error: { code: "unavailable", message: "device unavailable at /private/secret/song.wav", retryable: false } } }))
      : (() => { handler?.(envelope({ type: "playback_snapshot", payload: { snapshot: { queue: [track.id], current_index: 0, shuffle: false, repeat: "off", position_ms: 0, duration_ms: 1000, status: "playing", generation: 1 } } })); handler?.(envelope({ type: "track_started", payload: { track_id: track.id } })); })()); },
    pause: async () => { calls.push("pause"); if (commandFailures.includes("pause")) throw new Error("pause failed at /private/secret/song.wav"); },
    seek: async () => { calls.push("seek"); if (commandFailures.includes("seek")) throw new Error("seek failed at /private/secret/song.wav"); },
    playNext: async () => { calls.push("playNext"); if (commandFailures.includes("playNext")) throw new Error("queue failed"); },
    addToQueue: async () => { calls.push("addToQueue"); if (commandFailures.includes("addToQueue")) throw new Error("queue failed"); },
    next: async () => { calls.push("next"); if (commandFailures.includes("next")) throw new Error("next failed"); },
    previous: async () => { calls.push("previous"); if (commandFailures.includes("previous")) throw new Error("previous failed"); },
     shutdown: async () => { calls.push("shutdown"); if (emitShutdown) queueMicrotask(() => handler?.(envelope({ type: "shutdown", payload: { envelope: { version: 1, state: "complete", reason: "user_requested" } } }))); },
    subscribe: async (next) => { calls.push("subscribe"); handler = next; return () => { cleaned = true; }; },
    get cleaned() { return cleaned; },
  } as Gate3SmokeBridge & { calls: string[]; emit: (event: EventEnvelope) => void; cleaned: boolean };
}

describe("Gate 3 runtime smoke probe", () => {
  it("drives the real bridge command sequence and observes playback lifecycle events", async () => {
    const bridge = bridgeFor();
    const result = await runGate3SmokeProbe(bridge, { root: "/fixture/root", temporary: "/fixture/staged.wav", destination: "/fixture/song.wav", timeoutMs: 100 });
    expect(result.trackId).toBe(track.id);
    expect(result.playbackSnapshot?.status).toBe("playing");
    expect(result.trackStarted).toBe(true);
    expect(result.shutdown).toBe(true);
    expect(result.playbackError).toBeNull();
    expect(result.commands).toEqual(["subscribe", "registerRoot", "promoteAudioOutput", "scanLibrary", "getLibrary", "playWithContext", "pause", "seek", "resume", "playNext", "addToQueue", "next", "previous", "shutdown"]);
    expect(bridge.cleaned).toBe(true);
  });

  it("retains sanitized backend playback errors and never presents them as success", async () => {
    const bridge = bridgeFor(true);
    const result = await runGate3SmokeProbe(bridge, { root: "/fixture/root", temporary: "/fixture/staged.wav", destination: "/fixture/song.wav", timeoutMs: 100 });
    expect(result.playbackError).toBe("device unavailable at <path>");
    expect(result.playbackSnapshot).toBeNull();
    expect(result.trackStarted).toBe(false);
    expect(result.shutdown).toBe(true);
    expect(bridge.cleaned).toBe(true);
  });

  it("fails when shutdown returns without a complete shutdown event", async () => {
    const bridge = bridgeFor(false, [], false);
    const result = await runGate3SmokeProbe(bridge, { root: "/fixture/root", temporary: "/fixture/staged.wav", destination: "/fixture/song.wav", timeoutMs: 10 });
    expect(result.shutdown).toBe(false);
    expect(result.errors).toContainEqual({ command: "probe", message: "no complete shutdown event observed" });
    expect(bridge.cleaned).toBe(true);
  });

  it("continues recovery and always shuts down after backend failure", async () => {
    const bridge = bridgeFor(true, ["pause", "seek", "playNext", "next"]);
    const result = await runGate3SmokeProbe(bridge, { root: "/fixture/root", temporary: "/fixture/staged.wav", destination: "/fixture/song.wav", timeoutMs: 100 });
    expect(result.playbackError).toBe("device unavailable at <path>");
    expect(result.commands).toEqual(["subscribe", "registerRoot", "promoteAudioOutput", "scanLibrary", "getLibrary", "playWithContext", "pause", "seek", "resume", "playNext", "addToQueue", "next", "previous", "shutdown"]);
    expect(bridge.calls).toEqual(expect.arrayContaining(["pause", "seek", "playNext", "addToQueue", "next", "previous", "shutdown"]));
    expect(result.errors).toEqual(expect.arrayContaining([
      { command: "pause", message: "pause failed at <path>" },
      { command: "seek", message: "seek failed at <path>" },
      { command: "playNext", message: "queue failed" },
      { command: "next", message: "next failed" },
    ]));
    expect(result.shutdown).toBe(true);
    expect(bridge.cleaned).toBe(true);
  });
});

import { describe, expect, it } from "vitest";
import { runGate2SmokeProbe, type Gate2SmokeBridge } from "../app/gate2Smoke";
import type { EventEnvelope, LibrarySnapshot } from "../shared/contracts";

const track = { id: "track-1", path: null, title: "Fixture track", artist: null, album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null };
const snapshot: LibrarySnapshot = { version: 1, generation: 1, tracks: [track], playlists: [] };
const libraryEvent: EventEnvelope = { version: 1, event_id: "event-1", generation: 1, job_id: null, session_id: null, event: { type: "library_snapshot", payload: { snapshot } } };

function bridgeFor(response: unknown, onScan?: (emit: (event: EventEnvelope) => void) => void): Gate2SmokeBridge & { calls: string[]; unlisten: () => void; emit: (event: EventEnvelope) => void; cleaned: boolean } {
  const calls: string[] = [];
  let handler: ((event: EventEnvelope) => void) | undefined;
  let cleaned = false;
  const unlisten = () => { cleaned = true; };
  return {
    calls,
    unlisten,
    emit: (event) => handler?.(event),
    registerRoot: async () => { calls.push("registerRoot"); },
    promoteAudioOutput: async () => { calls.push("promoteAudioOutput"); },
    scanLibrary: async () => { calls.push("scanLibrary"); onScan?.((event) => handler?.(event)); },
    getLibrary: async () => { calls.push("getLibrary"); return response; },
    subscribe: async (next) => { calls.push("subscribe"); handler = next; return () => { cleaned = true; unlisten(); }; },
    get cleaned() { return cleaned; },
  } as Gate2SmokeBridge & { calls: string[]; unlisten: () => void; emit: (event: EventEnvelope) => void; cleaned: boolean };
}

describe("Gate 2 runtime smoke probe", () => {
  it("orders commands, waits for the subscribed snapshot, and reports tracks", async () => {
    const bridge = bridgeFor(snapshot, (emit) => queueMicrotask(() => emit(libraryEvent)));
    await expect(runGate2SmokeProbe(bridge, { root: "/fixture/root", temporary: "/fixture/staged.mp3", destination: "/fixture/song.mp3", timeoutMs: 100 })).resolves.toEqual({ trackCount: 1 });
    expect(bridge.calls).toEqual(["subscribe", "registerRoot", "promoteAudioOutput", "scanLibrary", "getLibrary"]);
    expect(bridge.cleaned).toBe(true);
  });

  it("does not get the library before the snapshot event", async () => {
    const bridge = bridgeFor(snapshot);
    const pending = runGate2SmokeProbe(bridge, { root: "r", temporary: "t", destination: "d", timeoutMs: 100 });
    await Promise.resolve();
    expect(bridge.calls).not.toContain("getLibrary");
    bridge.emit(libraryEvent);
    await pending;
  });

  it("cleans up on timeout and command failure", async () => {
    const timeoutBridge = bridgeFor(snapshot);
    await expect(runGate2SmokeProbe(timeoutBridge, { root: "r", temporary: "t", destination: "d", timeoutMs: 1 })).rejects.toThrow("timed out");
    expect(timeoutBridge.cleaned).toBe(true);

    const failingBridge = bridgeFor(snapshot);
    failingBridge.promoteAudioOutput = async () => { throw new Error("promotion failed"); };
    await expect(runGate2SmokeProbe(failingBridge, { root: "r", temporary: "t", destination: "d", timeoutMs: 100 })).rejects.toThrow("promotion failed");
    expect(failingBridge.cleaned).toBe(true);
  });

  it("rejects invalid and empty library responses", async () => {
    for (const response of [{ ...snapshot, tracks: [] }, { ...snapshot, tracks: [{ id: "broken" }] }]) {
      const bridge = bridgeFor(response, (emit) => emit(libraryEvent));
      await expect(runGate2SmokeProbe(bridge, { root: "r", temporary: "t", destination: "d", timeoutMs: 100 })).rejects.toThrow();
      expect(bridge.cleaned).toBe(true);
    }
  });
});

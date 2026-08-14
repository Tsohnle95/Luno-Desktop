import { describe, expect, it } from "vitest";
import { createBridge } from "../app/bridge";
import { createBackendProjection } from "../state/backend_projection";

const event = (event: unknown, generation = 1) => ({ version: 1 as const, event_id: `event-${generation}`, generation, job_id: null, session_id: null, event });

describe("typed IPC bridge and projection", () => {
  it("wraps commands and validates subscriptions", async () => {
    const calls: unknown[] = [];
    let callback = (_payload: unknown) => {};
    const bridge = createBridge({ invoke: async (_name, payload) => { calls.push(payload); return null; }, listen: async (_name, handler) => { callback = handler; return () => {}; } });
    await bridge.play("track-1");
    expect(calls).toHaveLength(1);
    expect((calls[0] as { version: number }).version).toBe(1);
    await bridge.subscribe(() => {});
    expect(calls).toHaveLength(2);
    expect(() => callback(event({ type: "error", payload: { error: { code: "internal", message: "x", retryable: false } } }))).not.toThrow();
    expect(() => callback({})).toThrow();
    expect(() => callback(event({ type: "playback_snapshot", payload: { snapshot: { queue: [], current_index: null, position_ms: -1, duration_ms: null, status: "stopped", generation: 1 } } }))).toThrow();
    expect(() => callback(event({ type: "job_updated", payload: { job: { id: "j", kind: "single", state: "running", track_id: null, progress: 2, generation: 1 } } }))).toThrow();
  });

  it("projects lifecycle events and rejects stale generations", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "error", payload: { error: { code: "internal", message: "new", retryable: false } } }, 2));
    store.apply(event({ type: "error", payload: { error: { code: "internal", message: "old", retryable: false } } }, 1));
    expect(store.getSnapshot().errors).toHaveLength(1);
    store.apply({ ...event({ type: "shutdown", payload: { envelope: { version: 1, state: "complete", reason: "system" } } }, 2), event_id: "3" });
    expect(store.getSnapshot().shutdown?.state).toBe("complete");
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
    expect(store.getSnapshot().lastEventId).toBe("event-2");
  });
});

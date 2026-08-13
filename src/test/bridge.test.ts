import { describe, expect, it } from "vitest";
import { createBridge } from "../app/bridge";
import { createBackendProjection } from "../state/backend_projection";

const event = (event: unknown, generation = 1) => ({ version: 1 as const, event_id: `${generation}`, generation, job_id: null, session_id: null, event });

describe("typed IPC bridge and projection", () => {
  it("wraps commands and validates subscriptions", async () => {
    const calls: unknown[] = [];
    let callback = (_payload: unknown) => {};
    const bridge = createBridge({ invoke: async (_name, payload) => { calls.push(payload); return null; }, listen: async (_name, handler) => { callback = handler; return () => {}; } });
    await bridge.play("track-1");
    expect(calls).toHaveLength(1);
    expect((calls[0] as { version: number }).version).toBe(1);
    await bridge.subscribe(() => {});
    expect(() => callback(event({ type: "error", payload: { error: { code: "internal", message: "x", retryable: false } } }))).not.toThrow();
    expect(() => callback({})).toThrow();
  });

  it("projects lifecycle events and rejects stale generations", () => {
    const store = createBackendProjection();
    store.apply(event({ type: "error", payload: { error: { code: "internal", message: "new", retryable: false } } }, 2));
    store.apply(event({ type: "error", payload: { error: { code: "internal", message: "old", retryable: false } } }, 1));
    expect(store.getSnapshot().errors).toHaveLength(1);
    store.apply(event({ type: "shutdown", payload: { envelope: { version: 1, state: "complete", reason: "system" } } }, 2));
    expect(store.getSnapshot().shutdown?.state).toBe("complete");
  });
});

import { describe, expect, it } from "vitest";
import { runGate5SmokeProbe, type Gate5SmokeBridge } from "../app/gate5Smoke";
import type { EventEnvelope } from "../shared/contracts";

const config = { root: "/fixture/library", preview: "/fixture/previews", video: "/fixture/video", timeoutMs: 5 };
const shutdownEvent: EventEnvelope = { version: 1, event_id: "event-1", generation: 1, job_id: null, session_id: null, event: { type: "shutdown", payload: { envelope: { version: 1, state: "complete", reason: "user_requested" } } } };
const result = { discovery_items: 1, discovery_artwork: true, artwork_design: 2, preview_ready: true, preview_hidden_before_save: true, saved_visible_after_scan: true, video_ready: true, unobserved: ["FFmpeg/process/native audio clock"] };

function fakeBridge(): Gate5SmokeBridge {
  let smoke: ((value: typeof result) => void) | undefined;
  let backend: ((value: EventEnvelope) => void) | undefined;
  return {
    subscribeGate5Smoke: async (handler) => { smoke = handler; return () => { smoke = undefined; }; },
    subscribe: async (handler) => { backend = handler; return () => { backend = undefined; }; },
    gate5Smoke: async () => { queueMicrotask(() => smoke?.(result)); return result; },
    shutdown: async () => { queueMicrotask(() => backend?.(shutdownEvent)); },
  };
}

describe("Gate 5 native smoke probe", () => {
  it("requires the native result event and complete shutdown observation", async () => {
    const outcome = await runGate5SmokeProbe(fakeBridge(), config);
    expect(outcome.passed).toBe(true);
    expect(outcome.eventObserved).toBe(true);
    expect(outcome.shutdownComplete).toBe(true);
  });

  it("does not pass when either native lifecycle observation is missing", async () => {
    const bridge = fakeBridge();
    const missingEvent = { ...bridge, subscribeGate5Smoke: async () => () => undefined };
    const eventOutcome = await runGate5SmokeProbe(missingEvent, config);
    expect(eventOutcome.passed).toBe(false);
    expect(eventOutcome.eventObserved).toBe(false);

    const missingShutdown = { ...fakeBridge(), shutdown: async () => undefined };
    const shutdownOutcome = await runGate5SmokeProbe(missingShutdown, config);
    expect(shutdownOutcome.passed).toBe(false);
    expect(shutdownOutcome.shutdownComplete).toBe(false);
  });

  it("does not wait forever for a native command that never resolves", async () => {
    const bridge = { ...fakeBridge(), gate5Smoke: async () => new Promise<typeof result>(() => {}) };
    const outcome = await runGate5SmokeProbe(bridge, { ...config, timeoutMs: 1 });
    expect(outcome.passed).toBe(false);
    expect(outcome.errors).toContain("gate5_smoke result timed out");
  });
});

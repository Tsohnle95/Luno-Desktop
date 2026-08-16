import { useEffect, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { Bridge, Gate5SmokeResult } from "./bridge";

export type Gate5SmokeBridge = Pick<Bridge, "gate5Smoke" | "subscribeGate5Smoke" | "subscribe" | "shutdown">;
export interface Gate5SmokeConfig { root: string; preview: string; video: string; timeoutMs?: number }
export interface Gate5SmokeOutcome { result?: Gate5SmokeResult; eventObserved: boolean; shutdownComplete: boolean; errors: string[]; passed: boolean }

export async function runGate5SmokeProbe(bridge: Gate5SmokeBridge, config: Gate5SmokeConfig): Promise<Gate5SmokeOutcome> {
  const errors: string[] = [];
  let eventObserved = false;
  let shutdownComplete = false;
  let result: Gate5SmokeResult | undefined;
  let unlistenSmoke: UnlistenFn | undefined;
  let unlistenBackend: UnlistenFn | undefined;
  const timeout = config.timeoutMs ?? 15_000;
  let resolveSmoke: (() => void) | undefined;
  let resolveShutdown: (() => void) | undefined;
  const smokeObserved = new Promise<void>((resolve) => { resolveSmoke = resolve; });
  const shutdownObserved = new Promise<void>((resolve) => { resolveShutdown = resolve; });
  const timeoutMarker = Symbol("timeout");
  const withinTimeout = async function <T>(operation: Promise<T>): Promise<T | typeof timeoutMarker> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      return await Promise.race([
        operation,
        new Promise<typeof timeoutMarker>((resolve) => { timer = setTimeout(() => resolve(timeoutMarker), timeout); }),
      ]);
    } finally {
      if (timer !== undefined) clearTimeout(timer);
    }
  };
  const observeWithinTimeout = (observation: Promise<void>) => withinTimeout(observation).then((value) => value !== timeoutMarker);
  try {
    unlistenSmoke = await bridge.subscribeGate5Smoke((value) => { eventObserved = true; result = value; resolveSmoke?.(); });
    unlistenBackend = await bridge.subscribe((event) => {
      if (event.event.type === "shutdown" && event.event.payload.envelope.state === "complete") {
        shutdownComplete = true;
        resolveShutdown?.();
      }
    });
    const nativeResult = await withinTimeout(bridge.gate5Smoke(config.root, config.preview, config.video));
    if (nativeResult === timeoutMarker) errors.push("gate5_smoke result timed out");
    else result = nativeResult;
    eventObserved = await observeWithinTimeout(smokeObserved);
  } catch (error) {
    errors.push(error instanceof Error ? error.message : String(error));
  } finally {
    try {
      const shutdownResult = await withinTimeout(bridge.shutdown("user_requested"));
      if (shutdownResult === timeoutMarker) errors.push("shutdown command timed out");
    } catch (error) { errors.push(error instanceof Error ? error.message : String(error)); }
    shutdownComplete = shutdownComplete || await observeWithinTimeout(shutdownObserved);
    unlistenSmoke?.();
    unlistenBackend?.();
  }
  const complete = result && result.discovery_items > 0 && result.discovery_artwork && result.preview_ready && result.preview_hidden_before_save && result.saved_visible_after_scan && result.video_ready;
  return { result, eventObserved, shutdownComplete, errors, passed: Boolean(complete && eventObserved && shutdownComplete && errors.length === 0) };
}

export function Gate5Smoke({ bridge, config }: { bridge: Gate5SmokeBridge; config: Gate5SmokeConfig }) {
  const started = useRef(false);
  const [outcome, setOutcome] = useState<Gate5SmokeOutcome>({ eventObserved: false, shutdownComplete: false, errors: [], passed: false });
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void runGate5SmokeProbe(bridge, config).then(setOutcome);
  }, [bridge, config]);
  const details = outcome.result ? `Discovery ${outcome.result.discovery_items}; preview ${outcome.result.preview_ready ? "ready" : "missing"}; video ${outcome.result.video_ready ? "ready" : "missing"}. Native event ${outcome.eventObserved ? "observed" : "missing"}; shutdown ${outcome.shutdownComplete ? "complete" : "missing"}. ${outcome.errors.length ? `Errors: ${outcome.errors.join("; ")}. ` : ""}Unobserved: ${outcome.result.unobserved.join("; ")}.` : "Native Gate 5 smoke is running.";
  return <div data-gate5-smoke="true" data-state={outcome.passed ? "pass" : outcome.result || outcome.errors.length ? "fail" : "running"} role="status" aria-live="polite" style={{ position: "fixed", top: 140, right: 12, zIndex: 9999, maxWidth: "min(700px, calc(100vw - 24px))", padding: "10px 12px", color: "#fff", background: "#202124", border: "1px solid #fff", borderRadius: 4, fontFamily: "system-ui, sans-serif", fontSize: 14, lineHeight: 1.4 }}>{details}</div>;
}
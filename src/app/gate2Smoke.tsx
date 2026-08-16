import { useEffect, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { isValidLibraryResponse, type Bridge } from "./bridge";
import type { EventEnvelope } from "../shared/contracts";

export type Gate2SmokeBridge = Pick<Bridge, "registerRoot" | "promoteAudioOutput" | "scanLibrary" | "getLibrary" | "subscribe">;
export interface Gate2SmokeConfig { root: string; temporary: string; destination: string; timeoutMs?: number }
export interface Gate2SmokeResult { trackCount: number }

function errorText(error: unknown, paths: string[]): string {
  const raw = error instanceof Error ? error.message
    : typeof error === "string" ? error
      : error && typeof error === "object" && typeof (error as Record<string, unknown>).message === "string" ? (error as Record<string, string>).message
        : error && typeof error === "object" && typeof (error as Record<string, unknown>).code === "string" ? (error as Record<string, string>).code
          : "Gate 2 smoke probe failed";
  const sanitized = paths.filter((path) => path.length >= 4).reduce((message, path) => message.split(path).join("<path>"), raw)
    .replace(/\/(?:Users|private|tmp)\/[^\s,;)}]+/g, "<path>");
  return sanitized.slice(0, 240) || "Gate 2 smoke probe failed";
}

export async function runGate2SmokeProbe(bridge: Gate2SmokeBridge, config: Gate2SmokeConfig): Promise<Gate2SmokeResult> {
  const timeoutMs = config.timeoutMs ?? 10_000;
  const paths = [config.root, config.temporary, config.destination];
  let unlisten: UnlistenFn | undefined;
  let subscriptionSettled = false;
  let aborted = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let resolveSnapshot!: () => void;
  let rejectSnapshot!: (error: Error) => void;
  const snapshotSeen = new Promise<void>((resolve, reject) => { resolveSnapshot = resolve; rejectSnapshot = reject; });
  const operation = async () => {
    const subscription = await bridge.subscribe((event: EventEnvelope) => {
      if (event.event.type === "library_snapshot") resolveSnapshot();
    });
    if (subscriptionSettled || aborted) subscription();
    else unlisten = subscription;
    if (aborted) throw new Error("Gate 2 smoke probe timed out");
    await bridge.registerRoot(config.root);
    if (aborted) throw new Error("Gate 2 smoke probe timed out");
    await bridge.promoteAudioOutput(config.temporary, config.destination);
    if (aborted) throw new Error("Gate 2 smoke probe timed out");
    await bridge.scanLibrary();
    if (aborted) throw new Error("Gate 2 smoke probe timed out");
    await snapshotSeen;
    const response = await bridge.getLibrary();
    if (!isValidLibraryResponse(response)) throw new Error("invalid library response");
    if (response.tracks.length === 0) throw new Error("library response contained no tracks");
    return { trackCount: response.tracks.length };
  };
  try {
    return await new Promise<Gate2SmokeResult>((resolve, reject) => {
      timer = setTimeout(() => { aborted = true; rejectSnapshot(new Error("timed out waiting for library_snapshot")); reject(new Error("Gate 2 smoke probe timed out")); }, timeoutMs);
      void operation().then(resolve, reject);
    });
  } catch (error) {
    throw new Error(errorText(error, paths));
  } finally {
    subscriptionSettled = true;
    if (timer) clearTimeout(timer);
    unlisten?.();
  }
}

export function Gate2Smoke({ bridge, config }: { bridge: Gate2SmokeBridge; config: Gate2SmokeConfig }) {
  const started = useRef(false);
  const [status, setStatus] = useState<{ state: "running" | "pass" | "fail"; text: string }>({ state: "running", text: "Gate 2 smoke probe running" });
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void runGate2SmokeProbe(bridge, config)
      .then(({ trackCount }) => setStatus({ state: "pass", text: `Gate 2 smoke pass: registerRoot, promoteAudioOutput, scanLibrary, library_snapshot, getLibrary; ${trackCount} track${trackCount === 1 ? "" : "s"} observed.` }))
      .catch((error: unknown) => setStatus({ state: "fail", text: `Gate 2 smoke fail: ${errorText(error, [config.root, config.temporary, config.destination])}` }));
  }, [bridge, config]);
  return <div data-gate2-smoke="true" data-state={status.state} role="status" aria-live="polite" style={{ position: "fixed", top: 12, right: 12, zIndex: 9999, maxWidth: "min(520px, calc(100vw - 24px))", padding: "10px 12px", color: "#fff", background: "#202124", border: "1px solid #fff", borderRadius: 4, fontFamily: "system-ui, sans-serif", fontSize: 14, lineHeight: 1.4, boxShadow: "0 2px 8px rgba(0, 0, 0, 0.35)" }}>{status.text}</div>;
}

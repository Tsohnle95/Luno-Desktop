import { useEffect, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { Bridge } from "./bridge";
import type { EventEnvelope, PlaybackSnapshot } from "../shared/contracts";

export type Gate3SmokeBridge = Pick<Bridge, "registerRoot" | "promoteAudioOutput" | "scanLibrary" | "getLibrary" | "playWithContext" | "pause" | "seek" | "playNext" | "addToQueue" | "next" | "previous" | "shutdown" | "subscribe">;
export interface Gate3SmokeConfig { root: string; temporary: string; destination: string; timeoutMs?: number }
export interface Gate3SmokeResult {
  trackId: string;
  playbackSnapshot: PlaybackSnapshot | null;
  trackStarted: boolean;
  playbackError: string | null;
  shutdown: boolean;
  commands: string[];
  errors: Array<{ command: string; message: string }>;
}

function errorText(error: unknown, paths: string[]): string {
  const raw = error instanceof Error ? error.message : typeof error === "string" ? error : "Gate 3 smoke probe failed";
  return paths.filter((path) => path.length >= 4).reduce((message, path) => message.split(path).join("<path>"), raw)
    .replace(/\/(?:Users|private|tmp)\/[^\s,;)}]+/g, "<path>").slice(0, 240);
}

export async function runGate3SmokeProbe(bridge: Gate3SmokeBridge, config: Gate3SmokeConfig): Promise<Gate3SmokeResult> {
  const timeoutMs = config.timeoutMs ?? 15_000;
  const paths = [config.root, config.temporary, config.destination];
  const commands: string[] = [];
  let unlisten: UnlistenFn | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let resolveLibrary!: (trackId: string) => void;
  let resolvePlayback!: () => void;
  let resolveShutdown!: () => void;
  let playbackSnapshot: PlaybackSnapshot | null = null;
  let trackStarted = false;
  let shutdownObserved = false;
  let playbackError: string | null = null;
  const errors: Array<{ command: string; message: string }> = [];
  const librarySeen = new Promise<string>((resolve) => { resolveLibrary = resolve; });
  const playbackSeen = new Promise<void>((resolve) => { resolvePlayback = resolve; });
  const shutdownSeen = new Promise<void>((resolve) => { resolveShutdown = resolve; });
  const recordError = (command: string, error: unknown) => {
    const message = errorText(error, paths);
    errors.push({ command, message });
    playbackError ??= message;
  };
  const runCommand = async (command: string, action: () => Promise<unknown>) => {
    commands.push(command);
    try {
      await action();
    } catch (error) {
      recordError(command, error);
    }
  };
  const waitFor = (signal: Promise<void>) => Promise.race([signal, new Promise<void>((resolve) => setTimeout(resolve, timeoutMs))]);
  const operation = async () => {
    unlisten = await bridge.subscribe((envelope: EventEnvelope) => {
      if (envelope.event.type === "library_snapshot") {
        const track = envelope.event.payload.snapshot.tracks[0];
        if (track) resolveLibrary(track.id);
       } else if (envelope.event.type === "playback_snapshot") {
         playbackSnapshot = envelope.event.payload.snapshot;
          if (playbackSnapshot.status === "failed") {
            playbackError = "playback snapshot reported failed";
            errors.push({ command: "playback_snapshot", message: playbackError });
          }
         if (trackStarted || playbackError) resolvePlayback();
       } else if (envelope.event.type === "track_started") {
         trackStarted = true;
         resolvePlayback();
       } else if (envelope.event.type === "error") {
          playbackError = errorText(envelope.event.payload.error.message, paths);
          errors.push({ command: "backend-event", message: playbackError });
          resolvePlayback();
       } else if (envelope.event.type === "shutdown" && envelope.event.payload.envelope.state === "complete") {
         shutdownObserved = true;
         resolveShutdown();
      }
    });
    commands.push("subscribe");
    let trackId = "unknown";
    try {
      await bridge.registerRoot(config.root);
      commands.push("registerRoot");
      await bridge.promoteAudioOutput(config.temporary, config.destination);
      commands.push("promoteAudioOutput");
      await bridge.scanLibrary();
      commands.push("scanLibrary");
      trackId = await Promise.race([librarySeen, new Promise<string>((resolve) => setTimeout(() => resolve("unknown"), timeoutMs))]);
      const library = await bridge.getLibrary();
      commands.push("getLibrary");
      if (!library || typeof library !== "object" || !Array.isArray((library as { tracks?: unknown }).tracks) || (library as { tracks: unknown[] }).tracks.length === 0) throw new Error("library response contained no tracks");
      await runCommand("playWithContext", () => bridge.playWithContext(trackId, [trackId]));
      if (playbackError) resolvePlayback();
      await waitFor(playbackSeen);
      if (!playbackSnapshot) errors.push({ command: "playback_snapshot", message: "no playback snapshot observed" });
      if (playbackSnapshot && playbackSnapshot.status !== "playing") errors.push({ command: "playback_snapshot", message: `playback status was ${playbackSnapshot.status}` });
      if (!trackStarted) errors.push({ command: "track_started", message: "no track_started event observed" });
      await runCommand("pause", () => bridge.pause());
      await runCommand("seek", () => bridge.seek(250));
      // The backend pause command toggles paused/playing, so this is the real resume operation.
      await runCommand("resume", () => bridge.pause());
      await runCommand("playNext", () => bridge.playNext([trackId]));
      await runCommand("addToQueue", () => bridge.addToQueue([trackId]));
      await runCommand("next", () => bridge.next());
      await runCommand("previous", () => bridge.previous());
    } catch (error) {
      recordError("probe", error);
    } finally {
      await runCommand("shutdown", () => bridge.shutdown("user_requested"));
      await waitFor(shutdownSeen);
      if (!shutdownObserved) errors.push({ command: "probe", message: "no complete shutdown event observed" });
    }
    return { trackId, playbackSnapshot, trackStarted, playbackError, shutdown: shutdownObserved, commands, errors };
  };
  try {
    timer = setTimeout(() => undefined, timeoutMs * 3);
    return await operation();
  } catch (error) {
    throw new Error(errorText(error, paths));
  } finally {
    if (timer) clearTimeout(timer);
    unlisten?.();
  }
}

export function Gate3Smoke({ bridge, config }: { bridge: Gate3SmokeBridge; config: Gate3SmokeConfig }) {
  const started = useRef(false);
  const [status, setStatus] = useState<{ state: "running" | "pass" | "fail"; text: string }>({ state: "running", text: "Gate 3 smoke probe running" });
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void runGate3SmokeProbe(bridge, config)
       .then((result) => {
           const failed = result.errors.length > 0 || result.playbackError || result.playbackSnapshot?.status !== "playing" || !result.trackStarted || !result.shutdown;
          setStatus({
            state: failed ? "fail" : "pass",
             text: `Gate 3 smoke ${failed ? "fail" : "observed"} ${result.trackId}: ${result.commands.join(", ")}${result.errors.length ? `; errors: ${result.errors.map((item) => `${item.command}: ${item.message}`).join("; ")}` : ""}${result.playbackSnapshot?.status !== "playing" ? "; no playing playback snapshot observed" : ""}${!result.trackStarted ? "; no track_started event observed" : ""}${!result.shutdown ? "; no complete shutdown event observed" : ""}`,
         });
       })
      .catch((error: unknown) => setStatus({ state: "fail", text: `Gate 3 smoke fail: ${errorText(error, [config.root, config.temporary, config.destination])}` }));
  }, [bridge, config]);
  return <div data-gate3-smoke="true" data-state={status.state} role="status" aria-live="polite" style={{ position: "fixed", top: 56, right: 12, zIndex: 9999, maxWidth: "min(620px, calc(100vw - 24px))", padding: "10px 12px", color: "#fff", background: "#202124", border: "1px solid #fff", borderRadius: 4, fontFamily: "system-ui, sans-serif", fontSize: 14, lineHeight: 1.4, boxShadow: "0 2px 8px rgba(0, 0, 0, 0.35)" }}>{status.text}</div>;
}

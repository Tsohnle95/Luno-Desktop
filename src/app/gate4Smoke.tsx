import { useEffect, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { Bridge } from "./bridge";
import type { EventEnvelope, JobState, SettingsInput } from "../shared/contracts";

export type Gate4SmokeBridge = Pick<Bridge, "loadSettings" | "saveSettings" | "registerRoot" | "selectLibraryRoot" | "previewCsv" | "validateDirectUrl" | "queueUrls" | "syncMarkers" | "transferExport" | "transferPreview" | "transferApply" | "startDownload" | "getLibrary" | "scanLibrary" | "shutdown" | "subscribe">;
export interface Gate4SmokeConfig { home: string; root: string; helper: string; destination: string; timeoutMs?: number }
export interface Gate4CommandResult { name: string; succeeded: boolean }
export interface Gate4SmokeResult {
  commands: Gate4CommandResult[];
  jobStates: Array<{ id: string; state: JobState }>;
  errors: Array<{ command: string; message: string }>;
  missingObservations: string[];
  shutdownComplete: boolean;
  unexercisedBoundaries: string[];
  passed: boolean;
}

const UNEXERCISED_BOUNDARIES = [
  "provider search and provider/network access",
  "native file-picker UI",
  "real external provider credentials or services",
];

function errorText(error: unknown, paths: string[]): string {
  const raw = error instanceof Error ? error.message : typeof error === "string" ? error : "Gate 4 smoke probe failed";
  return paths.filter((path) => path.length >= 1).reduce((message, path) => message.split(path).join("<path>"), raw)
    .replace(/\/(?:Users|private|tmp|var)\/[^\s,;)}]+/g, "<path>").slice(0, 240) || "Gate 4 smoke probe failed";
}

const fallbackManifest = JSON.stringify({ format: "luno.library.export", manifest_version: 1, scope: "library", created_at: "1970-01-01T00:00:00Z", tracks: [], playlists: [], unassigned_track_refs: [] });

export async function runGate4SmokeProbe(bridge: Gate4SmokeBridge, config: Gate4SmokeConfig): Promise<Gate4SmokeResult> {
  const timeoutMs = config.timeoutMs ?? 15_000;
  const paths = [config.home, config.root, config.helper, config.destination];
  const commands: Gate4CommandResult[] = [];
  const errors: Array<{ command: string; message: string }> = [];
  const jobStates: Array<{ id: string; state: JobState }> = [];
  const missingObservations: string[] = [];
  let unlisten: UnlistenFn | undefined;
  let shutdownComplete = false;
  let succeededJob = false;
  let resolveJob!: () => void;
  let resolveShutdown!: () => void;
  const jobSeen = new Promise<void>((resolve) => { resolveJob = resolve; });
  const shutdownSeen = new Promise<void>((resolve) => { resolveShutdown = resolve; });
  const jobId = "gate4-smoke-job";

  const recordError = (command: string, error: unknown) => errors.push({ command, message: errorText(error, paths) });
  const runCommand = async <T,>(name: string, action: () => Promise<T>): Promise<T | undefined> => {
    try {
      const value = await action();
      commands.push({ name, succeeded: true });
      return value;
    } catch (error) {
      commands.push({ name, succeeded: false });
      recordError(name, error);
      return undefined;
    }
  };
  const waitFor = (signal: Promise<void>) => Promise.race([signal, new Promise<void>((resolve) => setTimeout(resolve, timeoutMs))]);
  const settings: SettingsInput = { music_root: config.root, auto_discovery: false, volume: 0.5, sleep_timer: null, global_keys: false, favorite_songs: [] };

  try {
    unlisten = await bridge.subscribe((envelope: EventEnvelope) => {
      if (envelope.event.type === "job_updated" && envelope.event.payload.job.id === jobId) {
        const job = envelope.event.payload.job;
        jobStates.push({ id: job.id, state: job.state });
        if (job.state === "succeeded") { succeededJob = true; resolveJob(); }
        if (job.state === "failed" || job.state === "cancelled") resolveJob();
      }
      if (envelope.event.type === "error") recordError("backend-event", envelope.event.payload.error.message);
      if (envelope.event.type === "shutdown" && envelope.event.payload.envelope.state === "complete") { shutdownComplete = true; resolveShutdown(); }
    });
    commands.push({ name: "subscribe", succeeded: true });

    await runCommand("saveSettings", () => bridge.saveSettings(config.home, settings));
    await runCommand("loadSettings", () => bridge.loadSettings(config.home));
    await runCommand("registerRoot", () => bridge.registerRoot(config.root));
    await runCommand("selectLibraryRoot", () => bridge.selectLibraryRoot(config.root));
    await runCommand("previewCsv", () => bridge.previewCsv("Track Name,Artist Name(s)\nGate 4 Fixture,Fixture Artist\n"));
    await runCommand("validateDirectUrl", () => bridge.validateDirectUrl("https://example.test/gate4-fixture", config.destination));
    await runCommand("queueUrls", () => bridge.queueUrls(["https://example.test/gate4-fixture"]));
    await runCommand("syncMarkers", () => bridge.syncMarkers(config.root));
    await runCommand("scanLibrary", () => bridge.scanLibrary());
    await runCommand("getLibrary", () => bridge.getLibrary());
    const exported = await runCommand("transferExport", () => bridge.transferExport());
    const manifest = exported && typeof exported === "object" && typeof (exported as { manifest?: unknown }).manifest === "string"
      ? (exported as { manifest: string }).manifest : fallbackManifest;
    await runCommand("transferPreview", () => bridge.transferPreview(manifest, config.destination));
    await runCommand("transferApply", () => bridge.transferApply(manifest, config.destination, {}));
    await runCommand("startDownload", () => bridge.startDownload(jobId, `${config.destination}/gate4-smoke.mp3`, config.helper));
    await waitFor(jobSeen);
    if (!jobStates.length) missingObservations.push(`no job_updated event observed for ${jobId}`);
    else if (!succeededJob) missingObservations.push(`no succeeded job_updated event observed for ${jobId}`);
  } catch (error) {
    recordError("probe", error);
  } finally {
    await runCommand("shutdown", () => bridge.shutdown("user_requested"));
    await waitFor(shutdownSeen);
    if (!shutdownComplete) missingObservations.push("no complete shutdown event observed");
    unlisten?.();
  }

  const passed = commands.every((command) => command.succeeded) && errors.length === 0 && succeededJob && shutdownComplete && missingObservations.length === 0;
  return { commands, jobStates, errors, missingObservations, shutdownComplete, unexercisedBoundaries: UNEXERCISED_BOUNDARIES, passed };
}

export function Gate4Smoke({ bridge, config }: { bridge: Gate4SmokeBridge; config: Gate4SmokeConfig }) {
  const started = useRef(false);
  const [status, setStatus] = useState<{ state: "running" | "pass" | "fail"; result?: Gate4SmokeResult }>({ state: "running" });
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void runGate4SmokeProbe(bridge, config).then((result) => setStatus({ state: result.passed ? "pass" : "fail", result }));
  }, [bridge, config]);
  const result = status.result;
  const details = result ? `Commands: ${result.commands.map((command) => `${command.name}${command.succeeded ? "" : " (failed)"}`).join(", ")}. Jobs: ${result.jobStates.map((job) => `${job.id}=${job.state}`).join(", ") || "none"}. ${result.errors.length ? `Errors: ${result.errors.map((error) => `${error.command}: ${error.message}`).join("; ")}. ` : ""}${result.missingObservations.length ? `Missing: ${result.missingObservations.join("; ")}. ` : ""}Unexercised: ${result.unexercisedBoundaries.join("; ")}.` : "Commands, job completion, and shutdown observations are pending.";
  return <div data-gate4-smoke="true" data-state={status.state} role="status" aria-live="polite" style={{ position: "fixed", top: 100, right: 12, zIndex: 9999, maxWidth: "min(700px, calc(100vw - 24px))", padding: "10px 12px", color: "#fff", background: "#202124", border: "1px solid #fff", borderRadius: 4, fontFamily: "system-ui, sans-serif", fontSize: 14, lineHeight: 1.4, boxShadow: "0 2px 8px rgba(0, 0, 0, 0.35)" }}><strong>Gate 4 smoke {status.state}</strong><div>{details}</div></div>;
}

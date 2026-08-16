import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import { describe, expect, it } from "vitest";
import { Gate4Smoke, runGate4SmokeProbe, type Gate4SmokeBridge } from "../app/gate4Smoke";
import type { EventEnvelope } from "../shared/contracts";

const config = { home: "/fixture/home", root: "/fixture/root", helper: "/fixture/root/fake-helper", destination: "/fixture/root/destination", timeoutMs: 20 };
const envelope = (event: EventEnvelope["event"]): EventEnvelope => ({ version: 1, event_id: "event-1", generation: 1, job_id: null, session_id: null, event });

function fakeBridge(options: { jobState?: "succeeded" | "failed"; shutdown?: boolean } = {}) {
  let handler: ((event: EventEnvelope) => void) | undefined;
  const calls: string[] = [];
  const bridge: Gate4SmokeBridge = {
    subscribe: async (next) => { calls.push("subscribe"); handler = next; return () => { handler = undefined; }; },
    loadSettings: async () => { calls.push("loadSettings"); return {}; }, saveSettings: async () => { calls.push("saveSettings"); return {}; },
    registerRoot: async () => { calls.push("registerRoot"); }, selectLibraryRoot: async () => { calls.push("selectLibraryRoot"); },
    previewCsv: async () => { calls.push("previewCsv"); return {}; }, validateDirectUrl: async () => { calls.push("validateDirectUrl"); return {}; },
    queueUrls: async () => { calls.push("queueUrls"); return {}; }, syncMarkers: async () => { calls.push("syncMarkers"); return {}; },
    scanLibrary: async () => { calls.push("scanLibrary"); }, getLibrary: async () => { calls.push("getLibrary"); return {}; },
    transferExport: async () => { calls.push("transferExport"); return { manifest: JSON.stringify({ format: "luno.library.export", manifest_version: 1, scope: "library", created_at: "1970-01-01T00:00:00Z", tracks: [], playlists: [], unassigned_track_refs: [] }) }; },
    transferPreview: async () => { calls.push("transferPreview"); return {}; }, transferApply: async () => { calls.push("transferApply"); return {}; },
    startDownload: async () => { calls.push("startDownload"); queueMicrotask(() => handler?.(envelope({ type: "job_updated", payload: { job: { id: "gate4-smoke-job", kind: "single", state: options.jobState ?? "succeeded", track_id: null, progress: 1, generation: 1 } } }))); },
    shutdown: async () => { calls.push("shutdown"); if (options.shutdown !== false) queueMicrotask(() => handler?.(envelope({ type: "shutdown", payload: { envelope: { version: 1, state: "complete", reason: "user_requested" } } }))); },
  } as Gate4SmokeBridge;
  return { bridge, calls };
}

describe("Gate 4 runtime smoke probe", () => {
  it("renders a fixed running accessible overlay initially", () => {
    const html = renderToStaticMarkup(createElement(Gate4Smoke, { bridge: fakeBridge().bridge, config }));
    expect(html).toContain('data-gate4-smoke="true"');
    expect(html).toContain('data-state="running"');
    expect(html).toContain('role="status"');
  });

  it("records command, job, and shutdown observations and passes only on complete success", async () => {
    const fixture = fakeBridge();
    const result = await runGate4SmokeProbe(fixture.bridge, config);
    expect(result.passed).toBe(true);
    expect(result.commands.map((command) => command.name)).toEqual(["subscribe", "saveSettings", "loadSettings", "registerRoot", "selectLibraryRoot", "previewCsv", "validateDirectUrl", "queueUrls", "syncMarkers", "scanLibrary", "getLibrary", "transferExport", "transferPreview", "transferApply", "startDownload", "shutdown"]);
    expect(result.jobStates).toEqual([{ id: "gate4-smoke-job", state: "succeeded" }]);
  });

  it("sanitizes errors and reports failed or missing observations", async () => {
    const failed = await runGate4SmokeProbe(fakeBridge({ jobState: "failed", shutdown: false }).bridge, config);
    expect(failed.passed).toBe(false);
    expect(failed.missingObservations).toContain("no succeeded job_updated event observed for gate4-smoke-job");
    expect(failed.missingObservations).toContain("no complete shutdown event observed");
    const throwing = fakeBridge();
    throwing.bridge.saveSettings = async () => { throw new Error(`cannot write ${config.home}/.luno_config.json`); };
    const result = await runGate4SmokeProbe(throwing.bridge, config);
    expect(result.passed).toBe(false);
    expect(result.errors).toContainEqual({ command: "saveSettings", message: "cannot write <path>/.luno_config.json" });
  });
});

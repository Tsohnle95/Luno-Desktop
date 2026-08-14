import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";
import {
  CONTRACT_VERSION,
  type Command,
  type CommandEnvelope,
  type EventEnvelope,
  isCommandEnvelope,
  isEventEnvelope,
  type ShutdownReason,
  type TrackId,
  parseEventEnvelope,
} from "../shared/contracts";

export type MigrationStatus =
  | { status: "not_run" }
  | { status: "skipped"; reason: string }
  | { status: "completed"; warnings: number; snapshot_created: boolean; removed_previews: number }
  | { status: "failed"; reason: string };

export const BACKEND_EVENT = "backend-event";
export const BACKEND_COMMAND = "backend_command";

export interface BridgeTransport {
  invoke<T>(command: string, payload: unknown): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn>;
}

const tauriTransport: BridgeTransport = {
  invoke: <T>(command: string, payload: unknown) => tauriInvoke<T>(command, payload as InvokeArgs),
  listen: <T>(event: string, handler: (payload: T) => void) => listen<T>(event, (message) => handler(message.payload)),
};

let requestSequence = 0;
type UnitCommand = "get_library" | "scan_library" | "pause" | "video_play" | "video_pause";

function envelope(command: Command | UnitCommand): CommandEnvelope {
  const request_id = `frontend-${Date.now()}-${requestSequence++}`;
  const wireCommand: Command = typeof command === "string" ? { type: command } : command;
  const result = { version: CONTRACT_VERSION, request_id, command: wireCommand };
  if (!isCommandEnvelope(result)) throw new Error("invalid command envelope");
  return result;
}

function validEvent(value: unknown): EventEnvelope {
  const result = typeof value === "string" ? parseEventEnvelope(value) : value;
  if (!isEventEnvelope(result)) throw new Error("invalid or unsupported event envelope");
  return result;
}

export function createBridge(transport: BridgeTransport = tauriTransport) {
  const send = <T>(command: Command | UnitCommand) => transport.invoke<T>(BACKEND_COMMAND, envelope(command));
  return {
    getLibrary: () => send("get_library"),
    scanLibrary: () => send("scan_library"),
    play: (track_id: string) => send({ type: "play", payload: { track_id } }),
    pause: () => send("pause"),
    seek: (position_ms: number) => {
      if (!Number.isSafeInteger(position_ms) || position_ms < 0) return Promise.reject(new Error("position_ms must be a non-negative integer"));
      return send({ type: "seek", payload: { position_ms } });
    },
    videoSource: (track_id: TrackId) => send({ type: "video_source", payload: { track_id } }),
    videoStage: (track_id: TrackId) => send({ type: "video_stage", payload: { track_id } }),
    videoPlay: () => send("video_play"),
    videoPause: () => send("video_pause"),
    videoSeek: (position_ms: number) => send({ type: "video_seek", payload: { position_ms } }),
    videoResize: (width: number, height: number) => send({ type: "video_resize", payload: { width, height } }),
    videoFullscreen: (fullscreen: boolean) => send({ type: "video_fullscreen", payload: { fullscreen } }),
    migrationStatus: async (): Promise<MigrationStatus> => {
      const value = await transport.invoke<unknown>("migration_status", null);
      return parseMigrationStatus(value);
    },
    cancelJob: (job_id: string) => send({ type: "cancel_job", payload: { job_id } }),
    shutdown: (reason: ShutdownReason) => send({ type: "shutdown", payload: { reason } }),
  subscribe: (handler: (event: EventEnvelope) => void) =>
      transport.listen<unknown>(BACKEND_EVENT, (payload) => handler(validEvent(payload))).then((unlisten) => {
        // Listening first makes the command a replay request, not a race with startup emission.
        void send("get_library");
        return unlisten;
      }),
  };
}

export function parseMigrationStatus(value: unknown): MigrationStatus {
  if (value === "not_run") return { status: "not_run" };
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("invalid migration status");
  const entries = Object.entries(value as Record<string, unknown>);
  if (entries.length !== 1) throw new Error("invalid migration status");
  const [status, details] = entries[0];
  if (!details || typeof details !== "object" || Array.isArray(details) || !status) throw new Error("invalid migration status");
  const item = details as Record<string, unknown>;
  if ((status === "skipped" || status === "failed") && typeof item.reason === "string" && Object.keys(item).length === 1) return { status: status as "skipped" | "failed", reason: item.reason };
  if (status === "completed" && typeof item.warnings === "number" && Number.isSafeInteger(item.warnings) && item.warnings >= 0
    && typeof item.snapshot_created === "boolean" && typeof item.removed_previews === "number"
    && Number.isSafeInteger(item.removed_previews) && item.removed_previews >= 0 && Object.keys(item).length === 3) return { status: "completed", warnings: item.warnings, snapshot_created: item.snapshot_created, removed_previews: item.removed_previews };
  throw new Error("invalid migration status");
}

export const bridge = createBridge();
export type Bridge = ReturnType<typeof createBridge>;

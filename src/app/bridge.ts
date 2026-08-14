import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  CONTRACT_VERSION,
  type Command,
  type CommandEnvelope,
  type EventEnvelope,
  isCommandEnvelope,
  isEventEnvelope,
  type ShutdownReason,
  parseEventEnvelope,
} from "../shared/contracts";

export const BACKEND_EVENT = "backend-event";
export const BACKEND_COMMAND = "backend_command";

export interface BridgeTransport {
  invoke<T>(command: string, payload: unknown): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn>;
}

const tauriTransport: BridgeTransport = {
  invoke: (command, payload) => tauriInvoke(command, payload),
  listen: <T>(event, handler) => listen<T>(event, (message) => handler(message.payload)),
};

let requestSequence = 0;
type UnitCommand = "get_library" | "scan_library" | "pause";

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

export const bridge = createBridge();
export type Bridge = ReturnType<typeof createBridge>;

/** Versioned Rust/React IPC contract. Do not use paths as track identity. */
export const CONTRACT_VERSION = 1 as const;
export type ContractVersion = typeof CONTRACT_VERSION;
export type TrackId = string;
export type PlaylistId = string;
export type JobId = string;
export type SessionId = string;
export type Generation = number;

export interface ProviderId { provider: string; id: string }
export interface Track { id: TrackId; path: string | null; title: string; artist: string | null; album: string | null; duration_ms: number | null; artwork_url: string | null; favorite: boolean; provider: ProviderId | null }
export interface Playlist { id: PlaylistId; name: string; path: string | null; url: string | null; ordered_track_ids: TrackId[] }
export interface LibrarySnapshot { version: ContractVersion; generation: Generation; tracks: Track[]; playlists: Playlist[] }
export type PlaybackStatus = "stopped" | "playing" | "paused" | "failed";
export interface PlaybackSnapshot { queue: TrackId[]; current_index: number | null; position_ms: number; duration_ms: number | null; status: PlaybackStatus; generation: Generation }
export type DownloadKind = "single" | "csv" | "sync" | "url_queue";
export type JobState = "queued" | "running" | "cancelling" | "succeeded" | "failed" | "cancelled";
export interface DownloadJob { id: JobId; kind: DownloadKind; state: JobState; track_id: TrackId | null; progress: number | null; generation: Generation }
export type ErrorCode = "invalid_request" | "unsupported_version" | "not_found" | "busy" | "cancelled" | "internal" | "shutting_down";
export interface ContractError { code: ErrorCode; message: string; retryable: boolean }

export type Command =
  | { type: "get_library" } | { type: "scan_library" } | { type: "play"; payload: { track_id: TrackId } }
  | { type: "pause" } | { type: "seek"; payload: { position_ms: number } }
  | { type: "cancel_job"; payload: { job_id: JobId } }
  | { type: "shutdown"; payload: { reason: ShutdownReason } };
export interface CommandEnvelope { version: ContractVersion; request_id: string; command: Command }
export type RootStatus = "scanned" | "missing" | "inaccessible";
export type Event =
  | { type: "library_snapshot"; payload: { snapshot: LibrarySnapshot } }
  | { type: "library_root_status"; payload: { root_index: number; status: RootStatus } }
  | { type: "playback_snapshot"; payload: { snapshot: PlaybackSnapshot } }
  | { type: "track_started"; payload: { track_id: TrackId } }
  | { type: "queue_changed"; payload: { queue: TrackId[] } }
  | { type: "job_updated"; payload: { job: DownloadJob } }
  | { type: "error"; payload: { error: ContractError } }
  | { type: "shutdown"; payload: { envelope: ShutdownEnvelope } };
export interface EventEnvelope { version: ContractVersion; event_id: string; generation: Generation; job_id: JobId | null; session_id: SessionId | null; event: Event }
export type ShutdownState = "requested" | "draining" | "complete" | "timed_out" | "failed";
export type ShutdownReason = "user_requested" | "system" | "fatal_error";
export interface ShutdownEnvelope { version: ContractVersion; state: ShutdownState; reason: ShutdownReason }

const object = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const exact = (v: unknown, keys: string[]): v is Record<string, unknown> => object(v) && Object.keys(v).length === keys.length && Object.keys(v).every(k => keys.includes(k));
export function isContractVersion(v: unknown): v is ContractVersion { return v === CONTRACT_VERSION; }
function isCommand(v: unknown): v is Command {
  if (!object(v) || typeof v.type !== "string") return false;
  if (["get_library", "scan_library", "pause"].includes(v.type)) return Object.keys(v).length === 1;
  if (v.type === "play") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.track_id === "string" && Object.keys(v.payload).length === 1;
  if (v.type === "seek") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.position_ms === "number" && Object.keys(v.payload).length === 1;
  if (v.type === "cancel_job") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.job_id === "string" && Object.keys(v.payload).length === 1;
  if (v.type === "shutdown") return exact(v, ["type", "payload"]) && object(v.payload) && ["user_requested", "system", "fatal_error"].includes(String(v.payload.reason)) && Object.keys(v.payload).length === 1;
  return false;
}
function isEvent(v: unknown): v is Event {
  if (!object(v) || typeof v.type !== "string" || !exact(v, ["type", "payload"]) || !object(v.payload)) return false;
  if (v.type === "track_started") return exact(v.payload, ["track_id"]) && typeof v.payload.track_id === "string";
  if (v.type === "queue_changed") return exact(v.payload, ["queue"]) && Array.isArray(v.payload.queue) && v.payload.queue.every(id => typeof id === "string");
  if (v.type === "library_root_status") return exact(v.payload, ["root_index", "status"]) && Number.isInteger(v.payload.root_index) && v.payload.root_index >= 0 && ["scanned", "missing", "inaccessible"].includes(String(v.payload.status));
  if (v.type === "error") {
    const error = v.payload.error;
    return exact(v.payload, ["error"]) && object(error) && exact(error, ["code", "message", "retryable"]) && ["invalid_request", "unsupported_version", "not_found", "busy", "cancelled", "internal", "shutting_down"].includes(String(error.code)) && typeof error.message === "string" && typeof error.retryable === "boolean";
  }
  if (v.type === "shutdown") {
    const shutdown = v.payload.envelope;
    return exact(v.payload, ["envelope"]) && object(shutdown) && exact(shutdown, ["version", "state", "reason"]) && shutdown.version === CONTRACT_VERSION && ["requested", "draining", "complete", "timed_out", "failed"].includes(String(shutdown.state)) && ["user_requested", "system", "fatal_error"].includes(String(shutdown.reason));
  }
  return ["library_snapshot", "playback_snapshot", "job_updated"].includes(v.type);
}
export function isCommandEnvelope(v: unknown): v is CommandEnvelope { return exact(v, ["version", "request_id", "command"]) && isContractVersion(v.version) && typeof v.request_id === "string" && isCommand(v.command); }
export function isEventEnvelope(v: unknown): v is EventEnvelope { return exact(v, ["version", "event_id", "generation", "job_id", "session_id", "event"]) && isContractVersion(v.version) && typeof v.event_id === "string" && typeof v.generation === "number" && (v.job_id === null || typeof v.job_id === "string") && (v.session_id === null || typeof v.session_id === "string") && isEvent(v.event); }
export function parseCommandEnvelope(json: string): CommandEnvelope { const value: unknown = JSON.parse(json); if (!isCommandEnvelope(value)) throw new Error("invalid or unsupported command envelope"); return value; }
export function parseEventEnvelope(json: string): EventEnvelope { const value: unknown = JSON.parse(json); if (!isEventEnvelope(value)) throw new Error("invalid or unsupported event envelope"); return value; }

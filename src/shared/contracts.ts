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
export type VideoStatus = "not_checked" | "loading" | "ready" | "playing" | "paused" | "ended" | "unavailable" | "error";
export interface VideoState { track_id: TrackId | null; source_url: string | null; staged_path: string | null; status: VideoStatus; position_ms: number; fullscreen: boolean; generation: Generation }
export type DownloadKind = "single" | "csv" | "sync" | "url_queue";
export type JobState = "queued" | "running" | "cancelling" | "succeeded" | "failed" | "cancelled";
export interface DownloadJob { id: JobId; kind: DownloadKind; state: JobState; track_id: TrackId | null; progress: number | null; generation: Generation }
export type ErrorCode = "invalid_request" | "unsupported_version" | "not_found" | "busy" | "cancelled" | "unavailable" | "internal" | "shutting_down";
export interface ContractError { code: ErrorCode; message: string; retryable: boolean }

export type Command =
  | { type: "get_library" } | { type: "scan_library" } | { type: "play"; payload: { track_id: TrackId } }
  | { type: "pause" } | { type: "seek"; payload: { position_ms: number } }
  | { type: "video_source"; payload: { track_id: TrackId } } | { type: "video_stage"; payload: { track_id: TrackId } }
  | { type: "video_play" } | { type: "video_pause" } | { type: "video_seek"; payload: { position_ms: number } }
  | { type: "video_resize"; payload: { width: number; height: number } } | { type: "video_fullscreen"; payload: { fullscreen: boolean } }
  | { type: "cancel_job"; payload: { job_id: JobId } }
  | { type: "shutdown"; payload: { reason: ShutdownReason } };
export interface CommandEnvelope { version: ContractVersion; request_id: string; command: Command }
export type RootStatus = "scanned" | "missing" | "inaccessible";
export type Event =
  | { type: "library_snapshot"; payload: { snapshot: LibrarySnapshot } }
  | { type: "library_root_status"; payload: { root_index: number; status: RootStatus } }
  | { type: "playback_snapshot"; payload: { snapshot: PlaybackSnapshot } }
  | { type: "video_state"; payload: { state: VideoState } }
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
const nonEmptyId = (v: unknown): v is string => typeof v === "string" && v.trim().length > 0;
export function isContractVersion(v: unknown): v is ContractVersion { return v === CONTRACT_VERSION; }
function isCommand(v: unknown): v is Command {
  if (!object(v) || typeof v.type !== "string") return false;
  if (["get_library", "scan_library", "pause"].includes(v.type)) return Object.keys(v).length === 1;
   if (v.type === "play") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.track_id) && Object.keys(v.payload).length === 1;
   if (["video_play", "video_pause"].includes(v.type)) return Object.keys(v).length === 1;
   if (["seek", "video_seek"].includes(v.type)) return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.position_ms === "number" && Number.isSafeInteger(v.payload.position_ms) && v.payload.position_ms >= 0 && Object.keys(v.payload).length === 1;
   if (["video_source", "video_stage"].includes(v.type)) return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.track_id) && Object.keys(v.payload).length === 1;
   if (v.type === "video_resize") return exact(v, ["type", "payload"]) && object(v.payload) && Number.isSafeInteger(v.payload.width) && v.payload.width > 0 && Number.isSafeInteger(v.payload.height) && v.payload.height > 0 && Object.keys(v.payload).length === 2;
   if (v.type === "video_fullscreen") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.fullscreen === "boolean" && Object.keys(v.payload).length === 1;
   if (v.type === "cancel_job") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.job_id) && Object.keys(v.payload).length === 1;
  if (v.type === "shutdown") return exact(v, ["type", "payload"]) && object(v.payload) && ["user_requested", "system", "fatal_error"].includes(String(v.payload.reason)) && Object.keys(v.payload).length === 1;
  return false;
}
function isEvent(v: unknown): v is Event {
  if (!object(v) || typeof v.type !== "string" || !exact(v, ["type", "payload"]) || !object(v.payload)) return false;
   const id = nonEmptyId;
  const optionalString = (value: unknown): value is string | null => value === null || typeof value === "string";
  const generation = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
  const nonNegativeInt = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
  const provider = (value: unknown): value is ProviderId => exact(value, ["provider", "id"]) && id(value.provider) && id(value.id);
  const track = (value: unknown): value is Track => exact(value, ["id", "path", "title", "artist", "album", "duration_ms", "artwork_url", "favorite", "provider"]) && id(value.id) && optionalString(value.path) && typeof value.title === "string" && optionalString(value.artist) && optionalString(value.album) && (value.duration_ms === null || nonNegativeInt(value.duration_ms)) && optionalString(value.artwork_url) && typeof value.favorite === "boolean" && (value.provider === null || provider(value.provider));
  const playlist = (value: unknown): value is Playlist => exact(value, ["id", "name", "path", "url", "ordered_track_ids"]) && id(value.id) && typeof value.name === "string" && optionalString(value.path) && optionalString(value.url) && Array.isArray(value.ordered_track_ids) && value.ordered_track_ids.every(id);
  const library = (value: unknown): value is LibrarySnapshot => exact(value, ["version", "generation", "tracks", "playlists"]) && value.version === CONTRACT_VERSION && generation(value.generation) && Array.isArray(value.tracks) && value.tracks.every(track) && Array.isArray(value.playlists) && value.playlists.every(playlist);
   const playback = (value: unknown): value is PlaybackSnapshot => exact(value, ["queue", "current_index", "position_ms", "duration_ms", "status", "generation"]) && Array.isArray(value.queue) && value.queue.every(id) && (value.current_index === null || nonNegativeInt(value.current_index)) && nonNegativeInt(value.position_ms) && (value.duration_ms === null || nonNegativeInt(value.duration_ms)) && ["stopped", "playing", "paused", "failed"].includes(String(value.status)) && generation(value.generation);
   const video = (value: unknown): value is VideoState => exact(value, ["track_id", "source_url", "staged_path", "status", "position_ms", "fullscreen", "generation"]) && (value.track_id === null || id(value.track_id)) && optionalString(value.source_url) && optionalString(value.staged_path) && ["not_checked", "loading", "ready", "playing", "paused", "ended", "unavailable", "error"].includes(String(value.status)) && nonNegativeInt(value.position_ms) && typeof value.fullscreen === "boolean" && generation(value.generation);
  const progress = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 1;
  const job = (value: unknown): value is DownloadJob => exact(value, ["id", "kind", "state", "track_id", "progress", "generation"]) && id(value.id) && ["single", "csv", "sync", "url_queue"].includes(String(value.kind)) && ["queued", "running", "cancelling", "succeeded", "failed", "cancelled"].includes(String(value.state)) && (value.track_id === null || id(value.track_id)) && (value.progress === null || progress(value.progress)) && generation(value.generation);
  if (v.type === "track_started") return exact(v.payload, ["track_id"]) && id(v.payload.track_id);
  if (v.type === "queue_changed") return exact(v.payload, ["queue"]) && Array.isArray(v.payload.queue) && v.payload.queue.every(id);
  if (v.type === "library_root_status") return exact(v.payload, ["root_index", "status"]) && Number.isSafeInteger(v.payload.root_index) && v.payload.root_index >= 0 && ["scanned", "missing", "inaccessible"].includes(String(v.payload.status));
  if (v.type === "library_snapshot") return exact(v.payload, ["snapshot"]) && library(v.payload.snapshot);
   if (v.type === "playback_snapshot") return exact(v.payload, ["snapshot"]) && playback(v.payload.snapshot);
   if (v.type === "video_state") return exact(v.payload, ["state"]) && video(v.payload.state);
  if (v.type === "job_updated") return exact(v.payload, ["job"]) && job(v.payload.job);
  if (v.type === "error") {
    const error = v.payload.error;
     return exact(v.payload, ["error"]) && object(error) && exact(error, ["code", "message", "retryable"]) && ["invalid_request", "unsupported_version", "not_found", "busy", "cancelled", "unavailable", "internal", "shutting_down"].includes(String(error.code)) && typeof error.message === "string" && typeof error.retryable === "boolean";
  }
  if (v.type === "shutdown") {
    const shutdown = v.payload.envelope;
    return exact(v.payload, ["envelope"]) && object(shutdown) && exact(shutdown, ["version", "state", "reason"]) && shutdown.version === CONTRACT_VERSION && ["requested", "draining", "complete", "timed_out", "failed"].includes(String(shutdown.state)) && ["user_requested", "system", "fatal_error"].includes(String(shutdown.reason));
  }
  return false;
}
export function isCommandEnvelope(v: unknown): v is CommandEnvelope { return exact(v, ["version", "request_id", "command"]) && isContractVersion(v.version) && nonEmptyId(v.request_id) && isCommand(v.command); }
export function isDeliveryErrorEventId(value: unknown): value is string { return typeof value === "string" && /^delivery-error-[1-9]\d*$/.test(value) && Number.isSafeInteger(Number(value.slice(15))); }
export function isDomainEventId(value: unknown): value is string { return typeof value === "string" && /^event-[1-9]\d*$/.test(value) && Number.isSafeInteger(Number(value.slice(6))); }
export function isEventEnvelope(v: unknown): v is EventEnvelope { return exact(v, ["version", "event_id", "generation", "job_id", "session_id", "event"]) && isContractVersion(v.version) && (isDomainEventId(v.event_id) || isDeliveryErrorEventId(v.event_id)) && Number.isSafeInteger(v.generation) && v.generation >= 0 && (v.job_id === null || nonEmptyId(v.job_id)) && (v.session_id === null || nonEmptyId(v.session_id)) && isEvent(v.event); }
export function parseCommandEnvelope(json: string): CommandEnvelope { const value: unknown = JSON.parse(json); if (!isCommandEnvelope(value)) throw new Error("invalid or unsupported command envelope"); return value; }
export function parseEventEnvelope(json: string): EventEnvelope { const value: unknown = JSON.parse(json); if (!isEventEnvelope(value)) throw new Error("invalid or unsupported event envelope"); return value; }

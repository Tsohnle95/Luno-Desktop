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
  type JobId, type PlaylistId, type SettingsInput,
  parseEventEnvelope,
  isSearchResults,
  isRecommendationList,
  isPreviewSnapshot,
  type PreviewSeed,
  type Recommendation,
} from "../shared/contracts";
import type { LibrarySnapshot } from "../shared/contracts";

export type MigrationStatus =
  | { status: "not_run" }
  | { status: "skipped"; reason: string }
  | { status: "completed"; warnings: number; snapshot_created: boolean; removed_previews: number }
  | { status: "failed"; reason: string };

export const BACKEND_EVENT = "backend-event";
export const BACKEND_COMMAND = "backend_command";
export const GATE5_EVENT = "gate5-smoke-event";

export function backendCommandArgs(envelope: unknown): InvokeArgs {
  return { payload: envelope };
}

export function isValidLibraryResponse(value: unknown): value is LibrarySnapshot {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const snapshot = value as Record<string, unknown>;
  return isEventEnvelope({
    version: CONTRACT_VERSION,
    event_id: "event-1",
    generation: snapshot.generation,
    job_id: null,
    session_id: null,
    event: { type: "library_snapshot", payload: { snapshot: value } },
  });
}

export interface BridgeTransport {
  invoke<T>(command: string, payload: unknown): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn>;
}

export const tauriTransport: BridgeTransport = {
  invoke: <T>(command: string, payload: unknown) => command === BACKEND_COMMAND
    ? tauriInvoke<T>(command, backendCommandArgs(payload))
    : payload == null ? tauriInvoke<T>(command) : tauriInvoke<T>(command, payload as InvokeArgs),
  listen: <T>(event: string, handler: (payload: T) => void) => listen<T>(event, (message) => handler(message.payload)),
};

let requestSequence = 0;
type UnitCommand = "get_library" | "scan_library" | "pause" | "video_play" | "video_pause" | "get_recent" | "clear_recent" | "next" | "previous" | "clear_queue" | "toggle_shuffle";

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
    loadConfig: (home: string) => send({ type: "load_config", payload: { home } }),
    registerRoot: (root: string) => send({ type: "register_root", payload: { root } }),
    promoteAudioOutput: (temporary: string, destination: string) => send({ type: "promote_audio_output", payload: { temporary, destination } }),
     syncMarkers: (folder: string) => send({ type: "sync_markers", payload: { folder } }),
      searchDownloader: async (query: string, try_soundcloud = true) => { const value = await send<unknown>({ type: "search_downloader", payload: { query, try_soundcloud } }); if (!isSearchResults(value)) throw new Error("invalid downloader search response"); return value; },
      discover: async (artist: string, title: string, limit = 5) => { const value = await send<unknown>({ type: "discover", payload: { artist, title, limit } }); if (!isRecommendationList(value)) throw new Error("invalid discovery response"); return value; },
      previewTap: async (seeds: PreviewSeed[], key: string) => { const value = await send<unknown>({ type: "preview_tap", payload: { seeds, key } }); if (!isPreviewSnapshot(value)) throw new Error("invalid preview session response"); return value; },
      previewNext: async () => { const value = await send<unknown>({ type: "preview_next" }); if (!isPreviewSnapshot(value)) throw new Error("invalid preview session response"); return value; },
      previewPrevious: async () => { const value = await send<unknown>({ type: "preview_previous" }); if (!isPreviewSnapshot(value)) throw new Error("invalid preview session response"); return value; },
      previewCancel: async () => { const value = await send<unknown>({ type: "preview_cancel" }); if (!isPreviewSnapshot(value)) throw new Error("invalid preview session response"); return value; },
      saveRecommendation: (key: TrackId, playlist_id: PlaylistId) => send({ type: "save", payload: { key, playlist_id } }),
      validateDirectUrl: (url: string, destination: string) => send({ type: "validate_direct_url", payload: { url, destination } }),
      previewCsv: (csv: string) => send({ type: "preview_csv", payload: { csv } }),
      queueUrls: (urls: string[]) => send({ type: "queue_urls", payload: { urls } }),
      selectLibraryRoot: (root: string) => send({ type: "select_library_root", payload: { root } }),
     startDownload: (job_id: JobId, destination: string, helper: string, source = "") => send({ type: "start_download", payload: { job_id, destination, helper, source } }),
     bundleHelper: () => send({ type: "bundle_helper_path" }),
     retryDownload: (job_id: JobId) => send({ type: "retry_download", payload: { job_id } }),
     loadSettings: (home: string) => send({ type: "load_settings", payload: { home } }),
     resolveRuntimeHome: () => send({ type: "resolve_runtime_home" }),
     saveSettings: (home: string, settings: SettingsInput) => send({ type: "save_settings", payload: { home, settings } }),
     transferPreview: (manifest: string, destination: string) => send({ type: "transfer_preview", payload: { manifest, destination } }),
     transferExport: () => send({ type: "transfer_export" }),
     transferApply: (manifest: string, destination: string, approvals: Record<string, string>) => send({ type: "transfer_apply", payload: { manifest, destination, approvals } }),
    toggleFavorite: (track_id: TrackId) => send({ type: "toggle_favorite", payload: { track_id } }),
    recordRecent: (track_id: TrackId) => send({ type: "record_recent", payload: { track_id } }),
    getRecent: () => send("get_recent"),
    clearRecent: () => send("clear_recent"),
    play: (track_id: string) => send({ type: "play", payload: { track_id } }),
    playWithContext: (track_id: TrackId, context: TrackId[]) => send({ type: "play_with_context", payload: { track_id, context } }),
    next: () => send("next"),
    previous: () => send("previous"),
    playNext: (track_ids: TrackId[]) => send({ type: "play_next", payload: { track_ids } }),
    addToQueue: (track_ids: TrackId[]) => send({ type: "add_to_queue", payload: { track_ids } }),
    removeFromQueue: (index: number) => send({ type: "remove_from_queue", payload: { index } }),
    reorderQueue: (from: number, to: number) => send({ type: "reorder_queue", payload: { from, to } }),
    clearQueue: () => send("clear_queue"),
    toggleShuffle: () => send("toggle_shuffle"),
    setRepeat: (repeat: "off" | "one" | "all") => send({ type: "set_repeat", payload: { repeat } }),
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
     gate5Smoke: (root: string, preview_root: string, video_root: string) => transport.invoke<Gate5SmokeResult>("gate5_smoke", { root, preview_root, video_root }),
     subscribeGate5Smoke: (handler: (result: Gate5SmokeResult) => void) => transport.listen<Gate5SmokeResult>(GATE5_EVENT, handler),
   subscribe: (handler: (event: EventEnvelope) => void) =>
      transport.listen<unknown>(BACKEND_EVENT, (payload) => handler(validEvent(payload))),
  };
}

export interface Gate5SmokeResult {
  discovery_items: number;
  discovery_artwork: boolean;
  artwork_design: number;
  preview_ready: boolean;
  preview_hidden_before_save: boolean;
  saved_visible_after_scan: boolean;
  video_ready: boolean;
  unobserved: string[];
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

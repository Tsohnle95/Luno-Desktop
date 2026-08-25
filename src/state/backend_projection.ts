import { isDeliveryErrorEventId } from "../shared/contracts";
import type { EventEnvelope, LibrarySnapshot, PlaybackSnapshot, DownloadJob, ContractError, ShutdownEnvelope, TrackId, VideoState, Track, PreviewSnapshot, SaveState } from "../shared/contracts";
import type { Bridge } from "../app/bridge";

export interface BackendProjection {
  library: LibrarySnapshot | null;
  playback: PlaybackSnapshot | null;
  preview: PreviewSnapshot | null;
  video: VideoState | null;
  jobs: Record<string, DownloadJob>;
  saved: Record<string, SaveState>;
  errors: ContractError[];
  recentHistory: TrackId[];
  shutdown: ShutdownEnvelope | null;
  generations: { library: number; playback: number; preview: number; video: number; jobs: number; errors: number };
  lastEventIds: Partial<Record<"library" | "playback" | "preview" | "video" | "jobs" | "errors" | "control", string>>;
}

export const initialBackendProjection = (): BackendProjection => ({
  library: null, playback: null, preview: null, video: null, jobs: {}, saved: {}, errors: [], recentHistory: [], shutdown: null,
  generations: { library: -1, playback: -1, preview: -1, video: -1, jobs: -1, errors: -1 }, lastEventIds: {},
});

type Domain = keyof BackendProjection["generations"];
function domainOf(type: EventEnvelope["event"]["type"]): Domain | "control" {
  if (type === "library_snapshot" || type === "library_root_status") return "library";
  if (type === "playback_snapshot" || type === "track_started" || type === "queue_changed") return "playback";
  if (type === "preview_snapshot") return "preview";
  if (type === "video_state") return "video";
  if (type === "job_updated") return "jobs";
  if (type === "error") return "errors";
  return "control";
}

export function reduceBackendProjection(state: BackendProjection, envelope: EventEnvelope): BackendProjection {
  if (isDeliveryErrorEventId(envelope.event_id)) {
    if (envelope.event.type !== "error") return state;
    return { ...state, errors: [...state.errors, envelope.event.payload.error] };
  }
  const domain = domainOf(envelope.event.type);
  const payloadGeneration = domain === "library" && envelope.event.type === "library_snapshot" ? envelope.event.payload.snapshot.generation
    : domain === "playback" && envelope.event.type === "playback_snapshot" ? envelope.event.payload.snapshot.generation
    : domain === "preview" && envelope.event.type === "preview_snapshot" ? envelope.event.payload.snapshot.generation
    : domain === "video" && envelope.event.type === "video_state" ? envelope.event.payload.state.generation
    : domain === "jobs" && envelope.event.type === "job_updated" ? envelope.event.payload.job.generation
    : null;
  if (payloadGeneration !== null && payloadGeneration !== envelope.generation) return state;
  const lastEventId = state.lastEventIds[domain];
  if (lastEventId !== undefined && compareEventIds(envelope.event_id, lastEventId) <= 0) return state;
  // Generation 0 is reserved for shutdown control events. They are lifecycle
  // notifications, not work results, and must survive stale-work filtering.
  if (domain !== "control" && envelope.generation < state.generations[domain]) return state;
  const next = { ...state, generations: domain === "control" ? state.generations : { ...state.generations, [domain]: Math.max(state.generations[domain], envelope.generation) }, lastEventIds: { ...state.lastEventIds, [domain]: envelope.event_id } };
  const event = envelope.event;
  switch (event.type) {
    case "library_snapshot": return { ...next, library: event.payload.snapshot };
    case "library_root_status": return next;
    case "playback_snapshot": return { ...next, playback: event.payload.snapshot };
    case "preview_snapshot": return { ...next, preview: event.payload.snapshot };
    case "save_state": return { ...next, saved: { ...next.saved, [event.payload.state.key]: event.payload.state } };
    case "video_state": return { ...next, video: event.payload.state };
    case "track_started": {
      const playback = next.playback;
      const recentHistory = [event.payload.track_id, ...next.recentHistory.filter((id) => id !== event.payload.track_id)].slice(0, 100);
      if (!playback) return { ...next, recentHistory };
      const index = playback.queue.indexOf(event.payload.track_id as TrackId);
      return { ...next, recentHistory, playback: { ...playback, current_index: index < 0 ? playback.current_index : index } };
    }
    case "queue_changed": return next.playback ? { ...next, playback: { ...next.playback, queue: event.payload.queue } } : next;
    case "job_updated": return { ...next, jobs: { ...next.jobs, [event.payload.job.id]: event.payload.job } };
    case "error": return { ...next, errors: [...next.errors, event.payload.error] };
    case "shutdown": return { ...next, shutdown: event.payload.envelope };
  }
  throw new Error("unhandled backend event");
}

export function currentTrack(projection: Pick<BackendProjection, "library" | "playback">): Track | null {
  const playback = projection.playback;
  if (!playback || playback.current_index === null || !Number.isSafeInteger(playback.current_index) || playback.current_index < 0) return null;
  const id = playback.queue[playback.current_index];
  return id ? projection.library?.tracks.find((track) => track.id === id) ?? null : null;
}

function compareEventIds(left: string, right: string): number {
  const numeric = (value: string) => /^(?:event-)?\d+$/.test(value) ? BigInt(value.replace(/^event-/, "")) : null;
  const a = numeric(left);
  const b = numeric(right);
  if (a !== null && b !== null) return a < b ? -1 : a > b ? 1 : 0;
  return left < right ? -1 : left > right ? 1 : 0;
}

export function createBackendProjection() {
  let state = initialBackendProjection();
  const listeners = new Set<() => void>();
  return {
    getSnapshot: () => state,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => listeners.delete(listener); },
    apply: (event: EventEnvelope) => { const next = reduceBackendProjection(state, event); if (next !== state) { state = next; listeners.forEach((listener) => listener()); } },
    applyVideoSnapshot: (video: VideoState) => { const next = mergeVideoSnapshot(state, video); if (next !== state) { state = next; listeners.forEach((listener) => listener()); } },
    reset: () => { state = initialBackendProjection(); listeners.forEach((listener) => listener()); },
  };
}

/// Pull-based reconciliation for video state. Video reaches the UI almost
/// entirely through PUSHED `video_state` events; if one is lost (event burst,
/// listener attach timing), the song page sat on "Loading video preview..."
/// forever because nothing ever re-checked. The video_stage command RESPONSE
/// carries the same authoritative state — this merge lets the UI absorb it.
/// Event-id/generation sequencing is intentionally untouched: snapshots fill
/// gaps, they do not participate in ordering.
export function mergeVideoSnapshot(state: BackendProjection, incoming: VideoState): BackendProjection {
  const current = state.video;
  if (current && current.track_id === incoming.track_id) {
    // Never walk a live playback session backwards to a pre-play state.
    if ((current.status === "playing" || current.status === "paused") && (incoming.status === "ready" || incoming.status === "loading")) return state;
    const unchanged = current.status === incoming.status
      && current.staged_path === incoming.staged_path
      && current.source_url === incoming.source_url
      && current.position_ms === incoming.position_ms
      && current.download_progress === incoming.download_progress;
    if (unchanged) return state;
  }
  return { ...state, video: incoming };
}

export function subscribeBackendProjection(bridge: Pick<Bridge, "subscribe">, projection: ReturnType<typeof createBackendProjection>) {
  return bridge.subscribe((event) => projection.apply(event));
}

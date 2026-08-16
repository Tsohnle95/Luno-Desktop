import { isDeliveryErrorEventId } from "../shared/contracts";
import type { EventEnvelope, LibrarySnapshot, PlaybackSnapshot, DownloadJob, ContractError, ShutdownEnvelope, TrackId, VideoState, Track } from "../shared/contracts";
import type { Bridge } from "../app/bridge";

export interface BackendProjection {
  library: LibrarySnapshot | null;
  playback: PlaybackSnapshot | null;
  video: VideoState | null;
  jobs: Record<string, DownloadJob>;
  errors: ContractError[];
  recentHistory: TrackId[];
  shutdown: ShutdownEnvelope | null;
  generations: { library: number; playback: number; video: number; jobs: number; errors: number };
  lastEventIds: Partial<Record<"library" | "playback" | "video" | "jobs" | "errors" | "control", string>>;
}

export const initialBackendProjection = (): BackendProjection => ({
  library: null, playback: null, video: null, jobs: {}, errors: [], recentHistory: [], shutdown: null,
  generations: { library: -1, playback: -1, video: -1, jobs: -1, errors: -1 }, lastEventIds: {},
});

type Domain = keyof BackendProjection["generations"];
function domainOf(type: EventEnvelope["event"]["type"]): Domain | "control" {
  if (type === "library_snapshot" || type === "library_root_status") return "library";
  if (type === "playback_snapshot" || type === "track_started" || type === "queue_changed") return "playback";
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
    reset: () => { state = initialBackendProjection(); listeners.forEach((listener) => listener()); },
  };
}

export function subscribeBackendProjection(bridge: Pick<Bridge, "subscribe">, projection: ReturnType<typeof createBackendProjection>) {
  return bridge.subscribe((event) => projection.apply(event));
}

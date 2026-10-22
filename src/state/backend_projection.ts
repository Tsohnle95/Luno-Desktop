import { isDeliveryErrorEventId } from "../shared/contracts";
import type { EventEnvelope, LibrarySnapshot, PlaybackSnapshot, DownloadJob, ContractError, ShutdownEnvelope, TrackId, VideoState, Track, PreviewSnapshot, SaveState, DiscoverSnapshot, PlayStatisticsSnapshot } from "../shared/contracts";
import type { Bridge } from "../app/bridge";

export interface BackendProjection {
  library: LibrarySnapshot | null;
  playback: PlaybackSnapshot | null;
  preview: PreviewSnapshot | null;
  discover: DiscoverSnapshot | null;
  playStatistics?: PlayStatisticsSnapshot | null;
  video: VideoState | null;
  jobs: Record<string, DownloadJob>;
  jobEventIds?: Record<string, string>;
  jobErrors?: Record<string, string>;
  jobErrorEvents?: Record<string, { generation: number; eventId: string }>;
  saved: Record<string, SaveState>;
  errors: ContractError[];
  recentHistory: TrackId[];
  shutdown: ShutdownEnvelope | null;
  generations: { library: number; playback: number; preview: number; discover: number; play_statistics?: number; video: number; jobs: number; errors: number };
  lastEventIds: Partial<Record<"library" | "playback" | "preview" | "discover" | "play_statistics" | "video" | "jobs" | "errors" | "control", string>>;
}

export const initialBackendProjection = (): BackendProjection => ({
  library: null, playback: null, preview: null, discover: null, playStatistics: null, video: null, jobs: {}, saved: {}, errors: [], recentHistory: [], shutdown: null,
  generations: { library: -1, playback: -1, preview: -1, discover: -1, play_statistics: -1, video: -1, jobs: -1, errors: -1 }, lastEventIds: {},
});

/** Bound the error log so a retrying pipeline cannot grow it without limit. */
const MAX_PROJECTED_ERRORS = 50;

type Domain = keyof BackendProjection["generations"];
function domainOf(type: EventEnvelope["event"]["type"]): Domain | "control" {
  if (type === "library_snapshot" || type === "library_root_status" || type === "track_favorite_changed") return "library";
  if (type === "playback_snapshot" || type === "track_started" || type === "queue_changed") return "playback";
  if (type === "preview_snapshot") return "preview";
  if (type === "discover_snapshot") return "discover";
  if (type === "play_statistics_snapshot") return "play_statistics";
  if (type === "video_state") return "video";
  if (type === "job_updated") return "jobs";
  if (type === "error") return "errors";
  return "control";
}

export function reduceBackendProjection(state: BackendProjection, envelope: EventEnvelope): BackendProjection {
  if (isDeliveryErrorEventId(envelope.event_id)) {
    if (envelope.event.type !== "error") return state;
    return { ...state, errors: [...state.errors, envelope.event.payload.error].slice(-MAX_PROJECTED_ERRORS) };
  }
  if (envelope.event.type === "error" && envelope.job_id) {
    const id = envelope.job_id;
    const previous = state.jobErrorEvents?.[id];
    if (envelope.generation < (state.jobs[id]?.generation ?? -1)
      || (previous && (envelope.generation < previous.generation || compareEventIds(envelope.event_id, previous.eventId) <= 0))) return state;
    return { ...state, errors: state.jobs[id]?.kind === "sync" ? [...state.errors, envelope.event.payload.error].slice(-MAX_PROJECTED_ERRORS) : state.errors,
      jobErrors: { ...state.jobErrors, [id]: envelope.event.payload.error.message },
      jobErrorEvents: { ...state.jobErrorEvents, [id]: { generation: envelope.generation, eventId: envelope.event_id } } };
  }
  const domain = domainOf(envelope.event.type);
  const payloadGeneration = domain === "library" && envelope.event.type === "library_snapshot" ? envelope.event.payload.snapshot.generation
    : domain === "playback" && envelope.event.type === "playback_snapshot" ? envelope.event.payload.snapshot.generation
    : domain === "preview" && envelope.event.type === "preview_snapshot" ? envelope.event.payload.snapshot.generation
    : domain === "discover" && envelope.event.type === "discover_snapshot" ? envelope.event.payload.snapshot.generation
    : domain === "play_statistics" && envelope.event.type === "play_statistics_snapshot" ? envelope.event.payload.snapshot.generation
    : domain === "video" && envelope.event.type === "video_state" ? envelope.event.payload.state.generation
    : domain === "jobs" && envelope.event.type === "job_updated" ? envelope.event.payload.job.generation
    : null;
  if (payloadGeneration !== null && payloadGeneration !== envelope.generation) return state;
  if (envelope.event.type === "job_updated") {
    const job = envelope.event.payload.job;
    const previous = state.jobs[job.id];
    const previousEventId = state.jobEventIds?.[job.id];
    // A newer concurrent job cannot supersede another job's progress or
    // cancellation. Retry generations and event ordering belong to each ID.
    if (previous && job.generation < previous.generation) return state;
    if (previousEventId !== undefined && compareEventIds(envelope.event_id, previousEventId) <= 0) return state;
    const jobErrors = { ...state.jobErrors };
    if (["running", "queued"].includes(job.state) && job.generation > (state.jobErrorEvents?.[job.id]?.generation ?? -1)) delete jobErrors[job.id];
    return { ...state, jobErrors, jobs: { ...state.jobs, [job.id]: job },
      jobEventIds: { ...state.jobEventIds, [job.id]: envelope.event_id },
      generations: { ...state.generations, jobs: Math.max(state.generations.jobs, envelope.generation) } };
  }
  const lastEventId = state.lastEventIds[domain];
  if (lastEventId !== undefined && compareEventIds(envelope.event_id, lastEventId) <= 0) return state;
  // Generation 0 is reserved for shutdown control events. They are lifecycle
  // notifications, not work results, and must survive stale-work filtering.
  const currentGeneration = domain === "control" ? -1 : state.generations[domain] ?? -1;
  if (domain !== "control" && envelope.generation < currentGeneration) return state;
  // An early favorite event cannot stand in for the initial full library
  // snapshot; let that snapshot establish the library generation and rows.
  if (envelope.event.type === "track_favorite_changed" && !state.library) return state;
  const next = { ...state, generations: domain === "control" ? state.generations : { ...state.generations, [domain]: Math.max(currentGeneration, envelope.generation) }, lastEventIds: { ...state.lastEventIds, [domain]: envelope.event_id } };
  const event = envelope.event;
  switch (event.type) {
    case "library_snapshot": return { ...next, library: event.payload.snapshot };
    case "library_root_status": return next;
    case "track_favorite_changed": {
      if (!next.library) return state;
      const index = next.library.tracks.findIndex((track) => track.id === event.payload.track_id);
      if (index < 0 || next.library.tracks[index].favorite === event.payload.favorite) return next;
      const tracks = next.library.tracks.slice();
      tracks[index] = { ...tracks[index], favorite: event.payload.favorite };
      return { ...next, library: { ...next.library, tracks } };
    }
    case "playback_snapshot": return { ...next, playback: event.payload.snapshot };
    case "preview_snapshot": return { ...next, preview: event.payload.snapshot };
    case "discover_snapshot": return { ...next, discover: event.payload.snapshot };
    case "play_statistics_snapshot": return { ...next, playStatistics: event.payload.snapshot };
    case "save_state": return { ...next, saved: { ...next.saved, [event.payload.state.key]: event.payload.state } };
    case "video_state": return { ...next, video: event.payload.state };
    case "track_started": {
      // `track_started` rides the SAME generation as its authoritative
      // playback_snapshot, which already carries the true `current_index`.
      // Re-deriving it here with `queue.indexOf` regressed to the FIRST
      // occurrence of duplicate ids and ignored queue entries the frontend had
      // not resolved, so the queue panel's Now playing / Next up disagreed with
      // what the backend would actually play. Trust the snapshot.
      const recentHistory = [event.payload.track_id, ...next.recentHistory.filter((id) => id !== event.payload.track_id)].slice(0, 100);
      return { ...next, recentHistory };
    }
    case "queue_changed": return next.playback ? { ...next, playback: { ...next.playback, queue: event.payload.queue } } : next;
    case "error": return { ...next, errors: [...next.errors, event.payload.error].slice(-MAX_PROJECTED_ERRORS) };
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
    clearErrors: () => { if (state.errors.length === 0) return; state = { ...state, errors: [] }; listeners.forEach((listener) => listener()); },
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
      && current.download_progress === incoming.download_progress
      // A revision bump is the ONLY change some publications carry (the
      // background quality upgrade replaces the file in place) — it must
      // never be folded into "unchanged".
      && (current.revision ?? null) === (incoming.revision ?? null);
    if (unchanged) return state;
  }
  return { ...state, video: incoming };
}

export function subscribeBackendProjection(bridge: Pick<Bridge, "subscribe">, projection: ReturnType<typeof createBackendProjection>) {
  return bridge.subscribe((event) => projection.apply(event));
}

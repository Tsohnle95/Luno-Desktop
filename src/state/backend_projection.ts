import type { EventEnvelope, LibrarySnapshot, PlaybackSnapshot, DownloadJob, ContractError, ShutdownEnvelope, TrackId } from "../shared/contracts";
import type { Bridge } from "../app/bridge";

export interface BackendProjection {
  library: LibrarySnapshot | null;
  playback: PlaybackSnapshot | null;
  jobs: Record<string, DownloadJob>;
  errors: ContractError[];
  shutdown: ShutdownEnvelope | null;
  generation: number;
  lastEventId: string | null;
}

export const initialBackendProjection = (): BackendProjection => ({
  library: null, playback: null, jobs: {}, errors: [], shutdown: null, generation: -1, lastEventId: null,
});

export function reduceBackendProjection(state: BackendProjection, envelope: EventEnvelope): BackendProjection {
  // Generation 0 is reserved for shutdown control events. They are lifecycle
  // notifications, not work results, and must survive stale-work filtering.
  if (envelope.event.type !== "shutdown" && envelope.generation < state.generation) return state;
  const next = { ...state, generation: Math.max(state.generation, envelope.generation), lastEventId: envelope.event_id };
  switch (envelope.event.type) {
    case "library_snapshot": return { ...next, library: envelope.event.payload.snapshot };
    case "playback_snapshot": return { ...next, playback: envelope.event.payload.snapshot };
    case "track_started": {
      const playback = next.playback;
      if (!playback) return next;
      const index = playback.queue.indexOf(envelope.event.payload.track_id as TrackId);
      return { ...next, playback: { ...playback, current_index: index < 0 ? playback.current_index : index } };
    }
    case "queue_changed": return next.playback ? { ...next, playback: { ...next.playback, queue: envelope.event.payload.queue } } : next;
    case "job_updated": return { ...next, jobs: { ...next.jobs, [envelope.event.payload.job.id]: envelope.event.payload.job } };
    case "error": return { ...next, errors: [...next.errors, envelope.event.payload.error] };
    case "shutdown": return { ...next, shutdown: envelope.event.payload.envelope };
  }
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

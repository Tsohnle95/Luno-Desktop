export type TrackDurationFetcher = (trackId: string) => Promise<number | null>;

const CAP = 50_000;
const resolved = new Map<string, number | null>();
const inFlight = new Map<string, Promise<number | null>>();

function remember(trackId: string, durationMs: number | null): void {
  resolved.delete(trackId);
  resolved.set(trackId, durationMs);
  while (resolved.size > CAP) {
    const oldest = resolved.keys().next().value;
    if (oldest === undefined) break;
    resolved.delete(oldest);
  }
}

/** A resolved result is independent of the LibrarySnapshot generation. */
export function cachedTrackDuration(trackId: string): { found: true; value: number | null } | null {
  if (!resolved.has(trackId)) return null;
  const value = resolved.get(trackId)!;
  resolved.delete(trackId);
  resolved.set(trackId, value);
  return { found: true, value };
}

/** Coalesce probes across mounted views and retain results across route changes. */
export function requestTrackDuration(trackId: string, fetcher: TrackDurationFetcher): Promise<number | null> {
  const cached = cachedTrackDuration(trackId);
  if (cached) return Promise.resolve(cached.value);
  const active = inFlight.get(trackId);
  if (active) return active;
  const request = Promise.resolve()
    .then(() => fetcher(trackId))
    .then((durationMs) => {
      remember(trackId, durationMs);
      return durationMs;
    })
    .catch(() => {
      remember(trackId, null);
      return null;
    })
    .finally(() => inFlight.delete(trackId));
  inFlight.set(trackId, request);
  return request;
}

export function clearTrackDurationCacheForTests(): void {
  resolved.clear();
  inFlight.clear();
}

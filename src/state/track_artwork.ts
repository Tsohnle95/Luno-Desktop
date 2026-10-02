import { useEffect, useRef, useState } from "react";

export type ArtworkFetcher = (trackId: string) => Promise<string | null>;

/**
 * Lazily fetch embedded album art for the currently visible rows. Results are
 * kept in component state plus a module-level cache, so scrolling back to a
 * row never re-extracts artwork and tracks without artwork are not retried.
 * The fetcher is read from a ref, so a new inline closure per render does not
 * re-run the effect.
 *
 * The module cache is BOUNDED by both entry count (CAP) and total payload
 * bytes (MAX_CACHE_BYTES): every value is a full data-URL string, and
 * libraries reach thousands of rows. An unbounded Map here grew past
 * WebKit's memory limits on long scroll sessions and killed the web content
 * process (black window). Oversized payloads are served once without caching.
 *
 * Eviction is LAZY by design: when the LRU drops a data URL, components only
 * forget the id in their per-instance "requested" set. Nothing re-requests
 * until the id becomes visible again AND the effect re-runs (scroll, nav, or
 * a new id list). An earlier version re-requested evicted ids immediately,
 * which looped forever once the mounted working set exceeded the cap — every
 * fetch evicted another visible id, which instantly re-fetched, churning
 * gigabytes through the JS heap in the background (2026-09-09). Queued jobs
 * hold view-scoped demand too: leaving the viewport cancels queued work when
 * no other current view still needs that id.
 */
const CAP = 240;
const MAX_CACHE_BYTES = 24 * 1024 * 1024;
const MAX_CACHED_BYTES = 1_000_000;
const MAX_CONCURRENT = 6;
const MAX_BACKGROUND_CONCURRENT = 2;
const resolvedCache = new Map<string, { url: string | null; bytes: number }>();
const inFlight = new Map<string, Promise<string | null>>();
let activeCount = 0;
let activeBackgroundCount = 0;
let cachedBytes = 0;
let nextQueueOrder = 0;
const pendingQueue = new Map<string, { id: string; fetcher: ArtworkFetcher; priority: number; order: number }>();
const demandOwners = new Map<symbol, { ids: Set<string>; remaining: Set<string>; fetcher: ArtworkFetcher; priority: number; requested: Set<string>; onComplete?: () => void }>();
const requestedOwners = new Map<string, Set<Set<string>>>();
const artworkRevision = new Map<string, number>();

/**
 * Components subscribe to cache resolutions. Without this, only the fetches
 * already in flight when an effect ran were wired back into component state —
 * every later-resolving fetch (queued behind MAX_CONCURRENT) landed in the
 * module cache and stayed invisible, because a STATIC id list (sidebar
 * playlist cards, the queue window, Home cards) never re-runs its effect to
 * read the cache back. Playlist art beyond the first ~6 entries disappeared.
 *
 * A second event — eviction — lets components forget the per-instance
 * "requested" marker for ids the bounded module cache dropped. Without it,
 * scrolling a large library evicts older rows' data URLs (CAP is 240), and
 * the requested set then blocks re-requesting those rows forever: artwork
 * that was visible moments earlier stayed blank for the whole session
 * (single-song-page art survived because its one-id request is prioritized
 * fresh on every navigation).
 */
type Listener = (event: { type: "resolved"; urls: Record<string, string | null> } | { type: "evicted"; id: string }) => void;
const listeners = new Set<Listener>();

function publishResolved(id: string, url: string | null): void {
  const urls = { [id]: url };
  for (const listener of listeners) listener({ type: "resolved", urls });
}

function settleDemand(id: string): void {
  for (const demand of demandOwners.values()) {
    if (!demand.remaining.delete(id) || demand.remaining.size !== 0) continue;
    const onComplete = demand.onComplete;
    demand.onComplete = undefined;
    onComplete?.();
  }
}

function remember(id: string, url: string | null): void {
  // Delete-then-set keeps insertion order reflecting recency for LRU eviction.
  const previous = resolvedCache.get(id);
  if (previous) cachedBytes -= previous.bytes;
  const bytes = url ? url.length : 0;
  resolvedCache.delete(id);
  resolvedCache.set(id, { url, bytes });
  cachedBytes += bytes;
  settleDemand(id);
  publishResolved(id, url);
  requestedOwners.delete(id);
  while (resolvedCache.size > CAP || cachedBytes > MAX_CACHE_BYTES) {
    const oldest = resolvedCache.keys().next().value;
    if (oldest === undefined) break;
    // Only evicting a VALUE matters — a cached null means "no artwork,
    // never retry", and re-requesting it is exactly what the no-art
    // fast-path exists to prevent. Evicting a data URL means a later
    // scroll-back must be allowed to re-extract it, so forget the id in
    // every component's requested set. The component re-requests LAZILY on
    // its next visible pass — never here, or the eviction itself becomes a
    // fetch that evicts the next id (fetch-evict-refetch loop, 2026-09-09).
    const evicted = resolvedCache.get(oldest);
    const wasValue = evicted?.url !== null;
    cachedBytes -= evicted?.bytes ?? 0;
    resolvedCache.delete(oldest);
    if (wasValue) {
      for (const listener of listeners) listener({ type: "evicted", id: oldest });
    }
  }
}

function cached(id: string): string | null | undefined {
  const entry = resolvedCache.get(id);
  if (!entry) return undefined;
  resolvedCache.delete(id);
  resolvedCache.set(id, entry);
  return entry.url;
}

/** Cap-trim a component-state map to the most-recent CAP entries. */
function capTrimmed(next: Record<string, string>): Record<string, string> {
  const keys = Object.keys(next);
  if (keys.length <= CAP * 2) return next;
  const trimmed: Record<string, string> = {};
  for (const k of keys.slice(-CAP)) trimmed[k] = next[k];
  return trimmed;
}

/**
 * Whether a visible id still needs a fetch request on this pass.
 * Exported for deterministic tests of the eviction-retry contract.
 *
 * The per-component `requested` set is the "ever asked in this mount" gate.
 * It must be cleared when the module LRU cache evicts the id's entry —
 * otherwise a scroll-back after eviction would skip the id forever. The
 * module emits an `evicted` event exactly for that purpose; components
 * delete the id from their set and the next pass sees `requested === false`
 * and re-enqueues.
 */
export function needsArtworkRequest(
  requested: ReadonlySet<string>,
  resolved: ReadonlyMap<string, { url: string | null }>,
  inFlightIds: ReadonlySet<string>,
  id: string,
): boolean {
  if (resolved.has(id)) return false;      // known (value or definitive no-art)
  if (inFlightIds.has(id)) return false;   // already fetching
  if (requested.has(id)) return false;     // already asked this mount
  return true;
}

function scheduleNext(): void {
  while (activeCount < MAX_CONCURRENT && pendingQueue.size > 0) {
    let job: { id: string; fetcher: ArtworkFetcher; priority: number; order: number } | undefined;
    for (const queued of pendingQueue.values()) {
      if (!job || queued.priority > job.priority || (queued.priority === job.priority && queued.order < job.order)) job = queued;
    }
    if (!job) break;
    const background = job.priority < 1;
    if (background && activeBackgroundCount >= MAX_BACKGROUND_CONCURRENT) break;
    pendingQueue.delete(job.id);
    const { id, fetcher } = job;
    if (resolvedCache.has(id) || inFlight.has(id)) continue;
    const revision = artworkRevision.get(id) ?? 0;
    activeCount += 1;
    if (background) activeBackgroundCount += 1;
    const promise = Promise.resolve().then(() => fetcher(id)).finally(() => {
      activeCount -= 1;
      if (background) activeBackgroundCount -= 1;
      inFlight.delete(id);
      // Defer next tick to avoid deep recursion and to let React batch.
      queueMicrotask(scheduleNext);
    });
    inFlight.set(id, promise);
    promise.then((url) => {
      if ((artworkRevision.get(id) ?? 0) !== revision) return url;
      if (url === null || url.length <= MAX_CACHED_BYTES) remember(id, url);
      else {
        // Large results may be shown to current demand owners without being
        // retained in either bounded cache. Clear stale owners so a view that
        // was offscreen when this completed can request it when it returns.
        for (const owner of requestedOwners.get(id) ?? []) {
          const stillDemanded = [...demandOwners.values()].some((demand) => demand.requested === owner && demand.ids.has(id));
          if (!stillDemanded) owner.delete(id);
        }
        requestedOwners.delete(id);
        settleDemand(id);
        publishResolved(id, url);
      }
      return url;
    }).catch(() => {
      if ((artworkRevision.get(id) ?? 0) === revision) remember(id, null);
    });
  }
}

/** Replaces the cache value after a reviewed metadata write and updates every
 * mounted view. A revision prevents an older in-flight extraction from
 * overwriting the newly written cover when it completes later. */
export function storeTrackArtwork(trackId: string, artworkUrl: string): void {
  artworkRevision.set(trackId, (artworkRevision.get(trackId) ?? 0) + 1);
  pendingQueue.delete(trackId);
  for (const owners of requestedOwners.get(trackId) ?? []) owners.delete(trackId);
  requestedOwners.delete(trackId);
  remember(trackId, artworkUrl);
}

function updateQueuedDemand(id: string): void {
  const current = pendingQueue.get(id);
  if (!current) return;
  let best: { fetcher: ArtworkFetcher; priority: number } | null = null;
  for (const demand of demandOwners.values()) {
    if (!demand.ids.has(id)) continue;
    if (!best || demand.priority > best.priority) best = { fetcher: demand.fetcher, priority: demand.priority };
  }
  if (!best) {
    pendingQueue.delete(id);
    for (const owner of requestedOwners.get(id) ?? []) owner.delete(id);
    requestedOwners.delete(id);
    return;
  }
  current.fetcher = best.fetcher;
  current.priority = best.priority;
}

function requestForDemand(id: string, demand: { fetcher: ArtworkFetcher; priority: number; requested: Set<string> }): void {
  if (resolvedCache.has(id) || inFlight.has(id) || demand.requested.has(id)) return;
  demand.requested.add(id);
  let owners = requestedOwners.get(id);
  if (!owners) requestedOwners.set(id, owners = new Set());
  owners.add(demand.requested);
  const queued = pendingQueue.get(id);
  if (queued) {
    if (demand.priority > queued.priority) {
      queued.priority = demand.priority;
      queued.fetcher = demand.fetcher;
    }
    return;
  }
  pendingQueue.set(id, { id, fetcher: demand.fetcher, priority: demand.priority, order: nextQueueOrder++ });
}

function registerDemand(ids: string[], fetcher: ArtworkFetcher, priority: number, requested: Set<string>, onComplete?: () => void): () => void {
  const token = Symbol("artwork-demand");
  const uniqueIds = new Set(ids);
  const demand = { ids: uniqueIds, remaining: new Set([...uniqueIds].filter((id) => !resolvedCache.has(id))), fetcher, priority, requested, onComplete };
  demandOwners.set(token, demand);
  for (const id of demand.ids) requestForDemand(id, demand);
  scheduleNext();
  if (demand.remaining.size === 0) queueMicrotask(() => demand.onComplete?.());
  return () => {
    demandOwners.delete(token);
    for (const id of demand.ids) updateQueuedDemand(id);
    scheduleNext();
  };
}

/**
 * Keep the persistent backend thumbnail cache warm without retaining the
 * whole library's data URLs in React state. Foreground views raise matching
 * queued work above this low-priority demand as soon as rows are shown.
 */
export function useTrackArtworkWarmup(trackIds: string[], fetcher: ArtworkFetcher | undefined): void {
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;
  const fetchArtwork = useRef<ArtworkFetcher>((id) => fetcherRef.current?.(id) ?? Promise.resolve(null)).current;
  useEffect(() => {
    if (!fetcherRef.current) return;
    let cancelled = false;
    let cursor = 0;
    let release: (() => void) | null = null;
    const queueBatch = () => {
      if (cancelled) return;
      const batch = trackIds.slice(cursor, cursor + 64);
      cursor += batch.length;
      if (batch.length === 0) return;
      release = registerDemand(batch, fetchArtwork, -1, new Set(), () => {
        release?.();
        release = null;
        queueMicrotask(queueBatch);
      });
    };
    queueBatch();
    return () => { cancelled = true; release?.(); };
  }, [trackIds, fetcher !== undefined]);
}

export interface TrackArtworkState {
  artwork: Record<string, string>;
  /** IDs with no known result yet; a neutral tile is shown instead of fallback art. */
  loading: ReadonlySet<string>;
}

export function useTrackArtworkState(trackIds: string[], fetcher: ArtworkFetcher | undefined, priority = 1): TrackArtworkState {
  const [art, setArt] = useState<Record<string, string>>({});
  const [, setCacheEpoch] = useState(0);
  const artRef = useRef(art);
  artRef.current = art;
  const requested = useRef<Set<string>>(new Set());
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;
  const fetchArtwork = useRef<ArtworkFetcher>((id) => fetcherRef.current?.(id) ?? Promise.resolve(null)).current;
  const idsRef = useRef(trackIds);
  idsRef.current = trackIds;
  // Artwork id lists are already windowed by callers (library visible ~20,
  // queue ~40, home ~40), so a full join is cheap and exact. The old
  // length:first:last heuristic missed content changes (e.g. a new search
  // with the same length/endpoints never re-ran the effect, leaving rows on
  // ♫ while the footer — a single-id request — loaded fine).
  const idsKey = trackIds.join("\0");
  useEffect(() => {
    if (!fetcherRef.current) return;
    const fetchArt = fetchArtwork;
    const ids = [...new Set(idsRef.current)];
    // Prune component state to only keep ids that are still visible.
    // Without this, HomeView holding 300 artwork URLs (each 1–2 MB) would keep
    // 300–600 MB in this component alone, plus module cache.
    setArt((prev) => {
      let next: Record<string, string> | null = null;
      for (const k of Object.keys(prev)) {
        if (!ids.includes(k)) {
          if (!next) next = { ...prev };
          delete next[k];
        }
      }
      // Also pull in any newly cached visible ids
      for (const id of ids) {
        const cachedUrl = cached(id);
        if (cachedUrl && !prev[id]) {
          if (!next) next = { ...prev };
          next[id] = cachedUrl;
        }
      }
      return next ? capTrimmed(next) : prev;
    });

    for (const id of ids) {
      const cachedUrl = cached(id);
      if (cachedUrl !== undefined) {
        if (cachedUrl) {
          setArt((prev) => {
            if (prev[id] === cachedUrl) return prev;
            return capTrimmed({ ...prev, [id]: cachedUrl });
          });
        }
      }
    }
    const releaseDemand = registerDemand(ids, fetchArt, priority, requested.current);
    if (requested.current.size > 1000) {
      // Prune old completed requests, retaining active visible ids.
      const activeIds = new Set(ids);
      const removable = [...requested.current].filter((id) => !activeIds.has(id) && !inFlight.has(id) && !pendingQueue.has(id));
      for (const id of removable.slice(0, Math.max(0, requested.current.size - 1000))) requested.current.delete(id);
    }

    // Subscribe to module-cache resolutions: fetches queued behind
    // MAX_CONCURRENT resolve into the cache long after this effect ran, and a
    // STATIC id list (sidebar playlists, queue window, Home cards) never
    // re-runs its effect to read them back — without this subscription the
    // URLs stayed invisible and playlist art "disappeared".
    // Eviction events unmark the id so a later visible pass re-requests it
    // (the LRU cache drops old rows while scrolling a large library, and the
    // requested marker would otherwise keep those rows blank forever).
    const onCacheEvent: Listener = (event) => {
      if (event.type === "evicted") {
        // LAZY retry only: forget the per-instance marker so a later visible
        // pass may re-request. Never fetch from here — an eviction-triggered
        // fetch evicts the next id and loops forever once the mounted working
        // set exceeds the cap (2026-09-09: gigabytes of heap churn).
        if (requested.current.has(event.id)) requested.current.delete(event.id);
        if (idsRef.current.includes(event.id) && !artRef.current[event.id]) setCacheEpoch((epoch) => epoch + 1);
        return;
      }
      const currentIds = idsRef.current;
      let unresolvedResult = false;
      for (const id of Object.keys(event.urls)) {
        if (!currentIds.includes(id)) continue;
        const url = event.urls[id];
        if (!url) {
          unresolvedResult = true;
          continue;
        }
        setArt((prev) => {
          if (prev[id] === url) return prev;
          return capTrimmed({ ...prev, [id]: url });
        });
      }
      if (unresolvedResult) setCacheEpoch((epoch) => epoch + 1);
    };
    listeners.add(onCacheEvent);

    // Wire up in-flight promises that were already queued to update this
    // component (covers the subscribe-race where a promise resolved between
    // mount and this effect running).
    for (const id of ids) {
      const promise = inFlight.get(id);
      if (!promise) continue;
      promise.then((url) => {
        if (!url) return;
        // Only update if this component still cares about this id
        if (!idsRef.current.includes(id)) return;
        setArt((prev) => {
          if (prev[id] === url) return prev;
          return capTrimmed({ ...prev, [id]: url });
        });
      }).catch(() => {});
    }
    return () => { listeners.delete(onCacheEvent); releaseDemand(); };
  }, [idsKey, priority, fetcher !== undefined]);
  const artwork = { ...art };
  const loading = new Set<string>();
  if (fetcher !== undefined) {
    for (const id of trackIds) {
      if (!artwork[id]) {
        const cachedUrl = resolvedCache.get(id)?.url;
        if (cachedUrl) artwork[id] = cachedUrl;
      }
      const requestIsActive = pendingQueue.has(id) || inFlight.has(id);
      const requestWasSettled = requested.current.has(id) && !requestIsActive;
      if (!resolvedCache.has(id) && !artwork[id] && !requestWasSettled) loading.add(id);
    }
  }
  return { artwork, loading };
}

export function useTrackArtwork(trackIds: string[], fetcher: ArtworkFetcher | undefined, priority = 1): Record<string, string> {
  return useTrackArtworkState(trackIds, fetcher, priority).artwork;
}

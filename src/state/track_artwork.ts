import { useEffect, useRef, useState } from "react";

export type ArtworkFetcher = (trackId: string) => Promise<string | null>;

/**
 * Lazily fetch embedded album art for the currently visible rows. Results are
 * kept in component state plus a module-level cache, so scrolling back to a
 * row never re-extracts artwork and tracks without artwork are not retried.
 * The fetcher is read from a ref, so a new inline closure per render does not
 * re-run the effect.
 *
 * The module cache is BOUNDED: every value is a full data-URL string (often
 * hundreds of kilobytes), and libraries reach thousands of rows. An unbounded
 * Map here grew past WebKit's memory limits on long scroll sessions and killed
 * the web content process (black window). Entries are evicted least-recently
 * used beyond CAP, and oversized payloads are served once without caching.
 */
const CAP = 80;
const MAX_CACHED_BYTES = 1_000_000;
const MAX_CONCURRENT = 6;
const resolvedCache = new Map<string, string | null>();
const inFlight = new Map<string, Promise<string | null>>();
let activeCount = 0;
const pendingQueue: Array<{ id: string; fetcher: ArtworkFetcher }> = [];

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
 * scrolling a large library evicts older rows' data URLs (CAP is 80), and
 * the requested set then blocks re-requesting those rows forever: artwork
 * that was visible moments earlier stayed blank for the whole session
 * (single-song-page art survived because its one-id request is prioritized
 * fresh on every navigation).
 */
type Listener = (event: { type: "resolved"; urls: Record<string, string> } | { type: "evicted"; id: string }) => void;
const listeners = new Set<Listener>();

function remember(id: string, url: string | null): void {
  // Delete-then-set keeps insertion order reflecting recency for LRU eviction.
  resolvedCache.delete(id);
  resolvedCache.set(id, url);
  if (url) {
    const urls = { [id]: url };
    for (const listener of listeners) listener({ type: "resolved", urls });
  }
  while (resolvedCache.size > CAP) {
    const oldest = resolvedCache.keys().next().value;
    if (oldest === undefined) break;
    // Only evicting a VALUE matters — a cached null means "no artwork,
    // never retry", and re-requesting it is exactly what the no-art
    // fast-path exists to prevent. Evicting a data URL means a later
    // scroll-back must be allowed to re-extract it, so forget the id in
    // every component's requested set.
    const wasValue = resolvedCache.get(oldest) !== null;
    resolvedCache.delete(oldest);
    if (wasValue) {
      for (const listener of listeners) listener({ type: "evicted", id: oldest });
    }
  }
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
  resolved: ReadonlyMap<string, string | null>,
  inFlightIds: ReadonlySet<string>,
  id: string,
): boolean {
  if (resolved.has(id)) return false;      // known (value or definitive no-art)
  if (inFlightIds.has(id)) return false;   // already fetching
  if (requested.has(id)) return false;     // already asked this mount
  return true;
}

function scheduleNext(): void {
  while (activeCount < MAX_CONCURRENT && pendingQueue.length > 0) {
    const job = pendingQueue.shift();
    if (!job) break;
    const { id, fetcher } = job;
    if (resolvedCache.has(id) || inFlight.has(id)) continue;
    activeCount += 1;
    const promise = fetcher(id).finally(() => {
      activeCount -= 1;
      inFlight.delete(id);
      // Defer next tick to avoid deep recursion and to let React batch.
      queueMicrotask(scheduleNext);
    });
    inFlight.set(id, promise);
    promise.then((url) => {
      if (url === null || url.length <= MAX_CACHED_BYTES) remember(id, url);
      return url;
    }).catch(() => {
      remember(id, null);
    });
  }
}

export function useTrackArtwork(trackIds: string[], fetcher: ArtworkFetcher | undefined): Record<string, string> {
  const [art, setArt] = useState<Record<string, string>>({});
  const requested = useRef<Set<string>>(new Set());
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;
  const idsRef = useRef(trackIds);
  idsRef.current = trackIds;
  // Avoid allocating a huge join string for large queues (4k ids → ~150 KB per render).
  const idsKey = trackIds.length === 0 ? "" : `${trackIds.length}:${trackIds[0] ?? ""}:${trackIds[trackIds.length - 1] ?? ""}:${trackIds.slice(0, 3).join(",")}`;
  useEffect(() => {
    const fetchArt = fetcherRef.current;
    const ids = idsRef.current;
    if (!fetchArt) return;
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
        const cached = resolvedCache.get(id);
        if (cached && !prev[id]) {
          if (!next) next = { ...prev };
          next[id] = cached;
        }
      }
      return next ? capTrimmed(next) : prev;
    });

    const inFlightIds = new Set(inFlight.keys());
    for (const id of ids) {
      const cached = resolvedCache.get(id);
      if (cached !== undefined) {
        if (cached) {
          setArt((prev) => {
            if (prev[id] === cached) return prev;
            return capTrimmed({ ...prev, [id]: cached });
          });
        }
        continue;
      }
      if (!needsArtworkRequest(requested.current, resolvedCache, inFlightIds, id)) continue;
      requested.current.add(id);
      // Prioritize single-track requests (e.g., footer now-playing) over
      // bulk HomeView/Queue bursts so playing a song never waits behind 80
      // background fetches.
      if (ids.length === 1) pendingQueue.unshift({ id, fetcher: fetchArt });
      else pendingQueue.push({ id, fetcher: fetchArt });
      if (requested.current.size > 1000) {
        // Prune oldest half to keep the set bounded
        const vals = [...requested.current];
        requested.current = new Set(vals.slice(vals.length / 2));
      }
    }
    scheduleNext();

    // Subscribe to module-cache resolutions: fetches queued behind
    // MAX_CONCURRENT resolve into the cache long after this effect ran, and a
    // STATIC id list (sidebar playlists, queue window, Home cards) never
    // re-runs its effect to read them back — without this subscription the
    // URLs stayed invisible and playlist art "disappeared".
    // Eviction events unmark the id so a later visible pass re-requests it
    // (the LRU cache drops old rows while scrolling a large library, and the
    // requested marker would otherwise keep those rows blank forever).
    const onCacheEvent = (event: { type: "resolved"; urls: Record<string, string> } | { type: "evicted"; id: string }) => {
      if (event.type === "evicted") {
        if (requested.current.has(event.id)) requested.current.delete(event.id);
        return;
      }
      const currentIds = idsRef.current;
      for (const id of Object.keys(event.urls)) {
        if (!currentIds.includes(id)) continue;
        const url = event.urls[id];
        setArt((prev) => {
          if (prev[id] === url) return prev;
          return capTrimmed({ ...prev, [id]: url });
        });
      }
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
    return () => { listeners.delete(onCacheEvent); };
  }, [idsKey]);
  return art;
}

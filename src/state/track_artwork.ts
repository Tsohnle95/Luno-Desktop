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

function remember(id: string, url: string | null): void {
  // Delete-then-set keeps insertion order reflecting recency for LRU eviction.
  resolvedCache.delete(id);
  resolvedCache.set(id, url);
  while (resolvedCache.size > CAP) {
    const oldest = resolvedCache.keys().next().value;
    if (oldest === undefined) break;
    resolvedCache.delete(oldest);
  }
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
      if (next) {
        const keys = Object.keys(next);
        if (keys.length > CAP * 2) {
          const trimmed: Record<string, string> = {};
          for (const k of keys.slice(-CAP)) trimmed[k] = next[k];
          return trimmed;
        }
        return next;
      }
      // Also pull in any newly cached visible ids
      for (const id of ids) {
        const cached = resolvedCache.get(id);
        if (cached && !prev[id]) {
          if (!next) next = { ...prev };
          next[id] = cached;
        }
      }
      if (!next) return prev;
      const keys = Object.keys(next);
      if (keys.length > CAP * 2) {
        const trimmed: Record<string, string> = {};
        for (const k of keys.slice(-CAP)) trimmed[k] = next[k];
        return trimmed;
      }
      return next;
    });

    for (const id of ids) {
      const cached = resolvedCache.get(id);
      if (cached !== undefined) {
        if (cached) {
          setArt((prev) => {
            if (prev[id] === cached) return prev;
            const next = { ...prev, [id]: cached };
            const keys = Object.keys(next);
            if (keys.length > CAP * 2) {
              const trimmed: Record<string, string> = {};
              for (const k of keys.slice(-CAP)) trimmed[k] = next[k];
              return trimmed;
            }
            return next;
          });
        }
        continue;
      }
      if (requested.current.has(id) || inFlight.has(id)) continue;
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

    // Wire up in-flight promises that were already queued to update this component.
    for (const id of ids) {
      const promise = inFlight.get(id);
      if (!promise) continue;
      promise.then((url) => {
        if (!url) return;
        // Only update if this component still cares about this id
        if (!idsRef.current.includes(id)) return;
        setArt((prev) => {
          if (prev[id] === url) return prev;
          const next = { ...prev, [id]: url };
          const keys = Object.keys(next);
          if (keys.length > CAP * 2) {
            const trimmed: Record<string, string> = {};
            for (const k of keys.slice(-CAP)) trimmed[k] = next[k];
            return trimmed;
          }
          return next;
        });
      }).catch(() => {});
    }
  }, [idsKey]);
  return art;
}

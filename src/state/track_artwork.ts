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
const CAP = 200;
const MAX_CACHED_BYTES = 2_000_000;
const resolvedCache = new Map<string, string | null>();
const inFlight = new Map<string, Promise<string | null>>();

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

export function useTrackArtwork(trackIds: string[], fetcher: ArtworkFetcher | undefined): Record<string, string> {
  const [art, setArt] = useState<Record<string, string>>({});
  const requested = useRef<Set<string>>(new Set());
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;
  const idsKey = trackIds.join("\u0000");
  useEffect(() => {
    const fetchArt = fetcherRef.current;
    if (!fetchArt) return;
    for (const id of trackIds) {
      const cached = resolvedCache.get(id);
      if (cached !== undefined) {
        if (cached) setArt((prev) => (prev[id] === cached ? prev : { ...prev, [id]: cached }));
        continue;
      }
      if (requested.current.has(id)) continue;
      requested.current.add(id);
      const promise = inFlight.get(id) ?? fetchArt(id).finally(() => inFlight.delete(id));
      inFlight.set(id, promise);
      promise.then((url) => {
        // Oversized payloads still render, but are not retained: one such
        // entry must not crowd the bounded cache, let alone grow it.
        if (url === null || url.length <= MAX_CACHED_BYTES) remember(id, url);
        if (url) setArt((prev) => (prev[id] === url ? prev : { ...prev, [id]: url }));
      }).catch(() => {
        remember(id, null);
      });
    }
  }, [idsKey]);
  return art;
}

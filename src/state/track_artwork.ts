import { useEffect, useRef, useState } from "react";

export type ArtworkFetcher = (trackId: string) => Promise<string | null>;

/**
 * Lazily fetch embedded album art for the currently visible rows. Results are
 * kept in component state plus a module-level cache, so scrolling back to a
 * row never re-extracts artwork and tracks without artwork are not retried.
 * The fetcher is read from a ref, so a new inline closure per render does not
 * re-run the effect.
 */
const resolvedCache = new Map<string, string | null>();
const inFlight = new Map<string, Promise<string | null>>();

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
      if (cached) {
        setArt((prev) => (prev[id] === cached ? prev : { ...prev, [id]: cached }));
        continue;
      }
      if (requested.current.has(id)) continue;
      requested.current.add(id);
      const promise = inFlight.get(id) ?? fetchArt(id).finally(() => inFlight.delete(id));
      inFlight.set(id, promise);
      promise.then((url) => {
        resolvedCache.set(id, url);
        if (url) setArt((prev) => (prev[id] === url ? prev : { ...prev, [id]: url }));
      }).catch(() => {
        resolvedCache.set(id, null);
      });
    }
  }, [idsKey]);
  return art;
}
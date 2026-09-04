import { describe, expect, it } from "vitest";
import { needsArtworkRequest } from "../state/track_artwork";

/**
 * Regression: artwork for bulk surfaces (library rows, Home cards, queue,
 * sidebar) silently disappeared after scrolling while the single-song page
 * kept working.
 *
 * Root cause: the module artwork cache is bounded (80 entries, LRU). Scrolling
 * a large library evicts older rows' data URLs. The per-component "requested"
 * set — the ever-asked gate inside `useTrackArtwork` — was never told about
 * the eviction, so the next pass skipped those ids forever:
 *
 *   if (cached !== undefined) ... continue;      // miss after eviction
 *   if (requested.has(id)) continue;              // ← stuck here
 *
 * The single-song page survived only because its one-id request is prioritized
 * fresh on every navigation, so it never depended on the long-lived set.
 *
 * The fix emits an `evicted` event when the LRU cache drops a *value* (never
 * for a definitive null), and mounted hooks delete the id from their
 * `requested` set. This test pins the gate so a regression cannot silently
 * re-introduce the never-retry behavior.
 */
describe("artwork cache eviction retry", () => {
  const noIds = (): Set<string> => new Set<string>();
  const noFlight = (): Set<string> => new Set<string>();

  it("does not re-request an id that is still cached", () => {
    const requested = new Set(["t1"]);
    const resolved = new Map([["t1", "data:image/jpeg;base64,AAA"]]);
    expect(needsArtworkRequest(requested, resolved, noFlight(), "t1")).toBe(false);
  });

  it("does not re-request a definitive no-artwork id", () => {
    const requested = new Set(["t1"]);
    const resolved = new Map([["t1", null]]);
    expect(needsArtworkRequest(requested, resolved, noFlight(), "t1")).toBe(false);
  });

  it("skips an id already requested in this mount", () => {
    const requested = new Set(["t1"]);
    expect(needsArtworkRequest(requested, new Map<string, string | null>(), noFlight(), "t1")).toBe(false);
  });

  it("skips an id whose fetch is already in flight", () => {
    const requested = noIds();
    expect(needsArtworkRequest(requested, new Map<string, string | null>(), new Set(["t1"]), "t1")).toBe(false);
  });

  it("requests a fresh id", () => {
    const requested = new Set(["t0"]);
    expect(needsArtworkRequest(requested, new Map<string, string | null>(), noFlight(), "t1")).toBe(true);
  });

  it("re-requests an evicted value after the component forgot the request", () => {
    // Simulates exactly what the eviction event does: the LRU cache dropped
    // t1's data URL (it is no longer in `resolved`), and the mounted hook
    // removed t1 from its `requested` set in response to the event.
    const requested = noIds(); // t1 was deleted by the eviction handler
    expect(needsArtworkRequest(requested, new Map<string, string | null>(), noFlight(), "t1")).toBe(true);
  });

  it("keeps a requested marker effective until the cache itself evicts the value", () => {
    // The invariant that was broken: with the marker still present AND the
    // entry still cached, no re-request happens (fast path preserved). The
    // marker only stops mattering once the value is evicted — at which point
    // the eviction handler removes it from the set.
    const requested = new Set(["t1"]);
    const resolved = new Map<string, string | null>([["t1", "data:image/jpeg;base64,AAA"]]);
    expect(needsArtworkRequest(requested, resolved, noFlight(), "t1")).toBe(false);

    // Cache evicts t1 (LRU pressure)...
    resolved.delete("t1");
    // ...and the eviction event removes it from the requested set.
    requested.delete("t1");
    expect(needsArtworkRequest(requested, resolved, noFlight(), "t1")).toBe(true);
  });
});

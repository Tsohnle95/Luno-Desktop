import { describe, it, expect } from "vitest";
import { discoveryResultsReady, requestDiscoveryIntent } from "../state/discovery_intent";


describe("Discovery intent without recommendations", () => {
  it("enables while the current song has no recommendations without a banner or transport change", () => {
    const changes: unknown[] = [];
    requestDiscoveryIntent(false, {
      setRequested: (value) => changes.push(["requested", value]),
      reportError: (message) => changes.push(["error", message]),
    });
    expect(changes).toEqual([["requested", true]]);
  });
  it("still reports missing credentials before enabling", () => {
    const changes: unknown[] = [];
    requestDiscoveryIntent(true, {
      setRequested: (value) => changes.push(["requested", value]),
      reportError: (message) => changes.push(["error", message]),
    });
    expect(changes).toEqual([["error", "Add a Last.fm API key in Settings to enable discovery mode."]]);
  });
});


describe("Discovery admission while following a physical playlist", () => {
  it("waits without projecting an empty seed, then admits recommendations from the next physical song", () => {
    let requested = false;
    requestDiscoveryIntent(false, { setRequested: (value) => { requested = value; }, reportError: () => { throw new Error("unexpected layout banner"); } });
    expect(discoveryResultsReady(requested, "empty-song", { seedKey: "empty-song", state: "ready", items: [] })).toBe(false);
    expect(discoveryResultsReady(requested, "next-song", { seedKey: "empty-song", state: "ready", items: [{ title: "stale" }] })).toBe(false);
    expect(discoveryResultsReady(requested, "next-song", { seedKey: "next-song", state: "loading", items: [] })).toBe(false);
    expect(discoveryResultsReady(requested, "next-song", { seedKey: "next-song", state: "ready", items: [{ title: "new recommendation" }] })).toBe(true);
    expect(discoveryResultsReady(false, "next-song", { seedKey: "next-song", state: "ready", items: [1] })).toBe(false);
  });
});

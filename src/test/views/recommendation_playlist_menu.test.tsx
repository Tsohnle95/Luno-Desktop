// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Shell } from "../../app/Shell";
import type { Bridge } from "../../app/bridge";
import type { BackendProjection } from "../../state/backend_projection";
import { previewKey, type Recommendation, type Track } from "../../shared/contracts";

// jsdom has no layout engine; the shell only needs the observer to exist.
class ResizeObserverStub { observe() {} unobserve() {} disconnect() {} }
Object.defineProperty(globalThis, "ResizeObserver", { value: ResizeObserverStub, writable: true, configurable: true });

const libraryTrack: Track = { id: "track-seed", path: "/music/Gate Six/Aurora.m4a", title: "Aurora", artist: "Gate Six", album: "Chaos 93", duration_ms: 3000, artwork_url: null, favorite: false, provider: null };
const playlist = { id: "playlist:Recs", name: "Recs", path: "/music/Recs", url: null, ordered_track_ids: [] as string[] };
const recommendation: Recommendation = { artist: "Skrillex", title: "Bangarang", match_score: 0.9, artwork_url: null };
const recommendationKey = previewKey(recommendation.artist, recommendation.title);

function snapshot(overrides: Partial<BackendProjection> = {}): BackendProjection {
  return {
    library: { version: 1, generation: 1, tracks: [libraryTrack], playlists: [playlist] },
    playback: { queue: [libraryTrack.id], current_index: 0, audible_track_id: libraryTrack.id, shuffle: false, repeat: "off", status: "playing", position_ms: 1000, duration_ms: 3000, generation: 1 },
    preview: null, discover: null, video: null, jobs: {}, saved: {}, errors: [], recentHistory: [], shutdown: null,
    generations: { library: 1, playback: 1, preview: -1, discover: -1, video: -1, jobs: -1, errors: -1 },
    lastEventIds: {},
    ...overrides,
  } as BackendProjection;
}

/** Discover mode holds the recommendation in the physical queue, not the library. */
const discoverPlayback = snapshot({
  playback: { queue: [recommendationKey, libraryTrack.id], current_index: 0, audible_track_id: recommendationKey, shuffle: false, repeat: "off", status: "playing", position_ms: 500, duration_ms: null, generation: 2 },
  discover: { generation: 2, enabled: true, window: [recommendationKey], ready: 1, queued: 0, prepared: [recommendationKey], unavailable: [] },
});

function mockBridge(extra: Record<string, unknown> = {}) {
  const saveRecommendation = vi.fn((key: string, playlistId: string) => Promise.resolve({ key, playlist_id: playlistId, state: "saving" }));
  const addTracksToPlaylist = vi.fn(() => Promise.resolve({ copied: 1, failed: 0 }));
  const discover = vi.fn(() => Promise.resolve([recommendation]));
  const noop = () => Promise.resolve(null);
  const handlers: Record<string, unknown> = { saveRecommendation, addTracksToPlaylist, discover, ...extra };
  const bridge = new Proxy({}, {
    get: (_target, property) => (typeof property === "string" && property in handlers ? handlers[property] : noop),
  }) as unknown as Bridge;
  return { bridge, saveRecommendation, addTracksToPlaylist, discover };
}

function backendStore() {
  const store = { current: snapshot() };
  const listeners = new Set<() => void>();
  return {
    backendProjection: {
      getSnapshot: () => store.current,
      subscribe: (listener: () => void) => { listeners.add(listener); return () => listeners.delete(listener); },
    },
    update: (next: BackendProjection) => { store.current = next; listeners.forEach((listener) => listener()); },
  };
}

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  (globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.clearAllMocks();
});

async function flush() {
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
}

function menuItem(label: string) {
  return [...host.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((item) => item.textContent === label);
}

async function openContextMenu(trackId: string) {
  const card = host.querySelector<HTMLElement>(`[data-track-id="${trackId}"]`);
  expect(card).toBeTruthy();
  await act(async () => {
    card!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 40, clientY: 40 }));
  });
  return card!;
}

describe("queue recommendation context menu", () => {
  it("adds a playing recommendation to a playlist through the discovery save path", async () => {
    const { backendProjection, update } = backendStore();
    const { bridge, saveRecommendation, addTracksToPlaylist, discover } = mockBridge();

    await act(async () => { root.render(<Shell bridge={bridge} backendProjection={backendProjection} backendAvailable />); });
    await flush();
    // The seed track (still the last library track) fetched the recommendation metadata.
    expect(discover).toHaveBeenCalledWith("Gate Six", "Aurora", 20);

    await act(async () => { update(discoverPlayback); });
    const card = await openContextMenu(recommendationKey);
    expect(card.textContent).toContain("Bangarang");

    const addToPlaylist = menuItem("Add to playlist…");
    expect(addToPlaylist).toBeTruthy();
    // Regression: this was disabled because a recommendation key is not a library id.
    expect(addToPlaylist!.disabled).toBe(false);
    expect(menuItem("Add to new playlist…")!.disabled).toBe(false);
    // Library-only actions stay unavailable for recommendation rows.
    expect(menuItem("Play next")!.disabled).toBe(true);
    expect(menuItem("Add to queue")!.disabled).toBe(true);

    await act(async () => { addToPlaylist!.click(); });
    const picker = host.querySelector<HTMLElement>('[role="dialog"][aria-label="Add “Bangarang” to playlist"]');
    expect(picker).toBeTruthy();
    const target = [...picker!.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.getAttribute("aria-label")?.startsWith("Recs,"));
    expect(target).toBeTruthy();

    await act(async () => { target!.click(); });
    expect(saveRecommendation).toHaveBeenCalledExactlyOnceWith(recommendationKey, playlist.id);
    expect(addTracksToPlaylist).not.toHaveBeenCalled();

    // The terminal SaveState, not the command promise, reports the outcome.
    await act(async () => {
      update({
        ...discoverPlayback,
        saved: { [recommendationKey]: { key: recommendationKey, playlist_id: playlist.id, state: "saved", already_saved: false, path: "/music/Recs/Skrillex - Bangarang.m4a", job_id: null, error: null } },
      });
    });
    expect(host.textContent).toContain("Added “Bangarang” to “Recs”.");
  });

  it("creates a new playlist and saves the recommendation in one gesture", async () => {
    const { backendProjection, update } = backendStore();
    const createPlaylist = vi.fn(() => Promise.resolve({ path: "/music/New list" }));
    const { bridge, saveRecommendation } = mockBridge({ createPlaylist });

    await act(async () => { root.render(<Shell bridge={bridge} backendProjection={backendProjection} backendAvailable />); });
    await flush();
    await act(async () => { update(discoverPlayback); });
    await openContextMenu(recommendationKey);

    await act(async () => { menuItem("Add to new playlist…")!.click(); });
    const dialog = host.querySelector<HTMLElement>('[role="dialog"]');
    expect(dialog).toBeTruthy();
    const input = dialog!.querySelector<HTMLInputElement>("input")!;
    await act(async () => {
      const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      setValue.call(input, "New list");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    const submit = [...dialog!.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "Create playlist")!;
    expect(submit.disabled).toBe(false);
    await act(async () => { submit.click(); });
    await flush();

    expect(createPlaylist).toHaveBeenCalledExactlyOnceWith("New list");
    expect(saveRecommendation).toHaveBeenCalledExactlyOnceWith(recommendationKey, "playlist:New list");
  });

  it("still copies a library track into a playlist without the discovery save", async () => {
    const { backendProjection } = backendStore();
    const { bridge, saveRecommendation, addTracksToPlaylist } = mockBridge();

    await act(async () => { root.render(<Shell bridge={bridge} backendProjection={backendProjection} backendAvailable />); });
    await flush();

    const card = await openContextMenu(libraryTrack.id);
    expect(card.textContent).toContain("Aurora");
    const addToPlaylist = menuItem("Add to playlist…");
    expect(addToPlaylist!.disabled).toBe(false);

    await act(async () => { addToPlaylist!.click(); });
    const picker = host.querySelector<HTMLElement>('[role="dialog"]');
    expect(picker).toBeTruthy();
    const target = [...picker!.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.getAttribute("aria-label")?.startsWith("Recs,"));
    expect(target).toBeTruthy();

    await act(async () => { target!.click(); });
    expect(addTracksToPlaylist).toHaveBeenCalledExactlyOnceWith([libraryTrack.id], playlist.id);
    expect(saveRecommendation).not.toHaveBeenCalled();
  });
});

import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PlayerBar } from "../../components/PlayerBar";
import { Queue, queueVirtualWindow } from "../../components/Queue";
import { isBusyDownloadError, isMissingConfigurationError, isRetryableRuntimeCommandError, Shell, videoFullscreenClass } from "../../app/Shell";
import type { BackendProjection } from "../../state/backend_projection";

const projectionTrack = { id: "t1", path: null, title: "Aurora", artist: "Gate Six", album: "Chaos 93", duration_ms: 3000, artwork_url: null, favorite: false, provider: null };
const projection = {
  getSnapshot: (): BackendProjection => ({ library: { version: 1, generation: 1, tracks: [projectionTrack], playlists: [{ id: "p1", name: "Rap", path: null, url: null, ordered_track_ids: ["t1"] }] }, playback: { queue: ["t1"], current_index: 0, shuffle: false, repeat: "off", status: "playing", position_ms: 1250, duration_ms: 3000, generation: 1 }, preview: null, discover: null, video: null, jobs: {}, saved: {}, errors: [], recentHistory: [], shutdown: null, generations: { library: 1, playback: 1, preview: -1, discover: -1, video: -1, jobs: -1, errors: -1 }, lastEventIds: {} }),
  subscribe: () => () => {},
};
const favoritesProjection = {
  getSnapshot: (): BackendProjection => {
    const snapshot = projection.getSnapshot();
    return { ...snapshot, library: { ...snapshot.library!, tracks: snapshot.library!.tracks.map((track) => ({ ...track, favorite: true })) } };
  },
  subscribe: () => () => {},
};

describe("Gate 6 player bar", () => {
  it("uses the current library favorite and disables the action for unsaved previews", () => {
    const local = { ...projectionTrack, path: "/music/Aurora.mp3", favorite: true };
    const preview = { ...projectionTrack, id: "Artist-Preview", path: null, title: "Preview", favorite: false };
    const localHtml = renderToStaticMarkup(<PlayerBar track={preview} favoriteTrack={local} onFavoriteToggle={() => {}} />);
    expect(localHtml).toContain('aria-label="Remove from favorites"');
    expect(localHtml).not.toMatch(/aria-label="Remove from favorites"[^>]*disabled=""/);

    const previewHtml = renderToStaticMarkup(<PlayerBar track={preview} favoriteTrack={null} onFavoriteToggle={() => {}} />);
    expect(previewHtml).toMatch(/aria-label="Save this song to favorite it"[^>]*disabled=""/);
  });

  it("treats a missing settings file as first-run but surfaces other errors", () => {
    const missingConfig = { code: "not_found", message: "configuration is missing", retryable: false };
    expect(isMissingConfigurationError(missingConfig)).toBe(true);
    expect(isMissingConfigurationError(JSON.stringify(missingConfig))).toBe(true);
    expect(isMissingConfigurationError(new Error(JSON.stringify(missingConfig)))).toBe(true);
    expect(isMissingConfigurationError({ code: "not_found", message: "library root is missing" })).toBe(false);
    expect(isMissingConfigurationError({ code: "invalid_request", message: "configuration is corrupt" })).toBe(false);
  });

  it("recognizes retryable command errors when scheduling automatic preview prefetch", () => {
    const busy = { code: "busy", message: "preview preparation is waiting for background task capacity", retryable: true };
    expect(isRetryableRuntimeCommandError(busy)).toBe(true);
    expect(isRetryableRuntimeCommandError(JSON.stringify(busy))).toBe(true);
    expect(isRetryableRuntimeCommandError(new Error(JSON.stringify(busy)))).toBe(true);
    expect(isRetryableRuntimeCommandError({ ...busy, retryable: false })).toBe(false);
    expect(isRetryableRuntimeCommandError(new Error("command failed"))).toBe(false);
  });

  it("recognizes backend busy errors even when the message omits the code", () => {
    expect(isBusyDownloadError({ code: "busy", message: "download could not start", retryable: true })).toBe(true);
    expect(isBusyDownloadError(new Error(JSON.stringify({ code: "busy", message: "download could not start", retryable: true })))).toBe(true);
    expect(isBusyDownloadError({ code: "invalid_request", message: "download could not start", retryable: false })).toBe(false);
    expect(isBusyDownloadError(new Error("another download is already running"))).toBe(true);
  });

  it("uses an explicit shell state class for app fullscreen", () => {
    expect(videoFullscreenClass(false)).toBe("");
    expect(videoFullscreenClass(true)).toBe(" video-fullscreen");
    const html = renderToStaticMarkup(<Shell backendProjection={projection} backendAvailable={false} />);
    expect(html).toContain('data-video-fullscreen="false"');
    expect(html).not.toContain("window.setFullscreen");
  });

  it("keeps a multi-thousand-song queue bounded to a visible DOM window", () => {
    const tracks = Array.from({ length: 5000 }, (_, index) => ({ ...projectionTrack, id: `track-${index}`, title: `Track ${index}` }));
    const html = renderToStaticMarkup(<Queue tracks={tracks} currentIndex={0} />);
    expect((html.match(/class="luno-queue-row"/g) ?? []).length).toBeLessThanOrEqual(40);
    expect(html).not.toContain("Track 4999");
    expect(queueVirtualWindow(5600, 0, 560, 5000)).toEqual({ start: 88, end: 122 });
  });

  it("exposes labeled transport, scrubber, and volume controls", () => {
    const html = renderToStaticMarkup(<PlayerBar />);
    expect(html).toContain("Nothing playing");
    expect(html).toContain('aria-label="Previous track"');
    expect(html).toContain('aria-label="Next track"');
    expect(html).toContain('aria-label="Playback position"');
    expect(html).toContain('aria-label="Volume"');
  });

  it("announces playing state through the toggle label and position values", () => {
    const html = renderToStaticMarkup(<PlayerBar track={{ id: "t1", title: "Aurora", artist: "Gate Six", album: null, path: null, duration_ms: 3000, artwork_url: null, favorite: false, provider: null }} playback={{ queue: ["t1"], current_index: 0, shuffle: false, repeat: "off", status: "playing", position_ms: 1250, duration_ms: 3000, generation: 1 }} />);
    expect(html).toContain('aria-label="Pause"');
    expect(html).toContain("Aurora");
    expect(html).toContain("0:01");
    expect(html).toContain("0:03");
  });

  it("renders the Spotify-style frame: top bar nav, search, Your Library sidebar, and queue panel", () => {
    const html = renderToStaticMarkup(<Shell backendProjection={projection} backendAvailable={false} />);
    expect(html).toContain('aria-label="Primary navigation"');
    expect(html).toContain('aria-label="Home" aria-current="page"');
    expect(html).toContain("Search a song");
    // All other destinations live inside the settings rail — no icons in the bar.
    expect(html).not.toContain('title="Discover"');
    expect(html).not.toContain('title="Downloader"');
    expect(html).not.toContain('title="Recent"');
    expect(html).toContain("Your Library");
    expect(html).toContain('aria-label="Show playlists"');
    expect(html).toContain('aria-label="Show albums"');
    expect(html).toContain("All Music");
    expect(html).toContain("Favorites");
    expect(html).toContain('data-slot="queue"');
    expect(html).not.toContain('aria-label="Close queue panel"');
    expect(html).toContain('aria-label="Queue tab"');
    expect(html).toContain(">Recently played</button>");
    expect(html).toContain("Now playing");
    expect(html).toContain("Next from: Rap");
  });

  it("lands on the Home dashboard, keeps migration silent, and reaches settings via the top-left profile", () => {
    const html = renderToStaticMarkup(<Shell backendProjection={projection} backendAvailable={false} />);
    expect(html).not.toContain("Migration completed");
    expect(html).toContain("Made for you");
    expect(html).toContain("Recently played");
    expect(html).toContain("Most popular songs");
    expect(html).toContain("Favorite songs");
    expect(html).toContain("Most popular playlists");
    expect(html).toContain("Recently downloaded");
    expect(html.indexOf('aria-label="Recently played"')).toBeLessThan(html.indexOf('aria-label="Most popular songs"'));
    expect(html.indexOf('aria-label="Most popular songs"')).toBeLessThan(html.indexOf('aria-label="Favorite songs"'));
    expect(html.indexOf('aria-label="Favorite songs"')).toBeLessThan(html.indexOf('aria-label="Most popular playlists"'));
    // Profile avatar sits at the FAR top-right, after the primary nav, and
    // opens the dedicated settings page (no dropdown).
    expect(html.indexOf('aria-label="Primary navigation"')).toBeLessThan(html.indexOf('aria-label="Open settings"'));
    expect(html).not.toContain("aria-haspopup");
  });

  it("uses split artwork for All Music and Favorites sidebar entries", () => {
    const html = renderToStaticMarkup(<Shell backendProjection={favoritesProjection} backendAvailable={false} />);
    expect(html.match(/class="luno-playlist-mosaic luno-playlist-pin"/g) ?? []).toHaveLength(3);
  });

  it("exposes exactly one current navigation entry across the primary nav", () => {
    const html = renderToStaticMarkup(<Shell backendProjection={projection} backendAvailable={false} />);
    const region = html.split('aria-label="Primary navigation"')[1]?.split("</nav>")[0] ?? "";
    expect((region.match(/aria-current="page"/g) ?? []).length).toBe(1);
  });

  it("wires the player transport to shuffle and repeat; panels are drag-managed", () => {
    const html = renderToStaticMarkup(<Shell backendProjection={projection} backendAvailable={false} />);
    expect(html).toContain('aria-label="Enable shuffle"');
    expect(html).toContain('aria-label="Repeat off"');
    // Panels have NO close/open buttons anymore — both dividers stay mounted
    // (even while a panel is collapsed) so edge-drags snap them closed/open.
    expect(html).not.toContain('aria-label="Close queue panel"');
    expect(html).toContain('aria-label="Resize library panel width"');
    expect(html).toContain('aria-label="Resize queue panel width"');
  });

  it("surfaces the latest backend error event in the runtime status strip", () => {
    const failingProjection = {
      getSnapshot: (): BackendProjection => {
        const snapshot = projection.getSnapshot();
        return { ...snapshot, errors: [{ code: "internal", message: "boom", retryable: true }] };
      },
      subscribe: () => () => {},
    };
    const html = renderToStaticMarkup(<Shell backendProjection={failingProjection} backendAvailable={false} />);
    expect(html).toContain('data-state="backend-error"');
    expect(html).toContain("boom");
  });
});

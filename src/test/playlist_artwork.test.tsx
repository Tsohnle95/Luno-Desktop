import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PlaylistArtwork } from "../components/PlaylistArtwork";
import { LibrarySidebar } from "../components/LibrarySidebar";
import { LibraryView, RECENT_DOWNLOADS_SCOPE } from "../views/LibraryView";
import type { Track } from "../shared/contracts";
const tracks: Track[] = Array.from({ length: 4 }, (_, i) => ({ id: `t${i}`, title: `Song ${i}`, artist: "Artist", path: null, album: null, duration_ms: null, artwork_url: `https://example.test/${i}.jpg`, favorite: false, provider: null }));
describe("playlist artwork quadrants", () => {
  it.each([1, 2, 3, 4])("fills four equal cells with %i songs", (count) => {
    const html = renderToStaticMarkup(<PlaylistArtwork tracks={tracks.slice(0, count)} artwork={{}} />);
    expect(html.match(/class="luno-playlist-mosaic-cell"/g)).toHaveLength(4);
    expect(html).not.toContain("grid-row:1 / -1");
    expect(html.match(/<img /g)).toHaveLength(4);
  });
  it("uses the same cells for mixed real, loading and missing artwork", () => {
    const mixed = tracks.map((t, i) => ({ ...t, artwork_url: i === 0 ? t.artwork_url : null }));
    const html = renderToStaticMarkup(<PlaylistArtwork tracks={mixed} artwork={{}} loadingIds={new Set(["t1"])} />);
    expect(html.match(/class="luno-playlist-mosaic-cell"/g)).toHaveLength(4);
    expect(html).toContain("luno-artwork-loading");
    expect(html).toContain("luno-rec-art");
  });
  it("gives Recently downloaded a mosaic in the sidebar and hero", () => {
    const snapshot = { version: 1 as const, generation: 1, tracks, playlists: [] };
    const sidebar = renderToStaticMarkup(<LibrarySidebar snapshot={snapshot} tracksById={new Map(tracks.map(t => [t.id, t]))} selectedScope={RECENT_DOWNLOADS_SCOPE} available recentlyDownloadedTracks={tracks} onCreatePlaylist={() => {}} onSelectScope={() => {}} onPlaylistContextMenu={() => {}} />);
    const recent = sidebar.split('class="luno-playlist-card"').find(part => part.includes("Recently downloaded"))!;
    expect(recent.match(/class="luno-playlist-mosaic-cell"/g)).toHaveLength(4);
    const hero = renderToStaticMarkup(<LibraryView snapshot={snapshot} selectedScope={RECENT_DOWNLOADS_SCOPE} recentlyDownloadedTracks={tracks} />);
    expect(hero.match(/class="luno-playlist-mosaic-cell"/g)).toHaveLength(4);
  });
});

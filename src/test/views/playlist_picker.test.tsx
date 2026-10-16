import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Playlist, Track } from "../../shared/contracts";
import { PlaylistPicker, playlistMembershipKey } from "../../components/PlaylistPicker";

const source: Track = { id: "source-copy", path: null, title: "All Night", artist: "The Example", album: null, duration_ms: 210_000, artwork_url: null, favorite: true, provider: null };
const playlistCopy: Track = { ...source, id: "destination-copy", path: "/music/Road trip/The Example - All Night.mp3", favorite: true };
const playlists: Playlist[] = [{ id: "playlist:Road trip", name: "Road trip", path: "/music/Road trip", url: null, ordered_track_ids: [playlistCopy.id] }];

describe("PlaylistPicker", () => {
  it("highlights a playlist that already contains a copied version of the selected song", () => {
    const html = renderToStaticMarkup(<PlaylistPicker
      playlists={playlists}
      selectedTracks={[source]}
      tracksById={new Map([[playlistCopy.id, playlistCopy]])}
      onPick={() => undefined}
      onDismiss={() => undefined}
    />);
    expect(html).toContain("Already contains this song");
    expect(html).toContain("rgba(var(--luno-accent-rgb),0.12)");
    expect(html).toContain('aria-label="Road trip, 1 song, Already contains this song"');
  });

  it("matches normalized artist and title while keeping artistless songs unambiguous", () => {
    expect(playlistMembershipKey({ ...source, artist: "Thé Example", title: "All-Night" })).toBe(playlistMembershipKey(source));
    expect(playlistMembershipKey({ ...source, artist: "Музыкант", title: "夜" })).toBe("музыкант\u0000夜");
    expect(playlistMembershipKey({ ...source, artist: null })).toBeNull();
  });
});

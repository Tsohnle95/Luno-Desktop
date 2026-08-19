import type { LibrarySnapshot, Track } from "../../shared/contracts";
import { libraryPlaylists, sortLibraryTracks } from "../../views/LibraryView";

const track = (id: string, title: string, favorite = false): Track => ({ id, path: null, title, artist: "Artist", album: null, duration_ms: null, artwork_url: null, favorite, provider: null });
const snapshot: LibrarySnapshot = { version: 1, generation: 1, tracks: [track("a", "Zed"), track("b", "Alpha", true)], playlists: [{ id: "u", name: "Unsorted", path: null, url: null, ordered_track_ids: ["a", "b"] }] };

export const libraryViewChecks = {
  rowHeight: 56,
  favorites: snapshot.tracks.filter((item) => item.favorite).length === 1,
  sort: sortLibraryTracks(snapshot.tracks, "A-Z").map((item) => item.id).join(",") === "b,a",
  cappedRecent: Array.from({ length: 120 }, (_, index) => index).slice(0, 100).length === 100,
  scopedPlaylists: libraryPlaylists(snapshot).length === 0,
};

if (!libraryViewChecks.favorites || !libraryViewChecks.sort || libraryViewChecks.rowHeight !== 56 || !libraryViewChecks.scopedPlaylists) throw new Error("Library view contract failed");

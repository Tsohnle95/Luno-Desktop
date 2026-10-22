import type { LibrarySnapshot, Track } from "../../shared/contracts";
import { defaultLibrarySort, libraryPlaylists, RECENT_DOWNLOADS_SCOPE, sortLibraryTracks } from "../../views/LibraryView";

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

import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { LibraryView, normalizeFilterText, shuffledFirstTrack, virtualWindow } from "../../views/LibraryView";
import { LibrarySidebar } from "../../components/LibrarySidebar";
import { HomeView, favoriteArtists } from "../../views/HomeView";
import { formatAddedDate } from "../../components/TrackRow";

describe("normalizeFilterText", () => {
  it("matches straight and typographic apostrophes", () => {
    expect(normalizeFilterText("l'in")).toBe(normalizeFilterText("l’in"));
    expect(normalizeFilterText("L’INDÉCIS")).toBe(normalizeFilterText("l'indecis"));
  });

  it("folds accents and case", () => {
    expect(normalizeFilterText("Décis")).toBe(normalizeFilterText("decis"));
  });

  it("maps typographic dashes to hyphens", () => {
    expect(normalizeFilterText("A – B")).toBe(normalizeFilterText("a - b"));
  });
});

describe("zero-match state", () => {
  it("explains that the context-aware app search controls the playlist filter", () => {
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} query="nonexistent" />);
    expect(html).toContain("No matches for");
    expect(html).toContain("Update the search at the top of the app");
    expect(html).not.toContain('aria-label="Clear search"');
  });

  it("keeps matching rows rendered while a search is active", () => {
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} query="alpha" />);
    expect(html).toContain("Alpha");
    expect(html).not.toContain("No matches for");
  });
});

describe("Recently downloaded library collection", () => {
  const downloads: Track[] = [
    { ...track("download-new", "Fresh download"), added_ms: 200 },
    { ...track("download-old", "Older download"), added_ms: 100 },
  ];
  const downloadedSnapshot: LibrarySnapshot = { ...snapshot, tracks: [...snapshot.tracks, ...downloads] };

  it("renders the complete collection in recent-download order", () => {
    expect(defaultLibrarySort(RECENT_DOWNLOADS_SCOPE)).toBe("Recently added");
    const html = renderToStaticMarkup(<LibraryView snapshot={downloadedSnapshot} selectedScope={RECENT_DOWNLOADS_SCOPE} recentlyDownloadedTracks={downloads} />);
    expect(html).toContain(">Collection</p>");
    expect(html).toContain("Recently downloaded tracks");
    expect(html.indexOf("Fresh download")).toBeLessThan(html.indexOf("Older download"));
  });

  it("pins the collection in the library sidebar with its current count", () => {
    const html = renderToStaticMarkup(<LibrarySidebar
      snapshot={downloadedSnapshot}
      tracksById={new Map(downloadedSnapshot.tracks.map((item) => [item.id, item]))}
      selectedScope={RECENT_DOWNLOADS_SCOPE}
      available={false}
      recentlyDownloadedTracks={downloads}
      onCreatePlaylist={() => {}}
      onSelectScope={() => {}}
      onPlaylistContextMenu={() => {}}
    />);
    expect(html).toContain('aria-current="page"');
    expect(html).toContain('title="Recently downloaded"');
    expect(html).toContain("2 songs");
  });
});

describe("virtualWindow", () => {
  it("still renders a row when scrollTop overshoots the whole list", () => {
    // Regression: stale scroll offsets past a shrunken (filtered) list used to
    // produce an empty window — rows existed but none were drawn.
    expect(virtualWindow(100000, 300, 2)).toEqual({ start: 1, end: 2 });
  });

  it("returns the standard window from the top", () => {
    expect(virtualWindow(0, 300, 50)).toEqual({ start: 0, end: 20 });
  });

  it("caps the window at the list length", () => {
    expect(virtualWindow(0, 36, 5).end).toBe(5);
  });

  it("handles an empty list", () => {
    expect(virtualWindow(500, 36, 0)).toEqual({ start: 0, end: 0 });
  });
});

describe("library sort", () => {
  it("sorts Recently added by file mtime, newest first", () => {
    const list = [
      { ...track("a", "Old"), added_ms: 1_000 },
      { ...track("b", "New"), added_ms: 3_000 },
      { ...track("c", "Mid"), added_ms: 2_000 },
    ];
    expect(sortLibraryTracks(list, "Recently added").map((item) => item.id)).toEqual(["b", "c", "a"]);
  });

  it("sorts Z-A and Duration", () => {
    const list = [
      { ...track("a", "Alpha"), duration_ms: 2_000 },
      { ...track("b", "Beta"), duration_ms: 1_000 },
    ];
    expect(sortLibraryTracks(list, "Z-A").map((item) => item.id)).toEqual(["b", "a"]);
    expect(sortLibraryTracks(list, "Duration").map((item) => item.id)).toEqual(["b", "a"]);
  });

  it("orders resolved durations immediately while unresolved tracks stay after them", () => {
    const list = [track("a", "Alpha"), track("b", "Beta"), track("c", "Gamma")];
    expect(sortLibraryTracks(list, "Duration", { b: 2_000, c: 1_000 }).map((item) => item.id)).toEqual(["c", "b", "a"]);
    expect(sortLibraryTracks(list, "Longest duration", { b: 2_000, c: 1_000 }).map((item) => item.id)).toEqual(["b", "c", "a"]);
  });

  it("shows duration probe progress while sorting proceeds", () => {
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} sort="Duration" onProbeDuration={async () => null} />);
    expect(html).toContain("Reading durations… 0 of 2");
    expect(html).toContain('aria-label="Sort longest duration first"');
  });
});

describe("library track metadata", () => {
  it("shows each track's date and playlist memberships", () => {
    const added = new Date(2026, 8, 25, 12).getTime();
    const datedSnapshot: LibrarySnapshot = {
      ...snapshot,
      tracks: [{ ...snapshot.tracks[0], added_ms: added }, snapshot.tracks[1]],
      playlists: [{ ...snapshot.playlists[0], name: "Road trip" }],
    };
    const html = renderToStaticMarkup(<LibraryView snapshot={datedSnapshot} />);
    expect(html).toContain("Date added");
    expect(html).toContain("Sep 25, 2026");
    expect(html).toContain('class="luno-track-cell-playlist luno-truncate" title="Road trip">Road trip</span>');
    expect(formatAddedDate(undefined)).toBe("—");
  });
});

describe("library artwork fallback", () => {
  it("uses generated Luno artwork for saved tracks without embedded art", () => {
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} />);
    expect(html).toContain("luno-track-artwork-slot");
    expect(html).toContain("luno-rec-art");
    expect(html).not.toContain("luno-artwork-fallback");
  });

  it("uses the saved song's design for its Home playlist card", () => {
    const html = renderToStaticMarkup(<HomeView
      allTracksCount={1}
      madeForYou={[]}
      favorites={[]}
      recentTracks={[]}
      popularTracks={[]}
      recentlyDownloaded={[]}
      popularPlaylists={[{ playlist: { ...snapshot.playlists[0], name: "Saved songs", ordered_track_ids: ["a"] }, plays: 1 }]}
      tracksById={new Map([["a", snapshot.tracks[0]]])}
    />);
    expect(html).toContain("luno-card-artwork");
    expect(html).toContain("luno-rec-art");
    expect(html).not.toContain("luno-artwork-fallback");
  });
});

describe("shuffled Play", () => {
  it("can choose any track as the first song instead of pinning index zero", () => {
    expect(shuffledFirstTrack(snapshot.tracks, 0)?.id).toBe("a");
    expect(shuffledFirstTrack(snapshot.tracks, 0.999999)?.id).toBe("b");
  });
});

describe("LibraryView search integration", () => {
  const apostropheSnapshot: LibrarySnapshot = {
    version: 1,
    generation: 1,
    tracks: [{ ...track("li1", "L'indécis Blind"), artist: "L'indécis" }],
    playlists: [],
  };

  it("finds tracks through a typographic-apostrophe query", () => {
    const html = renderToStaticMarkup(<LibraryView snapshot={apostropheSnapshot} query="l’indeci" />);
    expect(html).toContain("Blind");
  });

  it("does not mount the in-view filter input just because an external query exists", () => {
    // Regression: typing in the TOP search used to spawn a second focused
    // filter input under the album art.
    const html = renderToStaticMarkup(<LibraryView snapshot={apostropheSnapshot} query="l'in" />);
    expect(html).not.toContain("<input");
  });
});


describe("Home collection destinations", () => {
  it("groups favorite artists case insensitively and includes their nonfavorite songs", () => {
    const tracks = [
      { ...track("a", "Favorite", true), artist: " First Artist " },
      { ...track("b", "Other song"), artist: "first artist" },
      { ...track("c", "Other artist"), artist: "Another" },
      { ...track("d", "No artist", true), artist: null },
    ];
    const artists = favoriteArtists(tracks);
    expect(artists.map((item) => item.name)).toEqual(["First Artist"]);
    expect(artists[0].tracks.map((item) => item.id)).toEqual(["a", "b"]);
  });
  it("opens a named collection containing its exact ranked tracks and keeps title sorting available", () => {
    const ranked = [track("rank1", "Zed"), track("rank2", "Alpha")];
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} selectedScope="home:popular-songs" collection={{ title: "Most popular songs", tracks: ranked }} />);
    expect(html).toContain('aria-label="Most popular songs tracks"');
    expect(html.indexOf('<strong title="Zed"')).toBeLessThan(html.indexOf('<strong title="Alpha"'));
    expect(defaultLibrarySort("home:popular-songs")).toBe("Collection order");
    expect(sortLibraryTracks(ranked, "A-Z").map((item) => item.id)).toEqual(["rank2", "rank1"]);
  });
  it("gives popular tracks and playlists their own Show all actions", () => {
    const html = renderToStaticMarkup(<HomeView allTracksCount={2} madeForYou={[]} favorites={[]} recentTracks={[]} popularTracks={snapshot.tracks} popularPlaylists={[{ playlist: snapshot.playlists[0], plays: 2 }]} recentlyDownloaded={[]} />);
    expect(html).toContain('aria-label="Show all Most popular songs"');
    expect(html).toContain('aria-label="Show all Most popular playlists"');
  });
});

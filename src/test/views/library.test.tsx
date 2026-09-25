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

import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { LibraryView, normalizeFilterText, shuffledFirstTrack, virtualWindow } from "../../views/LibraryView";
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
  it("offers a Clear search action instead of a dead end", () => {
    // The recurring "clicked a song and now there are no matches" reports
    // all end on this screen; it must be one click away from recovery.
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} query="nonexistent" />);
    expect(html).toContain("No matches for");
    expect(html).toContain('aria-label="Clear search"');
  });

  it("keeps matching rows rendered while a search is active", () => {
    const html = renderToStaticMarkup(<LibraryView snapshot={snapshot} query="alpha" />);
    expect(html).toContain("Alpha");
    expect(html).not.toContain("No matches for");
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

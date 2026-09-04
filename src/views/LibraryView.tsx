import { useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent, type UIEvent } from "react";
import type { LibrarySnapshot, PlaybackSnapshot, Playlist, Track } from "../shared/contracts";
import { TrackRow } from "../components/TrackRow";
import { IconClock, IconClose, IconPlay, IconRefresh, IconSearch, IconShuffle } from "../components/icons";
import { useTrackArtwork, type ArtworkFetcher } from "../state/track_artwork";

export type LibraryScope = "all" | "unsorted" | "favorites" | string;
export type LibrarySort = "A-Z" | "Z-A" | "Recently added" | "Duration";

/** Playlist navigation entries shared by the shell sidebar and the view header. */
export function libraryPlaylists(snapshot: LibrarySnapshot | null | undefined): Playlist[] {
  return (snapshot?.playlists ?? []).filter((playlist) => playlist.name.toLowerCase() !== "unsorted").sort((a, b) => a.name.localeCompare(b.name));
}

export function playlistTracks(snapshot: LibrarySnapshot | null | undefined, playlist: Playlist): Track[] {
  return playlist.ordered_track_ids.map((id) => snapshot?.tracks.find((track) => track.id === id)).filter((track): track is Track => !!track);
}

/** Client-side album groups derived from track metadata (sidebar Albums chip). */
export function libraryAlbums(snapshot: LibrarySnapshot | null | undefined): Array<{ name: string; tracks: Track[] }> {
  const groups = new Map<string, Track[]>();
  for (const track of snapshot?.tracks ?? []) {
    const album = track.album?.trim();
    if (!album) continue;
    const bucket = groups.get(album);
    if (bucket) bucket.push(track);
    else groups.set(album, [track]);
  }
  return [...groups.entries()].sort((a, b) => a[0].localeCompare(b[0])).map(([name, tracks]) => ({ name, tracks }));
}

export const ALBUM_SCOPE_PREFIX = "album:";
export const albumScope = (albumName: string): LibraryScope => `${ALBUM_SCOPE_PREFIX}${albumName}`;

/**
 * Fold text for filter comparison: macOS smart punctuation turns a typed '
 * into U+2019 (so "l'in" never matched "L'indécis"), accents and dash
 * variants should not block matches either. Applied to BOTH query and row
 * text before the substring check.
 */
export function normalizeFilterText(value: string): string {
  return value
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[\u2018\u2019\u201c\u201d]/g, "'")
    .replace(/[\u2013\u2014]/g, "-")
    .trim()
    .replace(/\s+/g, " ");
}

/** Deterministic gradient pick so every scope keeps a stable hero color. */
export function scopeGradient(name: string): string {
  let hash = 0;
  for (const char of name) hash = ((hash * 31) + char.charCodeAt(0)) >>> 0;
  return `var(--luno-gradient-${hash % 10})`;
}

/**
 * Pure virtualized-window math so the clamp behavior is unit-testable.
 * `scrollTop` is the live page scroll, `anchorHeight` is hero + column head,
 * `count` is the (possibly tiny) filtered list length. The window ALWAYS
 * contains at least min(count, 1) rows when count > 0: a stale scroll offset
 * past a shrunken list used to render an empty window over existing rows —
 * songs "disappeared" while still present and playable.
 */
export function virtualWindow(scrollTop: number, anchorHeight: number, count: number, overscan = 20): { start: number; end: number } {
  if (count <= 0) return { start: 0, end: 0 };
  const localTop = Math.max(0, scrollTop - anchorHeight);
  const start = Math.max(0, Math.min(Math.floor(localTop / 56), count - 1));
  return { start, end: Math.min(start + overscan, count) };
}

export interface LibraryViewProps {
  snapshot?: LibrarySnapshot | null;
  playback?: PlaybackSnapshot | null;
  loading?: boolean;
  error?: string | null;
  selectedScope?: LibraryScope;
  onScopeChange?: (scope: LibraryScope) => void;
  /** Controlled filter text; falls back to internal state when omitted. */
  query?: string | null;
  onQueryChange?: (query: string) => void;
  onPlay?: (track: Track, context: Track[]) => void;
  onShuffle?: (tracks: Track[]) => void;
  onFavoriteToggle?: (track: Track) => void;
  onScan?: () => void;
  onContextMenu?: (event: MouseEvent, track: Track) => void;
  onArtwork?: ArtworkFetcher;
  backendAvailable?: boolean;
}

function scopeTracks(snapshot: LibrarySnapshot | null | undefined, scope: LibraryScope): Track[] {
  if (!snapshot) return [];
  if (scope === "all") return snapshot.tracks;
  if (scope === "favorites") return snapshot.tracks.filter((track) => track.favorite);
  if (scope === "unsorted") {
    const playlist = snapshot.playlists.find((item) => item.name.toLowerCase() === "unsorted");
    return playlist ? playlist.ordered_track_ids.map((id) => snapshot.tracks.find((track) => track.id === id)).filter((track): track is Track => !!track) : [];
  }
  if (scope.startsWith(ALBUM_SCOPE_PREFIX)) {
    const album = scope.slice(ALBUM_SCOPE_PREFIX.length);
    return snapshot.tracks.filter((track) => track.album === album);
  }
  const playlist = snapshot.playlists.find((item) => item.id === scope || item.name === scope);
  return playlist ? playlist.ordered_track_ids.map((id) => snapshot.tracks.find((track) => track.id === id)).filter((track): track is Track => !!track) : [];
}

export function sortLibraryTracks(tracks: Track[], sort: LibrarySort): Track[] {
  return [...tracks].sort((a, b) => {
    if (sort === "Duration") return (a.duration_ms ?? 0) - (b.duration_ms ?? 0);
    const left = `${a.artist ?? ""} ${a.title}`.toLocaleLowerCase();
    const right = `${b.artist ?? ""} ${b.title}`.toLocaleLowerCase();
    return sort === "Z-A" ? right.localeCompare(left) : left.localeCompare(right);
  });
}

/** Pick the first song for shuffled Play. WebCrypto avoids the short-period,
 * implementation-defined Math.random stream that made large libraries feel
 * repetitive; injected values keep the selection deterministic in tests. */
export function shuffledFirstTrack(tracks: Track[], randomValue?: number): Track | undefined {
  if (tracks.length === 0) return undefined;
  let unit = randomValue;
  if (unit === undefined) {
    const values = new Uint32Array(1);
    if (typeof crypto !== "undefined" && crypto.getRandomValues) {
      crypto.getRandomValues(values);
      unit = values[0] / 0x1_0000_0000;
    } else {
      unit = Math.random();
    }
  }
  const index = Math.min(tracks.length - 1, Math.max(0, Math.floor(unit * tracks.length)));
  return tracks[index];
}

function formatTotal(ms: number) {
  if (ms <= 0) return "";
  const hours = Math.floor(ms / 3_600_000);
  const minutes = Math.round((ms % 3_600_000) / 60_000);
  return hours > 0 ? `, ${hours} hr ${minutes} min` : minutes > 0 ? `, ${minutes} min` : "";
}

/**
 * The whole view scrolls as one page; the hero (cover art + title + actions)
 * and the column head are a sticky "anchor" block pinned to the top while the
 * song rows slide beneath it. The virtualization window therefore subtracts
 * the measured anchor height from the live scroll position.
 */
export function LibraryView({ snapshot, playback, loading = false, error = null, selectedScope = "all", onScopeChange, query: externalQuery, onQueryChange, onPlay, onShuffle, onFavoriteToggle, onScan, onContextMenu, onArtwork, backendAvailable = true }: LibraryViewProps) {
  const [internalQuery, setInternalQuery] = useState("");
  const query = externalQuery != null ? externalQuery : internalQuery;
  const setQuery = (value: string) => { setInternalQuery(value); onQueryChange?.(value); };
  const [sort, setSort] = useState<LibrarySort>(selectedScope === "unsorted" ? "Recently added" : "A-Z");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [scrollTop, setScrollTop] = useState(0);
  const [filterOpen, setFilterOpen] = useState(false);
  // The in-view filter box opens ONLY via its own toggle button. Auto-mounting
  // it whenever the TOP search had text (`query !== ""`) spawned a phantom
  // focused input under the hero artwork and stole focus mid-typing.
  const showFilter = filterOpen;
  // Fold smart punctuation / accents on BOTH sides before matching so a typed
  // "l'in" (macOS inserts U+2019) still finds "L'indécis".
  const needle = useMemo(() => normalizeFilterText(query), [query]);
  const tracks = useMemo(() => {
    const filtered = scopeTracks(snapshot, selectedScope).filter((track) => normalizeFilterText(`${track.title} ${track.artist ?? ""} ${track.album ?? ""}`).includes(needle));
    return sort === "Recently added" ? filtered : sortLibraryTracks(filtered, sort);
  }, [snapshot, selectedScope, needle, sort]);
  // Unfiltered scope size: the hero must report the LIBRARY size even when the
  // active query matches nothing — a zero-match filter previously made the hero
  // read "0 songs" and the empty state claim "No songs in All Music", which
  // looked exactly like the library had been wiped.
  const scopeSize = useMemo(() => scopeTracks(snapshot, selectedScope).length, [snapshot, selectedScope]);
  const playlists = libraryPlaylists(snapshot);
  const playing = playback?.current_index == null ? null : playback.queue[playback.current_index];
  const isAlbumScope = selectedScope.startsWith(ALBUM_SCOPE_PREFIX);
  const scopeName = selectedScope === "all"
    ? "All Music"
    : selectedScope === "favorites"
      ? "Favorites"
      : selectedScope === "unsorted"
        ? "Unsorted"
        : isAlbumScope
          ? selectedScope.slice(ALBUM_SCOPE_PREFIX.length)
          : playlists.find((item) => item.id === selectedScope || item.name === selectedScope)?.name ?? String(selectedScope);
  const eyebrow = isAlbumScope ? "Album" : selectedScope === "all" || selectedScope === "unsorted" ? "Library" : "Playlist";
  const gradient = scopeGradient(scopeName);

  // Track the live DOM scroll position so the virtualization window always
  // reflects where the scroller actually is, never a stale event-time capture.
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const latestScrollRef = useRef(0);
  const scrollFrame = useRef<number | null>(null);

  // The sticky anchor (hero + column head) height feeds the row-window math.
  // The hero wrapper is measured; the column head below it has a fixed height.
  const anchorRef = useRef<HTMLDivElement | null>(null);
  const [anchorHeight, setAnchorHeight] = useState(36);
  useEffect(() => {
    const element = anchorRef.current;
    if (!element) return;
    const update = () => setAnchorHeight(element.offsetHeight + 36);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // At most one state update per animation frame, but the value read inside the
  // frame is the freshest position captured at frame time (not event time).
  const scheduleScrollUpdate = () => {
    if (scrollFrame.current !== null) return;
    scrollFrame.current = requestAnimationFrame(() => {
      scrollFrame.current = null;
      setScrollTop(latestScrollRef.current);
    });
  };

  const handleScroll = (_event: UIEvent<HTMLDivElement>) => {
    latestScrollRef.current = scrollRef.current?.scrollTop ?? 0;
    scheduleScrollUpdate();
  };

  // Keep the DOM scroller and state in agreement when the list resets.
  useEffect(() => {
    setScrollTop(0);
    setSelected(new Set());
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
    latestScrollRef.current = 0;
  }, [selectedScope, query, sort]);
  useEffect(() => () => { if (scrollFrame.current !== null) cancelAnimationFrame(scrollFrame.current); }, []);

  // Clamp scrollTop into a defensible range derived from the full page height,
  // then translate it into the row coordinate space under the hero + head.
  const { start: startIndex, end: endIndex } = virtualWindow(scrollTop, anchorHeight, tracks.length);
  const visible = tracks.slice(startIndex, endIndex);
  const artwork = useTrackArtwork(visible.map((track) => track.id), onArtwork);
  const coverArtwork = useTrackArtwork(tracks.slice(0, 8).map((track) => track.id), onArtwork);
  const play = (track: Track) => { if (backendAvailable) onPlay?.(track, tracks); };
  const toggleSelection = (track: Track, event: MouseEvent) => {
    if (!event.shiftKey && !event.metaKey && !event.ctrlKey) return setSelected(new Set([track.id]));
    setSelected((current) => { const next = new Set(current); if (next.has(track.id)) next.delete(track.id); else next.add(track.id); return next; });
  };
  const totalMs = tracks.reduce((sum, track) => sum + (track.duration_ms ?? 0), 0);
  const covers = tracks.slice(0, 4);
  return <section className="luno-library-view" aria-label="Library" style={{ "--luno-scope-gradient": gradient } as CSSProperties}>
    <div ref={scrollRef} role="list" aria-label={`${scopeName} tracks`} className="luno-track-scroller" onScroll={handleScroll}>
      <div ref={anchorRef}>
        <header className="luno-hero">
          <div className="luno-hero-row">
            <div className="luno-hero-cover" aria-hidden="true">
              {covers.length === 0
                ? <span style={{ gridColumn: "1 / -1", gridRow: "1 / -1", display: "grid", placeItems: "center", fontSize: 48, color: "rgba(255,255,255,0.55)" }}>♫</span>
                : covers.map((track, index) => {
                  const src = coverArtwork[track.id] ?? track.artwork_url;
                  // Mosaic spans: 1 artwork fills the tile, 2 split columns, 3 put one full-height lead column.
                  const span = covers.length === 1 ? { gridColumn: "1 / -1", gridRow: "1 / -1" } : covers.length === 2 ? { gridRow: "1 / -1" } : covers.length === 3 && index === 0 ? { gridRow: "1 / -1" } : {};
                  const background = src ? undefined : "rgba(255,255,255,0.08)";
                  return src
                    ? <img key={track.id} src={src} alt="" decoding="async" style={span} />
                    : <span key={track.id} style={{ ...span, background, display: "grid", placeItems: "center", fontSize: 24 }}>♫</span>;
                })}
            </div>
            <div className="luno-hero-info">
              <p className="luno-hero-eyebrow">{eyebrow}</p>
              <h1>{scopeName}</h1>
              <p className="luno-hero-meta"><strong>Luno</strong> • {scopeSize} song{scopeSize === 1 ? "" : "s"}{formatTotal(totalMs)}{query && tracks.length !== scopeSize ? ` • ${tracks.length} match${tracks.length === 1 ? "" : "es"}` : ""}</p>
            </div>
          </div>
          <div className="luno-hero-actions">
            <button className="luno-fab" type="button" aria-label={`Play ${scopeName}`} disabled={!backendAvailable || tracks.length === 0} onClick={() => { const first = playback?.shuffle ? shuffledFirstTrack(tracks) : tracks[0]; if (backendAvailable && first) onPlay?.(first, tracks); }}><IconPlay size={18} /></button>
            <button className="luno-action-toggle" type="button" aria-label={playback?.shuffle ? "Disable shuffle" : "Enable shuffle"} aria-pressed={!!playback?.shuffle} disabled={!backendAvailable || !onShuffle} onClick={() => onShuffle?.(tracks)}><IconShuffle size={17} /></button>
            <button className="luno-action-toggle" type="button" aria-label="Scan library for changes" disabled={!backendAvailable} onClick={() => onScan?.()}><IconRefresh size={16} /></button>
            {selected.size > 0 && <span className="luno-selection-count">{selected.size} selected</span>}
            <div className="luno-hero-right">
              <button className="luno-action-toggle" type="button" aria-label="Filter tracks" aria-expanded={showFilter} onClick={() => { setFilterOpen((open) => !open); }}>{showFilter ? <IconClose size={15} /> : <IconSearch size={15} />}</button>
              <select className="luno-sort-select" aria-label="Sort tracks" value={sort} onChange={(event) => setSort(event.target.value as LibrarySort)}>
                <option>A-Z</option><option>Z-A</option><option>Recently added</option><option>Duration</option>
              </select>
            </div>
          </div>
        </header>
      </div>
      <div className="luno-table-head luno-track-grid-template">
        {showFilter
          ? <div className="luno-table-filter">
            <input autoFocus aria-label="Filter tracks" placeholder={`Search in ${scopeName}`} value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape") { setQuery(""); setFilterOpen(false); } }} />
          </div>
          : <><span className="luno-th-index">#</span><span aria-hidden="true"></span><span>Title</span><span className="luno-track-cell-album">Album</span><span className="luno-track-cell-playlist">Playlist</span><span aria-hidden="true"></span><span className="luno-th-duration" title="Duration" aria-label="Duration"><IconClock size={13} /></span></>}
      </div>
      {loading ? <p role="status" className="luno-track-empty">Loading library...</p>
        : error ? <div role="alert" className="luno-track-empty">{error}<button type="button" disabled={!backendAvailable} onClick={onScan}>Retry</button></div>
          : !snapshot ? <p className="luno-track-empty">Library is not available.</p>
            : tracks.length === 0 && scopeSize > 0 ? <p className="luno-track-empty">No matches for “{query}” in {scopeName} — showing all {scopeSize} songs once you clear the search. <button type="button" aria-label="Clear search" onClick={() => setQuery("")}>Clear search</button></p>
              : tracks.length === 0 ? <p className="luno-track-empty">No songs in {scopeName}.</p>
              : <div style={{ height: tracks.length * 56, position: "relative" }}>
                <div style={{ transform: `translateY(${startIndex * 56}px)` }}>{visible.map((track, index) => <div role="listitem" key={`${track.id}:${startIndex + index}`} onClick={(event) => toggleSelection(track, event)}><TrackRow track={track} artwork={artwork[track.id] ?? track.artwork_url} index={startIndex + index} playlistName={isAlbumScope ? track.album ?? scopeName : scopeName} selected={selected.has(track.id)} playing={playing === track.id} onClick={play} onFavoriteToggle={onFavoriteToggle} onContextMenu={onContextMenu} backendAvailable={backendAvailable} /></div>)}</div>
              </div>}
    </div>
  </section>;
}

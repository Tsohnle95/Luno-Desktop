import { useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent, type UIEvent } from "react";
import type { LibrarySnapshot, PlaybackSnapshot, Playlist, Track } from "../shared/contracts";
import { TrackRow } from "../components/TrackRow";
import { IconClock, IconMoreHorizontal, IconPlay, IconShuffle } from "../components/icons";
import { useTrackArtworkState, type ArtworkFetcher } from "../state/track_artwork";
import { cachedTrackDuration, requestTrackDuration } from "../state/track_durations";
import { PlaylistArtwork } from "../components/PlaylistArtwork";

export type LibraryScope = "all" | "unsorted" | "favorites" | string;
export type LibrarySort = "A-Z" | "Z-A" | "Recently added" | "Oldest added" | "Duration" | "Longest duration";
const EMPTY_DURATION_MAP: Readonly<Record<string, number | null | undefined>> = {};

export function defaultLibrarySort(scope: LibraryScope): LibrarySort {
  return scope === "unsorted" ? "Recently added" : "A-Z";
}

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
  /** Search text from the context-aware app search in the top bar. */
  query?: string | null;
  sort?: LibrarySort;
  onSortChange?: (sort: LibrarySort) => void;
  onPlay?: (track: Track, context: Track[], sourcePlaylistId?: string) => void;
  onShuffle?: (tracks: Track[]) => void;
  onFavoriteToggle?: (track: Track) => void;
  onScan?: () => void;
  onProbeDuration?: (trackId: Track["id"]) => Promise<number | null>;
  onContextMenu?: (event: MouseEvent, track: Track) => void;
  onArtwork?: ArtworkFetcher;
  onOpenArtwork?: (scope: LibraryScope) => void;
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

export function sortLibraryTracks(tracks: Track[], sort: LibrarySort, probedDurations: Readonly<Record<string, number | null | undefined>> = {}): Track[] {
  const durationFor = (track: Track) => Object.prototype.hasOwnProperty.call(probedDurations, track.id)
    ? probedDurations[track.id]
    : track.duration_ms;
  const titleFor = (track: Track) => track.title.toLocaleLowerCase();
  const alphabetical = (a: Track, b: Track) => {
    const titleOrder = titleFor(a).localeCompare(titleFor(b));
    if (titleOrder !== 0) return titleOrder;
    return (a.artist ?? "").toLocaleLowerCase().localeCompare((b.artist ?? "").toLocaleLowerCase());
  };
  return [...tracks].sort((a, b) => {
    if (sort === "Duration" || sort === "Longest duration") {
      const left = durationFor(a);
      const right = durationFor(b);
      if (left == null && right != null) return 1;
      if (left != null && right == null) return -1;
      if (left != null && right != null && left !== right) return sort === "Duration" ? left - right : right - left;
      return alphabetical(a, b);
    }
    if (sort === "Recently added" || sort === "Oldest added") {
      const left = a.added_ms;
      const right = b.added_ms;
      if (left == null && right != null) return 1;
      if (left != null && right == null) return -1;
      if (left != null && right != null && left !== right) return sort === "Recently added" ? right - left : left - right;
    }
    return sort === "Z-A" ? alphabetical(b, a) : alphabetical(a, b);
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
export function LibraryView({ snapshot, playback, loading = false, error = null, selectedScope = "all", onScopeChange, query: externalQuery, sort: controlledSort, onSortChange, onPlay, onShuffle, onFavoriteToggle, onScan, onProbeDuration, onContextMenu, onArtwork, onOpenArtwork, backendAvailable = true }: LibraryViewProps) {
  const query = externalQuery ?? "";
  const [internalSort, setInternalSort] = useState<LibrarySort>(() => defaultLibrarySort(selectedScope));
  const sort = controlledSort ?? internalSort;
  const setSort = (next: LibrarySort) => {
    if (controlledSort == null) setInternalSort(next);
    onSortChange?.(next);
  };
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [probedDurations, setProbedDurations] = useState<Record<string, number | null | undefined>>({});
  const requestedDurations = useRef(new Set<string>());
  const activeDurationGeneration = useRef(snapshot?.generation ?? null);
  const pendingDurationResults = useRef<Record<string, number | null>>({});
  const durationResultFrame = useRef<number | null>(null);
  const probeDurationRef = useRef(onProbeDuration);
  const durationProbeMounted = useRef(false);
  probeDurationRef.current = onProbeDuration;
  const [scrollTop, setScrollTop] = useState(0);
  const [artworkMenuOpen, setArtworkMenuOpen] = useState(false);
  const artworkMenuRef = useRef<HTMLDivElement | null>(null);
  const needle = useMemo(() => normalizeFilterText(query), [query]);
  const filteredTracks = useMemo(() => {
    return scopeTracks(snapshot, selectedScope).filter((track) => normalizeFilterText(`${track.title} ${track.artist ?? ""} ${track.album ?? ""}`).includes(needle));
  }, [snapshot, selectedScope, needle]);
  const durationSort = sort === "Duration" || sort === "Longest duration";
  // Apply each known result immediately. Holding an old title sort until every
  // unknown track is probed made the duration header look broken on large lists.
  const durationValues = useMemo(() => {
    if (!durationSort) return EMPTY_DURATION_MAP;
    const values = { ...probedDurations };
    for (const track of filteredTracks) {
      if (track.duration_ms != null || Object.prototype.hasOwnProperty.call(values, track.id)) continue;
      const cached = cachedTrackDuration(track.id);
      if (cached) values[track.id] = cached.value;
    }
    return values;
  }, [durationSort, filteredTracks, probedDurations]);
  const tracks = useMemo(() => {
    return sortLibraryTracks(filteredTracks, sort, durationValues);
  }, [filteredTracks, sort, durationValues]);
  const playlistNamesByTrackId = useMemo(() => {
    const names = new Map<string, string[]>();
    for (const playlist of snapshot?.playlists ?? []) {
      for (const trackId of playlist.ordered_track_ids) {
        const trackNames = names.get(trackId);
        if (trackNames) trackNames.push(playlist.name);
        else names.set(trackId, [playlist.name]);
      }
    }
    return new Map([...names].map(([trackId, values]) => [trackId, values.join(", ")]));
  }, [snapshot]);
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
  const canOpenArtworkManager = !!onOpenArtwork && !isAlbumScope && (selectedScope === "all" || selectedScope === "favorites" || selectedScope === "unsorted" || !!snapshot?.playlists.some((playlist) => playlist.id === selectedScope));

  useEffect(() => {
    if (!artworkMenuOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!artworkMenuRef.current?.contains(event.target as Node)) setArtworkMenuOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") setArtworkMenuOpen(false); };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [artworkMenuOpen]);

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

  // Uncontrolled embeds still get a useful default on scope changes. The app
  // Shell controls this value so it survives route changes such as Downloader.
  useEffect(() => {
    if (controlledSort == null) setInternalSort(defaultLibrarySort(selectedScope));
  }, [selectedScope, controlledSort]);
  // Keep the DOM scroller and state in agreement when the list resets.
  useEffect(() => {
    setScrollTop(0);
    setSelected(new Set());
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
    latestScrollRef.current = 0;
  }, [selectedScope, query, sort]);
  useEffect(() => {
    durationProbeMounted.current = true;
    return () => {
      durationProbeMounted.current = false;
      if (scrollFrame.current !== null) cancelAnimationFrame(scrollFrame.current);
      if (durationResultFrame.current !== null) cancelAnimationFrame(durationResultFrame.current);
    };
  }, []);

  // Clamp scrollTop into a defensible range derived from the full page height,
  // then translate it into the row coordinate space under the hero + head.
  const { start: startIndex, end: endIndex } = virtualWindow(scrollTop, anchorHeight, tracks.length);
  const visible = useMemo(() => tracks.slice(startIndex, endIndex), [tracks, startIndex, endIndex]);
  useEffect(() => {
    const generation = snapshot?.generation ?? null;
    if (activeDurationGeneration.current === generation) return;
    activeDurationGeneration.current = generation;
    requestedDurations.current.clear();
    pendingDurationResults.current = {};
    if (durationResultFrame.current !== null) cancelAnimationFrame(durationResultFrame.current);
    durationResultFrame.current = null;
    setProbedDurations((current) => {
      const next: Record<string, number | null | undefined> = {};
      for (const track of snapshot?.tracks ?? []) {
        const cached = cachedTrackDuration(track.id);
        if (cached) next[track.id] = cached.value;
        else if (Object.prototype.hasOwnProperty.call(current, track.id) && current[track.id] !== undefined) next[track.id] = current[track.id];
      }
      return next;
    });
  }, [snapshot?.generation]);
  const durationProbeTargets = durationSort ? filteredTracks : visible;
  useEffect(() => {
    const probeDuration = probeDurationRef.current;
    if (!probeDuration) return;
    const generation = snapshot?.generation ?? null;
    const pending = durationProbeTargets.filter((track) => track.duration_ms == null && !requestedDurations.current.has(track.id));
    if (pending.length === 0) return;
    for (const track of pending) requestedDurations.current.add(track.id);
    setProbedDurations((current) => {
      const next = { ...current };
      for (const track of pending) {
        const cached = cachedTrackDuration(track.id);
        next[track.id] = cached ? cached.value : undefined;
      }
      return next;
    });
    const queueResult = (trackId: string, durationMs: number | null) => {
      if (!durationProbeMounted.current || activeDurationGeneration.current !== generation) return;
      pendingDurationResults.current[trackId] = durationMs;
      if (durationResultFrame.current !== null) return;
      durationResultFrame.current = requestAnimationFrame(() => {
        durationResultFrame.current = null;
        const results = pendingDurationResults.current;
        pendingDurationResults.current = {};
        if (durationProbeMounted.current && activeDurationGeneration.current === generation) setProbedDurations((current) => ({ ...current, ...results }));
      });
    };
    let cursor = 0;
    let cancelled = false;
    const started = new Set<string>();
    const worker = async () => {
      while (!cancelled && cursor < pending.length) {
        const track = pending[cursor++];
        started.add(track.id);
        try {
          queueResult(track.id, await requestTrackDuration(track.id, probeDuration));
        } catch {
          queueResult(track.id, null);
        }
      }
    };
    void Promise.all(Array.from({ length: Math.min(4, pending.length) }, () => worker()));
    return () => {
      cancelled = true;
      for (const track of pending) if (!started.has(track.id)) requestedDurations.current.delete(track.id);
    };
  }, [durationProbeTargets, onProbeDuration != null, snapshot?.generation]);
  const artworkState = useTrackArtworkState(visible.map((track) => track.id), onArtwork, 2);
  const coverArtworkState = useTrackArtworkState(tracks.slice(0, 8).map((track) => track.id), onArtwork);
  const artwork = artworkState.artwork;
  const coverArtwork = coverArtworkState.artwork;
  const sourcePlaylistId = snapshot?.playlists.find((playlist) => playlist.id === selectedScope)?.id;
  const play = (track: Track) => { if (backendAvailable) onPlay?.(track, tracks, sourcePlaylistId); };
  const toggleSelection = (track: Track, event: MouseEvent) => {
    if (!event.shiftKey && !event.metaKey && !event.ctrlKey) return setSelected(new Set([track.id]));
    setSelected((current) => { const next = new Set(current); if (next.has(track.id)) next.delete(track.id); else next.add(track.id); return next; });
  };
  const totalMs = tracks.reduce((sum, track) => sum + (track.duration_ms ?? 0), 0);
  const covers = tracks.slice(0, 4);
  const titleSorted = sort === "A-Z" || sort === "Z-A";
  const dateSorted = sort === "Recently added" || sort === "Oldest added";
  const durationSorted = sort === "Duration" || sort === "Longest duration";
  const durationKnownCount = filteredTracks.filter((track) => track.duration_ms != null || (Object.prototype.hasOwnProperty.call(durationValues, track.id) && durationValues[track.id] !== undefined)).length;
  const durationSortComplete = !durationSorted || !onProbeDuration || durationKnownCount === filteredTracks.length;
  return <section className="luno-library-view" aria-label="Library" style={{ "--luno-scope-gradient": gradient } as CSSProperties}>
    <div ref={scrollRef} role="list" aria-label={`${scopeName} tracks`} className="luno-track-scroller" onScroll={handleScroll}>
      <div ref={anchorRef}>
        <header className="luno-hero">
          {canOpenArtworkManager && <div className="luno-artwork-menu-control" ref={artworkMenuRef}>
            <button className="luno-artwork-menu-trigger" type="button" aria-label="Artwork options" aria-expanded={artworkMenuOpen} title="Artwork options" onClick={() => setArtworkMenuOpen((open) => !open)}><IconMoreHorizontal size={18} /></button>
            {artworkMenuOpen && <div className="luno-artwork-menu" role="menu"><button type="button" role="menuitem" onClick={() => { setArtworkMenuOpen(false); onOpenArtwork?.(selectedScope); }}>Find missing song artwork</button></div>}
          </div>}
          <div className="luno-hero-row">
            <div className="luno-hero-cover-control">
              <PlaylistArtwork
                className="luno-hero-cover"
                tracks={covers}
                artwork={coverArtwork}
                loadingIds={coverArtworkState.loading}
                eager
                emptyFallback={<span style={{ display: "grid", placeItems: "center", fontSize: 48, color: "rgba(255,255,255,0.55)" }}>♫</span>}
              />
            </div>
            <div className="luno-hero-info">
              <p className="luno-hero-eyebrow">{eyebrow}</p>
              <h1>{scopeName}</h1>
              <p className="luno-hero-meta"><strong>Luno</strong> • {scopeSize} song{scopeSize === 1 ? "" : "s"}{formatTotal(totalMs)}{query && tracks.length !== scopeSize ? ` • ${tracks.length} match${tracks.length === 1 ? "" : "es"}` : ""}</p>
            </div>
          </div>
          <div className="luno-hero-actions">
            <button className="luno-fab" type="button" aria-label={`Play ${scopeName}`} disabled={!backendAvailable || tracks.length === 0} onClick={() => { const first = playback?.shuffle ? shuffledFirstTrack(tracks) : tracks[0]; if (first) play(first); }}><IconPlay size={18} /></button>
            <button className="luno-action-toggle" type="button" aria-label={playback?.shuffle ? "Disable shuffle" : "Enable shuffle"} aria-pressed={!!playback?.shuffle} disabled={!backendAvailable || !onShuffle} onClick={() => onShuffle?.(tracks)}><IconShuffle size={17} /></button>
          </div>
        </header>
      </div>
      <div className="luno-table-head luno-track-grid-template">
        <span className="luno-th-index">#</span>
        <span aria-hidden="true"></span>
        <button className="luno-sort-button" type="button" aria-label={sort === "A-Z" ? "Sort title Z to A" : "Sort title A to Z"} aria-pressed={titleSorted} onClick={() => setSort(sort === "A-Z" ? "Z-A" : "A-Z")}>Title{titleSorted && <span aria-hidden="true">{sort === "A-Z" ? "↑" : "↓"}</span>}</button>
        <span className="luno-track-cell-playlist">Playlist</span>
        <button className="luno-sort-button luno-track-cell-added" type="button" aria-label={sort === "Recently added" ? "Sort oldest added first" : "Sort latest added first"} aria-pressed={dateSorted} onClick={() => setSort(sort === "Recently added" ? "Oldest added" : "Recently added")}>Date added{dateSorted && <span aria-hidden="true">{sort === "Recently added" ? "↓" : "↑"}</span>}</button>
        <span aria-hidden="true"></span>
        <button className="luno-sort-button luno-th-duration" type="button" aria-label={sort === "Duration" ? "Sort longest duration first" : "Sort shortest duration first"} title={sort === "Duration" ? "Longest duration first" : "Shortest duration first"} aria-pressed={durationSorted} onClick={() => setSort(sort === "Duration" ? "Longest duration" : "Duration")}><IconClock size={13} />{durationSorted && <span aria-hidden="true">{sort === "Duration" ? "↑" : "↓"}</span>}</button>
      </div>
      {durationSorted && !durationSortComplete && <p className="luno-duration-sort-status" role="status" aria-live="polite">Reading durations… {durationKnownCount} of {filteredTracks.length}</p>}
      {loading ? <p role="status" className="luno-track-empty">Loading library...</p>
        : error ? <div role="alert" className="luno-track-empty">{error}<button type="button" disabled={!backendAvailable} onClick={onScan}>Retry</button></div>
          : !snapshot ? <p className="luno-track-empty">Library is not available.</p>
            : tracks.length === 0 && scopeSize > 0 ? <p className="luno-track-empty">No matches for “{query}” in {scopeName}. Update the search at the top of the app to see other songs.</p>
              : tracks.length === 0 ? <p className="luno-track-empty">No songs in {scopeName}.</p>
              : <div style={{ height: tracks.length * 56, position: "relative" }}>
                <div style={{ transform: `translateY(${startIndex * 56}px)` }}>{visible.map((track, index) => {
                  const hasProbedDuration = Object.prototype.hasOwnProperty.call(durationValues, track.id);
                  const durationMs = hasProbedDuration ? durationValues[track.id] : track.duration_ms;
                  return <div role="listitem" key={`${track.id}:${startIndex + index}`} onClick={(event) => toggleSelection(track, event)}><TrackRow track={track} artwork={artwork[track.id] ?? track.artwork_url} artworkLoading={artworkState.loading.has(track.id)} durationMs={durationMs} durationLoading={hasProbedDuration && durationMs === undefined} index={startIndex + index} playlistName={playlistNamesByTrackId.get(track.id) ?? "—"} selected={selected.has(track.id)} playing={playing === track.id} onClick={play} onFavoriteToggle={onFavoriteToggle} onContextMenu={onContextMenu} backendAvailable={backendAvailable} /></div>;
                })}</div>
              </div>}
    </div>
  </section>;
}

import { useEffect, useMemo, useRef, useState, type MouseEvent, type UIEvent } from "react";
import type { LibrarySnapshot, PlaybackSnapshot, Playlist, Track } from "../shared/contracts";
import { TrackRow } from "../components/TrackRow";
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

export interface LibraryViewProps {
  snapshot?: LibrarySnapshot | null;
  playback?: PlaybackSnapshot | null;
  loading?: boolean;
  error?: string | null;
  selectedScope?: LibraryScope;
  onScopeChange?: (scope: LibraryScope) => void;
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

export function LibraryView({ snapshot, playback, loading = false, error = null, selectedScope = "all", onScopeChange, onPlay, onShuffle, onFavoriteToggle, onScan, onContextMenu, onArtwork, backendAvailable = true }: LibraryViewProps) {
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<LibrarySort>(selectedScope === "unsorted" ? "Recently added" : "A-Z");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [scrollTop, setScrollTop] = useState(0);
  const tracks = useMemo(() => {
    const filtered = scopeTracks(snapshot, selectedScope).filter((track) => `${track.title} ${track.artist ?? ""} ${track.album ?? ""}`.toLocaleLowerCase().includes(query.toLocaleLowerCase()));
    return sort === "Recently added" ? filtered : sortLibraryTracks(filtered, sort);
  }, [snapshot, selectedScope, query, sort]);
  const playlists = libraryPlaylists(snapshot);
  const playing = playback?.current_index == null ? null : playback.queue[playback.current_index];
  const scopeName = selectedScope === "all" ? "All Music" : selectedScope === "favorites" ? "Favorites" : selectedScope === "unsorted" ? "Unsorted" : playlists.find((item) => item.id === selectedScope || item.name === selectedScope)?.name ?? String(selectedScope);

  // Track the live DOM scroll position so the virtualization window always
  // reflects where the scroller actually is, never a stale event-time capture.
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const latestScrollRef = useRef(0);
  const scrollFrame = useRef<number | null>(null);

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

  // Clamp scrollTop into a defensible range derived from the list height.
  const maxScrollTop = Math.max(0, tracks.length * 56 - 1);
  const clampedScrollTop = Math.max(0, Math.min(scrollTop, maxScrollTop));
  const startIndex = Math.floor(clampedScrollTop / 56);
  const overscan = 20;
  const visible = tracks.slice(startIndex, Math.ceil(clampedScrollTop / 56) + overscan);
  const artwork = useTrackArtwork(visible.map((track) => track.id), onArtwork);
  const play = (track: Track) => { if (backendAvailable) onPlay?.(track, tracks); };
  const toggleSelection = (track: Track, event: MouseEvent) => {
    if (!event.shiftKey && !event.metaKey && !event.ctrlKey) return setSelected(new Set([track.id]));
    setSelected((current) => { const next = new Set(current); if (next.has(track.id)) next.delete(track.id); else next.add(track.id); return next; });
  };
  return <section className="luno-library-view" aria-label="Library" style={{ height: "100%", display: "flex", flexDirection: "column" }}>
    <header className="luno-library-header" style={{ padding: "32px 32px 20px", background: "var(--luno-gradient-0)" }}>
      <p style={{ margin: 0, color: "var(--luno-text-2)" }}>Library</p><h1 style={{ margin: "8px 0 18px" }}>{scopeName}</h1>
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}><input aria-label="Filter tracks" placeholder="Filter tracks..." value={query} onChange={(event) => setQuery(event.target.value)} style={{ flex: 1, minWidth: 0, padding: 10, background: "#0008", color: "inherit", border: 0, borderRadius: 6 }} /><select aria-label="Sort tracks" value={sort} onChange={(event) => setSort(event.target.value as LibrarySort)}><option>A-Z</option><option>Z-A</option><option>Recently added</option><option>Duration</option></select></div>
    </header>
     <div style={{ display: "flex", gap: 8, padding: "12px 32px", borderBottom: "1px solid var(--luno-border)" }}><button type="button" disabled={!backendAvailable} onClick={() => onScan?.()}>Scan</button><button type="button" disabled={!backendAvailable} onClick={() => onShuffle?.([...tracks].sort(() => Math.random() - .5))}>Shuffle</button><span>{selected.size ? `${selected.size} selected` : `${tracks.length} songs`}</span></div>
    {loading ? <p role="status" style={{ padding: 32 }}>Loading library...</p> : error ? <div role="alert" style={{ padding: 32 }}>{error}<button type="button" disabled={!backendAvailable} onClick={onScan}>Retry</button></div> : !snapshot ? <p style={{ padding: 32 }}>Library is not available.</p> : tracks.length === 0 ? <p style={{ padding: 32 }}>No songs in {scopeName}.</p> : <div ref={scrollRef} role="list" aria-label={`${scopeName} tracks`} style={{ flex: 1, minHeight: 0, overflowY: "auto", overscrollBehavior: "contain", position: "relative" }} onScroll={handleScroll}>
       <div style={{ height: tracks.length * 56, position: "relative" }}><div style={{ transform: `translateY(${Math.floor(clampedScrollTop / 56) * 56}px)` }}>{visible.map((track, index) => <div role="listitem" key={track.id} onClick={(event) => toggleSelection(track, event)}><TrackRow track={track} artwork={artwork[track.id] ?? track.artwork_url} index={startIndex + index} playlistName={scopeName} selected={selected.has(track.id)} playing={playing === track.id} onClick={play} onFavoriteToggle={onFavoriteToggle} onContextMenu={onContextMenu} backendAvailable={backendAvailable} /></div>)}</div></div>
   </div>}
  </section>;
}

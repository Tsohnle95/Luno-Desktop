import { useState, type MouseEvent } from "react";
import type { PlaybackSnapshot, Track } from "../shared/contracts";
import { TrackRow } from "../components/TrackRow";
import { useTrackArtwork, type ArtworkFetcher } from "../state/track_artwork";

export interface RecentViewProps { tracks?: Track[]; playback?: PlaybackSnapshot | null; loading?: boolean; error?: string | null; onPlay?: (track: Track, context: Track[]) => void; onFavoriteToggle?: (track: Track) => void; onContextMenu?: (event: MouseEvent, track: Track) => void; onClear?: () => void; onArtwork?: ArtworkFetcher; backendAvailable?: boolean; }

export function RecentView({ tracks = [], playback, loading = false, error = null, onPlay, onFavoriteToggle, onContextMenu, onClear, onArtwork, backendAvailable = true }: RecentViewProps) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const recent = tracks.slice(0, 100);
  const artwork = useTrackArtwork(recent.map((track) => track.id), onArtwork);
  const current = playback?.current_index == null ? null : playback.queue[playback.current_index];
  const select = (track: Track, event: MouseEvent) => setSelected((old) => { if (!event.shiftKey && !event.metaKey && !event.ctrlKey) return new Set([track.id]); const next = new Set(old); next.has(track.id) ? next.delete(track.id) : next.add(track.id); return next; });
  return <section aria-label="Recently Played" className="luno-recent-view"><header style={{ padding: "32px", background: "var(--luno-gradient-1)" }}><p style={{ margin: 0, color: "var(--luno-text-2)" }}>History</p><h1 style={{ margin: "8px 0" }}>Recently Played</h1><span>{recent.length} of 100 session tracks</span></header><div style={{ padding: "12px 32px", display: "flex", gap: 8 }}><button type="button" onClick={onClear} disabled={!recent.length}>Clear History</button>{selected.size > 0 && <span>{selected.size} selected</span>}</div>{loading ? <p role="status" style={{ padding: 32 }}>Loading history...</p> : error ? <p role="alert" style={{ padding: 32 }}>{error}</p> : recent.length === 0 ? <p style={{ padding: 32 }}>No recently played songs.</p> : <div role="list">{recent.map((track, index) => <div role="listitem" key={`${track.id}-${index}`} onClick={(event) => select(track, event)}><TrackRow track={track} artwork={artwork[track.id] ?? track.artwork_url} index={index} playlistName="Recently Played" selected={selected.has(track.id)} playing={current === track.id} onClick={(item) => backendAvailable && onPlay?.(item, recent)} onFavoriteToggle={onFavoriteToggle} onContextMenu={onContextMenu} backendAvailable={backendAvailable} /></div>)}</div>}</section>;
}

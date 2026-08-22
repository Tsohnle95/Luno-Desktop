import type { MouseEvent } from "react";
import type { Track } from "../shared/contracts";
import { Artwork } from "./Artwork";
import { IconPlay } from "./icons";

export interface TrackRowProps {
  track: Track;
  index?: number;
  playlistName?: string | null;
  selected?: boolean;
  playing?: boolean;
  artwork?: string | null;
  onClick?: (track: Track) => void;
  onFavoriteToggle?: (track: Track) => void;
  onContextMenu?: (event: MouseEvent, track: Track) => void;
  backendAvailable?: boolean;
}

function truncate(value: string, max: number) {
  return value.length > max && max > 3 ? `${value.slice(0, max - 3)}...` : value;
}

export function formatDuration(ms: number | null | undefined) {
  if (ms == null || !Number.isFinite(ms)) return "-";
  const seconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

export function TrackRow({ track, index, playlistName, selected = false, playing = false, artwork, onClick, onFavoriteToggle, onContextMenu, backendAvailable = true }: TrackRowProps) {
  return <div className={`luno-track-row${selected ? " is-selected" : ""}${playing ? " is-playing" : ""}`} role="button" tabIndex={backendAvailable ? 0 : -1}
    aria-current={playing ? "true" : undefined} aria-disabled={!backendAvailable} onClick={() => backendAvailable && onClick?.(track)} onContextMenu={(event) => { if (backendAvailable) onContextMenu?.(event, track); }}
    onKeyDown={(event) => { if (backendAvailable && (event.key === "Enter" || event.key === " ")) { event.preventDefault(); onClick?.(track); } }}>
    <span className="luno-track-grid">
      <span className="luno-track-cell-index" aria-hidden="true">
        {!playing && <IconPlay size={14} className="luno-track-hover-play" />}
        {playing ? <span className="luno-equalizer">{[0, 1, 2, 3].map((bar) => <i key={bar} style={{ animationDelay: `${bar * 0.12}s` }} />)}</span> : <span className="luno-track-num">{index == null ? "" : index + 1}</span>}
      </span>
      <Artwork className="luno-track-artwork-slot" src={artwork ?? track.artwork_url} alt={`${track.title} artwork`} size={40} />
      <span className="luno-track-copy"><strong title={track.title}>{truncate(track.title, 52)}</strong><span title={track.artist ?? "Unknown artist"}>{truncate(track.artist ?? "Unknown artist", 64)}</span></span>
      <span className="luno-track-cell-album" title={track.album ?? undefined}>{truncate(track.album ?? "", 40)}</span>
      <span className="luno-track-cell-playlist luno-truncate" title={playlistName ?? undefined}>{playlistName}</span>
      <span className="luno-track-favorite"><button className={`luno-favorite${track.favorite ? " is-on" : ""}`} type="button" aria-label={track.favorite ? `Remove ${track.title} from favorites` : `Add ${track.title} to favorites`} disabled={!backendAvailable} onClick={(event) => { event.stopPropagation(); if (backendAvailable) onFavoriteToggle?.(track); }}>{track.favorite ? "★" : "☆"}</button></span>
      <span className="luno-track-cell-duration">{formatDuration(track.duration_ms)}</span>
    </span>
  </div>;
}

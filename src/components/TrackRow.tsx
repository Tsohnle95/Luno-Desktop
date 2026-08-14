import type { MouseEvent } from "react";
import type { Track } from "../shared/contracts";
import { Artwork } from "./Artwork";

export interface TrackRowProps {
  track: Track;
  index?: number;
  playlistName?: string | null;
  selected?: boolean;
  playing?: boolean;
  onClick?: (track: Track) => void;
  onFavoriteToggle?: (track: Track) => void;
  onContextMenu?: (event: MouseEvent, track: Track) => void;
  backendAvailable?: boolean;
}

function truncate(value: string, max: number) {
  return value.length > max && max > 3 ? `${value.slice(0, max - 3)}...` : value;
}

export function TrackRow({ track, index, playlistName, selected = false, playing = false, onClick, onFavoriteToggle, onContextMenu, backendAvailable = true }: TrackRowProps) {
  return <div className={`luno-row luno-track-row${selected ? " is-selected" : ""}${playing ? " is-playing" : ""}`} role="button" tabIndex={backendAvailable ? 0 : -1}
    aria-current={playing ? "true" : undefined} aria-disabled={!backendAvailable} onClick={() => backendAvailable && onClick?.(track)} onContextMenu={(event) => { if (backendAvailable) onContextMenu?.(event, track); }}
    onKeyDown={(event) => { if (backendAvailable && (event.key === "Enter" || event.key === " ")) { event.preventDefault(); onClick?.(track); } }}>
    <Artwork src={track.artwork_url} alt={`${track.title} artwork`} size={44} />
    <span className="luno-track-index" aria-hidden="true">{playing ? <span className="luno-equalizer">{[0, 1, 2, 3].map((bar) => <i key={bar} style={{ animationDelay: `${bar * 0.12}s` }} />)}</span> : (index == null ? "" : index + 1)}</span>
    <span className="luno-track-copy"><strong title={track.title}>{truncate(track.title, 52)}</strong><span title={track.artist ?? "Unknown artist"}>{truncate(track.artist ?? "Unknown artist", 64)}</span></span>
    <span className="luno-track-playlist luno-truncate">{playlistName && <><button className="luno-favorite" type="button" aria-label={track.favorite ? "Remove from favorites" : "Add to favorites"} disabled={!backendAvailable} onClick={(event) => { event.stopPropagation(); if (backendAvailable) onFavoriteToggle?.(track); }}>{track.favorite ? "★" : "☆"}</button>{truncate(playlistName, 24)}</>}</span>
  </div>;
}

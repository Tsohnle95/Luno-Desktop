import type { CSSProperties } from "react";
import type { PlaybackSnapshot, Track } from "../shared/contracts";
import { Artwork } from "./Artwork";

export interface PlayerBarProps {
  track?: Track | null;
  playback?: PlaybackSnapshot | null;
  volume?: number;
  accent?: string;
  onPlayPause?: () => void;
  onNext?: () => void;
  onPrevious?: () => void;
  onSeek?: (positionMs: number) => void;
  onVolume?: (volume: number) => void;
  onFavoriteToggle?: (track: Track) => void;
  onOpenTrack?: (track: Track) => void;
}

const formatTime = (ms: number) => { const seconds = Math.max(0, Math.floor(ms / 1000)); return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`; };

export function PlayerBar({ track = null, playback = null, volume = 1, accent = "var(--luno-green)", onPlayPause, onNext, onPrevious, onSeek, onVolume, onFavoriteToggle, onOpenTrack }: PlayerBarProps) {
  const position = playback?.position_ms ?? 0;
  const duration = playback?.duration_ms ?? 0;
  const playing = playback?.status === "playing";
  return <div className="luno-player-bar" style={{ "--luno-player-accent": accent } as CSSProperties}>
    <button className="luno-player-track" type="button" disabled={!track} onClick={() => track && onOpenTrack?.(track)}><Artwork src={track?.artwork_url} alt="" size={56} /><span><strong>{track?.title ?? "Nothing playing"}</strong><small>{track?.artist ?? ""}</small></span></button>
    <div className="luno-player-controls"><div className="luno-transport"><button type="button" aria-label="Previous track" onClick={onPrevious}>⏮</button><button type="button" aria-label={playing ? "Pause" : "Play"} onClick={onPlayPause}>{playing ? "Ⅱ" : "▶"}</button><button type="button" aria-label="Next track" onClick={onNext}>⏭</button></div><div className="luno-scrubber"><span>{formatTime(position)}</span><input aria-label="Playback position" type="range" min="0" max={Math.max(duration, 1)} value={Math.min(position, Math.max(duration, 1))} onChange={(event) => onSeek?.(Number(event.target.value))} /><span>{formatTime(duration)}</span></div></div>
    <div className="luno-player-actions"><button type="button" aria-label={track?.favorite ? "Remove from favorites" : "Add to favorites"} disabled={!track} onClick={() => track && onFavoriteToggle?.(track)}>{track?.favorite ? "★" : "☆"}</button><label><span className="sr-only">Volume</span><input aria-label="Volume" type="range" min="0" max="1" step="0.01" value={volume} onChange={(event) => onVolume?.(Number(event.target.value))} /></label></div>
  </div>;
}

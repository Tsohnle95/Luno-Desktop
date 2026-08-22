import { useLayoutEffect, useRef } from "react";
import type { Track } from "../shared/contracts";
import { Artwork } from "./Artwork";
import { IconClose } from "./icons";
import { useTrackArtwork, type ArtworkFetcher } from "../state/track_artwork";

export interface QueueProps {
  tracks: Track[];
  currentIndex?: number | null;
  recent?: boolean;
  onPlay?: (track: Track, index: number) => void;
  onFavoriteToggle?: (track: Track) => void;
  onPlayNext?: (track: Track) => void;
  onAddToQueue?: (track: Track) => void;
  onRemove?: (index: number) => void;
  onReorder?: (from: number, to: number) => void;
  onClear?: () => void;
  onShuffle?: () => void;
  onRepeat?: (mode: "off" | "one" | "all") => void;
  shuffle?: boolean;
  repeat?: "off" | "one" | "all";
  backendAvailable?: boolean;
  /** Panel chrome: tab switcher + close button (Spotify-style header). */
  recentLabel?: string | null;
  onClose?: () => void;
  onTabChange?: (recent: boolean) => void;
  /** Section label above the upcoming rows ("Next from: <scope>"). */
  nextFromLabel?: string | null;
  /** Lazily resolves embedded artwork for queue rows (scans carry no artwork_url). */
  onArtwork?: ArtworkFetcher;
}

export function Queue({ tracks, currentIndex = null, recent = false, onPlay, onFavoriteToggle, onPlayNext, onAddToQueue, onRemove, onReorder, onClear, onShuffle, onRepeat, shuffle = false, repeat = "off", backendAvailable = true, recentLabel = null, onClose, onTabChange, nextFromLabel = null, onArtwork }: QueueProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const scrollTop = useRef(0);
  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTop = scrollTop.current;
  }, [tracks, recent]);
  const rememberScroll = () => { if (scrollRef.current) scrollTop.current = scrollRef.current.scrollTop; };
  const visible = recent ? tracks.slice(0, 100) : tracks;
  const current = currentIndex != null ? visible[currentIndex] : undefined;
  // Queue rows fetch embedded artwork lazily; the module-level cache in the
  // hook means re-renders never re-request the same track ids.
  const artwork = useTrackArtwork(visible.map((track) => track.id), onArtwork);
  return <div className="luno-queue-panel">
    {(onTabChange || onClose) && <header className="luno-queue-head">
      <button className="luno-queue-tab" type="button" aria-label="Queue tab" aria-pressed={!recent} disabled={!onTabChange} onClick={() => onTabChange?.(false)}>Queue</button>
      <button className="luno-queue-tab" type="button" aria-pressed={recent} disabled={!onTabChange} onClick={() => onTabChange?.(true)}>{recentLabel ?? "Recently played"}</button>
      {onClose && <button className="luno-icon-button luno-queue-close" type="button" aria-label="Close queue panel" onClick={onClose}><IconClose size={14} /></button>}
    </header>}
    {!recent && <div className="luno-queue-tools">
      <button className="luno-pill" type="button" aria-label={shuffle ? "Disable shuffle" : "Enable shuffle"} aria-pressed={shuffle} disabled={!backendAvailable} onClick={onShuffle}>Shuffle</button>
      <button className="luno-pill" type="button" aria-label={`Repeat ${repeat}`} aria-pressed={repeat !== "off"} disabled={!backendAvailable} onClick={() => onRepeat?.(repeat === "off" ? "all" : repeat === "all" ? "one" : "off")}>Repeat: {repeat}</button>
      <button className="luno-pill" type="button" aria-label="Clear queue" disabled={!backendAvailable || !tracks.length} onClick={onClear}>Clear</button>
    </div>}
    <div ref={scrollRef} className="luno-queue-body luno-scroll" onScroll={rememberScroll}>
      {current && !recent ? <>
        <h2 className="luno-queue-section">Now playing</h2>
        <div className="luno-queue-now-card">
          <Artwork src={artwork[current.id] ?? current.artwork_url} alt="" size={56} />
          <span className="luno-queue-copy"><strong>{current.title}</strong><small>{current.artist ?? "Unknown artist"}</small></span>
          {current.favorite && <span className="luno-queue-favorite" aria-label="Favorite">★</span>}
        </div>
        <h3 className="luno-queue-section">{nextFromLabel ? `Next from: ${nextFromLabel}` : "Next up"}<small>{Math.max(0, visible.length - Math.max(0, currentIndex ?? 0) - 1)} songs</small></h3>
      </>
      : !recent ? <h3 className="luno-queue-section">Queue is empty<small>Play a song to build the queue.</small></h3>
      : visible.length === 0 ? <p className="luno-queue-empty">No recently played tracks.</p>
      : <h3 className="luno-queue-section">Recently played<small>Session history</small></h3>}
      {visible.map((track, index) => {
        // The current track already has its Now playing card; skip its row but
        // keep every index in sync with the backend queue order.
        if (!recent && index === currentIndex) return null;
        return <div key={`${track.id}-${index}`} role="button" tabIndex={0}
          className="luno-queue-row"
          onClick={() => onPlay?.(track, index)}
          onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); onPlay?.(track, index); } }}>
          <Artwork src={artwork[track.id] ?? track.artwork_url} alt="" size={40} />
          <span className="luno-queue-copy"><strong title={track.title}>{track.title}</strong><small>{track.artist ?? "Unknown artist"}</small></span>
          {track.favorite && <span className="luno-queue-favorite" aria-label="Favorite">★</span>}
          {!recent && <span className="luno-queue-actions">
            <button type="button" aria-label={`Play ${track.title} next`} disabled={!backendAvailable} onClick={(event) => { event.stopPropagation(); onPlayNext?.(track); }}>▶</button>
            <button type="button" aria-label={`Add ${track.title} to queue`} disabled={!backendAvailable} onClick={(event) => { event.stopPropagation(); onAddToQueue?.(track); }}>＋</button>
            <button type="button" aria-label={`Move ${track.title} up`} disabled={!backendAvailable || index === 0} onClick={(event) => { event.stopPropagation(); onReorder?.(index, index - 1); }}>↑</button>
            <button type="button" aria-label={`Move ${track.title} down`} disabled={!backendAvailable || index === visible.length - 1} onClick={(event) => { event.stopPropagation(); onReorder?.(index, index + 1); }}>↓</button>
            <button type="button" aria-label={`Remove ${track.title} from queue`} disabled={!backendAvailable} onClick={(event) => { event.stopPropagation(); onRemove?.(index); }}>×</button>
          </span>}
        </div>;
      })}
    </div>
  </div>;
}

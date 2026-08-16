import { useLayoutEffect, useRef } from "react";
import type { Track } from "../shared/contracts";
import { Artwork } from "./Artwork";

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
}

export function Queue({ tracks, currentIndex = null, recent = false, onPlay, onFavoriteToggle, onPlayNext, onAddToQueue, onRemove, onReorder, onClear, onShuffle, onRepeat, shuffle = false, repeat = "off", backendAvailable = true }: QueueProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const scrollTop = useRef(0);
  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTop = scrollTop.current;
  }, [tracks, recent]);
  const rememberScroll = () => { if (scrollRef.current) scrollTop.current = scrollRef.current.scrollTop; };
  const visible = recent ? tracks.slice(0, 100) : tracks;
  return <div ref={scrollRef} className="luno-queue-list luno-scroll" onScroll={rememberScroll}>
    <h2>{recent ? "Recently Played" : "Queue"}</h2>
    {!recent && <div><button type="button" disabled={!backendAvailable} aria-pressed={shuffle} onClick={onShuffle}>Shuffle</button><button type="button" disabled={!backendAvailable} onClick={() => onRepeat?.(repeat === "off" ? "all" : repeat === "all" ? "one" : "off")}>Repeat: {repeat}</button><button type="button" disabled={!backendAvailable || !tracks.length} onClick={onClear}>Clear</button></div>}
    {!recent && currentIndex != null && visible[currentIndex] && <h3>Now Playing</h3>}
    {visible.length === 0 ? <p className="luno-empty">{recent ? "No recently played tracks" : "Queue is empty"}</p> : visible.map((track, index) => <div className={`luno-queue-row${!recent && index === currentIndex ? " is-playing" : ""}`} role="button" tabIndex={0} key={`${track.id}-${index}`} onClick={() => onPlay?.(track, index)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); onPlay?.(track, index); } }}>
      <Artwork src={track.artwork_url} alt="" size={36} /><span className="luno-queue-copy"><strong>{track.title}</strong><small>{track.artist ?? "Unknown artist"}</small></span>
      {track.favorite && <span className="luno-queue-favorite" aria-label="Favorite">★</span>}
       {!recent && <span><button type="button" aria-label={`Play ${track.title} next`} disabled={!backendAvailable} onClick={(event) => { event.stopPropagation(); onPlayNext?.(track); }}>Next</button><button type="button" aria-label={`Add ${track.title} to queue`} disabled={!backendAvailable} onClick={(event) => { event.stopPropagation(); onAddToQueue?.(track); }}>+</button><button type="button" aria-label={`Move ${track.title} up`} disabled={!backendAvailable || index === 0} onClick={(event) => { event.stopPropagation(); onReorder?.(index, index - 1); }}>↑</button><button type="button" aria-label={`Move ${track.title} down`} disabled={!backendAvailable || index === visible.length - 1} onClick={(event) => { event.stopPropagation(); onReorder?.(index, index + 1); }}>↓</button><button type="button" aria-label={`Remove ${track.title} from queue`} disabled={!backendAvailable} onClick={(event) => { event.stopPropagation(); onRemove?.(index); }}>×</button></span>}
    </div>)}
  </div>;
}

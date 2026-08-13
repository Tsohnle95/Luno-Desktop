import { useLayoutEffect, useRef } from "react";
import type { Track } from "../shared/contracts";
import { Artwork } from "./Artwork";

export interface QueueProps {
  tracks: Track[];
  currentIndex?: number | null;
  recent?: boolean;
  onPlay?: (track: Track, index: number) => void;
  onFavoriteToggle?: (track: Track) => void;
}

export function Queue({ tracks, currentIndex = null, recent = false, onPlay, onFavoriteToggle }: QueueProps) {
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
    {!recent && currentIndex != null && visible[currentIndex] && <h3>Now Playing</h3>}
    {visible.length === 0 ? <p className="luno-empty">{recent ? "No recently played tracks" : "Queue is empty"}</p> : visible.map((track, index) => <button className={`luno-queue-row${!recent && index === currentIndex ? " is-playing" : ""}`} type="button" key={`${track.id}-${index}`} onClick={() => onPlay?.(track, index)}>
      <Artwork src={track.artwork_url} alt="" size={36} /><span className="luno-queue-copy"><strong>{track.title}</strong><small>{track.artist ?? "Unknown artist"}</small></span>
      {track.favorite && <span className="luno-queue-favorite" aria-label="Favorite">★</span>}
    </button>)}
  </div>;
}

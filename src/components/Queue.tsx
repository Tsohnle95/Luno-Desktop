import { SongSelectCheckbox } from "./SongSelection";
import { useLayoutEffect, useRef, useState, type MouseEvent, type UIEvent } from "react";
import type { Track } from "../shared/contracts";
import { RecommendationArtworkImage } from "./RecommendationArtwork";
import { useTrackArtworkState, type ArtworkFetcher } from "../state/track_artwork";

export interface QueueProps {
  tracks: Track[];
  currentIndex?: number | null;
  recent?: boolean;
  onPlay?: (track: Track, index: number) => void;
  onFavoriteToggle?: (track: Track) => void;
  onPlayNext?: (track: Track) => void;
  onRemove?: (index: number) => void;
  onReorder?: (from: number, to: number) => void;
  onContextMenu?: (event: MouseEvent, track: Track) => void;
  backendAvailable?: boolean;
  /** Panel chrome: tab switcher header. Panels open/close by edge-drag only. */
  recentLabel?: string | null;
  onTabChange?: (recent: boolean) => void;
  /** Section label above the upcoming rows ("Next from: <scope>"). */
  nextFromLabel?: string | null;
  /** Lazily resolves embedded artwork for queue rows (scans carry no artwork_url). */
  onArtwork?: ArtworkFetcher;
}

/** Queue rows share the library's documented 56 px rhythm. */
export const QUEUE_ROW_HEIGHT = 56;
const QUEUE_OVERSCAN = 12;

/** Pure window math keeps a multi-thousand-song playback context out of the DOM. */
export function queueVirtualWindow(scrollTop: number, rowsTop: number, viewportHeight: number, count: number, overscan = QUEUE_OVERSCAN): { start: number; end: number } {
  if (count <= 0) return { start: 0, end: 0 };
  const localTop = Math.max(0, scrollTop - rowsTop);
  const visibleCount = Math.max(1, Math.ceil(Math.max(0, viewportHeight) / QUEUE_ROW_HEIGHT));
  const start = Math.max(0, Math.min(count - 1, Math.floor(localTop / QUEUE_ROW_HEIGHT) - overscan));
  return { start, end: Math.min(count, start + visibleCount + overscan * 2) };
}

export function Queue({ tracks, currentIndex = null, recent = false, onPlay, onFavoriteToggle, onPlayNext, onRemove, onReorder, onContextMenu, backendAvailable = true, recentLabel = null, onTabChange, nextFromLabel = null, onArtwork }: QueueProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const rowsRef = useRef<HTMLDivElement>(null);
  const scrollTop = useRef(0);
  const scrollFrame = useRef<number | null>(null);
  const lastQueueCursor = useRef(currentIndex);
  const lastRecentTab = useRef(recent);
  const [windowState, setWindowState] = useState({ scrollTop: 0, rowsTop: 0, viewportHeight: 560 });
  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    if (lastQueueCursor.current !== currentIndex || lastRecentTab.current !== recent) {
      scrollTop.current = 0;
      lastQueueCursor.current = currentIndex;
      lastRecentTab.current = recent;
    }
    element.scrollTop = scrollTop.current;
    setWindowState({
      scrollTop: element.scrollTop,
      rowsTop: rowsRef.current?.offsetTop ?? 0,
      viewportHeight: element.clientHeight || 560,
    });
  }, [tracks, recent, currentIndex]);
  useLayoutEffect(() => () => {
    if (scrollFrame.current !== null) cancelAnimationFrame(scrollFrame.current);
  }, []);
  const rememberScroll = (_event?: UIEvent<HTMLDivElement>) => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    scrollTop.current = scroller.scrollTop;
    if (scrollFrame.current !== null) return;
    scrollFrame.current = requestAnimationFrame(() => {
      scrollFrame.current = null;
      const live = scrollRef.current;
      setWindowState({
        scrollTop: live?.scrollTop ?? 0,
        rowsTop: rowsRef.current?.offsetTop ?? 0,
        viewportHeight: live?.clientHeight ?? 560,
      });
    });
  };
  // Cap queue artwork to avoid 4k-track spike (each 1 MB data URL → 4 GB).
  // Recent is already capped 100; the queue contains upcoming rows only.
  const MAX_QUEUE_ARTWORK = 40;
  const current = !recent && currentIndex != null && currentIndex >= 0 ? tracks[currentIndex] : undefined;
  const queueOffset = current && currentIndex != null ? currentIndex + 1 : 0;
  const visible = recent ? tracks.slice(0, 100) : tracks.slice(queueOffset);
  const { start, end } = queueVirtualWindow(windowState.scrollTop, windowState.rowsTop, windowState.viewportHeight, visible.length);
  const renderedRows = visible.slice(start, end);
  const artworkIds = renderedRows.slice(0, MAX_QUEUE_ARTWORK).map((track) => track.id);
  if (current && !artworkIds.includes(current.id)) artworkIds.unshift(current.id);
  // Queue rows fetch embedded artwork lazily; the module-level cache in the
  // hook means re-renders never re-request the same track ids.
  const artworkState = useTrackArtworkState(artworkIds, onArtwork);
  const artwork = artworkState.artwork;
  return <div className="luno-queue-panel">
    {onTabChange && <header className="luno-queue-head">
      <button className="luno-queue-tab" type="button" aria-label="Queue tab" aria-pressed={!recent} disabled={!onTabChange} onClick={() => onTabChange?.(false)}>Queue</button>
      <button className="luno-queue-tab" type="button" aria-pressed={recent} disabled={!onTabChange} onClick={() => onTabChange?.(true)}>{recentLabel ?? "Recently played"}</button>
    </header>}
    <div ref={scrollRef} className="luno-queue-body luno-scroll" onScroll={rememberScroll}>
      {current && !recent ? <>
        <h2 className="luno-queue-section">Now playing</h2>
        <div data-track-id={current.id} className="luno-queue-now-card" onContextMenu={(event) => onContextMenu?.(event, current)}><SongSelectCheckbox track={current} />
           <RecommendationArtworkImage artist={current.artist ?? ""} title={current.title} artworkUrl={artwork[current.id] ?? current.artwork_url} loading={artworkState.loading.has(current.id)} eager alt="" size={56} />
          <span className="luno-queue-copy"><strong>{current.title}</strong><small>{current.artist ?? "Unknown artist"}</small></span>
          {current.favorite && <span className="luno-queue-favorite" aria-label="Favorite">★</span>}
        </div>
        <h3 className="luno-queue-section">{nextFromLabel ? `Next from: ${nextFromLabel}` : "Next up"}<small>{visible.length} songs</small></h3>
      </>
      : !recent && visible.length === 0 ? <h3 className="luno-queue-section">Queue is empty<small>Play a song to build the queue.</small></h3>
      : !recent ? <h3 className="luno-queue-section">{nextFromLabel ? `Next from: ${nextFromLabel}` : "Next up"}<small>{visible.length} songs</small></h3>
      : visible.length === 0 ? <p className="luno-queue-empty">No recently played tracks.</p>
      : <h3 className="luno-queue-section">Recently played<small>Session history</small></h3>}
      <div ref={rowsRef} style={{ height: visible.length * QUEUE_ROW_HEIGHT, position: "relative" }}>
      <div style={{ transform: `translateY(${start * QUEUE_ROW_HEIGHT}px)` }}>
      {renderedRows.map((track, renderedIndex) => {
        const index = start + renderedIndex;
        const queueIndex = recent ? index : queueOffset + index;
        return <div data-track-id={track.id} key={`${track.id}-${queueIndex}`} role="button" tabIndex={0}
          className="luno-queue-row"
          onContextMenu={(event) => onContextMenu?.(event, track)}
          onClick={() => onPlay?.(track, queueIndex)}
          onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); onPlay?.(track, queueIndex); } }}>
           <SongSelectCheckbox track={track} /><RecommendationArtworkImage artist={track.artist ?? ""} title={track.title} artworkUrl={artwork[track.id] ?? track.artwork_url} loading={artworkState.loading.has(track.id)} eager alt="" size={40} />
          <span className="luno-queue-copy"><strong title={track.title}>{track.title}</strong><small>{track.artist ?? "Unknown artist"}</small></span>
          {track.favorite && <span className="luno-queue-favorite" aria-label="Favorite">★</span>}
          {!recent && <span className="luno-queue-actions">
            <button type="button" aria-label={`Play ${track.title} next`} disabled={!backendAvailable} onClick={(event) => { event.stopPropagation(); onPlayNext?.(track); }}>▶</button>
            <button type="button" aria-label={`Move ${track.title} up`} disabled={!backendAvailable || queueIndex === 0} onClick={(event) => { event.stopPropagation(); onReorder?.(queueIndex, queueIndex - 1); }}>↑</button>
            <button type="button" aria-label={`Move ${track.title} down`} disabled={!backendAvailable || queueIndex === tracks.length - 1} onClick={(event) => { event.stopPropagation(); onReorder?.(queueIndex, queueIndex + 1); }}>↓</button>
            <button type="button" aria-label={`Remove ${track.title} from queue`} disabled={!backendAvailable} onClick={(event) => { event.stopPropagation(); onRemove?.(queueIndex); }}>×</button>
          </span>}
        </div>;
      })}
      </div>
      </div>
    </div>
  </div>;
}

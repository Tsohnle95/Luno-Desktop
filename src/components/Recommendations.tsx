import { useRef, useState, type ReactNode } from "react";
import type { Track } from "../shared/contracts";
import { Artwork } from "./Artwork";

export interface Recommendation {
  id?: string;
  title: string;
  artist: string;
  artwork_url?: string | null;
  status?: "idle" | "waiting" | "resolving" | "preparing" | "ready" | "failed";
  error?: string | null;
  saved?: boolean;
}

export type RecommendationState = "idle" | "loading" | "error" | "ready";

export interface RecommendationsProps {
  seed: Track | null;
  items: Recommendation[];
  state: RecommendationState;
  hasApiKey?: boolean;
  error?: string | null;
  exhausted?: boolean;
  loadingMore?: boolean;
  loadMoreError?: string | null;
  sessionActive?: boolean;
  discoverMode?: boolean;
  onRetry?: () => void;
  onRefresh?: () => void;
  onLoadMore?: () => void;
  onPreview?: (item: Recommendation) => void;
  onSave?: (item: Recommendation) => void;
  emptyMessage?: ReactNode;
}

const recommendationKey = (item: Recommendation) => item.id ?? `${item.artist}\u0000${item.title}`;

export function Recommendations({ seed, items, state, hasApiKey = true, error, exhausted = false, loadingMore = false, loadMoreError, sessionActive = false, discoverMode = false, onRetry, onRefresh, onLoadMore, onPreview, onSave, emptyMessage = "No recommendations available for this song" }: RecommendationsProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [savedKeys, setSavedKeys] = useState<Set<string>>(new Set());
  const canRefresh = Boolean(seed && hasApiKey && !sessionActive);
  const save = (item: Recommendation) => {
    const key = recommendationKey(item);
    setSavedKeys((previous) => new Set(previous).add(key));
    onSave?.(item);
  };
  const stateMessage = !seed ? "Play a song to see recommendations" : !hasApiKey ? "Add a Last.fm API key in Settings to see recommendations" : state === "loading" ? "Finding similar songs..." : state === "error" ? (error || "Could not load recommendations") : state === "ready" && items.length === 0 ? emptyMessage : null;

  return <section className="luno-recommendations" aria-labelledby="recommendations-title">
    <header className="luno-recommendations-header">
      <div><h2 id="recommendations-title">Recommended for this song</h2><p>{sessionActive ? (discoverMode ? "Temporary preview queue - tap a song to restart from there" : "Temporary preview - Next returns to your library") : `${items.length} similar songs - tap to preview`}</p></div>
      <button type="button" aria-label="Refresh recommendations" disabled={!canRefresh} onClick={onRefresh}>⟳</button>
    </header>
    {seed && hasApiKey && <p className="luno-recommendations-hint">↓ Scroll for recommendations</p>}
    {stateMessage && <div className={`luno-recommendations-state ${state === "error" ? "is-error" : ""}`} role={state === "error" ? "alert" : undefined}><span>{stateMessage}</span>{state === "error" && <button type="button" onClick={onRetry}>Retry</button>}</div>}
    {items.length > 0 && <div ref={scrollRef} className="luno-recommendation-list" aria-live="polite">
      {items.map((item) => { const key = recommendationKey(item); const saved = item.saved || savedKeys.has(key); return <article className="luno-recommendation-row" key={key}>
        <Artwork src={item.artwork_url} alt={`${item.title} artwork`} size={48} />
        <button className="luno-recommendation-info" type="button" onClick={() => onPreview?.(item)}><strong>{item.title}</strong><span>{item.artist}</span><small>{item.status === "resolving" ? "Finding a playable source..." : item.status === "preparing" ? "Downloading temporary preview..." : item.status === "waiting" ? "Queued for temporary preview" : item.status === "failed" ? (item.error || "Preview failed") : item.status === "ready" ? "Ready in temporary queue" : "Tap to preview"}</small></button>
        <div className="luno-recommendation-actions">{item.status === "failed" && <button type="button" onClick={() => onPreview?.(item)}>Retry</button>} {item.status === "ready" && <button type="button" onClick={() => onPreview?.(item)}>Play</button>}<button type="button" disabled={saved} aria-label={saved ? `Saved ${item.title}` : `Add ${item.title} to playlist`} onClick={() => save(item)}>{saved ? "✓ Saved" : "＋ Add to playlist"}</button></div>
      </article>; })}
      {!exhausted && <footer className="luno-recommendations-footer">{loadMoreError && <p role="alert">{loadMoreError}</p>}{loadingMore ? <span>Loading more...</span> : <button type="button" onClick={onLoadMore}>Load 25 more</button>}</footer>}
    </div>}
  </section>;
}

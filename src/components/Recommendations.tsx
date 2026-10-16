import { useRef, useState, type ReactNode } from "react";
import type { Track, PreviewSnapshot, SaveState, Playlist, DownloadJob } from "../shared/contracts";
import { previewKey as backendPreviewKey } from "../shared/contracts";
import { RecommendationArtworkImage } from "./RecommendationArtwork";
import { DownloadRing } from "./VideoStage";
import { PlaylistPicker } from "./PlaylistPicker";
import { IconInfo } from "./icons";
import type { ArtworkFetcher } from "../state/track_artwork";

export interface Recommendation {
  id?: string;
  title: string;
  artist: string;
  artwork_url?: string | null;
  match_score?: number;
  status?: "idle" | "waiting" | "resolving" | "preparing" | "ready" | "failed";
  error?: string | null;
  saved?: boolean;
}

export type RecommendationState = "idle" | "loading" | "error" | "ready";

export interface RecommendationsProps {
  seed: Track | null;
  items: Recommendation[];
  state: RecommendationState;
  error?: string | null;
  exhausted?: boolean;
  loadingMore?: boolean;
  loadMoreError?: string | null;
  /** Deprecated compatibility prop; recommendations no longer expose a refresh control. */
  onRefresh?: () => void;
  onRetry?: () => void;
  onLoadMore?: () => void;
  onPreview?: (item: Recommendation) => void;
  onSave?: (item: Recommendation, playlistId?: string) => void;
  emptyMessage?: ReactNode;
  // Mobile parity additions
  preview?: PreviewSnapshot | null;
  saved?: Record<string, SaveState>;
  playlists?: Playlist[];
  jobs?: Record<string, DownloadJob>;
  backendAvailable?: boolean;
  onCreatePlaylist?: (item: Recommendation) => void;
  tracksById?: ReadonlyMap<string, Track>;
  onArtwork?: ArtworkFetcher;
}

const legacyKey = (item: Recommendation) => item.id ?? `${item.artist}\u0000${item.title}`;
const safeKey = (item: Recommendation) => backendPreviewKey(item.artist, item.title);

export function recommendationPreviewState(item: Recommendation, preview: PreviewSnapshot | null) {
  const key = safeKey(item);
  const legacy = legacyKey(item);
  const progress = preview?.progress?.find((entry) => entry.key === key || entry.key === legacy)?.percent ?? null;
  const prepared = Boolean(preview && (preview.prepared.includes(key) || preview.prepared.includes(legacy)));
  const unavailable = Boolean(preview && (preview.unavailable.includes(key) || preview.unavailable.includes(legacy)));
  const inWindow = Boolean(preview && (preview.window.includes(key) || preview.window.includes(legacy)));
  const current = Boolean(preview && (preview.current === key || preview.current === legacy));
  return { key, legacy, progress, prepared, unavailable, inWindow, current };
}

export function Recommendations({
  seed,
  items,
  state,
  error,
  exhausted = false,
  loadingMore = false,
  loadMoreError,
  onRetry,
  onLoadMore,
  onPreview,
  onSave,
  emptyMessage = "No recommended tunes returned for this song.",
  preview = null,
  saved = {},
  playlists = [],
  jobs = {},
  backendAvailable = true,
  onCreatePlaylist,
  tracksById,
  onArtwork,
}: RecommendationsProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [savedKeys, setSavedKeys] = useState<Set<string>>(new Set());
  const [pickerItem, setPickerItem] = useState<Recommendation | null>(null);
  const stateMessage = state === "loading"
    ? "Finding recommended songs…"
    : state === "error"
      ? error || "Could not load recommendations"
      : items.length === 0
        ? emptyMessage
        : null;

  const handleAddClick = (item: Recommendation) => {
    if (!backendAvailable) return;
    const key = safeKey(item);
    const lKey = legacyKey(item);
    const saveState = saved[key] ?? saved[lKey];
    if (saveState && (saveState.state === "saving" || saveState.state === "queued" || saveState.state === "saved")) return;
    // Mobile parity: the add button always opens the playlist picker so the
    // user chooses the destination. This replaces the old auto-pick-first-
    // playlist behaviour and keeps the picker visible even when no playlists
    // exist (empty state + create action).
    setPickerItem(item);
  };

  const handlePick = (pl: Playlist) => {
    if (!pickerItem) return;
    const item = pickerItem;
    setPickerItem(null);
    // Don't use local savedKeys when backend saved map is authoritative; the
    // backend will emit a SaveState event that drives the button label/spinner.
    // Only fall back to local for the no-backend case handled above.
    onSave?.(item, pl.id);
  };

  return (
    <section className="luno-recommendations" aria-labelledby="recommendations-title">
      <header className="luno-recommendations-header">
        <div className="luno-recommendations-title">
          <h2 id="recommendations-title">Recommended songs</h2>
          {seed && !seed.artist && <button className="luno-info-tip" type="button" aria-label="Why recommendations are missing" title="This song has no artist in its filename. Similar songs work best when the filename is “Artist - Title”."><IconInfo size={14} /></button>}
        </div>
      </header>
      {stateMessage && (
        <div className={`luno-recommendations-state ${state === "error" ? "is-error" : ""}`} role={state === "error" ? "alert" : undefined}>
          <span>{stateMessage}</span>
          {state === "error" && (
            <button type="button" onClick={onRetry}>
              Retry
            </button>
          )}
        </div>
      )}
      {items.length > 0 && (
        <div ref={scrollRef} className="luno-recommendation-list" aria-live="polite">
          {items.map((item) => {
             const previewState = recommendationPreviewState(item, preview);
             const { key: k, legacy: lK } = previewState;
            const saveState = saved[k] ?? saved[lK];
            const isLegacySaved = (item.saved || savedKeys.has(lK) || savedKeys.has(k)) && !saveState;
            const job = saveState?.job_id ? jobs[saveState.job_id] : undefined;
            const jobProgress = job?.progress;
            const determinate = typeof jobProgress === "number" && jobProgress !== null && Number.isFinite(jobProgress);
            const percent = determinate ? Math.round((jobProgress as number) * 100) : null;

             const isUnavailable = previewState.unavailable;
             const isCurrent = previewState.current;
             const isPrepared = previewState.prepared;
             const isInWindow = previewState.inWindow;
             const isQueuedPreview = Boolean(preview?.active && isInWindow && !isPrepared && !isCurrent && !isUnavailable);
             const previewProgress = previewState.progress;
             const preparingKey = preview?.window.find((candidate) =>
               !preview.prepared.includes(candidate) && !preview.unavailable.includes(candidate)
             );
             const isPreparingPreview = isQueuedPreview && (preparingKey === k || preparingKey === lK);

            // --- status text (save overrides preview, like mobile) ---
            let statusText: string;
            let statusIsError = false;
            if (saveState?.state === "saving") statusText = "Saving to playlist…";
            else if (saveState?.state === "queued") statusText = "Download queued for playlist";
            else if (saveState?.state === "saved") statusText = saveState.already_saved ? "Added to playlist" : "Saved to library";
            else if (saveState?.state === "failed") {
              statusText = saveState.error ? `Save failed: ${saveState.error}` : "Save failed";
              statusIsError = true;
            } else if (isUnavailable) {
              // Unavailable must outrank "current": a failed current item keeps
              // its session slot, so checking `isCurrent` first mislabelled the
              // dead preview as "Previewing now".
              statusText = "Preview unavailable";
              statusIsError = true;
            } else if (isCurrent) statusText = "Previewing now";
            else if (item.status === "failed" && item.error) {
              statusText = item.error;
              statusIsError = true;
            } else if (isPreparingPreview && previewProgress === null) statusText = "Finding a playable source…";
            else if (isPreparingPreview) statusText = `Downloading temporary preview… ${previewProgress}%`;
            else if (isQueuedPreview) statusText = "Queued for temporary preview";
            else if (isPrepared) statusText = "Ready in temporary queue";
            else if (item.status) {
              switch (item.status) {
                case "resolving":
                  statusText = "Finding a playable source…";
                  break;
                case "preparing":
                  statusText = "Downloading temporary preview…";
                  break;
                case "waiting":
                  statusText = "Queued for temporary preview";
                  break;
                case "failed":
                  statusText = item.error || "Preview failed";
                  statusIsError = true;
                  break;
                case "ready":
                  statusText = "Ready in temporary queue";
                  break;
                default:
                  statusText = "Tap to preview";
              }
            } else if (typeof item.match_score === "number") {
              statusText = `${item.artist} • ${Math.round(item.match_score * 100)}% match`;
            } else {
              statusText = "Tap to preview";
            }

            const saveLabel = (() => {
              if (isLegacySaved) return { label: "✓ Saved", disabled: true };
              if (!saveState) return { label: "＋ Add to playlist", disabled: !backendAvailable };
              if (saveState.state === "saving") return { label: "Saving…", disabled: true };
              if (saveState.state === "queued") return { label: "Queued…", disabled: true };
              if (saveState.state === "saved") return { label: "✓ Saved", disabled: true };
              if (saveState.state === "failed") return { label: "Retry", disabled: false };
              return { label: "＋ Add to playlist", disabled: !backendAvailable };
            })();

            const showSaveSpinner = saveState?.state === "saving" || saveState?.state === "queued";
            const showPreviewSpinner = !showSaveSpinner && isPreparingPreview && !!preview?.active;
            const showReadyCheck = !showSaveSpinner && !showPreviewSpinner && isPrepared && saveState?.state !== "saved";

            const canPlayPreview = isPrepared || item.status === "ready";
            const canRetryPreview = isUnavailable || item.status === "failed" || saveState?.state === "failed";

            // Use deterministic artwork fallback instead of grey ♫
            const art = (
              <RecommendationArtworkImage artist={item.artist} title={item.title} artworkUrl={item.artwork_url} size={48} alt={`${item.title} artwork`} />
            );

            return (
              <article className="luno-recommendation-row" key={k}>
                {art}
                <button className="luno-recommendation-info" type="button" onClick={() => onPreview?.(item)}>
                  <strong>{item.title}</strong>
                  <span>{item.artist}</span>
                  <small style={statusIsError ? { color: "#f08080" } : undefined}>{statusText}</small>
                </button>

                {/* Center status icon: preview spinner / ready check */}
                <span className="luno-rec-center" style={{ width: 32, height: 32, display: "inline-flex", alignItems: "center", justifyContent: "center", flex: "0 0 32px" }}>
                  {showPreviewSpinner ? <DownloadRing progress={previewProgress} size={22} /> : showReadyCheck ? <span style={{ color: "var(--luno-accent)", fontSize: 18, lineHeight: 1 }}>✓</span> : null}
                </span>

                <div className="luno-recommendation-actions">
                  {canRetryPreview && saveState?.state !== "failed" && (
                    <button type="button" onClick={() => onPreview?.(item)}>
                      Retry
                    </button>
                  )}
                  {saveState?.state === "failed" && (
                    <button type="button" onClick={() => handleAddClick(item)}>
                      Retry
                    </button>
                  )}
                  {canPlayPreview && !saveState && (
                    <button type="button" onClick={() => onPreview?.(item)}>
                      Play
                    </button>
                  )}
                  {showSaveSpinner ? (
                    <span className="luno-rec-save-progress" style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
                      <DownloadRing progress={percent} size={28} />
                    </span>
                  ) : (
                    <button
                      type="button"
                      disabled={saveLabel.disabled}
                      aria-label={saveLabel.disabled && saveLabel.label === "✓ Saved" ? `Saved ${item.title}` : `Add ${item.title} to playlist`}
                      onClick={() => handleAddClick(item)}
                      title={playlists.length === 0 && !isLegacySaved ? "Create a playlist in Your Library first" : undefined}
                    >
                      {saveLabel.label}
                    </button>
                  )}
                </div>
              </article>
            );
          })}
           {!exhausted && onLoadMore && (
            <footer className="luno-recommendations-footer">
              {loadMoreError && <p role="alert">{loadMoreError}</p>}
              {loadingMore ? <span>Loading more...</span> : <button type="button" onClick={onLoadMore}>Load 25 more</button>}
            </footer>
          )}
        </div>
      )}
      {pickerItem && (
        <PlaylistPicker
          title={`Add “${pickerItem.title}” to playlist`}
          playlists={playlists}
          selectedTracks={[{
            id: safeKey(pickerItem),
            path: null,
            title: pickerItem.title,
            artist: pickerItem.artist,
            album: null,
            duration_ms: null,
            artwork_url: pickerItem.artwork_url ?? null,
            favorite: false,
            provider: null,
          }]}
          tracksById={tracksById}
          onArtwork={onArtwork}
          onPick={handlePick}
          onDismiss={() => setPickerItem(null)}
          onCreateNew={
            onCreatePlaylist
              ? () => {
                  setPickerItem(null);
                  onCreatePlaylist(pickerItem);
                }
              : undefined
          }
        />
      )}
    </section>
  );
}

import { useEffect, useState } from "react";
import type { Track } from "../shared/contracts";
import { Artwork } from "../components/Artwork";
import { Recommendations, type Recommendation, type RecommendationState } from "../components/Recommendations";
import { VideoStage, type VideoStageProps } from "../components/VideoStage";

export interface SongPageViewProps {
  track?: Track | null;
  recommendations?: Recommendation[];
  recommendationState?: RecommendationState;
  recommendationError?: string | null;
  recommendationLoadMoreError?: string | null;
  recommendationsExhausted?: boolean;
  recommendationsLoadingMore?: boolean;
  hasApiKey?: boolean;
  sessionActive?: boolean;
  discoverMode?: boolean;
  onRetryRecommendations?: () => void;
  onRefreshRecommendations?: () => void;
  onLoadMoreRecommendations?: () => void;
  onPreviewRecommendation?: (item: Recommendation) => void;
  onSaveRecommendation?: (item: Recommendation) => void;
  onFavoriteToggle?: (track: Track) => void;
  backendAvailable?: boolean;
  video?: VideoStageProps;
  videoConnection?: "connected" | "loading" | "disconnected";
  /** Requests (or retries) the music video for the current track. */
  onLoadVideo?: () => void;
}

export function SongPageView({ track = null, recommendations = [], recommendationState = "idle", recommendationError, recommendationLoadMoreError, recommendationsExhausted, recommendationsLoadingMore, hasApiKey = true, sessionActive = false, discoverMode = false, onRetryRecommendations, onRefreshRecommendations, onLoadMoreRecommendations, onPreviewRecommendation, onSaveRecommendation, onFavoriteToggle, backendAvailable = true, video, videoConnection = "disconnected", onLoadVideo }: SongPageViewProps) {
  const [lastTrack, setLastTrack] = useState<Track | null>(track);
  useEffect(() => { if (track) setLastTrack(track); }, [track]);
  const displayedTrack = track ?? lastTrack;
  const favoriteLabel = displayedTrack?.favorite ? "Remove from favorites" : "Add to favorites";
  return <div className="luno-song-page" data-testid="song-page">
    <style>{songPageStyles}</style>
    <div className="luno-song-page-backdrop" aria-hidden="true" style={displayedTrack?.artwork_url ? { backgroundImage: `url(${displayedTrack.artwork_url})` } : undefined} />
    <div className="luno-song-page-scroll">
      <main className="luno-song-page-content">
        <div className="luno-song-media-slot" data-video-slot="song-page"><VideoStage {...video} status={videoConnection === "loading" ? "loading" : videoConnection === "disconnected" ? "not-checked" : video?.status} artworkUrl={video?.artworkUrl ?? displayedTrack?.artwork_url} artworkAlt={displayedTrack ? `${displayedTrack.title} artwork` : "Artwork unavailable"} /><span className="luno-video-connection" role="status">{videoConnection === "loading" ? "Connecting video preview..." : videoConnection === "disconnected" ? "Video preview unavailable; showing artwork." : null}</span>{onLoadVideo && backendAvailable && videoConnection === "connected" && !video?.src && video?.status !== "loading" && <button type="button" className="luno-video-load" onClick={onLoadVideo}>{video?.status === "unavailable" || video?.status === "error" || video?.status === "fallback" ? "Retry music video" : "Load music video"}</button>}</div>
        <div className="luno-song-meta">{displayedTrack ? <><h1>{displayedTrack.title}</h1><p>{displayedTrack.artist || "Unknown artist"}</p><button type="button" aria-label={favoriteLabel} aria-describedby={!backendAvailable ? "song-page-favorite-help" : undefined} disabled={!backendAvailable} onClick={() => backendAvailable && onFavoriteToggle?.(displayedTrack)}>{displayedTrack.favorite ? "★" : "☆"} {displayedTrack.favorite ? "Favorited" : "Add to Favorites"}</button>{!backendAvailable && <p id="song-page-favorite-help" role="status">Favorites are unavailable while the backend is disconnected.</p>}</> : <><h1>Nothing playing</h1><p>Play a song to open its page.</p></>}</div>
        <Recommendations seed={displayedTrack} items={recommendations} state={recommendationState} error={recommendationError} loadMoreError={recommendationLoadMoreError} exhausted={recommendationsExhausted} loadingMore={recommendationsLoadingMore} hasApiKey={hasApiKey} sessionActive={sessionActive} discoverMode={discoverMode} onRetry={onRetryRecommendations} onRefresh={onRefreshRecommendations} onLoadMore={onLoadMoreRecommendations} onPreview={onPreviewRecommendation} onSave={onSaveRecommendation} />
      </main>
    </div>
  </div>;
}

const songPageStyles = `
.luno-song-page { position:relative; min-height:100%; overflow:hidden; background:rgba(18,18,18,.78); }
.luno-song-page-backdrop { position:absolute; inset:-8%; background-position:center; background-size:cover; filter:blur(42px); opacity:.22; transform:scale(1.08); }
.luno-song-page-scroll { position:relative; height:100%; overflow:auto; overscroll-behavior:contain; }
.luno-song-page-content { width:min(100%, 900px); margin:0 auto; padding:clamp(24px,5vw,64px) clamp(16px,5vw,56px) 64px; }
.luno-song-media-slot { position:relative; width:min(100%, 800px); aspect-ratio:16 / 9; margin:0 auto 24px; overflow:hidden; border:1px solid rgba(255,255,255,.12); border-radius:12px; background:var(--luno-bg-3); box-shadow:0 20px 60px rgba(0,0,0,.35); }
.luno-song-media-slot .luno-artwork { width:100% !important; height:100% !important; }
.luno-song-media-slot img { object-fit:cover; }
.luno-video-stage { position:relative; width:100%; height:100%; background:var(--luno-bg-3); }
.luno-video-stage video, .luno-video-stage .luno-artwork { display:block; width:100% !important; height:100% !important; object-fit:contain; }
.luno-video-stage .luno-artwork img { object-fit:cover; }
.luno-video-status { position:absolute; inset:auto 12px 12px; margin:0; padding:7px 10px; border-radius:6px; background:rgba(0,0,0,.7); color:var(--luno-text-2); text-align:center; }
.luno-video-fullscreen { position:absolute; right:10px; top:10px; border:0; border-radius:6px; padding:6px 9px; color:var(--luno-text); background:rgba(0,0,0,.7); cursor:pointer; }
.luno-video-fullscreen:disabled { opacity:.45; cursor:default; }
.luno-video-load { position:absolute; left:10px; top:10px; border:0; border-radius:999px; padding:7px 13px; font-size:12px; font-weight:700; color:#001a08; background:var(--luno-green); cursor:pointer; box-shadow:0 6px 16px rgba(0,0,0,.4); }
.luno-video-load:hover { filter:brightness(1.08); }
.luno-song-meta { text-align:center; margin-bottom:40px; }
.luno-song-meta h1 { margin:0; font-size:clamp(28px,5vw,48px); line-height:1.1; }
.luno-song-meta p { margin:10px 0 20px; color:var(--luno-text-2); font-size:18px; }
.luno-song-meta button, .luno-recommendations button { border:0; border-radius:999px; padding:8px 12px; color:var(--luno-text-2); background:var(--luno-bg-3); cursor:pointer; }
.luno-song-meta button:hover, .luno-recommendations button:hover:not(:disabled) { color:var(--luno-text); background:var(--luno-bg-4); }
.luno-recommendations { padding:22px 0; border-top:1px solid rgba(255,255,255,.1); }
.luno-recommendations-header { display:flex; align-items:start; justify-content:space-between; gap:16px; }
.luno-recommendations-header h2 { margin:0; font-size:20px; }
.luno-recommendations-header p, .luno-recommendations-hint { margin:6px 0 0; color:var(--luno-text-2); font-size:13px; }
.luno-recommendations-header button { font-size:20px; padding:2px 10px; }
.luno-recommendations-hint { color:var(--luno-green); }
.luno-recommendations-state { display:flex; gap:12px; align-items:center; padding:20px 0; color:var(--luno-text-2); }
.luno-recommendations-state.is-error { color:#f08080; }
.luno-recommendation-list { margin-top:12px; }
.luno-recommendation-row { display:flex; align-items:center; gap:12px; min-height:72px; padding:10px 0; border-bottom:1px solid rgba(255,255,255,.06); }
.luno-recommendation-info { flex:1; min-width:0; display:flex; flex-direction:column; align-items:start; gap:3px; border-radius:8px !important; padding:5px !important; background:transparent !important; text-align:left; }
.luno-recommendation-info strong, .luno-recommendation-info span, .luno-recommendation-info small { max-width:100%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.luno-recommendation-info strong { color:var(--luno-text); }.luno-recommendation-info span, .luno-recommendation-info small { color:var(--luno-text-2); }
.luno-recommendation-actions { display:flex; gap:6px; flex-wrap:wrap; justify-content:end; }.luno-recommendation-actions button { font-size:12px; padding:6px 9px; }.luno-recommendations-footer { padding:18px 0; }.luno-recommendations-footer p { color:#f08080; font-size:13px; }.luno-recommendations button:disabled { opacity:.45; cursor:default; }
@media (max-width:600px) { .luno-song-page-content { padding-top:20px; } .luno-recommendation-row { align-items:flex-start; } .luno-recommendation-actions { flex-direction:column; } }
`;

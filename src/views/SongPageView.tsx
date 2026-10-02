import { useEffect, useRef, useState } from "react";
import type { DownloadJob, Playlist, PreviewSnapshot, SaveState, Track } from "../shared/contracts";
import { Recommendations, type Recommendation, type RecommendationState } from "../components/Recommendations";
import { VideoStage, type VideoStageProps } from "../components/VideoStage";
import { isPlaceholderArtwork, RecommendationArtwork } from "../components/RecommendationArtwork";
import type { ArtworkFetcher } from "../state/track_artwork";

/** Pill toggle for song-page video mode (exported for direct testing). */
export function VideoModePill({ enabled, onToggle }: { enabled: boolean; onToggle?: () => void }) {
  return <button type="button" role="menuitemradio" aria-checked={enabled} data-testid="video-mode-toggle" className={`luno-video-mode-pill${enabled ? " is-on" : ""}`} onClick={onToggle}>
    <span className="luno-video-mode-knob" aria-hidden="true" />
    <span className="luno-video-mode-label">{enabled ? "On" : "Off"}</span>
  </button>;
}

export interface SongPageViewProps { track?: Track | null;
  /** The AUDIBLE track when it differs from `track` (a recommendation preview):
   * drives the title, artwork, backdrop, and video correlation. */
  nowPlaying?: Track | null;
  recommendations?: Recommendation[];
  recommendationState?: RecommendationState;
  recommendationError?: string | null;
  recommendationLoadMoreError?: string | null;
  recommendationsExhausted?: boolean;
  recommendationsLoadingMore?: boolean;
  discoverMode?: boolean;
  onToggleDiscoverMode?: () => void;
  /** Deprecated compatibility prop; discovery guidance now lives in the info tip. */
  discoverHint?: string | null;
  onRetryRecommendations?: () => void;
  onLoadMoreRecommendations?: () => void;
  onPreviewRecommendation?: (item: Recommendation) => void;
  onSaveRecommendation?: (item: Recommendation, playlistId?: string) => void;
  onFavoriteToggle?: (track: Track) => void;
  backendAvailable?: boolean;
  preview?: PreviewSnapshot | null;
  saved?: Record<string, SaveState>;
  playlists?: Playlist[];
  jobs?: Record<string, DownloadJob>;
  onCreatePlaylist?: (item: Recommendation) => void;
  tracksById?: ReadonlyMap<string, Track>;
  onArtwork?: ArtworkFetcher;
  video?: VideoStageProps;
  videoConnection?: "connected" | "loading" | "disconnected";
  /** Requests (or retries) the music video for the current track. */
  onLoadVideo?: () => void;
  /** Song-page video mode (auto-stage current + prefetch next 5). */
  videoMode?: boolean;
  onVideoModeChange?: (enabled: boolean) => void;
}

export function SongPageView({ track = null, nowPlaying = null, recommendations = [], recommendationState = "idle", recommendationError, recommendationLoadMoreError, recommendationsExhausted, recommendationsLoadingMore, discoverMode = false, onToggleDiscoverMode, onRetryRecommendations, onLoadMoreRecommendations, onPreviewRecommendation, onSaveRecommendation, onFavoriteToggle, backendAvailable = true, preview = null, saved = {}, playlists = [], jobs = {}, onCreatePlaylist, tracksById, onArtwork, video, videoConnection = "disconnected", onLoadVideo, videoMode = false, onVideoModeChange }: SongPageViewProps) {
  const [lastTrack, setLastTrack] = useState<Track | null>(nowPlaying ?? track);
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => { const next = nowPlaying ?? track; if (next) setLastTrack(next); }, [nowPlaying, track]);
  // Close the ⋯ menu on any outside click or Escape — standard popover rules.
  useEffect(() => {
    if (!menuOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") setMenuOpen(false); };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => { document.removeEventListener("pointerdown", onPointerDown); document.removeEventListener("keydown", onKeyDown); };
  }, [menuOpen]);
  const displayedTrack = nowPlaying ?? track ?? lastTrack;
  // The page has one native scroller. WKWebView video elements can swallow
  // wheel input, so only wheels over a video use this frame-batched fallback.
  const pageRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const root = pageRef.current;
    const scroller = root?.querySelector<HTMLElement>(".luno-song-page-scroll");
    if (!root || !scroller) return;
    let pendingDelta = 0;
    let frame = 0;
    const onWheel = (event: WheelEvent) => {
      if (Math.abs(event.deltaY) <= Math.abs(event.deltaX) || !(event.target instanceof Element) || !event.target.closest("video")) return;
      event.preventDefault();
      pendingDelta += event.deltaY;
      if (frame) return;
      frame = requestAnimationFrame(() => {
        scroller.scrollTop += pendingDelta;
        pendingDelta = 0;
        frame = 0;
      });
    };
    root.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      root.removeEventListener("wheel", onWheel);
      if (frame) cancelAnimationFrame(frame);
    };
  }, []);
  // Favorites toggle lives in the footer PlayerBar; the song page keeps only
  // title/artist here (see onFavoriteToggle prop retained for API compat).
  void onFavoriteToggle;
  const backdropArtwork = video?.artworkUrl ?? displayedTrack?.artwork_url;
  const hasBackdropArtwork = !!backdropArtwork && !isPlaceholderArtwork(backdropArtwork);
  const canRetryVideo = !!onLoadVideo && backendAvailable && videoConnection === "connected" && videoMode && !video?.src
    && (video?.status === "unavailable" || video?.status === "error" || video?.status === "fallback");
  return <div ref={pageRef} className="luno-song-page" data-testid="song-page">
    <style>{songPageStyles}</style>
    <div className="luno-song-page-backdrop" aria-hidden="true" style={hasBackdropArtwork ? { backgroundImage: `url(${backdropArtwork})` } : undefined}>
      {!hasBackdropArtwork && displayedTrack && !video?.artworkLoading && <RecommendationArtwork artist={displayedTrack.artist ?? ""} title={displayedTrack.title} size="100%" />}
    </div>
    {onVideoModeChange && backendAvailable && null}
    <div className="luno-song-page-scroll">
      <main className="luno-song-page-content">
        <div className="luno-song-media-slot" data-video-slot="song-page"><VideoStage {...video} status={videoConnection === "loading" ? "loading" : videoConnection === "disconnected" ? "not-checked" : video?.status} artworkUrl={video?.artworkUrl ?? displayedTrack?.artwork_url} artworkAlt={displayedTrack ? `${displayedTrack.title} artwork` : "Artwork unavailable"} artworkArtist={displayedTrack?.artist} artworkTitle={displayedTrack?.title} videoModeEnabled={videoMode} onVideoModeChange={onVideoModeChange} onRetry={canRetryVideo ? onLoadVideo : undefined} /><span className="luno-video-connection" role="status">{videoConnection === "loading" ? "Connecting video preview..." : videoConnection === "disconnected" ? "Video preview unavailable; showing artwork." : null}</span></div>
        <div className="luno-song-meta">{displayedTrack ? <><h1>{displayedTrack.title}</h1><p>{displayedTrack.artist || "Unknown artist"}</p></> : <><h1>Nothing playing</h1><p>Play a song to open its page.</p></>}</div>
        {onToggleDiscoverMode && <div className="luno-discover-toggle">
          <button type="button" className={`luno-discovery-switch${discoverMode ? " is-on" : ""}`} role="switch" aria-checked={discoverMode} aria-label={`Discovery ${discoverMode ? "on" : "off"}`} disabled={!backendAvailable} onClick={onToggleDiscoverMode}>
            <span className="luno-discovery-switch-label">Discovery</span>
            <span className="luno-discovery-switch-track" aria-hidden="true"><span /></span>
          </button>
        </div>}
        <Recommendations seed={track ?? lastTrack} items={recommendations} state={recommendationState} error={recommendationError} loadMoreError={recommendationLoadMoreError} exhausted={recommendationsExhausted} loadingMore={recommendationsLoadingMore} onRetry={onRetryRecommendations} onLoadMore={onLoadMoreRecommendations} onPreview={onPreviewRecommendation} onSave={onSaveRecommendation} preview={preview} saved={saved} playlists={playlists} jobs={jobs} backendAvailable={backendAvailable} onCreatePlaylist={onCreatePlaylist} tracksById={tracksById} onArtwork={onArtwork} />
      </main>
    </div>
  </div>;
}

const songPageStyles = `
.luno-song-page { position:relative; height:100%; min-height:100%; overflow:hidden; background:rgba(18,18,18,.78); }
.luno-song-page-backdrop { position:absolute; inset:-8%; background-position:center; background-size:cover; filter:blur(42px); opacity:.22; transform:scale(1.08); overflow:hidden; }
.luno-song-page-backdrop .luno-rec-art { width:100% !important; height:100% !important; border-radius:0 !important; }
.luno-video-mode-pill { display:flex; align-items:center; gap:8px; width:100%; border:0; border-radius:999px; padding:4px 4px 4px 4px; background:rgba(255,255,255,.09); cursor:pointer; }
.luno-video-mode-pill .luno-video-mode-knob { position:relative; left:auto; display:inline-block; width:30px; height:16px; border-radius:999px; background:rgba(255,255,255,.25); transition:background .15s ease; }
.luno-video-mode-pill .luno-video-mode-knob::after { content:""; position:absolute; top:2px; left:2px; width:12px; height:12px; border-radius:999px; background:#fff; transition:left .15s ease; }
.luno-video-mode-pill.is-on .luno-video-mode-knob { background:var(--luno-green,#1db954); }
.luno-video-mode-pill.is-on .luno-video-mode-knob::after { left:16px; }
.luno-video-mode-label { font-size:12px; font-weight:700; color:var(--luno-text-2); }
.luno-video-mode-pill.is-on .luno-video-mode-label { color:var(--luno-green,#1db954); }
.luno-song-page-backdrop { position:absolute; inset:-8%; background-position:center; background-size:cover; filter:blur(42px); opacity:.22; transform:scale(1.08); }
.luno-song-page-scroll { position:relative; height:100%; overflow:auto; overscroll-behavior:contain; scrollbar-gutter:stable; }
.luno-song-page-content { width:100%; margin:0 auto; padding:0 0 64px; }
.luno-song-media-slot { position:relative; width:100%; aspect-ratio:16 / 9; margin:0 0 24px; overflow:hidden; border:0; border-radius:0; background:var(--luno-bg-3); box-shadow:none; }
.luno-song-media-slot .luno-artwork { width:100% !important; height:100% !important; }
.luno-song-media-slot img { object-fit:cover; }
.luno-video-stage { position:relative; width:100%; height:100%; background:var(--luno-bg-3); }
.luno-video-stage video, .luno-video-stage .luno-artwork { display:block; width:100% !important; height:100% !important; object-fit:contain; }
.luno-video-stage .luno-artwork img { object-fit:cover; }
.luno-video-fullscreen { position:absolute; right:10px; top:10px; border:0; border-radius:6px; padding:6px 9px; color:var(--luno-text); background:rgba(0,0,0,.7); cursor:pointer; }
.luno-video-hover-controls { position:absolute; right:10px; top:10px; z-index:6; display:flex; gap:6px; align-items:center; opacity:0; pointer-events:none; transition:opacity .15s ease; }
.luno-video-stage:hover .luno-video-hover-controls, .luno-video-stage:focus-within .luno-video-hover-controls { opacity:1; pointer-events:auto; }
.luno-video-hover-controls .luno-video-fullscreen { position:static; }
.luno-video-hover-error { padding:6px 9px; border-radius:6px; background:rgba(0,0,0,.75); color:var(--luno-text); font-size:12px; white-space:nowrap; }
.luno-video-hover-retry { border:0; border-radius:6px; padding:6px 9px; color:var(--luno-text); background:rgba(0,0,0,.75); font-size:12px; cursor:pointer; }
.luno-video-hover-retry:hover { background:var(--luno-bg-4); }
.luno-video-volume { display:flex; align-items:center; gap:7px; min-height:30px; padding:0 9px; border-radius:6px; color:#fff; background:rgba(0,0,0,.7); }
.luno-video-volume input { width:min(22vw,150px); accent-color:var(--luno-green,#1db954); }
.luno-video-mode-mini { position:relative; width:38px; height:22px; border:0; border-radius:999px; background:rgba(255,255,255,.28); cursor:pointer; transition:background .15s ease; }
.luno-video-mode-mini.is-on { background:var(--luno-green,#1db954); }
.luno-video-mode-mini .luno-video-mode-mini-knob { position:absolute; top:3px; left:3px; width:16px; height:16px; border-radius:999px; background:#fff; transition:left .15s ease; }
.luno-video-mode-mini.is-on .luno-video-mode-mini-knob { left:19px; }
.luno-video-fullscreen:disabled { opacity:.45; cursor:default; }
.luno-song-meta { text-align:center; margin-bottom:40px; }
.luno-song-meta h1 { margin:0; font-size:clamp(28px,5vw,48px); line-height:1.1; }
.luno-song-meta p { margin:10px 0 20px; color:var(--luno-text-2); font-size:18px; }
.luno-song-meta button, .luno-recommendations button { border:0; border-radius:999px; padding:8px 12px; color:var(--luno-text-2); background:var(--luno-bg-3); cursor:pointer; }
.luno-song-meta button:hover, .luno-recommendations button:hover:not(:disabled) { color:var(--luno-text); background:var(--luno-bg-4); }
.luno-recommendations { padding:22px 0; border-top:1px solid rgba(255,255,255,.1); }
.luno-discover-toggle { display:flex; justify-content:center; margin:0 0 16px; }
.luno-discovery-switch { display:inline-flex; align-items:center; gap:9px; padding:6px 10px !important; border-radius:999px !important; background:var(--luno-bg-3) !important; color:var(--luno-text-2) !important; font-size:12px; font-weight:650; }
.luno-discovery-switch.is-on { color:var(--luno-green,#1db954) !important; }
.luno-discovery-switch:disabled { opacity:.55; cursor:default; }
.luno-discovery-switch-track { position:relative; width:30px; height:17px; border-radius:999px; background:rgba(255,255,255,.22); transition:background .15s ease; }
.luno-discovery-switch-track span { position:absolute; top:2px; left:2px; width:13px; height:13px; border-radius:50%; background:white; transition:transform .15s ease; }
.luno-discovery-switch.is-on .luno-discovery-switch-track { background:var(--luno-green,#1db954); }
.luno-discovery-switch.is-on .luno-discovery-switch-track span { transform:translateX(13px); }
.luno-recommendations-header { display:flex; align-items:center; gap:8px; }
.luno-recommendations-title { display:flex; align-items:center; gap:7px; }
.luno-recommendations-header h2 { margin:0; font-size:20px; }
.luno-info-tip { display:inline-flex; align-items:center; justify-content:center; width:22px; height:22px; padding:0 !important; color:var(--luno-text-2) !important; background:transparent !important; }
.luno-info-tip:hover { color:var(--luno-text) !important; background:var(--luno-bg-3) !important; }
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

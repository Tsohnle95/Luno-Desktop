import { useEffect, useRef, useState, type SyntheticEvent } from "react";
import { RecommendationArtworkImage } from "./RecommendationArtwork";
import { IconVolume } from "./icons";

export type VideoStageStatus = "not-checked" | "loading" | "ready" | "playing" | "paused" | "ended" | "unavailable" | "error" | "fallback";

export interface VideoStageProps {
  src?: string | null;
  status?: VideoStageStatus;
  positionMs?: number;
  playing?: boolean;
  artworkUrl?: string | null;
  artworkAlt?: string;
  artworkArtist?: string | null;
  artworkTitle?: string | null;
  /** 0–100 while a download is running; shown on the loading overlay. */
  downloadProgress?: number | null;
  onResize?: (size: { width: number; height: number }) => void;
  onSeek?: (positionMs: number) => void;
  onPause?: (positionMs: number) => void;
  onResume?: (positionMs: number) => void;
  onEnded?: () => void;
  onError?: () => void;
  onFullscreenChange?: (fullscreen: boolean) => void;
  /** Master audio volume shown with the fullscreen hover controls. */
  volume?: number;
  onVolume?: (volume: number) => void;
  onVolumeCommit?: (volume: number) => void;
  /** Song-page video mode (hover pill). Undefined hides the control. */
  videoModeEnabled?: boolean;
  onVideoModeChange?: (enabled: boolean) => void;
}

const fallbackStatuses = new Set<VideoStageStatus>(["not-checked", "unavailable", "error", "fallback"]);

/** Ring geometry for the download-progress badge (viewBox 36, r 15.5). */
export const VIDEO_RING_LENGTH = 2 * Math.PI * 15.5;

/** Maps a 0–100 download percentage onto the ring's dash offset (clamped). */
export function ringDashOffset(progressPercent: number): number {
  const percent = Math.min(100, Math.max(0, Number.isFinite(progressPercent) ? progressPercent : 0));
  return Math.round(VIDEO_RING_LENGTH * (1 - percent / 100) * 100) / 100;
}

/**
 * Whether a video that reached its end must STAY on its final frame instead
 * of being restarted by the transport-follow effect. While the SONG keeps
 * playing past the clip's end, the hold stays up (the backend heartbeat
 * otherwise flips an ended video back to "Playing", restarting it from zero
 * in a loop until the song finished). The hold releases only when the
 * authoritative audio position moves meaningfully BACKWARD — a real seek
 * back before the end point means the user wants the video again.
 */
export function endedHoldShouldStay(holdAtMs: number | null, playing: boolean, positionMs: number): boolean {
  if (holdAtMs === null) return false;
  if (!playing) return true;
  return Math.max(0, positionMs) >= holdAtMs - 1500;
}

/**
 * Sync thresholds. The insight that fixes the pause/resume glitches: between
 * heartbeat publications (400 ms) the <video> free-runs ahead of the last
 * published AUDIO position, so a frozen element is routinely ~300–500 ms
 * "off" from the prop WITHOUT being meaningfully desynced. Chasing those
 * sub-second deltas produced a visible rewind on every pause and a jump on
 * every resume. Only genuine navigations/desyncs justify repositioning.
 */
export const VIDEO_SYNC = {
  /** Paused scrub-follow: move the frozen frame only for real jumps. */
  PAUSED_FOLLOW_MS: 800,
  /** Resume: play in place when close; seek-rejoin beyond this. */
  RESUME_REJOIN_MS: 600,
  /** Playing coarse correction: tightened from 1.5 s so medium scrubs and
   * buffering desyncs actually correct (sub-1.5 s offsets used to persist). */
  PLAYING_CORRECT_MS: 900,
  /** While an async seek is in flight, ignore tick updates near its target —
   * WKWebView RESTARTS the seek on every currentTime assignment, which made
   * big scrubs land late (each 400 ms tick pushed the target forward). */
  SEEK_THRASH_GUARD_MS: 500,
} as const;

/** What the transport-sync effect should do this run (pure; unit-tested). */
export type VideoSyncDecision =
  | { action: "hold" }
  | { action: "none" }
  | { action: "pause" }
  | { action: "play"; seekToMs: number | null }
  | { action: "seek"; toMs: number };

export function planVideoSync(input: {
  endedHoldAtMs: number | null;
  playing: boolean;
  positionMs: number;
  elementPaused: boolean;
  elementSeconds: number;
  elementSeeking?: boolean;
  pendingSeekMs?: number | null;
}): VideoSyncDecision {
  const { endedHoldAtMs, playing, positionMs, elementPaused, elementSeconds } = input;
  if (endedHoldShouldStay(endedHoldAtMs, playing, positionMs)) return { action: "hold" };
  const audioMs = Math.max(0, positionMs);
  const driftMs = Math.abs(elementSeconds * 1000 - audioMs);
  if (!playing) {
    if (!elementPaused) return { action: "pause" };
    // PAUSED: freeze exactly where the user saw it. Only a REAL navigation
    // moves the frame; heartbeat-staleness deltas must be ignored or every
    // pause visibly rewinds the picture.
    if (driftMs > VIDEO_SYNC.PAUSED_FOLLOW_MS) return { action: "seek", toMs: audioMs };
    return { action: "none" };
  }
  if (elementPaused) {
    // RESUME: play IN PLACE when close enough — re-seeking every unpause is
    // the glitch. A seek-rejoin is reserved for genuine desyncs (buffering
    // stalls, missed events, mode re-enable mid-song).
    if (driftMs > VIDEO_SYNC.RESUME_REJOIN_MS) return { action: "play", seekToMs: audioMs };
    return { action: "play", seekToMs: null };
  }
  // PLAYING: never restart an in-flight seek whose target is still current.
  if (input.elementSeeking === true) {
    const pending = input.pendingSeekMs;
    if (pending !== null && pending !== undefined && Math.abs(audioMs - pending) <= VIDEO_SYNC.SEEK_THRASH_GUARD_MS) {
      return { action: "none" };
    }
  }
  if (driftMs >= VIDEO_SYNC.PLAYING_CORRECT_MS && positionMs > 0) {
    return { action: "play", seekToMs: audioMs };
  }
  return { action: "none" };
}

/** The green download ring, reusable outside the media stage (player bar). */
export function DownloadRing({ progress, size = 64 }: { progress: number | null; size?: number }) {
  const determinate = typeof progress === "number";
  return (
    <span className={`luno-video-progress${determinate ? " is-determinate" : ""}${progress === 0 ? " is-starting" : ""}`} style={{ width: size, height: size }}>
      <svg viewBox="0 0 36 36" aria-hidden="true">
        <circle className="luno-video-progress-track" cx="18" cy="18" r="15.5" />
        <circle className="luno-video-progress-fill" cx="18" cy="18" r="15.5" strokeDasharray={VIDEO_RING_LENGTH} strokeDashoffset={determinate ? ringDashOffset(progress) : VIDEO_RING_LENGTH * 0.72} />
      </svg>
      {determinate && <b className="luno-video-progress-pct" style={size <= 32 ? { fontSize: 9 } : undefined}>{Math.round(progress)}%</b>}
    </span>
  );
}

/** Self-contained chrome for the loading/download overlay (any host view). */
const stageChromeStyles = `
.luno-video-loading { position:absolute; inset:0; z-index:5; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:12px; background:rgba(0,0,0,.35); }
.luno-video-progress { position:relative; width:64px; height:64px; display:inline-flex; align-items:center; justify-content:center; }
.luno-video-progress svg { width:100%; height:100%; overflow:visible; }
.luno-video-progress circle { fill:none; stroke-width:3.5; }
.luno-video-progress-track { stroke:rgba(255,255,255,.16); }
.luno-video-progress-fill { stroke:var(--luno-green,#1db954); stroke-linecap:round; transition:stroke-dashoffset .45s linear; }
.luno-video-progress.is-determinate svg { transform:rotate(-90deg); }
.luno-video-progress.is-starting svg { animation:luno-video-spin 1.1s linear infinite; }
.luno-video-progress:not(.is-determinate) svg { animation:luno-video-spin 1.1s linear infinite; }
.luno-video-progress:not(.is-determinate) .luno-video-progress-fill { transition:none; }
@keyframes luno-video-spin { from { transform:rotate(0deg); } to { transform:rotate(360deg); } }
.luno-video-progress-pct { position:absolute; font-size:13px; font-weight:700; color:#fff; font-variant-numeric:tabular-nums; }
.luno-video-loading-caption { padding:6px 10px; border-radius:6px; background:rgba(0,0,0,.55); color:var(--luno-text-2,#a7a7a7); font-size:12px; text-align:center; }
`;

/** Presentation-only video surface. The supplied audio position remains authoritative. */
export function VideoStage({ src, status = "not-checked", positionMs = 0, playing = false, artworkUrl, artworkAlt = "Album artwork", artworkArtist = "", artworkTitle = "", onResize, onSeek, onPause, onResume, onEnded, onError, onFullscreenChange, volume = 1, onVolume, onVolumeCommit, videoModeEnabled, onVideoModeChange, downloadProgress: videoProgress }: VideoStageProps & { downloadProgress?: number | null }) {
  const stageRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  // Position we last pushed INTO the element. Every programmatic currentTime
  // assignment raises a `seeked` event; echoing that straight back as a
  // video_seek command produced a self-sustaining IPC/render loop several
  // times per second. Only report seeks that differ from what we pushed.
  const syncedToMsRef = useRef(0);
  // Audio position (ms) at which the clip fired its `ended` event, or null.
  // While set, transport-follow must NOT restart the finished video (see
  // endedHoldShouldStay); it releases on a real seek back before that point.
  const endedHoldRef = useRef<number | null>(null);
  // Target of the seek currently in flight (ms), or null. Guards against
  // seek thrash: WKWebView RESTARTS an async seek whenever currentTime is
  // reassigned, so heartbeat ticks re-issuing the same scrub made big jumps
  // land late. Cleared on `seeked` / when the element reports it is done.
  const pendingSeekRef = useRef<number | null>(null);
  const latestPositionRef = useRef(positionMs);
  latestPositionRef.current = positionMs;
  const [fullscreenWanted, setFullscreenWanted] = useState(false);
  const [volumeDraft, setVolumeDraft] = useState(volume);
  useEffect(() => setVolumeDraft(volume), [volume]);
  const showVideo = Boolean(src) && !fallbackStatuses.has(status);
  // Fullscreen is CSS-driven: the stage expands to fill the entire app
  // window (video + hover controls only) — WKWebView cannot do native
  // element fullscreen, and OS-level window fullscreen was overkill.
  const toggleFullscreen = () => {
    const next = !fullscreenWanted;
    setFullscreenWanted(next);
    onFullscreenChange?.(next);
  };
  // Escape always leaves CSS fullscreen — the stage is the only visible
  // surface then, so the keyboard path must live here.
  useEffect(() => {
    if (!fullscreenWanted) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") toggleFullscreen();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  });
  // Hover controls are pure CSS (opacity on .luno-video-stage:hover), so they
  // work identically over video and over artwork, and are unit-testable.
  const hoverControls = <div className="luno-video-hover-controls" data-testid="video-hover-controls">
    {fullscreenWanted && onVolume && <label className="luno-video-volume"><IconVolume size={16} /><span className="sr-only">Volume</span><input aria-label="Fullscreen volume" type="range" min="0" max="1" step="0.01" value={Math.min(1, Math.max(0, volumeDraft))} onChange={(event) => { const next = Number(event.currentTarget.value); setVolumeDraft(next); onVolume(next); }} onPointerUp={(event) => onVolumeCommit?.(Number(event.currentTarget.value))} onKeyUp={(event) => onVolumeCommit?.(Number(event.currentTarget.value))} /></label>}
    {onVideoModeChange !== undefined && <button type="button" className={`luno-video-mode-mini${videoModeEnabled ? " is-on" : ""}`} role="switch" aria-checked={!!videoModeEnabled} aria-label={videoModeEnabled ? "Music mode on" : "Music mode off"} title="Music mode" onClick={() => onVideoModeChange(!videoModeEnabled)}><span className="luno-video-mode-mini-knob" aria-hidden="true" /></button>}
    {showVideo && <button type="button" className="luno-video-fullscreen" aria-label={fullscreenWanted ? "Exit fullscreen" : "Enter fullscreen"} onClick={toggleFullscreen}>⛶</button>}
  </div>;

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage || !onResize || !showVideo) return;
    const report = () => onResize({ width: stage.clientWidth, height: stage.clientHeight });
    report();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(report);
    observer?.observe(stage);
    return () => observer?.disconnect();
  }, [onResize, showVideo]);

  useEffect(() => {
    // A new source is a new media timeline — a stale end-hold must never
    // survive a track change (or the next clip would refuse to start).
    endedHoldRef.current = null;
  }, [src]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video || !showVideo) return;
    // All transport decisions come from ONE tested planner — see
    // planVideoSync for why each threshold exists (pause-rewind glitch,
    // resume jump, scrub seek-thrash).
    const decision = planVideoSync({
      endedHoldAtMs: endedHoldRef.current,
      playing,
      positionMs,
      elementPaused: video.paused,
      elementSeconds: video.currentTime,
      elementSeeking: video.seeking,
      pendingSeekMs: pendingSeekRef.current,
    });
    switch (decision.action) {
      case "hold":
        return;
      case "pause":
        video.pause();
        return;
      case "seek":
        syncedToMsRef.current = decision.toMs;
        pendingSeekRef.current = decision.toMs;
        video.currentTime = decision.toMs / 1000;
        return;
      case "play": {
        if (decision.seekToMs !== null) {
          syncedToMsRef.current = decision.seekToMs;
          pendingSeekRef.current = decision.seekToMs;
          video.currentTime = decision.seekToMs / 1000;
        }
        void video.play().catch(() => undefined);
        return;
      }
      case "none":
        break;
    }
    if (!video.seeking) pendingSeekRef.current = null;
  }, [positionMs, playing, showVideo]);

  const time = () => (videoRef.current?.currentTime ?? positionMs / 1000) * 1000;
  const handleError = () => onError?.();
  const handleSeeked = () => {
    if (!showVideo) return;
    // The in-flight seek landed — future ticks may correct freely again.
    pendingSeekRef.current = null;
    const ms = time();
    if (Math.abs(ms - syncedToMsRef.current) > 250) onSeek?.(ms);
  };
  const handlePause = () => { if (showVideo) onPause?.(time()); };
  const handlePlay = () => { if (showVideo) onResume?.(time()); };
  const handleEnded = () => {
    if (!showVideo) return;
    // Freeze on the final frame: record WHERE the clip ended (in audio-ms)
    // so transport-follow holds instead of restarting it while the song runs.
    endedHoldRef.current = Math.max(0, latestPositionRef.current);
    onEnded?.();
  };
  const onVideoError = (event: SyntheticEvent<HTMLVideoElement>) => { event.currentTarget.pause(); if (showVideo) handleError(); };

  return <div ref={stageRef} className={`luno-video-stage${fullscreenWanted ? " is-fullscreen" : ""}`} data-testid="video-stage" aria-label="Song video preview">
    <style>{stageChromeStyles}</style>
    {showVideo ? <video ref={videoRef} src={src!} preload="auto" playsInline muted aria-label="Song video" onError={onVideoError} onSeeked={handleSeeked} onPause={handlePause} onPlay={handlePlay} onEnded={handleEnded} /> : <RecommendationArtworkImage artworkUrl={artworkUrl} alt={artworkAlt} artist={artworkArtist ?? ""} title={artworkTitle ?? ""} size="100%" />}
    {status === "loading" && <div className="luno-video-loading" role="status">
      <DownloadRing progress={typeof videoProgress === "number" ? videoProgress : null} />
      {/* null covers lookup, extractor setup, and unknown-total transfers;
       * only a byte-derived percentage is determinate. */}
      <span className="luno-video-loading-caption">{typeof videoProgress === "number" ? `Downloading video… ${Math.round(videoProgress)}%` : "Preparing video…"}</span>
    </div>}
    {status === "unavailable" && <p className="luno-video-status" role="status">Video unavailable. Showing artwork.</p>}
    {(status === "error" || status === "fallback") && <p className="luno-video-status" role="alert">Video preview failed. Showing artwork.</p>}
    {onFullscreenChange && <style>{`.luno-video-stage.is-fullscreen { position:fixed; inset:0; z-index:1000; width:100vw; height:100vh; border-radius:0; background:#000; visibility:visible; } .luno-video-stage.is-fullscreen video { object-fit:contain; } .luno-shell:has(.luno-video-stage.is-fullscreen) > *:not(main), .luno-shell:has(.luno-video-stage.is-fullscreen) main > .luno-main-content { visibility:hidden; } .luno-shell:has(.luno-video-stage.is-fullscreen) footer, .luno-shell:has(.luno-video-stage.is-fullscreen) aside { visibility:hidden; }`}</style>}
    {hoverControls}
  </div>;
}

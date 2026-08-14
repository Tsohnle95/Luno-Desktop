import { useEffect, useRef, useState, type SyntheticEvent } from "react";
import { Artwork } from "./Artwork";

export type VideoStageStatus = "not-checked" | "loading" | "ready" | "playing" | "paused" | "ended" | "unavailable" | "error" | "fallback";

export interface VideoStageProps {
  src?: string | null;
  status?: VideoStageStatus;
  positionMs?: number;
  playing?: boolean;
  artworkUrl?: string | null;
  artworkAlt?: string;
  onResize?: (size: { width: number; height: number }) => void;
  onSeek?: (positionMs: number) => void;
  onPause?: (positionMs: number) => void;
  onResume?: (positionMs: number) => void;
  onEnded?: () => void;
  onError?: () => void;
  onFullscreenChange?: (fullscreen: boolean) => void;
}

const fallbackStatuses = new Set<VideoStageStatus>(["not-checked", "unavailable", "error", "fallback"]);

/** Presentation-only video surface. The supplied audio position remains authoritative. */
export function VideoStage({ src, status = "not-checked", positionMs = 0, playing = false, artworkUrl, artworkAlt = "Album artwork", onResize, onSeek, onPause, onResume, onEnded, onError, onFullscreenChange }: VideoStageProps) {
  const stageRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const showVideo = Boolean(src) && !fallbackStatuses.has(status);

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
    const video = videoRef.current;
    if (!video || !showVideo) return;
    const target = Math.max(0, positionMs) / 1000;
    if (Math.abs(video.currentTime - target) > 0.2) video.currentTime = target;
    if (playing && video.paused) void video.play().catch(() => undefined);
    if (!playing && !video.paused) video.pause();
  }, [positionMs, playing, showVideo]);

  useEffect(() => {
    const changed = () => {
      const next = document.fullscreenElement === stageRef.current;
      setFullscreen(next);
      onFullscreenChange?.(next);
    };
    document.addEventListener("fullscreenchange", changed);
    return () => document.removeEventListener("fullscreenchange", changed);
  }, [onFullscreenChange]);

  const toggleFullscreen = async () => {
    if (document.fullscreenElement) await document.exitFullscreen();
    else await stageRef.current?.requestFullscreen();
  };
  const time = () => (videoRef.current?.currentTime ?? positionMs / 1000) * 1000;
  const handleError = () => onError?.();
  const handleSeeked = () => { if (showVideo) onSeek?.(time()); };
  const handlePause = () => { if (showVideo) onPause?.(time()); };
  const handlePlay = () => { if (showVideo) onResume?.(time()); };
  const handleEnded = () => { if (showVideo) onEnded?.(); };
  const onVideoError = (event: SyntheticEvent<HTMLVideoElement>) => { event.currentTarget.pause(); if (showVideo) handleError(); };

  return <div ref={stageRef} className="luno-video-stage" data-testid="video-stage" aria-label="Song video preview">
    {showVideo ? <video ref={videoRef} src={src!} preload="metadata" playsInline aria-label="Song video" onError={onVideoError} onSeeked={handleSeeked} onPause={handlePause} onPlay={handlePlay} onEnded={handleEnded} /> : <Artwork src={artworkUrl} alt={artworkAlt} size="100%" />}
    {status === "loading" && <p className="luno-video-status" role="status">Loading video preview...</p>}
    {status === "unavailable" && <p className="luno-video-status" role="status">Video unavailable. Showing artwork.</p>}
    {(status === "error" || status === "fallback") && <p className="luno-video-status" role="alert">Video preview failed. Showing artwork.</p>}
    <button type="button" className="luno-video-fullscreen" aria-label={fullscreen ? "Exit fullscreen" : "Enter fullscreen"} onClick={() => void toggleFullscreen()} disabled={!showVideo}>⛶</button>
  </div>;
}

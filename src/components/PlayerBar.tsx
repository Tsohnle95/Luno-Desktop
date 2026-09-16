import { useEffect, useRef, useState, type CSSProperties } from "react";
import type { PlaybackSnapshot, Track } from "../shared/contracts";
import { Artwork } from "./Artwork";
import { RecommendationArtwork } from "./RecommendationArtwork";
import { IconPlay, IconPause, IconQueueList, IconRepeat, IconShuffle, IconSkipBack, IconSkipForward, IconVolume } from "./icons";

export interface PlayerBarProps {
  track?: Track | null;
  playback?: PlaybackSnapshot | null;
  artwork?: string | null;
  volume?: number;
  accent?: string;
  onPlayPause?: () => void;
  onNext?: () => void;
  onPrevious?: () => void;
  onSeek?: (positionMs: number) => void;
  /** Live preview while the thumb is being dragged (frame-coalesced). */
  onVolume?: (volume: number) => void;
  /** Final commit on release (also persists the setting). */
  onVolumeCommit?: (volume: number) => void;
  onFavoriteToggle?: (track: Track) => void;
  onOpenTrack?: (track: Track) => void;
  backendAvailable?: boolean;
  /** Engine shuffle/repeat mirrors for the Spotify-style transport cluster. */
  shuffle?: boolean;
  repeat?: "off" | "one" | "all";
  onToggleShuffle?: () => void;
  onCycleRepeat?: () => void;
  /** Queue panel visibility toggle in the right action cluster. */
  queueOpen?: boolean;
  onToggleQueue?: () => void;
}

const formatTime = (ms: number) => { const seconds = Math.max(0, Math.floor(ms / 1000)); return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`; };
const clamp = (value: number, min: number, max: number) => Math.min(Math.max(min, value), max);

/**
 * Footer mini player: album art + track info on the left, transport controls
 * centered, favorite/volume on the right, and a wide centered scrubber along
 * the bottom. Both sliders are ref-driven and pointer-owned: the range inputs
 * carry no controlled `value` (only `defaultValue`), the browser owns the
 * thumb during the gesture (we never call `setPointerCapture` on a native
 * range input — WKWebView keeps that capture indefinitely, gluing the thumb
 * to the pointer with no `pointerup`), and the gesture ends on a window-level
 * `pointerup`/`pointercancel`. The volume slider tracks the thumb live during
 * the drag (one frame-coalesced command at a time) and commits once on
 * release; the scrubber commits one seek on release and holds the thumb at
 * the committed position until the backend catches up (no snap-back). The
 * volume slider mirrors the persisted volume value when idle.
 */
export function PlayerBar({ track = null, playback = null, artwork = null, volume = 1, accent = "var(--luno-green)", onPlayPause, onNext, onPrevious, onSeek, onVolume, onVolumeCommit, onFavoriteToggle, onOpenTrack, backendAvailable = true, shuffle = false, repeat = "off", onToggleShuffle, onCycleRepeat, queueOpen = true, onToggleQueue }: PlayerBarProps) {
  const position = playback?.position_ms ?? 0;
  const duration = playback?.duration_ms ?? 0;
  const playing = playback?.status === "playing";
  const max = Math.max(duration, 1);

  // ---- Seek scrubber: ref-owned input, local display value only ----
  const seekRef = useRef<HTMLInputElement>(null);
  const seekDraggingRef = useRef(false);
  const [seekDragging, setSeekDragging] = useState(false);
  const seekLiveRef = useRef(0);
  // value shown in the time label (never a controlled `value` on the input)
  const [seekDisplay, setSeekDisplay] = useState(position);
  // committed position held until the backend actually arrives
  const [pendingSeek, setPendingSeek] = useState<number | null>(null);
  const pendingTimeoutRef = useRef<number | null>(null);

  const startSeekDrag = () => {
    seekDraggingRef.current = true;
    setSeekDragging(true);
  };
  const clearPendingTimer = () => {
    if (pendingTimeoutRef.current !== null) {
      window.clearTimeout(pendingTimeoutRef.current);
      pendingTimeoutRef.current = null;
    }
  };
  const commitSeek = () => {
    if (!seekDraggingRef.current) return;
    clearPendingTimer();
    const el = seekRef.current;
    if (!el) return;
    const committed = clamp(Number(el.value), 0, max);
    seekDraggingRef.current = false;
    setSeekDragging(false);
    setSeekDisplay(committed);
    setPendingSeek(committed);
    onSeek?.(committed);
  };
  const commitSeekKeyboard = () => {
    seekDraggingRef.current = true;
    commitSeek();
  };
  // While a gesture is active, commit on ANY window-level release: the range
  // input handles the thumb drag natively, so the release can land anywhere
  // (including outside the element) without us holding pointer capture. We
  // deliberately do NOT end the gesture on button-less pointermove: WKWebView
  // can report `buttons: 0` mid-drag, which would commit a stale intermediate
  // value and swallow the real release commit (the slider then jumps around).
  useEffect(() => {
    if (!seekDragging) return;
    const release = () => commitSeek();
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", release);
    return () => {
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", release);
    };
  }, [seekDragging]);
  // While a gesture is inactive, keep the DOM value aligned with whatever the
  // committed/backend state dictates; never touch it mid-drag. The hold is
  // released only when the backend position is NEAR the committed target on
  // either side, so backward seeks (where the stale pre-seek position is
  // numerically ABOVE the target) do not release early and snap the thumb back.
  useEffect(() => {
    const el = seekRef.current;
    if (!el || seekDraggingRef.current) return;
    if (pendingSeek !== null && Math.abs(position - pendingSeek) <= 200) {
      setPendingSeek(null);
      return;
    }
    const target = pendingSeek !== null ? pendingSeek : position;
    const display = clamp(target, 0, max);
    setSeekDisplay(display);
    const current = Number(el.value);
    if (current !== display) el.value = String(display);
  }, [position, pendingSeek, max]);
  // Fallback: drop the hold if the backend never catches up (~1500ms).
  useEffect(() => {
    if (pendingSeek === null) return;
    clearPendingTimer();
    pendingTimeoutRef.current = window.setTimeout(() => setPendingSeek(null), 1500);
    return () => clearPendingTimer();
  }, [pendingSeek]);

  // ---- Volume slider: ref-owned input, always mirrors prop when idle ----
  const volumeRef = useRef<HTMLInputElement>(null);
  const volumeDraggingRef = useRef(false);
  const [volumeDragging, setVolumeDragging] = useState(false);
  // Live preview state: the latest value not yet flushed, coalesced to one
  // `onVolume` command per animation frame so the audio level tracks the
  // thumb during the drag instead of jumping at release.
  const volumeLiveRef = useRef<number | null>(null);
  const volumeFrameRef = useRef<number | null>(null);

  const startVolumeDrag = () => {
    volumeDraggingRef.current = true;
    setVolumeDragging(true);
  };
  const flushVolumeLive = () => {
    volumeFrameRef.current = null;
    const value = volumeLiveRef.current;
    if (value !== null) {
      volumeLiveRef.current = null;
      onVolume?.(value);
    }
  };
  const previewVolume = (value: number) => {
    if (!volumeDraggingRef.current) return;
    volumeLiveRef.current = value;
    if (volumeFrameRef.current === null) {
      volumeFrameRef.current = requestAnimationFrame(flushVolumeLive);
    }
  };
  const commitVolume = () => {
    if (!volumeDraggingRef.current) return;
    volumeDraggingRef.current = false;
    setVolumeDragging(false);
    if (volumeFrameRef.current !== null) {
      cancelAnimationFrame(volumeFrameRef.current);
      volumeFrameRef.current = null;
    }
    volumeLiveRef.current = null;
    const el = volumeRef.current;
    if (!el) return;
    const committed = clamp(Number(el.value), 0, 1);
    if (onVolumeCommit) onVolumeCommit(committed);
    else onVolume?.(committed);
  };
  const commitVolumeKeyboard = () => {
    volumeDraggingRef.current = true;
    commitVolume();
  };
  // Same release guarantee as the scrubber: no pointer capture on the native
  // range input; commit when the pointer is released anywhere on the window.
  // (No button-less pointermove heuristic here either — see the scrubber.)
  useEffect(() => {
    if (!volumeDragging) return;
    const release = () => commitVolume();
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", release);
    return () => {
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", release);
    };
  }, [volumeDragging]);
  useEffect(() => {
    const el = volumeRef.current;
    if (!el || volumeDraggingRef.current) return;
    const v = clamp(volume, 0, 1);
    const current = Number(el.value);
    if (current !== v) el.value = String(v);
  }, [volume]);

  // ---- Volume mute toggle: remembers the last audible level ----
  const [muted, setMuted] = useState(false);
  const lastAudibleRef = useRef(volume > 0 ? volume : 1);
  if (volume > 0) lastAudibleRef.current = volume;
  const commitVolumeValue = (value: number) => { if (onVolumeCommit) onVolumeCommit(value); else onVolume?.(value); };
  const toggleMute = () => {
    if (!backendAvailable) return;
    if (muted) { setMuted(false); commitVolumeValue(clamp(lastAudibleRef.current, 0, 1)); }
    else { setMuted(true); commitVolumeValue(0); }
  };

  return <div className="luno-player-bar" style={{ "--luno-player-accent": accent } as CSSProperties} aria-disabled={!backendAvailable}>
    <div className="luno-player-row">
      <button className="luno-player-track" type="button" disabled={!backendAvailable || !track} onClick={() => track && onOpenTrack?.(track)}>
        <Artwork src={artwork ?? track?.artwork_url} alt="" size={52} fallback={track ? <RecommendationArtwork artist={track.artist ?? ""} title={track.title} size={52} /> : undefined} />
        <span className="luno-player-copy"><strong title={track?.title ?? undefined}>{track?.title ?? "Nothing playing"}</strong><small>{track?.artist ?? ""}</small></span>
      </button>
      <div className="luno-transport" role="group" aria-label="Transport controls">
        <button type="button" aria-label={shuffle ? "Disable shuffle" : "Enable shuffle"} aria-pressed={shuffle} disabled={!backendAvailable || !onToggleShuffle} onClick={onToggleShuffle}><IconShuffle size={16} /></button>
        <button type="button" aria-label="Previous track" disabled={!backendAvailable} onClick={onPrevious}><IconSkipBack size={18} /></button>
        <button className="luno-transport-play" type="button" aria-label={playing ? "Pause" : "Play"} disabled={!backendAvailable || !track} onClick={onPlayPause}>{playing ? <IconPause size={16} /> : <IconPlay size={16} />}</button>
        <button type="button" aria-label="Next track" disabled={!backendAvailable} onClick={onNext}><IconSkipForward size={18} /></button>
        <button type="button" aria-label={`Repeat ${repeat}`} aria-pressed={repeat !== "off"} disabled={!backendAvailable || !onCycleRepeat} onClick={onCycleRepeat}>{repeat === "one" ? "①" : <IconRepeat size={16} />}</button>
      </div>
      <div className="luno-player-actions">
        {onToggleQueue && <button type="button" aria-label={queueOpen ? "Close queue panel" : "Open queue panel"} aria-pressed={queueOpen} onClick={onToggleQueue}><IconQueueList size={17} /></button>}
        <button type="button" className={`luno-favorite${track?.favorite ? " is-on" : ""}`} aria-label={track?.favorite ? "Remove from favorites" : "Add to favorites"} disabled={!backendAvailable || !track} onClick={() => track && onFavoriteToggle?.(track)}>{track?.favorite ? "★" : "☆"}</button>
        <button type="button" aria-label={muted || volume <= 0 ? "Unmute" : "Mute"} aria-pressed={muted || volume <= 0} disabled={!backendAvailable} onClick={toggleMute}><IconVolume size={16} /></button>
        <label className="luno-volume"><span className="sr-only">Volume</span><input ref={volumeRef} defaultValue={clamp(volume, 0, 1)} aria-label="Volume" type="range" min="0" max="1" step="0.01" disabled={!backendAvailable} onChange={(event) => previewVolume(Number(event.currentTarget.value))} onPointerDown={startVolumeDrag} onPointerUp={commitVolume} onPointerCancel={commitVolume} onKeyUp={commitVolumeKeyboard} /></label>
      </div>
    </div>
    <div className="luno-scrubber-row">
      <span className="luno-time">{formatTime(seekDisplay)}</span>
      <input ref={seekRef} defaultValue={clamp(position, 0, max)} aria-label="Playback position" type="range" min="0" max={max} disabled={!backendAvailable || !track}
        onChange={(event) => { seekLiveRef.current = Number(event.currentTarget.value); setSeekDisplay(seekLiveRef.current); }}
        onPointerDown={startSeekDrag}
        onPointerUp={commitSeek}
        onPointerCancel={commitSeek}
        onKeyUp={commitSeekKeyboard} />
      <span className="luno-time">{formatTime(duration)}</span>
    </div>
  </div>;
}

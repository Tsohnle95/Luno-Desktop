import { useEffect, useState, useSyncExternalStore, type ReactNode } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import "../styles/tokens.css";
import "../styles/layout.css";
import { SettingsView } from "../views/SettingsView";
import { ErrorLogView } from "../views/ErrorLogView";
import { LibraryView } from "../views/LibraryView";
import { RecentView } from "../views/RecentView";
import { SongPageView } from "../views/SongPageView";
import { DownloaderView } from "../views/DownloaderView";
import { DiscoverView } from "../views/DiscoverView";
import type { BackendProjection } from "../state/backend_projection";
import { currentTrack } from "../state/backend_projection";
import type { Bridge, MigrationStatus } from "./bridge";
import type { Track } from "../shared/contracts";
import { Queue } from "../components/Queue";
import { PlayerBar } from "../components/PlayerBar";
import type { VideoStageStatus } from "../components/VideoStage";

export const ROUTES = ["library", "recent", "discover", "downloader", "settings", "song_page", "error_log"] as const;
export type Route = (typeof ROUTES)[number];
export const routeLabels: Record<Route, string> = {
  library: "Home", recent: "Recent", discover: "Discover", downloader: "Downloader", settings: "Settings", song_page: "Song", error_log: "Error Log",
};

export interface ShellProps { views?: Partial<Record<Route, ReactNode>>; queue?: ReactNode; player?: ReactNode; bridge?: Bridge; backendProjection?: { getSnapshot: () => BackendProjection; subscribe: (listener: () => void) => () => void }; backendAvailable?: boolean; videoConnected?: boolean; }

export function clampScrollTop(scrollTop: number, scrollHeight: number, clientHeight: number): number {
  return Math.min(Math.max(0, scrollTop), Math.max(0, scrollHeight - clientHeight));
}

export function runBridgeCommand(command: Promise<unknown> | null | undefined, onError: (message: string) => void): void {
  if (command) void command.catch((error: unknown) => onError(error instanceof Error ? error.message : "Runtime command unavailable"));
}

export function playRecentTrack(bridge: Pick<Bridge, "play"> | undefined, track: Track, onError: (message: string) => void): void {
  runBridgeCommand(bridge?.play(track.id), onError);
}

export function Shell({ views = {}, queue, player, bridge, backendProjection, backendAvailable, videoConnected }: ShellProps) {
  const [route, setRoute] = useState<Route>("library");
  const projection = useSyncExternalStore(backendProjection?.subscribe ?? (() => () => {}), backendProjection?.getSnapshot ?? (() => ({ library: null, playback: null } as BackendProjection)));
  const [migrationStatus, setMigrationStatus] = useState<MigrationStatus | null>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const runCommand = (command: Promise<unknown> | null | undefined) => runBridgeCommand(command, setCommandError);
  useEffect(() => { if (!bridge) return; bridge.migrationStatus().then(setMigrationStatus).catch(() => setMigrationStatus(null)); }, [bridge]);
  const track = currentTrack(projection);
  const tracks = projection.library?.tracks ?? [];
  const queueTracks = projection.playback?.queue.map((id) => tracks.find((item) => item.id === id)).filter((item): item is Track => !!item) ?? [];
  const available = backendAvailable === true;
  const currentVideo = track && projection.video?.track_id === track.id ? projection.video : null;
  const connectedVideo = videoConnected ?? currentVideo !== null;
  const video = track ? { src: safeVideoSource(currentVideo?.source_url, currentVideo?.staged_path), status: (currentVideo?.status === "not_checked" ? "not-checked" : currentVideo?.status) as VideoStageStatus | undefined, positionMs: currentVideo?.position_ms ?? 0, playing: currentVideo?.status === "playing", artworkUrl: track.artwork_url, onResize: (size: { width: number; height: number }) => runCommand(bridge?.videoResize(size.width, size.height)), onSeek: (positionMs: number) => runCommand(bridge?.videoSeek(positionMs)), onPause: (positionMs: number) => { runCommand(bridge?.videoPause()); runCommand(bridge?.videoSeek(positionMs)); }, onResume: (positionMs: number) => { runCommand(bridge?.videoSeek(positionMs)); runCommand(bridge?.videoPlay()); }, onEnded: () => runCommand(bridge?.videoPause()), onFullscreenChange: (fullscreen: boolean) => runCommand(bridge?.videoFullscreen(fullscreen)) } : undefined;
  const recentTracks = projection.recentHistory.map((id) => tracks.find((item) => item.id === id)).filter((item): item is Track => !!item);
  const content = views[route] ?? (route === "library" ? <LibraryView snapshot={projection.library} playback={projection.playback} onPlay={(item) => runCommand(bridge?.play(item.id))} onScan={() => runCommand(bridge?.scanLibrary())} /> : route === "recent" ? <RecentView tracks={recentTracks} playback={projection.playback} onPlay={(item) => playRecentTrack(bridge, item, setCommandError)} /> : route === "discover" ? <DiscoverView /> : route === "downloader" ? <DownloaderView /> : route === "settings" ? <SettingsView onOpenErrorLog={() => setRoute("error_log")} /> : route === "error_log" ? <ErrorLogView /> : route === "song_page" ? <SongPageView track={track} video={video} videoConnection={connectedVideo ? "connected" : "disconnected"} /> : <div className="luno-route-empty">{routeLabels[route]}</div>);
  return <div className="luno-shell" data-route={route} data-backend-state={available ? "available" : "unavailable"} data-video-state={connectedVideo ? "connected" : "unconnected"} data-migration-state={migrationStatus?.status ?? "unknown"}>
    <aside className="luno-sidebar" data-slot="sidebar">
      <div className="luno-brand" aria-label="Luno">Luno</div>
      <nav className="luno-nav" aria-label="Primary navigation"><ul className="luno-nav-list">
        {ROUTES.map((item) => <li key={item}><button className="luno-nav-button" type="button" aria-current={route === item ? "page" : undefined} onClick={() => setRoute(item)}>{routeLabels[item]}</button></li>)}
      </ul></nav>
      <div className="luno-playlists luno-scroll" data-slot="playlists" aria-label="Playlists" />
    </aside>
       <main className="luno-main" data-slot="main" aria-disabled={!available}>{(!available || !connectedVideo || migrationStatus || commandError) && <div className="luno-runtime-status" role="status">{!available && <span data-state="backend-unavailable">Backend unavailable; runtime commands are unavailable.</span>}{commandError && <span data-state="command-rejected">{commandError}</span>}{!connectedVideo && <span data-state="video-unconnected">Video preview unavailable; artwork fallback is active.</span>}{migrationStatus?.status === "completed" && <span data-state="migration-completed">Migration completed.</span>}{migrationStatus?.status === "skipped" && <span data-state="migration-skipped">Migration skipped: {migrationStatus.reason}</span>}{migrationStatus?.status === "failed" && <span data-state="migration-failed">Migration failed: {migrationStatus.reason}</span>}</div>}<div className="luno-main-content luno-scroll">{content}</div></main>
     <aside className="luno-queue luno-scroll" data-slot="queue" aria-label="Queue">{queue ?? <Queue tracks={queueTracks} currentIndex={projection.playback?.current_index} />}</aside>
      <footer className="luno-player" data-slot="player" aria-label="Player">{player ?? <PlayerBar track={track} playback={projection.playback} onPlayPause={() => runCommand(projection.playback?.status === "playing" ? bridge?.pause() : track && bridge?.play(track.id))} onSeek={(positionMs) => runCommand(bridge?.seek(positionMs))} />}</footer>
  </div>;
}

export function safeVideoSource(sourceUrl: string | null | undefined, stagedPath: string | null | undefined): string | null {
  if (stagedPath && stagedPath.startsWith("/") && !stagedPath.includes("\0") && !stagedPath.split(/[\\/]+/).includes("..")) { try { return convertFileSrc(stagedPath); } catch { return null; } }
  if (sourceUrl) { try { const url = new URL(sourceUrl); if (url.protocol === "http:" || url.protocol === "https:") return url.href; } catch { return null; } }
  return null;
}

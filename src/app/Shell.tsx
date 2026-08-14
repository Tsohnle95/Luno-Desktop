import { useEffect, useState, useSyncExternalStore, type ReactNode } from "react";
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
import type { Bridge } from "./bridge";
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

export function Shell({ views = {}, queue, player, bridge, backendProjection, backendAvailable, videoConnected }: ShellProps) {
  const [route, setRoute] = useState<Route>("library");
  const projection = useSyncExternalStore(backendProjection?.subscribe ?? (() => () => {}), backendProjection?.getSnapshot ?? (() => ({ library: null, playback: null } as BackendProjection)));
  const [migrationKnown, setMigrationKnown] = useState(false);
  useEffect(() => { if (!bridge) return; bridge.migrationStatus().then(() => setMigrationKnown(true)).catch(() => setMigrationKnown(false)); }, [bridge]);
  const track = currentTrack(projection);
  const tracks = projection.library?.tracks ?? [];
  const queueTracks = projection.playback?.queue.map((id) => tracks.find((item) => item.id === id)).filter((item): item is Track => !!item) ?? [];
  const available = backendAvailable ?? projection.library !== null;
  const connectedVideo = videoConnected ?? projection.video !== null;
  const videoStatus = (projection.video?.status === "not_checked" ? "not-checked" : projection.video?.status) as VideoStageStatus | undefined;
  const video = track ? { src: projection.video?.source_url, status: videoStatus, positionMs: projection.playback?.position_ms ?? 0, playing: projection.playback?.status === "playing", artworkUrl: track.artwork_url, onResize: (size: { width: number; height: number }) => void bridge?.videoResize(size.width, size.height), onSeek: (positionMs: number) => void bridge?.videoSeek(positionMs), onPause: (positionMs: number) => { void bridge?.videoPause(); void bridge?.videoSeek(positionMs); }, onResume: (positionMs: number) => { void bridge?.videoSeek(positionMs); void bridge?.videoPlay(); }, onEnded: () => void bridge?.videoPause(), onFullscreenChange: (fullscreen: boolean) => void bridge?.videoFullscreen(fullscreen) } : undefined;
  const content = views[route] ?? (route === "library" ? <LibraryView snapshot={projection.library} playback={projection.playback} onPlay={(item) => void bridge?.play(item.id)} onScan={() => void bridge?.scanLibrary()} /> : route === "recent" ? <RecentView tracks={queueTracks} playback={projection.playback} /> : route === "discover" ? <DiscoverView /> : route === "downloader" ? <DownloaderView /> : route === "settings" ? <SettingsView onOpenErrorLog={() => setRoute("error_log")} /> : route === "error_log" ? <ErrorLogView /> : route === "song_page" ? <SongPageView track={track} video={video} videoConnection={connectedVideo ? "connected" : "disconnected"} /> : <div className="luno-route-empty">{routeLabels[route]}</div>);
  return <div className="luno-shell" data-route={route} data-backend-state={available ? "available" : "unavailable"} data-video-state={connectedVideo ? "connected" : "unconnected"} data-migration-state={migrationKnown ? "available" : "unknown"}>
    <aside className="luno-sidebar" data-slot="sidebar">
      <div className="luno-brand" aria-label="Luno">Luno</div>
      <nav className="luno-nav" aria-label="Primary navigation"><ul className="luno-nav-list">
        {ROUTES.map((item) => <li key={item}><button className="luno-nav-button" type="button" aria-current={route === item ? "page" : undefined} onClick={() => setRoute(item)}>{routeLabels[item]}</button></li>)}
      </ul></nav>
      <div className="luno-playlists luno-scroll" data-slot="playlists" aria-label="Playlists" />
    </aside>
     <main className="luno-main" data-slot="main">{(!available || !connectedVideo) && <div className="luno-runtime-status" role="status">{!available && <span data-state="backend-unavailable">Backend unavailable; runtime commands are disabled.</span>}{!connectedVideo && <span data-state="video-unconnected">Video preview unconnected; artwork fallback is active.</span>}</div>}<div className="luno-main-content luno-scroll">{content}</div></main>
     <aside className="luno-queue luno-scroll" data-slot="queue" aria-label="Queue">{queue ?? <Queue tracks={queueTracks} currentIndex={projection.playback?.current_index} />}</aside>
     <footer className="luno-player" data-slot="player" aria-label="Player">{player ?? <PlayerBar track={track} playback={projection.playback} onPlayPause={() => void (projection.playback?.status === "playing" ? bridge?.pause() : track && bridge?.play(track.id))} onSeek={(positionMs) => void bridge?.seek(positionMs)} />}</footer>
  </div>;
}

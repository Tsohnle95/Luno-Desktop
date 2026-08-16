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
import type { SearchResult, SettingsInput, Track } from "../shared/contracts";
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

export function normalizeRuntimeCommandError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    const details = error as Record<string, unknown>;
    if (typeof details.message === "string") return details.message;
    if (typeof details.code === "string") return details.code;
    try {
      const serialized = JSON.stringify(error);
      if (serialized) return serialized;
    } catch {
      // Fall through for values that cannot be serialized.
    }
  }
  return "Runtime command unavailable";
}

export function runBridgeCommand(command: Promise<unknown> | null | undefined, onError: (message: string) => void): void {
  if (command) void command.catch((error: unknown) => onError(normalizeRuntimeCommandError(error)));
}

export function playRecentTrack(bridge: Pick<Bridge, "play"> | undefined, track: Track, onError: (message: string) => void): void {
  runBridgeCommand(bridge?.play(track.id), onError);
}

export function currentVideoForTrack(projection: Pick<BackendProjection, "playback" | "video">, trackId: string): BackendProjection["video"] {
  const video = projection.video;
  return video?.track_id === trackId && projection.playback?.generation === video.generation ? video : null;
}

export function videoCommandsEnabled(available: boolean, connected: boolean, source: string | null, status: VideoStageStatus | undefined): boolean {
  return available && connected && !!source && !!status && !["not-checked", "loading", "unavailable", "error", "fallback"].includes(status);
}

export function Shell({ views = {}, queue, player, bridge, backendProjection, backendAvailable, videoConnected }: ShellProps) {
  const [route, setRoute] = useState<Route>("library");
  const projection = useSyncExternalStore(backendProjection?.subscribe ?? (() => () => {}), backendProjection?.getSnapshot ?? (() => ({ library: null, playback: null } as BackendProjection)));
  const [migrationStatus, setMigrationStatus] = useState<MigrationStatus | null>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [downloadSources, setDownloadSources] = useState<SearchResult[]>([]);
  const [csvPreview, setCsvPreview] = useState<Array<{ id: string; title: string; artist?: string }>>([]);
  const [musicRoot, setMusicRoot] = useState("Not configured");
  const [settings, setSettings] = useState<SettingsInput>({ music_root: null, auto_discovery: false, volume: 1, sleep_timer: null, global_keys: true, favorite_songs: [] });
  const [transferManifest, setTransferManifest] = useState("");
  const [transferDestination, setTransferDestination] = useState("");
  const [transferResult, setTransferResult] = useState("");
  const viteEnv = (import.meta as ImportMeta & { env?: { DEV?: boolean; VITE_LUNO_GATE4_HOME?: string; VITE_LUNO_GATE4_HELPER?: string; VITE_LUNO_GATE4_DESTINATION?: string } }).env;
  const fixtureHome = viteEnv?.DEV ? viteEnv.VITE_LUNO_GATE4_HOME : undefined;
  const fixtureHelper = viteEnv?.DEV ? viteEnv.VITE_LUNO_GATE4_HELPER : undefined;
  const fixtureDestination = viteEnv?.DEV ? viteEnv.VITE_LUNO_GATE4_DESTINATION : undefined;
  const available = backendAvailable === true;
  const runCommand = (command: Promise<unknown> | null | undefined) => { if (available) runBridgeCommand(command, setCommandError); };
  useEffect(() => { if (!bridge) return; bridge.migrationStatus().then(setMigrationStatus).catch(() => setMigrationStatus(null)); }, [bridge]);
  useEffect(() => {
    if (!fixtureHome) return;
    if (!bridge) return;
    bridge.loadSettings(fixtureHome).then((value) => {
      if (!value || typeof value !== "object") return;
      const item = value as Partial<SettingsInput>;
      setSettings((current) => ({ ...current, ...item }));
      if (typeof item.music_root === "string") setMusicRoot(item.music_root);
    }).catch((error) => setCommandError(normalizeRuntimeCommandError(error)));
  }, [bridge, fixtureHome]);
  const saveSettings = (next: SettingsInput) => {
    setSettings(next);
    if (!fixtureHome) { setCommandError("Settings persistence is unavailable outside the development fixture runtime."); return; }
    runCommand(bridge?.saveSettings(fixtureHome, next));
  };
  const chooseRoot = () => {
    if (!viteEnv?.DEV) { setCommandError("Native library root picker is unavailable in this runtime."); return; }
    const root = window.prompt("Development probe: enter an existing absolute library folder", musicRoot === "Not configured" ? "" : musicRoot);
    if (!root || !bridge) return;
    runBridgeCommand(bridge.selectLibraryRoot(root).then(() => { setMusicRoot(root); saveSettings({ ...settings, music_root: root }); }), setCommandError);
  };
  const track = currentTrack(projection);
  const tracks = projection.library?.tracks ?? [];
  const queueTracks = projection.playback?.queue.map((id) => tracks.find((item) => item.id === id)).filter((item): item is Track => !!item) ?? [];
  const currentVideo = track ? currentVideoForTrack(projection, track.id) : null;
  const connectedVideo = videoConnected === true;
  const videoSrc = safeVideoSource(currentVideo?.source_url, currentVideo?.staged_path);
  const videoStatus = (currentVideo?.status === "not_checked" ? "not-checked" : currentVideo?.status) as VideoStageStatus | undefined;
  const videoActive = videoCommandsEnabled(available, connectedVideo, videoSrc, videoStatus);
  const video = track ? { src: videoSrc, status: videoStatus, positionMs: currentVideo?.position_ms ?? 0, playing: videoActive && currentVideo?.status === "playing", artworkUrl: track.artwork_url, onResize: (size: { width: number; height: number }) => { if (videoActive) runCommand(bridge?.videoResize(size.width, size.height)); }, onSeek: (positionMs: number) => { if (videoActive) runCommand(bridge?.videoSeek(positionMs)); }, onPause: (positionMs: number) => { if (videoActive) { runCommand(bridge?.videoPause()); runCommand(bridge?.videoSeek(positionMs)); } }, onResume: (positionMs: number) => { if (videoActive) { runCommand(bridge?.videoSeek(positionMs)); runCommand(bridge?.videoPlay()); } }, onEnded: () => { if (videoActive) runCommand(bridge?.videoPause()); }, onFullscreenChange: (fullscreen: boolean) => { if (videoActive) runCommand(bridge?.videoFullscreen(fullscreen)); } } : undefined;
  const recentTracks = projection.recentHistory.map((id) => tracks.find((item) => item.id === id)).filter((item): item is Track => !!item);
   const content = views[route] ?? (route === "library" ? <LibraryView snapshot={projection.library} playback={projection.playback} backendAvailable={available} onPlay={(item, context) => runCommand(bridge?.playWithContext(item.id, context.map((track) => track.id)))} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} onScan={() => runCommand(bridge?.scanLibrary())} /> : route === "recent" ? <RecentView tracks={recentTracks} playback={projection.playback} backendAvailable={available} onPlay={(item, context) => runCommand(bridge?.playWithContext(item.id, context.map((track) => track.id)))} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} /> : route === "discover" ? <DiscoverView /> : route === "downloader" ? <DownloaderView jobs={projection.jobs} destinations={fixtureDestination ? [fixtureDestination] : []} searchResults={downloadSources.map((item) => ({ id: `${item.provider}:${item.query}`, title: item.query, artist: item.provider }))} previewTracks={csvPreview} onSearch={(query) => { if (bridge) bridge.searchDownloader(query).then((value) => setDownloadSources(value)).catch((error) => setCommandError(normalizeRuntimeCommandError(error))); }} onPreview={(value, kind) => { if (!bridge) return; if (kind === "csv") bridge.previewCsv(value).then((result) => { const rows = (result as { rows?: Array<{ id: string; title: string; artist?: string }> }).rows ?? []; setCsvPreview(rows); }).catch((error) => setCommandError(normalizeRuntimeCommandError(error))); }} onDownload={(ids) => { if (!fixtureHelper || !fixtureDestination) { setCommandError("External download helpers are unavailable; configure the development fixture helper."); return; } const jobId = `fixture-${Date.now()}-${ids[0] ?? "download"}`; runCommand(bridge?.startDownload(jobId, `${fixtureDestination}/${jobId}.mp3`, fixtureHelper)); }} onDirectUrl={(url, destination) => runCommand(bridge?.validateDirectUrl(url, destination))} onQueueUrl={(url) => runCommand(bridge?.queueUrls([url]))} onSync={(_, destination) => { if (!destination.startsWith("/")) { setCommandError("Sync requires the configured library folder, not a playlist label."); return; } runCommand(bridge?.syncMarkers(destination)); }} onCancel={(jobId) => runCommand(bridge?.cancelJob(jobId))} onRetry={(jobId) => { if (fixtureHelper && fixtureDestination) runCommand(bridge?.retryDownload(jobId)); else setCommandError("Download retry is available only with the development fixture helper."); }} /> : route === "settings" ? <SettingsView musicRoot={musicRoot} autoDiscovery={settings.auto_discovery ?? false} mediaKeys={settings.global_keys ?? true} transferManifest={transferManifest} transferDestination={transferDestination} transferResult={transferResult} error={commandError} onChooseMusicRoot={chooseRoot} onAutoDiscoveryChange={(value) => saveSettings({ ...settings, auto_discovery: value })} onMediaKeysChange={(value) => saveSettings({ ...settings, global_keys: value })} onTransferManifestChange={setTransferManifest} onTransferDestinationChange={setTransferDestination} onTransferExport={() => { if (!available) { setCommandError("Transfer export is unavailable until the backend is connected."); return; } runBridgeCommand(bridge?.transferExport().then((result) => { const manifest = (result as { manifest?: string }).manifest ?? ""; setTransferManifest(manifest); setTransferResult(manifest ? "Exported manifest is ready for review." : "Export returned no manifest."); }), setCommandError); }} onTransferPreview={() => runBridgeCommand(bridge?.transferPreview(transferManifest, transferDestination).then((result) => setTransferResult(JSON.stringify(result))), setCommandError)} onTransferApply={() => runBridgeCommand(bridge?.transferApply(transferManifest, transferDestination, {}).then((result) => setTransferResult(JSON.stringify(result))), setCommandError)} onOpenErrorLog={() => setRoute("error_log")} /> : route === "error_log" ? <ErrorLogView /> : route === "song_page" ? <SongPageView track={track} backendAvailable={available} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} video={video} videoConnection={connectedVideo ? "connected" : "disconnected"} /> : <div className="luno-route-empty">{routeLabels[route]}</div>);
  return <div className="luno-shell" data-route={route} data-backend-state={available ? "available" : "unavailable"} data-video-state={connectedVideo ? "connected" : "unconnected"} data-migration-state={migrationStatus?.status ?? "unknown"}>
    <aside className="luno-sidebar" data-slot="sidebar">
      <div className="luno-brand" aria-label="Luno">Luno</div>
      <nav className="luno-nav" aria-label="Primary navigation"><ul className="luno-nav-list">
        {ROUTES.map((item) => <li key={item}><button className="luno-nav-button" type="button" aria-current={route === item ? "page" : undefined} onClick={() => setRoute(item)}>{routeLabels[item]}</button></li>)}
      </ul></nav>
      <div className="luno-playlists luno-scroll" data-slot="playlists" aria-label="Playlists" />
    </aside>
        <main className="luno-main" data-slot="main" aria-disabled={!available}>{(!available || !connectedVideo || migrationStatus || commandError) && <div className="luno-runtime-status" role="status">{!available && <span data-state="backend-unavailable">Backend unavailable; runtime commands are unavailable.</span>}{commandError && <span data-state="command-rejected">{commandError}</span>}{!connectedVideo && <span data-state="video-unconnected">Video preview unavailable; artwork fallback is active.</span>}{migrationStatus?.status === "completed" && <span data-state="migration-completed">Migration completed.</span>}{migrationStatus?.status === "skipped" && <span data-state="migration-skipped">Migration skipped: {migrationStatus.reason}</span>}{migrationStatus?.status === "failed" && <span data-state="migration-failed">Migration failed: {migrationStatus.reason}</span>}</div>}<div className="luno-main-content luno-scroll">{content}</div></main>
       <aside className="luno-queue luno-scroll" data-slot="queue" aria-label="Queue">{queue ?? <Queue tracks={queueTracks} currentIndex={projection.playback?.current_index} shuffle={projection.playback?.shuffle} repeat={projection.playback?.repeat} backendAvailable={available} onPlay={(item) => runCommand(bridge?.playWithContext(item.id, queueTracks.map((track) => track.id)))} onPlayNext={(item) => runCommand(bridge?.playNext([item.id]))} onAddToQueue={(item) => runCommand(bridge?.addToQueue([item.id]))} onRemove={(index) => runCommand(bridge?.removeFromQueue(index))} onReorder={(from, to) => runCommand(bridge?.reorderQueue(from, to))} onClear={() => runCommand(bridge?.clearQueue())} onShuffle={() => runCommand(bridge?.toggleShuffle())} onRepeat={(mode) => runCommand(bridge?.setRepeat(mode))} />}</aside>
        <footer className="luno-player" data-slot="player" aria-label="Player">{player ?? <PlayerBar track={track} playback={projection.playback} backendAvailable={available} onPlayPause={() => runCommand(projection.playback?.status === "playing" ? bridge?.pause() : track && bridge?.playWithContext(track.id, queueTracks.map((item) => item.id)))} onNext={() => runCommand(bridge?.next())} onPrevious={() => runCommand(bridge?.previous())} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} onSeek={(positionMs) => runCommand(bridge?.seek(positionMs))} />}</footer>
  </div>;
}

export function safeVideoSource(sourceUrl: string | null | undefined, stagedPath: string | null | undefined): string | null {
  if (stagedPath && stagedPath.startsWith("/") && !stagedPath.includes("\0") && !stagedPath.split(/[\\/]+/).includes("..")) { try { return convertFileSrc(stagedPath); } catch { return null; } }
  if (sourceUrl) { try { const url = new URL(sourceUrl); if (url.protocol === "http:" || url.protocol === "https:") return url.href; } catch { return null; } }
  return null;
}

import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "../styles/tokens.css";
import "../styles/layout.css";
import { SettingsView } from "../views/SettingsView";
import { ErrorLogView } from "../views/ErrorLogView";
import { HomeView } from "../views/HomeView";
import { LibraryView, libraryPlaylists, libraryAlbums, playlistTracks, shuffledFirstTrack, albumScope, type LibraryScope } from "../views/LibraryView";
import { RecentView } from "../views/RecentView";
import { SongPageView } from "../views/SongPageView";
import { DownloaderView } from "../views/DownloaderView";
import { DiscoverView } from "../views/DiscoverView";
import type { BackendProjection } from "../state/backend_projection";
import { currentTrack } from "../state/backend_projection";
import { useTrackArtwork } from "../state/track_artwork";
import { bumpPlayCount, loadDownloadJournal, loadPlayCounts, madeForYouIds, popularTrackIds, recordDownloadCompletions, saveDownloadJournal, savePlayCounts, type DownloadEntry, type PlayCounts } from "../state/play_stats";
import { downloadDisplayLabel, downloadFilename, type DownloadMetadata } from "../state/download_naming";
import type { Bridge, MigrationStatus } from "./bridge";
import { type Playlist, type PlaylistSyncCandidate, type PlaylistSyncIssue, type Recommendation, type SearchResult, type SettingsInput, type Track } from "../shared/contracts";
import { previewKey } from "../shared/contracts";
import type { RecommendationState } from "../components/Recommendations";
import { Queue } from "../components/Queue";
import { PlayerBar } from "../components/PlayerBar";
import { PlaylistPicker } from "../components/PlaylistPicker";
import { EditTrackDialog } from "../components/EditTrackDialog";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { PlaylistDialog, PlaylistUrlDialog, PlaylistSyncLogDialog, PlaylistSyncErrorsDialog, PlaylistSyncReviewDialog, type PlaylistSyncCandidateLookup, type PlaylistSyncLogEntry } from "../components/PlaylistDialogs";
import type { VideoStageStatus } from "../components/VideoStage";
import type { SettingsNavLink } from "../views/SettingsView";
import { IconChevronLeft, IconChevronRight, IconClose, IconDownload, IconHeart, IconHome, IconLibrary, IconMusicNote, IconSearch, IconUser } from "../components/icons";

export const ROUTES = ["library", "recent", "discover", "downloader", "settings", "song_page", "error_log"] as const;
export type Route = (typeof ROUTES)[number];
export const routeLabels: Record<Route, string> = {
  library: "Home", recent: "Recent", discover: "Discover", downloader: "Downloader", settings: "Settings", song_page: "Song", error_log: "Error Log",
};

export interface ShellProps { views?: Partial<Record<Route, ReactNode>>; queue?: ReactNode; player?: ReactNode; bridge?: Bridge; backendProjection?: { getSnapshot: () => BackendProjection; subscribe: (listener: () => void) => () => void; applyVideoSnapshot?: (video: import("../shared/contracts").VideoState) => void; clearErrors?: () => void }; backendAvailable?: boolean; videoConnected?: boolean; }

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

export function isMissingConfigurationError(error: unknown): boolean {
  let details: unknown = error instanceof Error ? error.message : error;
  if (typeof details === "string") {
    try {
      details = JSON.parse(details);
    } catch {
      return false;
    }
  }
  if (!details || typeof details !== "object" || Array.isArray(details)) return false;
  const contractError = details as Record<string, unknown>;
  return contractError.code === "not_found" && contractError.message === "configuration is missing";
}

export function isRetryableRuntimeCommandError(error: unknown): boolean {
  let details: unknown = error instanceof Error ? error.message : error;
  if (typeof details === "string") {
    try {
      details = JSON.parse(details);
    } catch {
      return false;
    }
  }
  return Boolean(details && typeof details === "object" && !Array.isArray(details)
    && (details as Record<string, unknown>).retryable === true);
}

export function runBridgeCommand(command: Promise<unknown> | null | undefined, onError: (message: string) => void): void {
  if (command) void command.catch((error: unknown) => onError(normalizeRuntimeCommandError(error)));
}

export function playRecentTrack(bridge: Pick<Bridge, "play"> | undefined, track: Track, onError: (message: string) => void): void {
  runBridgeCommand(bridge?.play(track.id), onError);
}

export function currentVideoForTrack(projection: Pick<BackendProjection, "playback" | "video">, trackId: string): BackendProjection["video"] {
  // Correlate by TRACK IDENTITY only: video state and playback snapshots use
  // independent generation counters on the backend, so comparing generations
  // here could never match and every staged video stayed invisible. Event
  // ordering is already guaranteed by the sequenced publication queue, so
  // `projection.video` is always the LATEST known state for its track.
  const video = projection.video;
  return video?.track_id === trackId ? video : null;
}

/** Focused preview replaces only the audible/current row; successors stay in the physical queue. */
export function focusedQueueProjection(queue: Track[], currentIndex: number | null | undefined, previewTrack: Track | null): Track[] {
  if (!previewTrack || currentIndex == null) return queue;
  return queue.map((item, index) => index === currentIndex ? previewTrack : item);
}

export function videoCommandsEnabled(available: boolean, connected: boolean, source: string | null, status: VideoStageStatus | undefined): boolean {
  return available && connected && !!source && !!status && !["not-checked", "loading", "unavailable", "error", "fallback"].includes(status);
}

/** Keep the stage mounted while a video source is resolving or downloading. */
export function videoSurfaceActive(available: boolean, connected: boolean, mode: boolean, hasTrack: boolean): boolean {
  return available && connected && mode && hasTrack;
}

/** Shell class suffix used by CSS fullscreen to collapse app chrome and grid tracks. */
export function videoFullscreenClass(fullscreen: boolean): string {
  return fullscreen ? " video-fullscreen" : "";
}

interface SidebarEntry { key: string; label: string; subtitle: string; scope: LibraryScope; kind: "all" | "favorites" | "unsorted" | "playlist" | "album"; artId?: string }

const songs = (count: number) => `${count} ${count === 1 ? "song" : "songs"}`;

function buildSidebarEntries(snapshot: BackendProjection["library"], chip: "playlists" | "albums"): SidebarEntry[] {
  const entries: SidebarEntry[] = [
    { key: "pin-all", label: "All Music", subtitle: songs(snapshot?.tracks.length ?? 0), scope: "all", kind: "all" },
    { key: "pin-favorites", label: "Favorites", subtitle: songs(snapshot?.tracks.filter((track) => track.favorite).length ?? 0), scope: "favorites", kind: "favorites" },
    { key: "pin-unsorted", label: "Unsorted", subtitle: songs(snapshot?.playlists.find((playlist) => playlist.name.toLowerCase() === "unsorted")?.ordered_track_ids.length ?? 0), scope: "unsorted", kind: "unsorted" },
  ];
  if (chip === "playlists") {
    for (const playlist of libraryPlaylists(snapshot)) entries.push({ key: playlist.id, label: playlist.name, subtitle: songs(playlistTracks(snapshot, playlist).length), scope: playlist.id, kind: "playlist", artId: playlist.ordered_track_ids[0] });
  } else {
    for (const album of libraryAlbums(snapshot)) entries.push({ key: albumScope(album.name), label: album.name, subtitle: songs(album.tracks.length), scope: albumScope(album.name), kind: "album", artId: album.tracks[0].id });
  }
  return entries;
}

export function Shell({ views = {}, queue, player, bridge, backendProjection, backendAvailable, videoConnected }: ShellProps) {
  const [history, setHistory] = useState<{ stack: Route[]; index: number }>({ stack: ["library"], index: 0 });
  const route = history.stack[history.index];
  const navigate = (next: Route) => setHistory((current) => {
    if (current.stack[current.index] === next) return current;
    const stack = [...current.stack.slice(0, current.index + 1), next];
    return { stack, index: stack.length - 1 };
  });
  const goBack = () => setHistory((current) => ({ ...current, index: Math.max(0, current.index - 1) }));
  const goForward = () => setHistory((current) => ({ ...current, index: Math.min(current.stack.length - 1, current.index + 1) }));
  const [queueOpen, setQueueOpen] = useState(true);
  const [panelRecent, setPanelRecent] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [sidebarChip, setSidebarChip] = useState<"playlists" | "albums">("playlists");
  const [sidebarQuery, setSidebarQuery] = useState("");
  const [sidebarSearchOpen, setSidebarSearchOpen] = useState(false);
  const [libraryQuery, setLibraryQuery] = useState("");
  const [videoMode, setVideoMode] = useState(false);
  const [videoFullscreen, setVideoFullscreen] = useState(false);
  const [libraryScope, setLibraryScope] = useState<LibraryScope>("all");
  // The Home dashboard is the default landing view; opening any library scope
  // (or typing a search) swaps in the full LibraryView list.
  const [libraryHome, setLibraryHome] = useState(true);
  // Inside the settings page, rail tabs embed their screen on the right side
  // of the SAME settings surface instead of leaving it — except Home, which
  // always exits back to the home dashboard.
  const [settingsScreen, setSettingsScreen] = useState<SettingsNavLink | null>(null);
  const projection = useSyncExternalStore(backendProjection?.subscribe ?? (() => () => {}), backendProjection?.getSnapshot ?? (() => ({ library: null, playback: null } as BackendProjection)), backendProjection?.getSnapshot ?? (() => ({ library: null, playback: null } as BackendProjection)));
  const [migrationStatus, setMigrationStatus] = useState<MigrationStatus | null>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [commandNotice, setCommandNotice] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; track?: Track; playlist?: Playlist; libraryTrack?: boolean } | null>(null);
  const [playlistPickerSelection, setPlaylistPickerSelection] = useState<{ tracks: Track[]; title: string } | null>(null);
  const [editTrack, setEditTrack] = useState<Track | null>(null);
  const [editTrackBusy, setEditTrackBusy] = useState(false);
  const [editTrackError, setEditTrackError] = useState<string | null>(null);
  const [confirmDialog, setConfirmDialog] = useState<{ title: string; message: string; confirmLabel: string; danger?: boolean; onConfirm: () => void } | null>(null);
  const [confirmBusy, setConfirmBusy] = useState(false);
  const [confirmError, setConfirmError] = useState<string | null>(null);
  const [playlistCreateRequest, setPlaylistCreateRequest] = useState<{ tracks: Track[]; onCreated?: (name: string) => void } | null>(null);
  const [playlistCreateBusy, setPlaylistCreateBusy] = useState(false);
  const [playlistCreateError, setPlaylistCreateError] = useState<string | null>(null);
  const [playlistUrlRequest, setPlaylistUrlRequest] = useState<Playlist | null>(null);
  const [playlistUrlBusy, setPlaylistUrlBusy] = useState(false);
  const [playlistUrlError, setPlaylistUrlError] = useState<string | null>(null);
  const [activePlaylistSync, setActivePlaylistSync] = useState<{ jobId: string; playlist: Playlist; startedAtMs: number; sequence: number } | null>(null);
  const playlistSyncSequence = useRef(0);
  const [lastFailedPlaylistSync, setLastFailedPlaylistSync] = useState<{ playlist: Playlist; startedAtMs: number } | null>(null);
  const [playlistSyncLog, setPlaylistSyncLog] = useState<{ name: string; entries: PlaylistSyncLogEntry[]; loading: boolean; error: string | null } | null>(null);
  const [playlistSyncErrors, setPlaylistSyncErrors] = useState<{ name: string; entries: PlaylistSyncLogEntry[]; loading: boolean; error: string | null } | null>(null);
  const [playlistSyncReview, setPlaylistSyncReview] = useState<{ playlist: Playlist; entries: PlaylistSyncIssue[]; loading: boolean; error: string | null; candidateLookups: Record<string, PlaylistSyncCandidateLookup>; selectingVideoId: string | null } | null>(null);
  const [playlistSyncReviewDownloadingVideoId, setPlaylistSyncReviewDownloadingVideoId] = useState<string | null>(null);
  const [playlistSyncReviewOffer, setPlaylistSyncReviewOffer] = useState<{ playlist: Playlist; count: number } | null>(null);
  const askConfirm = (request: { title: string; message: string; confirmLabel: string; danger?: boolean; onConfirm: () => void }) => {
    setConfirmError(null);
    setConfirmDialog(request);
  };
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") close(); };
    window.addEventListener("pointerdown", close);
    window.addEventListener("blur", close);
    document.addEventListener("keydown", escape);
    return () => { window.removeEventListener("pointerdown", close); window.removeEventListener("blur", close); document.removeEventListener("keydown", escape); };
  }, [contextMenu]);
  useEffect(() => {
    if (!commandError) return;
    // Auto-dismiss poison/worker errors after a short window — they now
    // self-heal via lock poisoning recovery, so a sticky banner is just noise.
    // Manual dismiss remains via the button; successful commands also clear.
    const isPoison = commandError.includes("PoisonError") || commandError.includes("poison recovered") || commandError.includes("command worker failed");
    if (!isPoison) return;
    const timer = window.setTimeout(() => setCommandError(null), 8000);
    return () => window.clearTimeout(timer);
  }, [commandError]);
  useEffect(() => {
    if (!commandNotice) return;
    if (activePlaylistSync) return;
    // Success/partial notices are transient confirmations, not sticky state.
    const timer = window.setTimeout(() => setCommandNotice(null), 5000);
    return () => window.clearTimeout(timer);
  }, [commandNotice]);
  useEffect(() => {
    if (!activePlaylistSync) return;
    const job = projection.jobs[activePlaylistSync.jobId];
    if (!job || !["succeeded", "failed", "cancelled"].includes(job.state)) return;
    const { playlist } = activePlaylistSync;
    const sequence = activePlaylistSync.sequence;
    if (job.state === "cancelled") setCommandNotice(`Stopped syncing “${playlist.name}”.`);
    else {
      if (job.state === "failed") setLastFailedPlaylistSync(activePlaylistSync);
      void bridge?.playlistSyncIssues(playlist.id).then((entries) => {
        if (playlistSyncSequence.current !== sequence) return;
        if (entries.length > 0) setPlaylistSyncReviewOffer({ playlist, count: entries.length });
        else setPlaylistSyncReviewOffer(null);
        if (job.state === "succeeded") {
          setCommandNotice(entries.length > 0
            ? `Sync complete. ${entries.length} ${entries.length === 1 ? "song needs" : "songs need"} source review.`
            : `“${playlist.name}” is up to date.`);
        }
      }).catch((error: unknown) => {
        if (playlistSyncSequence.current !== sequence) return;
        if (job.state === "succeeded") setCommandNotice(`Sync completed for “${playlist.name}”; unresolved songs could not be checked.`);
        setCommandError(normalizeRuntimeCommandError(error));
      });
    }
    setActivePlaylistSync(null);
  }, [activePlaylistSync, projection.jobs, bridge]);
  const [downloadSources, setDownloadSources] = useState<SearchResult[]>([]);
  const [downloadSearchLoading, setDownloadSearchLoading] = useState(false);
  const [queuedDownloadUrls, setQueuedDownloadUrls] = useState<string[]>([]);
  const [discover, setDiscover] = useState<{ items: Recommendation[]; loading: boolean; error: string | null }>({ items: [], loading: false, error: null });
  const [discoverRefresh, setDiscoverRefresh] = useState(0);
  const [csvPreview, setCsvPreview] = useState<Array<{ id: string; title: string; artist?: string; source?: string }>>([]);
  const [bundleHelperPath, setBundleHelperPath] = useState<string | null>(null);
  const [musicRoot, setMusicRoot] = useState("Not configured");
  const [libraryRootPath, setLibraryRootPath] = useState<string | null>(null);
  const [runtimeHome, setRuntimeHome] = useState<string | null>(null);
  const [settings, setSettings] = useState<SettingsInput>({ music_root: null, auto_discovery: false, volume: 1, sleep_timer: null, global_keys: true, favorite_songs: [] });
  const [transferManifest, setTransferManifest] = useState("");
  const [transferDestination, setTransferDestination] = useState("");
  const [transferResult, setTransferResult] = useState("");
  const viteEnv = (import.meta as ImportMeta & { env?: { DEV?: boolean; VITE_LUNO_GATE4_HOME?: string; VITE_LUNO_GATE4_HELPER?: string; VITE_LUNO_GATE4_DESTINATION?: string } }).env;
  const fixtureHome = viteEnv?.DEV ? viteEnv.VITE_LUNO_GATE4_HOME : undefined;
  const fixtureHelper = viteEnv?.DEV ? viteEnv.VITE_LUNO_GATE4_HELPER : undefined;
  const fixtureDestination = viteEnv?.DEV ? viteEnv.VITE_LUNO_GATE4_DESTINATION : undefined;
  const available = backendAvailable === true;
  const runCommand = (command: Promise<unknown> | null | undefined) => {
    if (!available || !command) return;
    void command.then(() => setCommandError(null)).catch((error: unknown) => setCommandError(normalizeRuntimeCommandError(error)));
  };
  const openTrackMenu = (event: ReactMouseEvent, item: Track) => {
    event.preventDefault(); event.stopPropagation();
    // Recommendation/preview rows live outside the library; edit/delete only
    // apply to real library files, so flag membership for the menu.
    const libraryTrack = (projection.library?.tracks ?? []).some((track) => track.id === item.id);
    setContextMenu({ x: Math.max(8, Math.min(event.clientX, window.innerWidth - 230)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - 350)), track: item, libraryTrack });
  };
  const cleanPlaylistName = (value: string) => value.trim().replace(/[\\/:*?"<>|]/g, "").replace(/\s+/g, " ").slice(0, 96);
  // Copying into a playlist folder is asynchronous (the scanner rebuilds
  // membership on disk). Report the actual {copied, failed} result instead of
  // closing the picker silently — "I added a song and I don't think it worked"
  // was an invisible success plus a delayed scan.
  const addTracksToPlaylist = (tracks: Track[], playlist: Pick<Playlist, "id" | "name">) => {
    if (!bridge || !available || tracks.length === 0) return;
    void bridge.addTracksToPlaylist(tracks.map((track) => track.id), playlist.id)
      .then((result) => {
        const value = result as { copied?: number; failed?: number } | null;
        const copied = value?.copied ?? 0;
        const failed = value?.failed ?? 0;
        if (failed > 0) {
          setCommandError(`Added ${copied} of ${copied + failed} songs to “${playlist.name}”; ${failed} could not be added.`);
        } else if (copied > 0) {
          setCommandError(null);
          setCommandNotice(`Added ${copied} ${copied === 1 ? "song" : "songs"} to “${playlist.name}”.`);
        } else {
          // add_files skips an existing destination filename (it is not a
          // failure), so copied==0 means nothing new was written; don't assert
          // the song is present (a colliding filename can also cause this).
          setCommandError(null);
          setCommandNotice(`No new songs were added to “${playlist.name}” (matching filenames already exist).`);
        }
      })
      .catch((error: unknown) => setCommandError(normalizeRuntimeCommandError(error)));
  };
  const createPlaylist = (tracksToAdd: Track[] = [], onCreated?: (name: string) => void) => {
    if (!bridge || !available) return;
    // Tauri's webview has no native window.prompt; collect the name in-app.
    setPlaylistCreateError(null);
    setPlaylistCreateRequest({ tracks: tracksToAdd, onCreated });
  };
  const submitCreatePlaylist = (rawName: string) => {
    if (!bridge) return;
    const name = cleanPlaylistName(rawName);
    if (!name || name.toLowerCase() === "unsorted") { setPlaylistCreateError("Choose a non-empty playlist name other than Unsorted."); return; }
    const request = playlistCreateRequest;
    setPlaylistCreateBusy(true);
    setPlaylistCreateError(null);
    void bridge.createPlaylist(name)
      .then(() => {
        setPlaylistCreateBusy(false);
        setPlaylistCreateRequest(null);
        setCommandError(null);
        if (request?.tracks.length) addTracksToPlaylist(request.tracks, { id: `playlist:${name}`, name });
        else if (!request?.onCreated) setCommandNotice(`Created playlist “${name}”.`);
        request?.onCreated?.(name);
      })
      .catch((error: unknown) => { setPlaylistCreateBusy(false); setPlaylistCreateError(normalizeRuntimeCommandError(error)); });
  };
  const submitPlaylistUrl = (url: string | null) => {
    if (!bridge || !playlistUrlRequest) return;
    const playlist = playlistUrlRequest;
    setPlaylistUrlBusy(true);
    setPlaylistUrlError(null);
    void bridge.setPlaylistUrl(playlist.id, url)
      .then(() => {
        setPlaylistUrlBusy(false);
        setPlaylistUrlRequest(null);
        setCommandError(null);
        setCommandNotice(url ? `Linked “${playlist.name}” to YouTube.` : `Removed the YouTube link from “${playlist.name}”.`);
      })
      .catch((error: unknown) => { setPlaylistUrlBusy(false); setPlaylistUrlError(normalizeRuntimeCommandError(error)); });
  };
  const syncPlaylist = (playlist: Playlist) => {
    if (!bridge || !available || !playlist.url) return;
    const sequence = ++playlistSyncSequence.current;
    setCommandError(null);
    backendProjection?.clearErrors?.();
    setLastFailedPlaylistSync(null);
    setPlaylistSyncReviewOffer(null);
    setCommandNotice(`Starting sync for “${playlist.name}”…`);
    const startedAtMs = Date.now();
    void bridge.syncPlaylist(playlist.id)
      .then((value) => {
        const jobId = value && typeof value === "object" && "job_id" in value && typeof value.job_id === "string" ? value.job_id : null;
        if (!jobId) throw new Error("sync did not return a job id");
        setCommandNotice(null);
        setActivePlaylistSync({ jobId, playlist, startedAtMs, sequence });
      })
      .catch((error: unknown) => { setCommandNotice(null); setCommandError(normalizeRuntimeCommandError(error)); });
  };
  const openPlaylistSyncLog = (playlist: Playlist) => {
    if (!bridge || !available) return;
    setPlaylistSyncLog({ name: playlist.name, entries: [], loading: true, error: null });
    void bridge.playlistSyncLog(playlist.id).then((value) => {
      setPlaylistSyncLog((current) => current?.name === playlist.name ? { ...current, entries: value.entries, loading: false } : current);
    }).catch((error: unknown) => {
      setPlaylistSyncLog((current) => current?.name === playlist.name ? { ...current, loading: false, error: normalizeRuntimeCommandError(error) } : current);
    });
  };
  const openPlaylistSyncReview = (playlist: Playlist) => {
    if (!bridge || !available) return;
    const sequence = playlistSyncSequence.current;
    setPlaylistSyncReviewDownloadingVideoId(null);
    setPlaylistSyncReview({ playlist, entries: [], loading: true, error: null, candidateLookups: {}, selectingVideoId: null });
    void bridge.playlistSyncIssues(playlist.id).then((entries) => {
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? { ...current, entries, loading: false } : current);
      if (playlistSyncSequence.current === sequence) setPlaylistSyncReviewOffer(entries.length > 0 ? { playlist, count: entries.length } : null);
    }).catch((error: unknown) => {
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? { ...current, loading: false, error: normalizeRuntimeCommandError(error) } : current);
    });
  };
  const findPlaylistSyncCandidates = (videoId: string) => {
    if (!bridge || !playlistSyncReview) return;
    const playlist = playlistSyncReview.playlist;
    setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? {
      ...current,
      candidateLookups: { ...current.candidateLookups, [videoId]: { candidates: [], loading: true, error: null } },
    } : current);
    void bridge.playlistSyncCandidates(playlist.id, videoId).then((candidates) => {
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? {
        ...current,
        candidateLookups: { ...current.candidateLookups, [videoId]: { candidates, loading: false, error: null } },
      } : current);
    }).catch((error: unknown) => {
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? {
        ...current,
        candidateLookups: { ...current.candidateLookups, [videoId]: { candidates: [], loading: false, error: normalizeRuntimeCommandError(error) } },
      } : current);
    });
  };
  const persistPlaylistSyncSource = (videoId: string, sourceVideoId: string) => {
    if (!bridge || !available || !playlistSyncReview || activePlaylistSync) return;
    const playlist = playlistSyncReview.playlist;
    setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? { ...current, selectingVideoId: videoId, error: null } : current);
    void bridge.playlistSyncSelectSource(playlist.id, videoId, sourceVideoId).then((selection) => {
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? {
        ...current,
        entries: current.entries.map((issue) => issue.video_id === videoId ? { ...issue, selected_source_id: selection.source_video_id, retry_pending: true } : issue),
        selectingVideoId: null,
      } : current);
    }).catch((error: unknown) => {
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? { ...current, selectingVideoId: null, error: normalizeRuntimeCommandError(error) } : current);
    });
  };
  const downloadPlaylistSyncCandidate = (videoId: string, candidate: PlaylistSyncCandidate) => {
    if (!bridge || !available || !playlistSyncReview) return;
    if (!libraryRootPath || !downloadHelperPath) {
      setPlaylistSyncReview((current) => current?.playlist.id === playlistSyncReview.playlist.id
        ? { ...current, error: "Downloads are unavailable until the library folder and download helper are configured." }
        : current);
      return;
    }
    if (activePlaylistSync || playlistSyncReviewDownloadingVideoId) return;
    const { playlist } = playlistSyncReview;
    const issue = playlistSyncReview.entries.find((entry) => entry.video_id === videoId);
    if (!issue) return;
    const destination = playlist.path ?? `${libraryRootPath}/${playlist.name}`;
    if (!destination.startsWith(`${libraryRootPath}/`)) {
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? { ...current, error: "Playlist download folder is outside the configured library." } : current);
      return;
    }
    setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? { ...current, selectingVideoId: videoId, error: null } : current);
    void bridge.playlistSyncSelectSource(playlist.id, videoId, candidate.video_id).then((selection) => {
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? {
        ...current,
        entries: current.entries.map((entry) => entry.video_id === videoId ? { ...entry, selected_source_id: selection.source_video_id, retry_pending: true } : entry),
        selectingVideoId: null,
      } : current);
      setPlaylistSyncReviewDownloadingVideoId(videoId);
      setCommandError(null);
      setCommandNotice(`Downloading “${issue.title}” into “${playlist.name}”…`);
      downloadQueueRef.current.push({
        id: candidate.video_id,
        source: candidate.url,
        destination,
        title: issue.title,
        artist: candidate.channel ?? issue.channel,
        playlistReviewTarget: { playlist, videoId, title: issue.title },
      });
      startNextDownload();
    }).catch((error: unknown) => {
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? { ...current, selectingVideoId: null, error: normalizeRuntimeCommandError(error) } : current);
    });
  };
  const retryOriginalPlaylistSource = (videoId: string) => persistPlaylistSyncSource(videoId, videoId);
  const syncSelectedPlaylistSources = () => {
    if (!playlistSyncReview || activePlaylistSync || !playlistSyncReview.entries.some((issue) => issue.selected_source_id && issue.retry_pending)) return;
    const playlist = playlistSyncReview.playlist;
    setPlaylistSyncReview(null);
    syncPlaylist(playlist);
  };
  const openPlaylistSyncErrors = () => {
    if (!bridge || !available || !lastFailedPlaylistSync) return;
    const { playlist, startedAtMs } = lastFailedPlaylistSync;
    setPlaylistSyncErrors({ name: playlist.name, entries: [], loading: true, error: null });
    void bridge.playlistSyncLog(playlist.id).then((value) => {
      const entries = value.entries.filter((entry) => ["failed", "unavailable"].includes(entry.status) && entry.at_ms >= startedAtMs);
      setPlaylistSyncErrors((current) => current?.name === playlist.name ? { ...current, entries, loading: false } : current);
    }).catch((error: unknown) => {
      setPlaylistSyncErrors((current) => current?.name === playlist.name ? { ...current, loading: false, error: normalizeRuntimeCommandError(error) } : current);
    });
  };
  const playShuffled = (items: Track[]) => {
    const first = shuffledFirstTrack(items);
    if (!first || !bridge || !available) return;
    const enable = projection.playback?.shuffle ? Promise.resolve(null) : bridge.toggleShuffle();
    void enable.then(() => bridge.playWithContext(first.id, items.map((item) => item.id))).catch((error: unknown) => setCommandError(normalizeRuntimeCommandError(error)));
  };
  useEffect(() => { if (!bridge) return; bridge.migrationStatus().then(setMigrationStatus).catch(() => setMigrationStatus(null)); }, [bridge]);
  useEffect(() => {
    if (!bridge) return;
    const applySettings = (value: unknown) => {
      if (!value || typeof value !== "object") return;
      const item = value as Partial<SettingsInput>;
      setSettings((current) => ({ ...current, ...item }));
      if (typeof item.music_root === "string" && item.music_root) {
        setMusicRoot(item.music_root);
        setLibraryRootPath(item.music_root);
      }
      if (typeof item.video_mode === "boolean") setVideoMode(item.video_mode);
    };
    if (fixtureHome) {
      bridge.loadSettings(fixtureHome).then(applySettings).catch((error) => {
        if (!isMissingConfigurationError(error)) setCommandError(normalizeRuntimeCommandError(error));
      });
      return;
    }
    bridge.resolveRuntimeHome().then((value) => {
      const home = (value as { home?: string } | null)?.home ?? null;
      if (!home) return;
      setRuntimeHome(home);
      return bridge.loadSettings(home).then(applySettings);
    }).catch((error) => {
      if (!isMissingConfigurationError(error)) setCommandError(normalizeRuntimeCommandError(error));
    }).then(() => bundleHelperFetch());
  }, [bridge, fixtureHome]);
  const bundleHelperFetch = () => {
    if (!bridge) return Promise.resolve();
    return bridge.bundleHelper().then((value) => {
      const helper = (value as { helper?: string } | null)?.helper ?? null;
      if (helper) setBundleHelperPath(helper);
    }).catch(() => undefined);
  };
  const settingsHome = fixtureHome ?? runtimeHome;
  const saveSettings = (next: SettingsInput) => {
    setSettings(next);
    if (!settingsHome) { setCommandError("Settings persistence is unavailable until the runtime home is resolved."); return; }
    runCommand(bridge?.saveSettings(settingsHome, next));
  };
  const submitLibraryRoot = (path: string) => {
    const root = path.trim();
    if (!root) { setCommandError("Library folder cannot be empty."); return; }
    runCommand(bridge?.selectLibraryRoot(root).then(() => {
      setLibraryRootPath(root);
      setMusicRoot(root);
      saveSettings({ ...settings, music_root: root });
      runCommand(bridge?.scanLibrary());
    }));
  };
  const [activeDownloadJob, setActiveDownloadJob] = useState<{ jobId: string } | null>(null);
  const [activeDownloadLabel, setActiveDownloadLabel] = useState<string | null>(null);
  const activeDownloadJobRef = useRef<string | null>(null);
  const reviewDownloadTargetsRef = useRef(new Map<string, { playlist: Playlist; videoId: string; title: string }>());
  const downloadQueueRef = useRef<Array<{ id: string; source: string; destination: string; playlistReviewTarget?: { playlist: Playlist; videoId: string; title: string } } & DownloadMetadata>>([]);
  const downloadHelperPath = fixtureHelper ?? bundleHelperPath;
  // Destinations are concrete folders under the library root. The default is
  // `Unsorted` (created inside the root when missing); playlist folders that
  // the scanner already discovered appear as additional choices.
  const downloadDestinations = useMemo(() => {
    if (fixtureDestination) return [fixtureDestination];
    if (!libraryRootPath) return [];
    const unsorted = `${libraryRootPath}/Unsorted`;
    const playlistDirs = (projection.library?.playlists ?? [])
      .map((playlist) => playlist.path)
      .filter((path): path is string => !!path && path.startsWith(`${libraryRootPath}/`) && path !== unsorted)
      .sort((a, b) => a.localeCompare(b));
    return [unsorted, ...playlistDirs];
  }, [fixtureDestination, libraryRootPath, projection.library]);
  const downloadBase = downloadDestinations[0] ?? null;
  const sourceForDownload = (id: string) => csvPreview.find((item) => item.id === id)?.source ?? downloadSources.find((item) => item.query === id || item.url === id)?.url ?? downloadSources.find((item) => item.query === id || item.url === id)?.query ?? id;
  const startNextDownload = () => {
    if (activeDownloadJobRef.current) return;
    const next = downloadQueueRef.current.shift();
    if (!next) { setActiveDownloadLabel(null); return; }
    const reviewTarget = next.playlistReviewTarget;
    if (!next.destination || !downloadHelperPath) {
      setCommandError("Downloads are unavailable until a library folder and the bundled download helper are configured.");
      setActiveDownloadJob(null);
      setActiveDownloadLabel(null);
      return;
    }
    const safeId = next.id.replace(/\s+/g, "-").replace(/[^a-zA-Z0-9_-]/g, "").slice(0, 24) || "item";
    const jobId = `dl-${Date.now()}-${safeId}`;
    const extension = fixtureDestination ? "mp3" : "m4a";
    const row = csvPreview.find((item) => item.id === next.id || item.source === next.source);
    const searchRow = downloadSources.find((item) => item.query === next.id || item.url === next.id);
    const metadata: DownloadMetadata = {
      artist: next.artist ?? row?.artist ?? searchRow?.artist ?? null,
      title: next.title ?? row?.title ?? searchRow?.title ?? null,
    };
    // The library scanner derives Artist/Title from the FILENAME
    // ("Artist - Title.ext"); URL/queue downloads resolve metadata up front so
    // they are not stuck with the opaque job id and "Unknown artist".
    const filename = downloadFilename(metadata, extension, jobId);
    activeDownloadJobRef.current = jobId;
    if (reviewTarget) reviewDownloadTargetsRef.current.set(jobId, reviewTarget);
    setActiveDownloadJob({ jobId });
    setActiveDownloadLabel(downloadDisplayLabel(metadata));
    // A rejected start must clear the active-job gate so the NEXT queued item
    // can run; otherwise the 15s fallback in the job-watch effect would
    // double-start (Busy) and the queue wedges silently — the "nothing
    // happened" symptom when a download could not start.
    const start = bridge?.startDownload(jobId, `${next.destination}/${filename}`, downloadHelperPath, next.source.replace(/\r|\n/g, " ").trim());
    if (!start) {
      reviewDownloadTargetsRef.current.delete(jobId);
      if (reviewTarget) setPlaylistSyncReviewDownloadingVideoId(null);
      activeDownloadJobRef.current = null; setActiveDownloadJob(null); setActiveDownloadLabel(null); return;
    }
    start.then(() => setCommandError(null)).catch((error: unknown) => {
      reviewDownloadTargetsRef.current.delete(jobId);
      if (reviewTarget) {
        setPlaylistSyncReviewDownloadingVideoId(null);
        setPlaylistSyncReview((current) => current?.playlist.id === reviewTarget.playlist.id
          ? { ...current, error: normalizeRuntimeCommandError(error) }
          : current);
      }
      activeDownloadJobRef.current = null;
      setActiveDownloadJob(null);
      setActiveDownloadLabel(null);
      setCommandError(normalizeRuntimeCommandError(error));
      startNextDownload();
    });
  };
  useEffect(() => {
    if (!activeDownloadJob) return;
    const job = projection.jobs[activeDownloadJob.jobId];
    if (job) {
      if (["succeeded", "failed", "cancelled"].includes(job.state)) {
        const reviewTarget = reviewDownloadTargetsRef.current.get(activeDownloadJob.jobId);
        if (reviewTarget) {
          reviewDownloadTargetsRef.current.delete(activeDownloadJob.jobId);
          setPlaylistSyncReviewDownloadingVideoId(null);
          if (job.state === "succeeded" && bridge) {
            void bridge.playlistSyncIssues(reviewTarget.playlist.id).then((entries) => {
              setPlaylistSyncReview((current) => current?.playlist.id === reviewTarget.playlist.id
                ? { ...current, entries, error: null }
                : current);
              setPlaylistSyncReviewOffer(entries.length > 0
                ? { playlist: reviewTarget.playlist, count: entries.length }
                : null);
              setCommandNotice(`Downloaded “${reviewTarget.title}” into “${reviewTarget.playlist.name}”.`);
            }).catch((error: unknown) => {
              setPlaylistSyncReview((current) => current?.playlist.id === reviewTarget.playlist.id
                ? { ...current, error: normalizeRuntimeCommandError(error) }
                : current);
            });
          } else {
            setCommandNotice(null);
            setPlaylistSyncReview((current) => current?.playlist.id === reviewTarget.playlist.id
              ? { ...current, error: job.state === "failed" ? "This version failed to download. The app status message has the downloader's error details; choose another version or retry." : "Download cancelled." }
              : current);
          }
        }
        activeDownloadJobRef.current = null;
        setActiveDownloadJob(null);
        setActiveDownloadLabel(null);
        startNextDownload();
      }
      return;
    }
    const deadline = window.setTimeout(() => {
      const reviewTarget = reviewDownloadTargetsRef.current.get(activeDownloadJob.jobId);
      if (reviewTarget) {
        reviewDownloadTargetsRef.current.delete(activeDownloadJob.jobId);
        setPlaylistSyncReviewDownloadingVideoId(null);
        setPlaylistSyncReview((current) => current?.playlist.id === reviewTarget.playlist.id
          ? { ...current, error: "The candidate download did not start. Check the app status message and try again." }
          : current);
        setCommandNotice(null);
      }
      activeDownloadJobRef.current = null;
      setActiveDownloadJob(null);
      setActiveDownloadLabel(null);
      setCommandError("The download did not start. Check the download helper and try again.");
      startNextDownload();
    }, 15000);
    return () => window.clearTimeout(deadline);
  }, [projection.jobs, activeDownloadJob, bridge]);
  const chooseRoot = () => {
    if (!viteEnv?.DEV) { setCommandError("Native library root picker is unavailable in this runtime."); return; }
    const root = window.prompt("Development probe: enter an existing absolute library folder", musicRoot === "Not configured" ? "" : musicRoot);
    if (!root || !bridge) return;
    runCommand(bridge.selectLibraryRoot(root).then(() => { setMusicRoot(root); saveSettings({ ...settings, music_root: root }); }));
  };
  const track = currentTrack(projection);
  // While discover mode plays a recommendation, `currentTrack` is null (the
  // queue holds recommendation keys, not library ids). Keep the last library
  // track so the recommendation metadata list survives that gap.
  const lastLibraryTrackRef = useRef<Track | null>(null);
  if (track) lastLibraryTrackRef.current = track;
  const seedTrack = track ?? lastLibraryTrackRef.current;
  const footerArtwork = useTrackArtwork(track ? [track.id] : [], bridge ? (id) => bridge.trackArtwork(id) : undefined);
  const currentArtwork = track ? (footerArtwork[track.id] ?? track.artwork_url) : null;
  const tracks = projection.library?.tracks ?? [];
  const tracksKey = `${projection.library?.generation ?? 0}`;
  // Fast O(1) track lookup — the previous `tracks.find` per queue entry was
  // O(Q·N) and caused jank with 4k-track queues on every queue change.
  const tracksByIdForQueue = useMemo(() => {
    const map = new Map<string, Track>();
    for (const t of tracks) map.set(t.id, t);
    return map;
  }, [tracksKey]);
  // Keep queue referentially stable across heartbeats: only recompute when the
  // actual queue content changes, not on every position tick. Avoid
  // `queue.join` for large queues (4k ids → ~150 KB string alloc per render).
  const playbackQueue = projection.playback?.queue ?? [];
  const queueVersion = useRef<{ queue: string[]; tracks: Track[]; byId: Map<string, Track> | null; recs: Map<string, Track> | null }>({ queue: [], tracks: [], byId: null, recs: null });
  const connectedVideo = videoConnected === true;
  const discoverSeed = seedTrack?.artist && seedTrack.title ? { artist: seedTrack.artist, title: seedTrack.title } : null;
  // ---- Song Page recommendations (Last.fm similar tracks, mobile parity) ----
  const [songRecs, setSongRecs] = useState<{ items: Recommendation[]; state: RecommendationState; error: string | null }>({ items: [], state: "idle", error: null });
  const [songRecRefresh, setSongRecRefresh] = useState(0);
  const autoDiscoveryKey = useRef<string | null>(null);
  useEffect(() => {
    // Recommendations are also the source for the automatic temporary
    // queue, so fetch them when playback changes, not only when the Song Page
    // happens to be open.
    if (!discoverSeed || !bridge) {
      setSongRecs((current) => (current.state === "idle" ? current : { items: [], state: "idle", error: null }));
      return;
    }
    let active = true;
    setSongRecs((current) => ({ items: current.items, state: "loading", error: null }));
     bridge.discover(discoverSeed.artist, discoverSeed.title, 20)
      .then((items) => { if (active) setSongRecs({ items, state: "ready", error: null }); })
      .catch((error) => { if (active) setSongRecs({ items: [], state: "error", error: normalizeRuntimeCommandError(error) }); });
    return () => { active = false; };
  }, [route, discoverSeed?.artist, discoverSeed?.title, songRecRefresh, bridge]);
  useEffect(() => {
    // Discover mode owns the temporary queue while enabled; a prefetch session
    // must not race it for the asset store.
    if (projection.discover?.enabled) return;
    if (!bridge || !available || songRecs.state !== "ready" || songRecs.items.length === 0 || !discoverSeed) return;
    const seedKey = `${discoverSeed.artist}\u0000${discoverSeed.title}`;
    // The key alone is insufficient: enabling Discover invalidates the
    // prefetch session. When Discover is disabled for the same song, rebuild
    // the five-item temporary window instead of assuming the old session is
    // still alive.
    if (autoDiscoveryKey.current === seedKey && projection.preview?.active) return;
    const seeds = songRecs.items.slice(0, 50).map((item) => ({ artist: item.artist, title: item.title }));
    // Restart the temporary queue for each song. The backend keeps the
    // library queue intact and stages the first five recommendations.
    let cancelled = false;
    let retry: ReturnType<typeof setTimeout> | null = null;
    let attempts = 0;
    const attempt = () => {
       void bridge.previewPrefetch(seeds).then(() => {
         if (!cancelled) autoDiscoveryKey.current = seedKey;
      }).catch((error: unknown) => {
        if (cancelled) return;
        attempts += 1;
        // Playback snapshots and command workers can settle a few hundred ms
        // apart when a track was just selected. Retry those initial races,
        // then keep retrying explicit backend-busy responses with backoff so
        // bounded task capacity cannot silently skip this seed's window.
        if (attempts < 5 || isRetryableRuntimeCommandError(error)) {
          const delay = attempts < 5 ? 500 : Math.min(30_000, 500 * (2 ** Math.min(attempts - 5, 6)));
          retry = setTimeout(attempt, delay);
        }
        else setCommandError(normalizeRuntimeCommandError(error));
      });
    };
    attempt();
    return () => { cancelled = true; if (retry) clearTimeout(retry); };
  }, [route, bridge, available, songRecs, discoverSeed?.artist, discoverSeed?.title, projection.discover?.enabled, projection.preview?.active]);
  const previewItem = projection.preview?.active && projection.preview.current
    ? [...songRecs.items, ...discover.items].find((item) => previewKey(item.artist, item.title) === projection.preview?.current)
    : undefined;
  const previewDisplayTrack: Track | null = previewItem ? {
    id: previewKey(previewItem.artist, previewItem.title), path: null, title: previewItem.title,
    artist: previewItem.artist, album: null, duration_ms: null, artwork_url: previewItem.artwork_url ?? null,
    favorite: false, provider: null,
  } : null;
  // Recommendations that show up in the playback queue (discover mode injects
  // their keys after the current track). Without resolving them here, they were
  // silently dropped from the queue array while the raw backend index was still
  // applied — shifting Now playing / Next up onto the wrong rows.
  const recommendationTracks = useMemo(() => {
    const map = new Map<string, Track>();
    for (const item of [...songRecs.items, ...discover.items]) {
      const id = previewKey(item.artist, item.title);
      if (!map.has(id)) map.set(id, { id, path: null, title: item.title, artist: item.artist, album: null, duration_ms: null, artwork_url: item.artwork_url ?? null, favorite: false, provider: null });
    }
    return map;
  }, [songRecs.items, discover.items]);
  // Discover mode's current queue entry may be a recommendation key; resolve it
  // to its metadata so the song page, player bar, and video slot show the
  // recommendation instead of the library track behind it (or "Nothing playing").
  const discoverCurrentId = projection.discover?.enabled && projection.playback?.current_index != null
    ? projection.playback.queue[projection.playback.current_index]
    : null;
  const discoverCurrentTrack = discoverCurrentId
    ? recommendationTracks.get(discoverCurrentId) ?? (tracksByIdForQueue.has(discoverCurrentId) ? null : {
        id: discoverCurrentId,
        path: null,
        title: "Unavailable recommendation",
        artist: null,
        album: null,
        duration_ms: null,
        artwork_url: null,
        favorite: false,
        provider: null,
      })
    : null;
  // Discover owns a separate queue projection, so its ready assets must stay
  // visible to recommendation rows after the focused preview session hands
  // off. Keep one presentation snapshot for both session owners.
  const recommendationPreview = projection.preview?.active
    ? projection.preview
    : projection.discover?.enabled
      ? {
          generation: projection.discover.generation,
          active: true,
          current: discoverCurrentId && recommendationTracks.has(discoverCurrentId) ? discoverCurrentId : null,
          window: projection.discover.window,
          prepared: projection.discover.prepared ?? [],
          unavailable: projection.discover.unavailable ?? [],
          cancelled: false,
          progress: [],
        }
      : projection.preview;
  // The track whose identity the AUDIBLE output and every correlated surface
  // (song page, video slot, queue Now playing) must reflect.
  const displayTrack = previewDisplayTrack ?? discoverCurrentTrack ?? track;
  const audibleArtwork = displayTrack && displayTrack !== track ? displayTrack.artwork_url : currentArtwork;
  // Keep queue referentially stable across heartbeats: only recompute when the
  // actual queue content changes, not on every position tick. Unresolvable ids
  // keep a placeholder slot so every later row stays index-aligned with the
  // backend queue instead of shifting up.
  const queueTracks = useMemo(() => {
    const prev = queueVersion.current;
    // Cache on BOTH the queue content AND the resolver maps: a library
    // generation change (favorite/rescan) or recommendations arriving after
    // the queue must recompute, or rows stay stale ("Unavailable track",
    // missing favorite star) until the queue itself changes.
    if (prev.queue.length === playbackQueue.length && prev.queue.every((v, i) => v === playbackQueue[i])
      && prev.byId === tracksByIdForQueue && prev.recs === recommendationTracks) {
      return prev.tracks;
    }
    const out: Track[] = playbackQueue.map((id) => tracksByIdForQueue.get(id) ?? recommendationTracks.get(id) ?? {
      id, path: null, title: "Unavailable track", artist: null, album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null,
    });
    queueVersion.current = { queue: [...playbackQueue], tracks: out, byId: tracksByIdForQueue, recs: recommendationTracks };
    return out;
  }, [playbackQueue, tracksByIdForQueue, recommendationTracks]);
  // Focused preview is an audible overlay over the captured physical queue.
  // Only Discover changes the backend queue; focused recommendations replace
  // the visible current row while Next Up stays in the physical context.
  const panelQueueTracks = focusedQueueProjection(queueTracks, projection.playback?.current_index, previewDisplayTrack);
  const panelQueueIndex = projection.playback?.current_index;
  const currentVideo = displayTrack ? currentVideoForTrack(projection, displayTrack.id) : null;
  // A queue row can be a temporary recommendation key (discover mode injects
  // them). Playing one must open/advance the preview session, not hand an
  // unknown id to the library resolver; library rows play against the library
  // context only so recommendation keys never leak into play_with_context.
  const playQueueEntry = (item: Track, context: Track[]) => {
    if (recommendationTracks.has(item.id) && !tracksByIdForQueue.has(item.id)) {
      if (projection.discover?.enabled) {
        runCommand(bridge?.discoverSelect(item.id));
        return;
      }
      const seeds = [...songRecs.items, ...discover.items].slice(0, 50).map((entry) => ({ artist: entry.artist, title: entry.title }));
      runCommand(bridge?.previewTap(seeds, item.id));
      return;
    }
    if (!tracksByIdForQueue.has(item.id)) {
      // Placeholder row (a stale/dangling queue id). play_with_context would
      // reject a context that does not contain the selection; say so instead.
      setCommandNotice("That track is no longer available in your library.");
      return;
    }
    runCommand(bridge?.playWithContext(item.id, context.map((track) => track.id).filter((id) => tracksByIdForQueue.has(id))));
  };
  // ---- Last.fm API key presence (app-data key file) for honest discovery UI ----
  const [discoveryKeyConfigured, setDiscoveryKeyConfigured] = useState<boolean | null>(null);
  useEffect(() => {
    if (!bridge) return;
    let active = true;
    bridge.discoveryKeyStatus()
      .then((status) => { if (active) setDiscoveryKeyConfigured(status.configured); })
      .catch(() => { if (active) setDiscoveryKeyConfigured(false); });
    return () => { active = false; };
  }, [bridge]);
  const saveLastfmKey = (key: string) => {
    runCommand(bridge?.saveDiscoveryKey(key).then((status) => setDiscoveryKeyConfigured(status.configured)));
  };
  // Stage the song video when its page opens (worker runs on the backend).
  useEffect(() => {
    // Stage the song video when its page opens. The command RESPONSE carries
    // the authoritative video state — absorb it so the page never depends
    // solely on a pushed event (a lost video_state event used to strand the
    // stage on "Loading video preview..." forever). While Loading, poll the
    // snapshot as a self-healing reconciliation path.
    let timer: ReturnType<typeof setTimeout> | null = null;
    let cancelled = false;
    const absorb = (snapshot: unknown) => {
      if (cancelled || !snapshot || typeof snapshot !== "object" || !("status" in (snapshot as Record<string, unknown>))) return;
      backendProjection?.applyVideoSnapshot?.(snapshot as NonNullable<BackendProjection["video"]>);
    };
    const request = () => {
      if (cancelled || !displayTrack || !available || !bridge) return;
      bridge.videoStage(displayTrack.id)
        .then(absorb)
        .catch(() => undefined);
    };
    request();
    const tick = () => {
      if (cancelled) return;
      const id = displayTrack?.id;
      if (!id) return;
      // Keep re-requesting until THIS track holds a RESOLVED video state.
      // Busy collisions are silent on the backend (by design — publishing
      // pending states minted generations that poisoned event ordering), so
      // "no state / another track's state / not_checked" must all keep the
      // poll alive. Only genuine resolutions stop it: success OR a real
      // Unavailable/error for this exact track.
      const current = backendProjection?.getSnapshot().video ?? null;
      const settled = current !== null
        && current.track_id === id
        && ["ready", "playing", "paused", "unavailable", "error"].includes(current.status);
      if (settled) return;
      request();
      timer = setTimeout(tick, 2000);
    };
    timer = setTimeout(tick, 2000);
    return () => { cancelled = true; if (timer) clearTimeout(timer); };
  }, [route, displayTrack?.id, available, bridge]);
  useEffect(() => {
    // Only fetch while a Discover surface is visible (route or settings-rail
    // embed) — otherwise every track change fires an unseen network request.
    if (route !== "discover" && settingsScreen !== "discover") return;
    if (!discoverSeed || !bridge) { setDiscover((current) => ({ ...current, items: [], error: null })); return; }
    let active = true;
    setDiscover((current) => ({ ...current, loading: true, error: null }));
    bridge.discover(discoverSeed.artist, discoverSeed.title, 5)
      .then((items) => { if (active) setDiscover({ items, loading: false, error: null }); })
      .catch((error) => { if (active) setDiscover({ items: [], loading: false, error: normalizeRuntimeCommandError(error) }); });
    return () => { active = false; };
  }, [route, settingsScreen, discoverSeed?.artist, discoverSeed?.title, discoverRefresh, bridge]);
  const stagedVideoSrc = safeVideoSource(currentVideo?.source_url, currentVideo?.staged_path);
  // QUALITY UPGRADE HOT-SWAP: after the background pass replaces the quick-tier
  // file in place it republishes VideoState with revision 2. Appending a
  // #fragment CHANGES the src string so React reloads the <video> element onto
  // the upgraded bytes (WebKit strips fragments before fetching); the sync
  // planner then re-seeks the fresh element to the authoritative audio
  // position, so the swap is a quality jump rather than a restart.
  const videoRevision = typeof currentVideo?.revision === "number" ? currentVideo.revision : 0;
  const videoSrc = stagedVideoSrc !== null && videoRevision >= 2 ? `${stagedVideoSrc}#luno-r${videoRevision}` : stagedVideoSrc;
  // A <video> whose source the webview cannot decode (e.g. Opus-in-MP4 from
  // older staging) must degrade to artwork instead of sitting as a black box.
  const [videoFailedFor, setVideoFailedFor] = useState<string | null>(null);
  useEffect(() => { setVideoFailedFor(null); }, [videoSrc]);
  const videoFailed = videoSrc !== null && videoFailedFor === videoSrc;
  const rawVideoStatus = (currentVideo?.status === "not_checked" ? "not-checked" : currentVideo?.status) as VideoStageStatus | undefined;
  const videoStatus: VideoStageStatus | undefined = videoFailed ? "error" : rawVideoStatus;
  // Video MODE is the user's intent: when it is OFF the song page shows
  // artwork, even if this track's video happens to be staged.
  // Keep the surface active while a source is being downloaded. The previous
  // gate required a playable source, so `loading` was immediately converted
  // to `not-checked` and VideoStage had no chance to render its progress ring.
  const videoModeActive = videoSurfaceActive(available, connectedVideo, videoMode, !!displayTrack);
  useEffect(() => {
    // Route changes, track changes, and turning video mode off unmount the
    // stage. Clear the shell state at the same boundary so app chrome cannot
    // remain collapsed after the stage that owned fullscreen is gone.
    if (route !== "song_page" || !videoModeActive || !displayTrack) setVideoFullscreen(false);
  }, [route, videoModeActive, displayTrack?.id]);
  // TRANSPORT AUTHORITY: the <video> follows the AUDIO status directly —
  // pausing the song pauses the picture in the same render, even if a
  // video_state event were delayed or lost. Deriving this from
  // currentVideo.status instead relied on a second event stream staying in
  // lockstep and was exactly what let the clip keep playing while the song
  // was paused. VideoStage additionally holds a clip that reached its end,
  // so "audio playing + finished video" stays on the final frame here too.
  const audioPlaying = projection.playback?.status === "playing";
  // POSITION AUTHORITY: the <video> syncs against the AUDIO transport clock
  // (`playback.position_ms`), which the backend publishes synchronously on
  // every seek/pause/resume command AND every heartbeat tick. Feeding sync
  // from the video-event stream instead added a second, slower hop: scrubbing
  // lagged behind until the next video tick and pause/resume corrections ran
  // on stale positions (the visible jump). VideoState position remains for
  // bookkeeping only.
  const audioPositionMs = projection.playback?.position_ms ?? 0;
  // DOWNLOAD RING: only while video mode is ON and THIS track is staging.
  // Mode OFF means artwork, never a spinner — the backend may still finish a
  // superseded download in the background, but the stage must not show it.
  const downloadProgress = videoModeActive && currentVideo?.status === "loading" ? currentVideo.download_progress ?? null : null;
  const video = displayTrack ? { src: videoModeActive ? videoSrc : null, status: videoModeActive ? videoStatus : "not-checked", positionMs: audioPositionMs, downloadProgress, playing: videoModeActive && audioPlaying, artworkUrl: audibleArtwork, volume: settings.volume ?? 1, onVolume: (value: number) => runCommand(bridge?.setVolume(value)), onVolumeCommit: (value: number) => { runCommand(bridge?.setVolume(value)); saveSettings({ ...settings, volume: value }); }, onResize: (size: { width: number; height: number }) => { if (videoModeActive) runCommand(bridge?.videoResize(size.width, size.height)); }, onSeek: (positionMs: number) => { if (videoModeActive) runCommand(bridge?.videoSeek(positionMs)); }, onPause: (positionMs: number) => { if (videoModeActive) { runCommand(bridge?.videoPause()); runCommand(bridge?.videoSeek(positionMs)); } }, onResume: (positionMs: number) => { if (videoModeActive) { runCommand(bridge?.videoSeek(positionMs)); runCommand(bridge?.videoPlay()); } }, onEnded: () => { if (videoModeActive) runCommand(bridge?.videoPause()); }, onError: () => { if (videoModeActive || videoSrc) setVideoFailedFor(videoSrc); }, onFullscreenChange: (fullscreen: boolean) => { setVideoFullscreen(fullscreen); if (!available) return; /* CSS-driven only: window.setFullscreen() blacks out the whole window over the CSS hide-transition and can stay black on exit in WKWebView. */ if (videoModeActive) runCommand(bridge?.videoFullscreen(fullscreen)); } } : undefined;
  const recentHistory = projection.recentHistory ?? [];
  const recentKey = recentHistory.join("\u0000");
  const recentTracks = useMemo(() => {
    const out: Track[] = [];
    for (const id of recentHistory) {
      const t = tracksByIdForQueue.get(id);
      if (t) {
        out.push(t);
        continue;
      }
      // Focused/discover playback can start a temporary recommendation. Keep
      // that audible identity in the session history while its recommendation
      // metadata is still available instead of silently dropping the row.
      const recommendation = [...songRecs.items, ...discover.items].find((item) => previewKey(item.artist, item.title) === id);
      if (recommendation) out.push({ id, path: null, title: recommendation.title, artist: recommendation.artist, album: null, duration_ms: null, artwork_url: recommendation.artwork_url ?? null, favorite: false, provider: null });
    }
    return out;
  }, [recentHistory, tracksByIdForQueue, songRecs.items, discover.items]);

  // ---- Local listening stats (Home "Most popular" / "Recently downloaded") ----
  // The backend keeps no play counters, so plays are counted client-side and
  // persisted in localStorage; download completions are journalled the same
  // way because succeeded jobs do not survive an app restart.
  const [playCounts, setPlayCounts] = useState<PlayCounts>(() => loadPlayCounts());
  const [downloadJournal, setDownloadJournal] = useState<DownloadEntry[]>(() => loadDownloadJournal());
  const primedStatsRef = useRef(false);
  const countedTrackRef = useRef<string | null>(null);
  useEffect(() => {
    const head = projection.recentHistory[0] ?? null;
    // Prime on mount WITHOUT counting so reopening the app doesn't inflate the
    // last-played track; every subsequent new head is one real play.
    if (!primedStatsRef.current) { primedStatsRef.current = true; countedTrackRef.current = head; return; }
    if (!head || head === countedTrackRef.current) return;
    countedTrackRef.current = head;
    setPlayCounts((current) => {
      const next = bumpPlayCount(current, head);
      savePlayCounts(next);
      return next;
    });
  }, [recentKey]);
  const jobStatesRef = useRef<Map<string, string>>(new Map());
  useEffect(() => {
    const additions: DownloadEntry[] = [];
    for (const job of Object.values(projection.jobs)) {
      const previous = jobStatesRef.current.get(job.id);
      jobStatesRef.current.set(job.id, job.state);
      if (job.state === "succeeded" && previous !== "succeeded" && job.track_id) additions.push({ trackId: job.track_id, at: Date.now() });
    }
    if (additions.length === 0) return;
    setDownloadJournal((current) => {
      const next = recordDownloadCompletions(current, additions);
      saveDownloadJournal(next);
      return next;
    });
  }, [projection.jobs]);

  const tracksById = useMemo(() => {
    const map: Record<string, Track> = {};
    for (const item of tracks) map[item.id] = item;
    return map;
  }, [tracksKey]);
  // Made-for-you: deterministic sample keyed to catalogue membership (same
  // policy as mobile) — stable across re-renders, reshuffled when tracks are
  // added or removed.
  const madeForYouTracks = useMemo(() => madeForYouIds(Object.keys(tracksById), 50).map((id) => tracksById[id]).filter((item): item is Track => !!item), [tracksKey]);
  const popularTracks = useMemo(() => popularTrackIds(playCounts, 20).map((id) => tracksById[id]).filter((item): item is Track => !!item), [playCounts, tracksKey]);
  const popularPlaylists = useMemo(() => (projection.library?.playlists ?? [])
    .map((playlist) => ({ playlist, plays: playlist.ordered_track_ids.reduce((sum, id) => sum + (playCounts[id] ?? 0), 0) }))
    .filter((entry) => entry.plays > 0)
    .sort((a, b) => b.plays - a.plays || a.playlist.name.localeCompare(b.playlist.name))
    .slice(0, 10), [projection.library, playCounts]);
  const downloadedTracks = useMemo(() => {
    const seen = new Set<string>();
    const resolved: Track[] = [];
    for (const entry of downloadJournal) {
      const item = tracksById[entry.trackId];
      if (item && !seen.has(item.id)) { seen.add(item.id); resolved.push(item); }
    }
    return resolved.slice(0, 50);
  }, [downloadJournal, tracksKey]);

  // ---- Sidebar ("Your Library") ----
  const sidebarEntries = useMemo(() => buildSidebarEntries(projection.library, sidebarChip), [projection.library, sidebarChip]);
  const visibleSidebarEntries = useMemo(() => {
    const needle = sidebarQuery.trim().toLocaleLowerCase();
    return needle ? sidebarEntries.filter((entry) => entry.label.toLocaleLowerCase().includes(needle)) : sidebarEntries;
  }, [sidebarEntries, sidebarQuery]);
  const artworkFetcher = bridge ? (id: string) => bridge.trackArtwork(id) : undefined;
  const sidebarArtwork = useTrackArtwork(visibleSidebarEntries.map((entry) => entry.artId).filter((id): id is string => !!id), artworkFetcher);
  const nextFromLabel = useMemo(() => {
    if (!seedTrack || !projection.library) return null;
    const owner = projection.library.playlists.find((playlist) => playlist.ordered_track_ids.includes(seedTrack.id));
    return owner && owner.name.toLowerCase() !== "unsorted" ? owner.name : null;
  }, [projection.library, seedTrack?.id]);
  // The profile avatar is a settings TOGGLE: open on click, and clicking it
  // again returns to whatever screen was open before settings.
  const lastNonSettingsRoute = useRef<Route>("library");
  useEffect(() => { if (route !== "settings") lastNonSettingsRoute.current = route; }, [route]);
  const toggleSettings = () => { if (route === "settings") navigate(lastNonSettingsRoute.current); else navigate("settings"); };

  // Native window dragging from the top bar. The injected data-tauri-drag-region
  // script needs exact-target matches AND the start-dragging permission, so we
  // ALSO call the window API directly on any press over non-interactive chrome.
  const startWindowDrag = (event: ReactPointerEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest("button, input, select, textarea, a, form")) return;
    try {
      if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
      void getCurrentWindow().startDragging();
    } catch { /* dragging is unavailable outside the Tauri runtime */ }
  };

  // ---- Resizable panels: pixel columns + draggable divider handles ----
  // Panels OPEN at their minimum widths (user-set: they opened too large);
  // edge-drag resizes from there and snaps closed/open near the edges.
  const SIDEBAR_MIN_WIDTH = 240;
  const QUEUE_MIN_WIDTH = 240;
  const [sidebarWidth, setSidebarWidth] = useState(SIDEBAR_MIN_WIDTH);
  const [queueWidth, setQueueWidth] = useState(QUEUE_MIN_WIDTH);
  /** Dragging a divider into this distance from its window edge snaps the
   * panel closed; pulling back out past it snaps open at minimum width. */
  const PANEL_SNAP_CLOSE_PX = 150;
  const PANEL_MIN_WIDTH = 240;
  const shellRef = useRef<HTMLDivElement | null>(null);
  const [shellWidth, setShellWidth] = useState(0);
  useEffect(() => {
    const element = shellRef.current;
    if (!element) return;
    const update = () => setShellWidth(element.clientWidth);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const startPanelDrag = (kind: "sidebar" | "queue") => (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const handle = event.currentTarget;
    const startX = event.clientX;
    const startWidth = kind === "sidebar" ? sidebarWidth : queueWidth;
    const collapsedAtStart = kind === "sidebar" ? sidebarCollapsed : !queueOpen;
    try { handle.setPointerCapture(event.pointerId); } catch { /* pointer may vanish; drag still works via listeners */ }
    // PANEL SNAP: dragging a panel into the last ~90px before its edge CLOSES
    // it; pulling an edge back out past that line snaps it OPEN at its
    // minimum width. There are no dedicated close/open buttons anymore.
    const distanceFromEdge = (e: PointerEvent) => kind === "sidebar" ? e.clientX : shellWidth - e.clientX;
    const move = (e: PointerEvent) => {
      const fromEdge = distanceFromEdge(e);
      if (fromEdge < PANEL_SNAP_CLOSE_PX) {
        // Snap zone → panel closed. (Sidebar collapses; queue closes.)
        if (kind === "sidebar") setSidebarCollapsed(true);
        else { setQueueOpen(false); setPanelRecent(false); }
        return;
      }
      const dragged = Math.round(collapsedAtStart ? fromEdge : startWidth + (kind === "sidebar" ? e.clientX - startX : startX - e.clientX));
      if (dragged < PANEL_MIN_WIDTH) return; // stay closed until past the snap line
      if (kind === "sidebar") { setSidebarCollapsed(false); setSidebarWidth(Math.round(Math.min(440, Math.max(240, dragged)))); }
      else { setQueueOpen(true); setQueueWidth(Math.round(Math.min(480, Math.max(240, dragged)))); }
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", up);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
  };

  const homeViewEl = <HomeView allTracksCount={tracks.length} madeForYou={madeForYouTracks} favorites={tracks.filter((track) => track.favorite)} recentTracks={recentTracks} popularTracks={popularTracks} popularPlaylists={popularPlaylists} recentlyDownloaded={downloadedTracks} onPlayTrack={(item, context) => runCommand(bridge?.playWithContext(item.id, context.map((track) => track.id)))} onTrackContextMenu={openTrackMenu} onOpenLibrary={() => setLibraryHome(false)} onOpenRecent={() => navigate("recent")} onOpenDownloader={() => navigate("downloader")} onOpenPlaylist={(playlistId) => { setLibraryScope(playlistId); setLibraryHome(false); navigate("library"); }} onArtwork={(id) => (bridge ? bridge.trackArtwork(id) : Promise.resolve(null))} backendAvailable={available} />;
  const recentViewEl = <RecentView tracks={recentTracks} playback={projection.playback} backendAvailable={available} onPlay={(item, context) => runCommand(bridge?.playWithContext(item.id, context.map((track) => track.id)))} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} onContextMenu={openTrackMenu} onArtwork={(id) => (bridge ? bridge.trackArtwork(id) : Promise.resolve(null))} />;
  const discoverViewEl = <DiscoverView seed={discoverSeed} artistMissingTitle={track && !track.artist ? track.title : null} recommendations={discover.items} session={recommendationPreview} saved={projection.saved} playlists={projection.library?.playlists ?? []} loading={discover.loading} error={discover.error} onRefresh={() => setDiscoverRefresh((value) => value + 1)} onTap={(key) => runCommand(projection.discover?.enabled ? bridge?.discoverSelect(key) : bridge?.previewTap(discover.items.slice(0, 50).map((item) => ({ artist: item.artist, title: item.title })), key))} onNext={() => runCommand(projection.discover?.enabled ? bridge?.next() : bridge?.previewNext())} onPrevious={() => runCommand(projection.discover?.enabled ? bridge?.previous() : bridge?.previewPrevious())} onCancel={() => runCommand(projection.discover?.enabled ? bridge?.discoverDisable() : bridge?.previewCancel())} onSave={(key, playlistId) => runCommand(bridge?.saveRecommendation(key, playlistId))} />;
  const createDownloadDestination = () => {
    if (!libraryRootPath) { setCommandError("Create a library folder in Settings first."); return; }
    createPlaylist();
  };
  const downloaderViewEl = <DownloaderView error={commandError} jobs={projection.jobs} activeLabel={activeDownloadLabel} destinations={downloadDestinations} defaultDestination={downloadBase ?? undefined} queue={queuedDownloadUrls} searching={downloadSearchLoading} searchResults={downloadSources.map((item) => ({ id: item.url || item.query || item.title, title: item.title, artist: item.artist ?? item.provider, duration: item.duration ?? undefined, thumbnailUrl: item.thumbnail_url ?? undefined, source: item.url || item.query }))} previewTracks={csvPreview} onSearch={(query, requested) => { if (!bridge) return; const trimmed = query.trim(); if (!trimmed) return; setDownloadSearchLoading(true); setDownloadSources([]); setCommandError(null); bridge.searchDownloader(trimmed, true, requested ?? 10).then((value) => setDownloadSources(value)).catch((error) => setCommandError(normalizeRuntimeCommandError(error))).finally(() => setDownloadSearchLoading(false)); }} onPreview={(value, kind) => { if (!bridge) return; if (kind === "csv") bridge.previewCsv(value).then((result) => { const rows = (result as { rows?: Array<{ id: string; title: string; artist?: string; url?: string }> }).rows ?? []; setCsvPreview(rows.map((row) => ({ id: row.id, title: row.title, artist: row.artist, source: row.url }))); }).catch((error) => setCommandError(normalizeRuntimeCommandError(error))); else if (/^https?:\/\//i.test(value.trim())) { bridge.searchDownloader(value.trim(), false, 3).then((results) => { setCsvPreview(results.map((item) => ({ id: item.url ?? item.query, title: item.title, artist: item.artist ?? undefined, source: item.url ?? item.query }))); }).catch((error) => setCommandError(normalizeRuntimeCommandError(error))); } else bridge.searchDownloader(value, false, 10).then((results) => { setCsvPreview(results.map((item) => ({ id: item.url ?? item.query, title: item.title, artist: item.artist ?? undefined, source: item.url ?? item.query }))); }).catch((error) => setCommandError(normalizeRuntimeCommandError(error))); }} onDownload={(ids, destination) => { if (!destination || !downloadHelperPath) { setCommandError("Downloads are unavailable until a library folder and the bundled download helper are configured."); return; } for (const id of ids) downloadQueueRef.current.push({ id, source: sourceForDownload(id).replace(/\r|\n/g, " ").trim(), destination }); startNextDownload(); }} onDirectUrl={(url, destination) => { if (!bridge || !downloadHelperPath) { setCommandError("Downloads are unavailable until the bundled download helper is configured."); return; } bridge.validateDirectUrl(url, destination).then((result) => { const meta = (result ?? {}) as DownloadMetadata; downloadQueueRef.current.push({ id: url, source: url, destination, title: meta.title ?? null, artist: meta.artist ?? null }); startNextDownload(); }).catch((error) => setCommandError(normalizeRuntimeCommandError(error))); }} onQueueUrl={(url) => { if (!bridge) return; bridge.queueUrls([url]).then((result) => { const accepted = (result as { accepted?: string[] }).accepted ?? []; if (accepted.length) setQueuedDownloadUrls((current) => [...current, ...accepted]); }).catch((error) => setCommandError(normalizeRuntimeCommandError(error))); }} onDownloadQueue={(destination) => { if (!destination || !downloadHelperPath) { setCommandError("Downloads are unavailable until a library folder and the bundled download helper are configured."); return; } const urls = queuedDownloadUrls; setQueuedDownloadUrls([]); if (!bridge) return; if (urls.length) setCommandNotice(`Resolving ${urls.length} URL${urls.length === 1 ? "" : "s"}…`); const resolved = urls.reduce<Promise<void>>((chain, url) => chain.then(() => bridge.validateDirectUrl(url, destination).then((result) => { const meta = (result ?? {}) as DownloadMetadata; downloadQueueRef.current.push({ id: url, source: url, destination, title: meta.title ?? null, artist: meta.artist ?? null }); }).catch(() => { downloadQueueRef.current.push({ id: url, source: url, destination }); })), Promise.resolve()); void resolved.then(() => startNextDownload()); }} onSync={(_, destination) => { if (!destination.startsWith("/")) { setCommandError("Sync requires the configured library folder, not a playlist label."); return; } runCommand(bridge?.syncMarkers(destination)); }} onCancel={(jobId) => runCommand(bridge?.cancelJob(jobId))} onRetry={(jobId) => { if (!downloadHelperPath) { setCommandError("Download retry is unavailable until the download helper is configured."); return; } runCommand(bridge?.retryDownload(jobId)); }} onCreateDestination={createDownloadDestination} />;
  const errorLogViewEl = <ErrorLogView />;
  const settingsScreens: Record<SettingsNavLink, ReactNode> = { home: homeViewEl, recent: recentViewEl, discover: discoverViewEl, downloader: downloaderViewEl, error_log: errorLogViewEl };

   const content = views[route] ?? (route === "library" ? (libraryHome ? homeViewEl : <LibraryView snapshot={projection.library} loading={!projection.library || projection.library.generation <= 0} playback={projection.playback} backendAvailable={available} selectedScope={libraryScope} query={libraryQuery} onQueryChange={setLibraryQuery} onPlay={(item, context) => runCommand(bridge?.playWithContext(item.id, context.map((track) => track.id)))} onShuffle={() => runCommand(bridge?.toggleShuffle())} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} onContextMenu={openTrackMenu} onScan={() => runCommand(bridge?.scanLibrary())} onArtwork={(id) => (bridge ? bridge.trackArtwork(id) : Promise.resolve(null))} />) : route === "recent" ? recentViewEl : route === "discover" ? discoverViewEl : route === "downloader" ? downloaderViewEl : route === "settings" ? <SettingsView lastfmKey="" keychainStatus={discoveryKeyConfigured === null ? "loading" : discoveryKeyConfigured ? "available" : "missing"} onSaveLastfmKey={available ? saveLastfmKey : undefined} musicRoot={musicRoot} autoDiscovery={settings.auto_discovery ?? false} mediaKeys={settings.global_keys ?? true} transferManifest={transferManifest} transferDestination={transferDestination} transferResult={transferResult} error={commandError} libraryRootPath={viteEnv?.DEV ? undefined : libraryRootPath ?? undefined} onLibraryRootSubmit={viteEnv?.DEV ? undefined : submitLibraryRoot} onChooseMusicRoot={viteEnv?.DEV ? chooseRoot : undefined} onAutoDiscoveryChange={(value) => saveSettings({ ...settings, auto_discovery: value })} onMediaKeysChange={(value) => saveSettings({ ...settings, global_keys: value })} onTransferManifestChange={setTransferManifest} onTransferDestinationChange={setTransferDestination} onTransferExport={() => { if (!available) { setCommandError("Transfer export is unavailable until the backend is connected."); return; } runCommand(bridge?.transferExport().then((result) => { const manifest = (result as { manifest?: string }).manifest ?? ""; setTransferManifest(manifest); setTransferResult(manifest ? "Exported manifest is ready for review." : "Export returned no manifest."); })); }} onTransferPreview={() => runCommand(bridge?.transferPreview(transferManifest, transferDestination).then((result) => setTransferResult(JSON.stringify(result))))} onTransferApply={() => runCommand(bridge?.transferApply(transferManifest, transferDestination, {}).then((result) => setTransferResult(JSON.stringify(result))))} onOpenErrorLog={() => navigate("error_log")} activeScreen={settingsScreen} screen={settingsScreen ? settingsScreens[settingsScreen] : null} onNavigate={(target) => { if (target === "home") { setSettingsScreen(null); navigate("library"); setLibraryHome(true); } else setSettingsScreen(target); }} onScreenClosed={() => setSettingsScreen(null)} /> : route === "error_log" ? errorLogViewEl : route === "song_page" ? <SongPageView track={seedTrack} nowPlaying={displayTrack} backendAvailable={available} recommendations={songRecs.items} recommendationState={songRecs.state} recommendationError={songRecs.error} hasApiKey={discoveryKeyConfigured !== false} sessionActive={!!projection.preview?.active} discoverMode={!!projection.discover?.enabled} discoverHint={projection.discover?.enabled ? ("Pre-loaded " + String(projection.discover.ready ?? 0) + " of " + String(projection.discover.window.length) + " recommendations" + (projection.discover.queued ? " (" + String(projection.discover.queued) + " in queue)" : "") + ". Next/Previous walk them; turn off to restore your library.") : track && !track.artist ? `“${track.title}” has no artist info — similar tracks need "Artist - Title" in the filename.` : discoveryKeyConfigured === false ? "Add a Last.fm API key in Settings to enable discovery mode." : songRecs.state === "loading" ? "Finding recommendations…" : songRecs.items.length === 0 ? "No recommendations are available for this song yet." : null} onToggleDiscoverMode={() => { if (!bridge || !available) return; if (projection.discover?.enabled) { runCommand(bridge.discoverDisable()); return; } if (discoveryKeyConfigured === false) { setCommandError("Add a Last.fm API key in Settings to enable discovery mode."); return; } if (track && !track.artist) { setCommandError(`“${track.title}” has no artist info, so there are no recommendations to play yet.`); return; } if (songRecs.state === "loading") { setCommandNotice("Still finding recommendations for this song…"); return; } if (songRecs.items.length === 0) { setCommandError("No recommendations to play yet."); return; } runCommand(bridge.discoverEnable(songRecs.items.slice(0, 10).map((entry) => ({ artist: entry.artist, title: entry.title })))); }} onRetryRecommendations={() => setSongRecRefresh((value) => value + 1)} onRefreshRecommendations={() => setSongRecRefresh((value) => value + 1)} onPreviewRecommendation={(item) => { const key = previewKey(item.artist, item.title); runCommand(projection.discover?.enabled ? bridge?.discoverSelect(key) : bridge?.previewTap(songRecs.items.slice(0, 50).map((entry) => ({ artist: entry.artist, title: entry.title })), key)); }} onSaveRecommendation={(item, playlistId) => {
                      if (playlistId) {
                        runCommand(bridge?.saveRecommendation(previewKey(item.artist, item.title), playlistId));
                        return;
                      }
                      const playlists = projection.library?.playlists ?? [];
                      const target = playlists.find((playlist) => playlist.name.toLowerCase() !== "unsorted") ?? playlists[0];
                      if (!target) { setCommandError("Create a playlist before saving recommendations."); return; }
                      runCommand(bridge?.saveRecommendation(previewKey(item.artist, item.title), target.id));
                      }} preview={recommendationPreview} saved={projection.saved} playlists={projection.library?.playlists ?? []} jobs={projection.jobs} onCreatePlaylist={(item) => { if (!bridge || !available) return; createPlaylist([], (name) => runCommand(bridge.saveRecommendation(previewKey(item.artist, item.title), `playlist:${name}`))); }} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} video={video} videoConnection={connectedVideo ? "connected" : "disconnected"} onLoadVideo={() => { if (displayTrack && available) bridge?.videoStage(displayTrack.id).catch(() => undefined); }} videoMode={videoMode} onVideoModeChange={(enabled) => { setVideoMode(enabled); runCommand(bridge?.setVideoMode(enabled)); }} /> : <div className="luno-route-empty">{routeLabels[route]}</div>);

  const showSidebar = !sidebarCollapsed && route !== "settings";
  const narrowViewport = shellWidth > 0 && shellWidth < 1100;
  const showQueue = queueOpen && route !== "settings" && !narrowViewport;
  const focusMode = route === "settings";

  return <div ref={shellRef} className={`luno-shell${queueOpen ? "" : " no-queue"}${sidebarCollapsed ? " no-sidebar" : ""}${focusMode ? " focus-mode" : ""}${videoFullscreenClass(videoFullscreen)}`} style={{ gridTemplateColumns: `${showSidebar ? sidebarWidth : 0}px minmax(0, 1fr) ${showQueue ? queueWidth : 0}px` }} data-route={route} data-backend-state={available ? "available" : "unavailable"} data-video-state={connectedVideo ? "connected" : "unconnected"} data-video-fullscreen={videoFullscreen ? "true" : "false"} data-migration-state={migrationStatus?.status ?? "unknown"}>
    <header className="luno-topbar" data-tauri-drag-region onPointerDown={startWindowDrag}>

      <div className="luno-history">
        <button className="luno-icon-button" type="button" aria-label="Go back" disabled={history.index === 0} onClick={goBack}><IconChevronLeft size={16} /></button>
        <button className="luno-icon-button" type="button" aria-label="Go forward" disabled={history.index >= history.stack.length - 1} onClick={goForward}><IconChevronRight size={16} /></button>
      </div>
      {route !== "settings" && <nav className="luno-top-nav" aria-label="Primary navigation">
        <div className="luno-top-center">
          <button className="luno-home-button" type="button" aria-label="Home" aria-current={route === "library" ? "page" : undefined} title="Home" onClick={() => { navigate("library"); setLibraryHome(true); }}><IconHome size={16} /></button>
          <form className="luno-search" role="search" onSubmit={(event) => event.preventDefault()}>
            <IconSearch size={15} />
            <input aria-label="Search a song" placeholder="Search a song" autoCorrect="off" spellCheck={false} value={libraryQuery} onFocus={(event) => event.currentTarget.select()} onMouseUp={(event) => { if (libraryQuery) event.currentTarget.select(); }} onChange={(event) => { navigate("library"); setLibraryQuery(event.target.value); if (event.target.value) setLibraryHome(false); }} onKeyDown={(event) => { if (event.key === "Escape") setLibraryQuery(""); }} />
            {libraryQuery && <button className="luno-icon-button" type="button" aria-label="Clear search" onClick={() => setLibraryQuery("")}><IconClose size={11} /></button>}
          </form>
        </div>
      </nav>}
      <button className="luno-user-avatar" type="button" aria-label={route === "settings" ? "Close settings" : "Open settings"} title={route === "settings" ? "Close settings" : "Settings"} aria-current={route === "settings" ? "page" : undefined} onClick={toggleSettings}><IconUser size={16} /></button>
    </header>
    <aside className="luno-sidebar" data-slot="sidebar">
      <div className="luno-lib-head">
        <h2 className="luno-lib-head-title">Your Library</h2>
      </div>
      <div className="luno-lib-tools">
        <button className="luno-chip" type="button" aria-label="Show playlists" aria-pressed={sidebarChip === "playlists"} onClick={() => setSidebarChip("playlists")}>Playlists</button>
        <button className="luno-chip" type="button" aria-label="Show albums" aria-pressed={sidebarChip === "albums"} onClick={() => setSidebarChip("albums")}>Albums</button>
        <button className="luno-icon-button" type="button" style={{ marginLeft: "auto" }} aria-label="Filter your library" aria-expanded={sidebarSearchOpen} onClick={() => { setSidebarSearchOpen((open) => !open); setSidebarQuery(""); }}><IconSearch size={13} /></button>
      </div>
      {sidebarSearchOpen && <div style={{ padding: "0 16px 10px" }}><input className="luno-lib-filter" autoFocus autoCorrect="off" spellCheck={false} aria-label="Filter playlists" placeholder="Filter…" value={sidebarQuery} onChange={(event) => setSidebarQuery(event.target.value)} /></div>}
      <div className="luno-playlist-list luno-scroll" data-slot="playlists" aria-label="Playlists">
        {visibleSidebarEntries.map((entry) => {
          const active = libraryScope === entry.scope;
          const art = entry.artId ? sidebarArtwork[entry.artId] : undefined;
          return <button className="luno-playlist-card" type="button" key={`${entry.kind}-${entry.key}`} aria-current={active ? "page" : undefined}
            onContextMenu={(event) => { if (entry.kind !== "playlist") return; event.preventDefault(); event.stopPropagation(); const playlist = projection.library?.playlists.find((item) => item.id === entry.scope); if (playlist) setContextMenu({ x: Math.max(8, Math.min(event.clientX, window.innerWidth - 230)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - 470)), playlist }); }}
            onClick={() => { setLibraryScope(entry.scope); setLibraryHome(false); navigate("library"); }}>
            {entry.kind === "favorites"
              ? <span className="luno-playlist-pin" style={{ background: "linear-gradient(135deg, #4c8bf5, #b3b3b3)" }}><IconHeart size={18} /></span>
              : entry.kind === "all"
                ? <span className="luno-playlist-pin" style={{ background: "linear-gradient(135deg, #1db954, #158a3e)" }}><IconMusicNote size={18} /></span>
                : entry.kind === "unsorted"
                  ? <span className="luno-playlist-pin" style={{ background: "linear-gradient(135deg, #3e3e3e, #181818)" }}><IconDownload size={16} /></span>
                  : art
                    ? <span className="luno-artwork" style={{ width: 48, height: 48, flex: "0 0 48px", borderRadius: 5 }}><img src={art} alt="" decoding="async" /></span>
                    : <span className="luno-playlist-pin" style={{ fontSize: 20 }}>♫</span>}
            <span className="luno-playlist-copy"><strong>{entry.label}</strong><small>{entry.subtitle}</small></span>
          </button>;
        })}
        {visibleSidebarEntries.length === 0 && <p style={{ padding: "8px 10px", color: "var(--luno-text-2)", fontSize: 13 }}>{sidebarChip === "albums" ? "No albums with metadata yet." : sidebarQuery ? "No matching playlists." : "No playlists yet."}</p>}
      </div>
    </aside>
        <main className="luno-main" data-slot="main" aria-disabled={!available}>{(() => { const backendError = projection.errors.length ? projection.errors[projection.errors.length - 1].message : null; const displayCommandError = commandError && (commandError.includes("PoisonError") || commandError.includes("poison recovered") || commandError.includes("command worker failed"))
              ? "A background task hit an internal error and was recovered. If this repeats, restart the app."
              : commandError;
          const videoNotice = !connectedVideo && route === "song_page";
          const syncJob = activePlaylistSync ? projection.jobs[activePlaylistSync.jobId] : null;
          return (!available || displayCommandError || backendError || commandNotice || activePlaylistSync || videoNotice || playlistSyncReviewOffer) && <div className="luno-runtime-status" role="status">
            {!available && <span data-state="backend-unavailable">Backend unavailable; runtime commands are unavailable.</span>}
            {displayCommandError && <span data-state="command-rejected">{displayCommandError} <button type="button" onClick={() => setCommandError(null)} aria-label="Dismiss error">Dismiss</button></span>}
            {backendError && <span data-state="backend-error">{backendError} <button type="button" onClick={() => backendProjection?.clearErrors?.()} aria-label="Dismiss error">Dismiss</button>{backendError.startsWith("playlist sync") && lastFailedPlaylistSync && <button type="button" onClick={openPlaylistSyncErrors}>View sync errors</button>}</span>}
            {activePlaylistSync && <span data-state="playlist-sync">{syncJob?.current_item ? `Downloading “${syncJob.current_item}”` : `Checking “${activePlaylistSync.playlist.name}”`} {syncJob?.progress != null && syncJob.current_item ? `· playlist ${Math.round(syncJob.progress * 100)}%` : ""}</span>}
            {commandNotice && <span data-state="command-notice">{commandNotice} <button type="button" onClick={() => setCommandNotice(null)} aria-label="Dismiss notice">Dismiss</button></span>}
            {playlistSyncReviewOffer && <span data-state="playlist-sync-review"><button type="button" onClick={() => openPlaylistSyncReview(playlistSyncReviewOffer.playlist)}>Review {playlistSyncReviewOffer.count} unavailable {playlistSyncReviewOffer.count === 1 ? "song" : "songs"}</button></span>}
            {videoNotice && <span data-state="video-unconnected">Video preview unavailable; artwork fallback is active.</span>}
          </div>; })()}<div className="luno-main-content luno-scroll">{content}</div></main>
        <aside className="luno-queue" data-slot="queue" aria-label="Queue">{queue ?? <Queue tracks={panelRecent ? recentTracks : panelQueueTracks} currentIndex={panelRecent ? null : panelQueueIndex} recent={panelRecent} backendAvailable={available} onTabChange={(recent) => setPanelRecent(recent)} nextFromLabel={nextFromLabel} onArtwork={(id) => (bridge ? bridge.trackArtwork(id) : Promise.resolve(null))} onPlay={(item) => playQueueEntry(item, panelRecent ? recentTracks : panelQueueTracks)} onPlayNext={(item) => (tracksByIdForQueue.has(item.id) ? runCommand(bridge?.playNext([item.id])) : setCommandNotice("Only library tracks can be queued."))} onAddToQueue={(item) => (tracksByIdForQueue.has(item.id) ? runCommand(bridge?.addToQueue([item.id])) : setCommandNotice("Only library tracks can be queued."))} onRemove={(index) => runCommand(bridge?.removeFromQueue(index))} onReorder={(from, to) => runCommand(bridge?.reorderQueue(from, to))} onContextMenu={openTrackMenu} />}</aside>
        <footer className="luno-player" data-slot="player" aria-label="Player">{player ?? <PlayerBar track={displayTrack} playback={projection.playback} artwork={audibleArtwork} volume={settings.volume ?? 1} backendAvailable={available} shuffle={!!projection.playback?.shuffle} repeat={projection.playback?.repeat ?? "off"} onToggleShuffle={() => runCommand(bridge?.toggleShuffle())} onCycleRepeat={() => runCommand(bridge?.setRepeat((projection.playback?.repeat ?? "off") === "off" ? "all" : projection.playback?.repeat === "all" ? "one" : "off"))} onPlayPause={() => runCommand(projection.playback?.status === "playing" ? bridge?.pause() : projection.playback?.status === "paused" ? bridge?.resume() : track && bridge?.playWithContext(track.id, queueTracks.map((item) => item.id)))} onNext={() => { if (!bridge || !available) return; runCommand(bridge.next()); }} onPrevious={() => { if (!bridge || !available) return; runCommand(bridge.previous()); }} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} onSeek={(positionMs) => runCommand(bridge?.seek(positionMs))} onVolume={(value) => runCommand(bridge?.setVolume(value))} onVolumeCommit={(value) => { runCommand(bridge?.setVolume(value)); saveSettings({ ...settings, volume: value }); }} onOpenTrack={() => navigate("song_page")} onTrackContextMenu={openTrackMenu} />}</footer>
    {route !== "settings" && <div className="luno-resize-handle" style={{ left: showSidebar ? 8 + sidebarWidth - 4 : 2 }} role="separator" aria-orientation="vertical" aria-label="Resize library panel width" onPointerDown={startPanelDrag("sidebar")} />}
    {route !== "settings" && !narrowViewport && <div className="luno-resize-handle" style={{ left: showQueue ? Math.max(0, shellWidth - 8 - queueWidth - 4) : Math.max(2, shellWidth - 6) }} role="separator" aria-orientation="vertical" aria-label="Resize queue panel width" onPointerDown={startPanelDrag("queue")} />}
    {contextMenu && <div className="luno-context-menu" role="menu" aria-label={contextMenu.track ? `${contextMenu.track.title} actions` : `${contextMenu.playlist?.name ?? "Playlist"} actions`} style={{ left: contextMenu.x, top: contextMenu.y }} onPointerDown={(event) => event.stopPropagation()}>
      {contextMenu.track ? <>
        <button role="menuitem" type="button" onClick={() => { runCommand(bridge?.playWithContext(contextMenu.track!.id, [contextMenu.track!.id])); setContextMenu(null); }}>Play now</button>
        <button role="menuitem" type="button" onClick={() => { runCommand(bridge?.playNext([contextMenu.track!.id])); setContextMenu(null); }}>Play next</button>
        <button role="menuitem" type="button" onClick={() => { runCommand(bridge?.addToQueue([contextMenu.track!.id])); setContextMenu(null); }}>Add to queue</button>
        <hr />
        <button role="menuitem" type="button" onClick={() => { const item = contextMenu.track!; setPlaylistPickerSelection({ tracks: [item], title: `Add “${item.title}” to playlist` }); setContextMenu(null); }}>Add to playlist…</button>
        <button role="menuitem" type="button" onClick={() => { createPlaylist([contextMenu.track!]); setContextMenu(null); }}>Add to new playlist…</button>
        <button role="menuitem" type="button" onClick={() => { runCommand(bridge?.toggleFavorite(contextMenu.track!.id)); setContextMenu(null); }}>{contextMenu.track.favorite ? "Remove from favorites" : "Add to favorites"}</button>
        {contextMenu.libraryTrack && <>
          <button role="menuitem" type="button" onClick={() => { const item = contextMenu.track!; setEditTrackError(null); setEditTrack(item); setContextMenu(null); }}>Edit song info…</button>
          <hr />
          <button className="is-danger" role="menuitem" type="button" onClick={() => { const item = contextMenu.track!; setContextMenu(null); askConfirm({ title: "Delete from library", message: `Permanently delete “${item.title}” from your library?`, confirmLabel: "Delete from library", danger: true, onConfirm: () => runCommand(bridge?.deleteTracks([item.id])) }); }}>Delete from library…</button>
        </>}
      </> : contextMenu.playlist ? <>
        <button role="menuitem" type="button" disabled={contextMenu.playlist.ordered_track_ids.length === 0} onClick={() => { const items = playlistTracks(projection.library, contextMenu.playlist!); const first = items[0]; if (first) runCommand(bridge?.playWithContext(first.id, items.map((item) => item.id))); setContextMenu(null); }}>Play</button>
        <button role="menuitem" type="button" disabled={contextMenu.playlist.ordered_track_ids.length === 0} onClick={() => { playShuffled(playlistTracks(projection.library, contextMenu.playlist!)); setContextMenu(null); }}>Shuffle and play</button>
        <hr />
        <button role="menuitem" type="button" disabled={contextMenu.playlist.ordered_track_ids.length === 0} onClick={() => { const item = contextMenu.playlist!; setPlaylistPickerSelection({ tracks: playlistTracks(projection.library, item), title: `Add “${item.name}” songs to playlist` }); setContextMenu(null); }}>Add all to playlist…</button>
        <button role="menuitem" type="button" disabled={contextMenu.playlist.ordered_track_ids.length === 0} onClick={() => { createPlaylist(playlistTracks(projection.library, contextMenu.playlist!)); setContextMenu(null); }}>Add all to new playlist…</button>
        <button role="menuitem" type="button" onClick={() => { createPlaylist(); setContextMenu(null); }}>Create new playlist…</button>
        <hr />
        <button role="menuitem" type="button" onClick={() => { setPlaylistUrlError(null); setPlaylistUrlRequest(contextMenu.playlist!); setContextMenu(null); }}>{contextMenu.playlist.url ? "Edit YouTube playlist link…" : "Add YouTube playlist link…"}</button>
        <button role="menuitem" type="button" disabled={!contextMenu.playlist.url || activePlaylistSync !== null} title={contextMenu.playlist.url ? undefined : "Add a YouTube playlist link first"} onClick={() => { syncPlaylist(contextMenu.playlist!); setContextMenu(null); }}>Sync playlist</button>
        <button role="menuitem" type="button" disabled={!contextMenu.playlist.url || activePlaylistSync !== null} title={contextMenu.playlist.url ? undefined : "Add a YouTube playlist link first"} onClick={() => { openPlaylistSyncReview(contextMenu.playlist!); setContextMenu(null); }}>Review unavailable songs…</button>
        <button role="menuitem" type="button" onClick={() => { openPlaylistSyncLog(contextMenu.playlist!); setContextMenu(null); }}>Log</button>
        <hr />
        <button className="is-danger" role="menuitem" type="button" disabled={contextMenu.playlist.ordered_track_ids.length === 0} onClick={() => { const item = contextMenu.playlist!; setContextMenu(null); askConfirm({ title: "Empty playlist", message: `Permanently delete every song in “${item.name}” but keep the playlist?`, confirmLabel: "Empty playlist", danger: true, onConfirm: () => runCommand(bridge?.deleteTracks(item.ordered_track_ids)) }); }}>Empty playlist…</button>
        <button className="is-danger" role="menuitem" type="button" onClick={() => { const item = contextMenu.playlist!; setContextMenu(null); askConfirm({ title: "Delete playlist", message: `Permanently delete the playlist “${item.name}” and all files inside it?`, confirmLabel: "Delete playlist", danger: true, onConfirm: () => { if (libraryScope === item.id) { setLibraryScope("all"); setLibraryHome(false); } runCommand(bridge?.deletePlaylist(item.id)); } }); }}>Delete playlist…</button>
      </> : null}
    </div>}
    {playlistPickerSelection && <PlaylistPicker title={playlistPickerSelection.title} playlists={(projection.library?.playlists ?? []).filter((playlist) => playlist.name.toLowerCase() !== "unsorted")} onPick={(playlist) => { const items = playlistPickerSelection.tracks; setPlaylistPickerSelection(null); addTracksToPlaylist(items, playlist); }} onCreateNew={() => { const items = playlistPickerSelection.tracks; setPlaylistPickerSelection(null); createPlaylist(items); }} onDismiss={() => setPlaylistPickerSelection(null)} />}
    {editTrack && <EditTrackDialog track={editTrack} busy={editTrackBusy} error={editTrackError} onClose={() => { if (!editTrackBusy) { setEditTrack(null); setEditTrackError(null); } }} onSubmit={(value) => { if (!bridge || !available) return; setEditTrackBusy(true); setEditTrackError(null); void bridge.editTrackMetadata(editTrack.id, value.artist, value.title).then(() => { setEditTrackBusy(false); setEditTrack(null); setCommandNotice(`Updated “${[value.artist, value.title].filter(Boolean).join(" — ")}”.`); }).catch((error: unknown) => { setEditTrackBusy(false); setEditTrackError(normalizeRuntimeCommandError(error)); }); }} />}
    {confirmDialog && <ConfirmDialog title={confirmDialog.title} message={confirmDialog.message} confirmLabel={confirmDialog.confirmLabel} danger={confirmDialog.danger} busy={confirmBusy} error={confirmError} onClose={() => { if (!confirmBusy) { setConfirmDialog(null); setConfirmError(null); } }} onConfirm={() => { setConfirmBusy(true); setConfirmError(null); try { confirmDialog.onConfirm(); setConfirmBusy(false); setConfirmDialog(null); } catch (error) { setConfirmBusy(false); setConfirmError(normalizeRuntimeCommandError(error)); } }} />}
    {playlistCreateRequest && <PlaylistDialog action="create" busy={playlistCreateBusy} error={playlistCreateError} onClose={() => { if (!playlistCreateBusy) { setPlaylistCreateRequest(null); setPlaylistCreateError(null); } }} onSubmit={(value) => submitCreatePlaylist(value.name ?? "")} />}
    {playlistUrlRequest && <PlaylistUrlDialog playlistName={playlistUrlRequest.name} initialUrl={playlistUrlRequest.url} busy={playlistUrlBusy} error={playlistUrlError} onClose={() => { if (!playlistUrlBusy) { setPlaylistUrlRequest(null); setPlaylistUrlError(null); } }} onSubmit={submitPlaylistUrl} />}
    {playlistSyncLog && <PlaylistSyncLogDialog playlistName={playlistSyncLog.name} entries={playlistSyncLog.entries} loading={playlistSyncLog.loading} error={playlistSyncLog.error} onClose={() => setPlaylistSyncLog(null)} />}
    {playlistSyncErrors && <PlaylistSyncErrorsDialog playlistName={playlistSyncErrors.name} entries={playlistSyncErrors.entries} loading={playlistSyncErrors.loading} error={playlistSyncErrors.error} onClose={() => setPlaylistSyncErrors(null)} />}
    {playlistSyncReview && <PlaylistSyncReviewDialog playlistName={playlistSyncReview.playlist.name} entries={playlistSyncReview.entries} loading={playlistSyncReview.loading} error={playlistSyncReview.error} candidateLookups={playlistSyncReview.candidateLookups} selectingVideoId={playlistSyncReview.selectingVideoId} downloadingVideoId={playlistSyncReviewDownloadingVideoId} onFindCandidates={findPlaylistSyncCandidates} onChooseCandidate={downloadPlaylistSyncCandidate} onRetryOriginal={retryOriginalPlaylistSource} onSyncSelected={syncSelectedPlaylistSources} onClose={() => { if (!playlistSyncReview.selectingVideoId && !playlistSyncReviewDownloadingVideoId) setPlaylistSyncReview(null); }} />}
  </div>;
}

export function safeVideoSource(sourceUrl: string | null | undefined, stagedPath: string | null | undefined): string | null {
  // "luno-video" is the app's range-aware streaming scheme (src-tauri video::stream):
  // every response is a ≤2 MB byte window, unlike the stock asset protocol which
  // buffers a whole staged MP4 (100 MB+) into one blob per load/seek-rebuffer.
  if (stagedPath && stagedPath.startsWith("/") && !stagedPath.includes("\0") && !stagedPath.split(/[\\/]+/).includes("..")) { try { return convertFileSrc(stagedPath, "luno-video"); } catch { return null; } }
  if (sourceUrl) { try { const url = new URL(sourceUrl); if (url.protocol === "http:" || url.protocol === "https:") return url.href; } catch { return null; } }
  return null;
}

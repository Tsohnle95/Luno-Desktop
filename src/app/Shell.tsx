import { downloadDuplicateGroups, parseAllowedDownloadDuplicates } from "../state/download_duplicates";
import { captureQueuedDownload, csvDownloadSource, type QueuedDownload } from "../state/download_inputs";
import { discoveryResultsReady, requestDiscoveryIntent } from "../state/discovery_intent";
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "../styles/tokens.css";
import "../styles/layout.css";
import { SettingsView } from "../views/SettingsView";
import { ErrorLogView } from "../views/ErrorLogView";
import { HomeView, favoriteArtists } from "../views/HomeView";
import { defaultLibrarySort, LibraryView, playlistTracks, RECENT_DOWNLOADS_SCOPE, shuffledFirstTrack, type LibraryScope, type LibrarySort } from "../views/LibraryView";
import { RecentView } from "../views/RecentView";
import { SongPageView } from "../views/SongPageView";
import { destinationName, DownloaderView } from "../views/DownloaderView";
import { DiscoverView } from "../views/DiscoverView";
import { PlaylistSyncReviewView } from "../views/PlaylistSyncReviewView";
import { ArtworkManager } from "../views/ArtworkManager";
import type { BackendProjection } from "../state/backend_projection";
import { currentTrack } from "../state/backend_projection";
import { storeTrackArtwork, useTrackArtworkState, useTrackArtworkWarmup } from "../state/track_artwork";
import { clearPlayCounts, loadDownloadJournal, loadPlayCounts, madeForYouIds, popularTrackIds, recordDownloadCompletions, saveDownloadJournal, type DownloadEntry } from "../state/play_stats";
import { downloadDisplayLabel, downloadFilename, type DownloadMetadata } from "../state/download_naming";
import type { Bridge, MigrationStatus } from "./bridge";
import { type Playlist, type PlaylistSyncCandidate, type PlaylistSyncIssue, type PlaylistSyncLocalMatch, type PlaylistSyncReviewSummary, type Recommendation, type SafeDeleteResult, type SearchResult, type SettingsInput, type Track, type YtDlpCookieBrowser } from "../shared/contracts";
import { previewKey } from "../shared/contracts";
import type { RecommendationState } from "../components/Recommendations";
import { Queue } from "../components/Queue";
import { PlayerBar } from "../components/PlayerBar";
import { PlaylistPicker } from "../components/PlaylistPicker";
import { LibrarySidebar } from "../components/LibrarySidebar";
import { EditTrackDialog } from "../components/EditTrackDialog";
import { ConfirmDialog, FinalCopyDeleteDialog } from "../components/ConfirmDialog";
import { PlaylistDialog, PlaylistUrlDialog, PlaylistUrlLoadingDialog, PlaylistSyncAuthDialog, PlaylistSyncLogDialog, PlaylistSyncErrorsDialog, PlaylistSyncReviewDialog, type PlaylistSyncCandidateLookup, type PlaylistSyncLogEntry } from "../components/PlaylistDialogs";
import type { VideoStageStatus } from "../components/VideoStage";
import type { SettingsNavLink } from "../views/SettingsView";
import { createPageHistory, recordPage, stepPage } from "../state/page_history";
import { IconChevronLeft, IconChevronRight, IconClose, IconHome, IconSearch, IconUser } from "../components/icons";

export const ROUTES = ["library", "recent", "discover", "downloader", "settings", "song_page", "error_log"] as const;
export type Route = (typeof ROUTES)[number];
export const routeLabels: Record<Route, string> = {
  library: "Home", recent: "Recent", discover: "Discover", downloader: "Downloader", settings: "Settings", song_page: "Song", error_log: "Error Log",
};
const MAX_PARALLEL_DOWNLOADS = 3;

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

export function isBusyDownloadError(error: unknown): boolean {
  const messagePattern = /busy|already running|another download/i;
  let details: unknown = error;
  if (details && typeof details === "object" && !Array.isArray(details)) {
    const object = details as Record<string, unknown>;
    if (object.code === "busy") return true;
    if (typeof object.message === "string" && messagePattern.test(object.message)) return true;
    details = error instanceof Error ? error.message : object.message;
  }
  if (typeof details !== "string") return false;
  if (messagePattern.test(details)) return true;
  try {
    const parsed = JSON.parse(details) as Record<string, unknown>;
    return parsed?.code === "busy" || (typeof parsed?.message === "string" && messagePattern.test(parsed.message));
  } catch {
    return false;
  }
}

export function discoverSessionAttempt(previous: string | null, enabled: boolean, occurrence: string | null): { key: string | null; start: boolean } {
  if (enabled) return { key: null, start: false };
  if (!occurrence || previous === occurrence) return { key: previous, start: false };
  return { key: occurrence, start: true };
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

/** Discover selects from its projected queue; outside it, the audible ID can be outside the physical queue. */
export function playbackDisplayTrack(discoverEnabled: boolean, discoverCurrent: Track | null, audible: Track | null, previewCurrent: Track | null, libraryCurrent: Track | null): Track | null {
  return discoverEnabled
    ? discoverCurrent ?? audible ?? libraryCurrent
    : audible ?? previewCurrent ?? libraryCurrent;
}

export function libraryFavoriteTarget(displayed: Track | null, libraryTracks: ReadonlyMap<string, Track>): Track | null {
  if (!displayed) return null;
  return libraryTracks.get(displayed.id) ?? null;
}

export function recommendationTrackMap(items: Recommendation[]): Map<string, Track> {
  const tracks = new Map<string, Track>();
  for (const item of items) {
    const id = previewKey(item.artist, item.title);
    if (!tracks.has(id)) tracks.set(id, {
      id, path: null, title: item.title, artist: item.artist, album: null,
      duration_ms: null, artwork_url: item.artwork_url ?? null, favorite: false, provider: null,
    });
  }
  return tracks;
}

/** Lookup results can change before the backend retires their queue or audible IDs. */
export function retainRecommendationTracks(current: Map<string, Track>, previous: Map<string, Track> | null, retainedIds: readonly string[]): Map<string, Track> {
  const tracks = new Map(current);
  for (const id of retainedIds) {
    const retained = previous?.get(id);
    if (!tracks.has(id) && retained) tracks.set(id, retained);
  }
  if (previous && tracks.size === previous.size && [...tracks].every(([id, track]) => previous.get(id) === track)) return previous;
  return tracks;
}

export function stoppedPlaybackCommand(bridge: Pick<Bridge, "discoverSelect" | "previewTap" | "playWithContext"> | undefined, selected: Track | null, libraryCurrent: Track | null, discoverEnabled: boolean, recommendations: Recommendation[], queue: Track[]): Promise<unknown> | undefined {
  if (!bridge || !selected) return;
  const index = recommendations.findIndex((item) => previewKey(item.artist, item.title) === selected.id);
  if (discoverEnabled && (index >= 0 || selected.id !== libraryCurrent?.id)) return bridge.discoverSelect(selected.id);
  if (index >= 0) {
    const seeds = recommendations.slice(Math.floor(index / 50) * 50, Math.floor(index / 50) * 50 + 50)
      .map((item) => ({ artist: item.artist, title: item.title }));
    return bridge.previewTap(seeds, selected.id);
  }
  if (libraryCurrent) return bridge.playWithContext(libraryCurrent.id, queue.map((item) => item.id));
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

export function Shell({ views = {}, queue, player, bridge, backendProjection, backendAvailable, videoConnected }: ShellProps) {
  const [route, setRoute] = useState<Route>("library");
  const navigate = (next: Route) => setRoute(next);
  const [queueOpen, setQueueOpen] = useState(true);
  const [panelRecent, setPanelRecent] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [libraryQuery, setLibraryQuery] = useState("");
  const [videoMode, setVideoMode] = useState(false);
  const [videoFullscreen, setVideoFullscreen] = useState(false);
  const [libraryScope, setLibraryScope] = useState<LibraryScope>("all");
  // LibraryView unmounts when routes change; keep each scope's sort direction
  // here so a downloader save cannot reset the playlist to its default order.
  const [librarySorts, setLibrarySorts] = useState<Partial<Record<string, LibrarySort>>>({});
  const [artworkScope, setArtworkScope] = useState<LibraryScope>("all");
  // The Home dashboard is the default landing view; opening any library scope
  // (or typing a search) swaps in the full LibraryView list.
  const [libraryHome, setLibraryHome] = useState(true);
  // Inside the settings page, rail tabs embed their screen on the right side
  // of the SAME settings surface instead of leaving it — except Home, which
  // always exits back to the home dashboard.
  const [settingsScreen, setSettingsScreen] = useState<SettingsNavLink | null>(null);
  const page = { route, libraryScope, libraryHome, settingsScreen, libraryQuery };
  const [pageHistory, setPageHistory] = useState(() => createPageHistory(page));
  const restoringPage = useRef(false);
  useEffect(() => {
    if (restoringPage.current) { restoringPage.current = false; return; }
    setPageHistory((history) => recordPage(history, page));
    // Search keystrokes refine the current page rather than adding history entries.
  }, [route, libraryScope, libraryHome, settingsScreen]);
  useEffect(() => {
    setPageHistory((history) => {
      const current = history.entries[history.index];
      if (current.libraryQuery === libraryQuery) return history;
      const entries = [...history.entries];
      entries[history.index] = { ...current, libraryQuery };
      return { ...history, entries };
    });
  }, [libraryQuery]);
  const travelPage = (delta: -1 | 1) => {
    const next = stepPage(pageHistory, delta);
    if (next === pageHistory) return;
    restoringPage.current = true;
    setPageHistory(next);
    const target = next.entries[next.index];
    setRoute(target.route);
    setLibraryScope(target.libraryScope);
    setLibraryHome(target.libraryHome);
    setSettingsScreen(target.settingsScreen);
    setLibraryQuery(target.libraryQuery);
  };
  const projection = useSyncExternalStore(backendProjection?.subscribe ?? (() => () => {}), backendProjection?.getSnapshot ?? (() => ({ library: null, playback: null } as BackendProjection)), backendProjection?.getSnapshot ?? (() => ({ library: null, playback: null } as BackendProjection)));
  const projectionRef = useRef(projection);
  projectionRef.current = projection;
  const [migrationStatus, setMigrationStatus] = useState<MigrationStatus | null>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [videoRequestError, setVideoRequestError] = useState<{ trackId: string; message: string } | null>(null);
  const [commandNotice, setCommandNotice] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; track?: Track; playlist?: Playlist; libraryTrack?: boolean } | null>(null);
  const [playlistPickerSelection, setPlaylistPickerSelection] = useState<{ tracks: Track[]; title: string } | null>(null);
  const [editTrack, setEditTrack] = useState<Track | null>(null);
  const [editTrackBusy, setEditTrackBusy] = useState(false);
  const [editTrackError, setEditTrackError] = useState<string | null>(null);
  const [confirmDialog, setConfirmDialog] = useState<{ title: string; message: string; confirmLabel: string; danger?: boolean; onConfirm: () => void | Promise<void> } | null>(null);
  const [confirmBusy, setConfirmBusy] = useState(false);
  const [confirmError, setConfirmError] = useState<string | null>(null);
  const [finalCopyDelete, setFinalCopyDelete] = useState<{ files: string[]; reviewId: string; verificationIncomplete: boolean; confirm: (reviewId: string) => Promise<SafeDeleteResult>; onDeleted: (result: SafeDeleteResult) => void } | null>(null);
  const [finalCopyDeleteBusy, setFinalCopyDeleteBusy] = useState(false);
  const [finalCopyDeleteError, setFinalCopyDeleteError] = useState<string | null>(null);
  const [playlistCreateRequest, setPlaylistCreateRequest] = useState<{
    tracks: Track[];
    onCreated?: (name: string, path?: string) => void;
    onAlreadyExists?: (name: string) => void;
  } | null>(null);
  const [playlistCreateBusy, setPlaylistCreateBusy] = useState(false);
  const [playlistCreateError, setPlaylistCreateError] = useState<string | null>(null);
  const [playlistUrlRequest, setPlaylistUrlRequest] = useState<{ playlist: Playlist; urls: string[] } | null>(null);
  const [playlistUrlLoading, setPlaylistUrlLoading] = useState<Playlist | null>(null);
  const playlistUrlLoadSequence = useRef(0);
  const [playlistUrlBusy, setPlaylistUrlBusy] = useState(false);
  const [playlistUrlError, setPlaylistUrlError] = useState<string | null>(null);
  const [playlistSyncAuthRequest, setPlaylistSyncAuthRequest] = useState<Playlist | null>(null);
  const [playlistSyncAuthBusy, setPlaylistSyncAuthBusy] = useState(false);
  const [playlistSyncAuthError, setPlaylistSyncAuthError] = useState<string | null>(null);
  const [activePlaylistSync, setActivePlaylistSync] = useState<{ jobId: string; playlist: Playlist; startedAtMs: number; sequence: number } | null>(null);
  const playlistSyncSequence = useRef(0);
  const [lastFailedPlaylistSync, setLastFailedPlaylistSync] = useState<{ playlist: Playlist; startedAtMs: number } | null>(null);
  const [playlistSyncLog, setPlaylistSyncLog] = useState<{ name: string; entries: PlaylistSyncLogEntry[]; loading: boolean; error: string | null } | null>(null);
  const [playlistSyncErrors, setPlaylistSyncErrors] = useState<{ name: string; entries: PlaylistSyncLogEntry[]; loading: boolean; error: string | null } | null>(null);
  const [playlistSyncReview, setPlaylistSyncReview] = useState<{ playlist: Playlist; entries: PlaylistSyncIssue[]; loading: boolean; error: string | null; candidateLookups: Record<string, PlaylistSyncCandidateLookup>; selectingVideoId: string | null } | null>(null);
  const [playlistSyncReviewDownloadingVideoId, setPlaylistSyncReviewDownloadingVideoId] = useState<string | null>(null);
  const [playlistSyncReviewOffer, setPlaylistSyncReviewOffer] = useState<{ playlist: Playlist; count: number } | null>(null);
  const [playlistSyncReviewSummary, setPlaylistSyncReviewSummary] = useState<{ entries: PlaylistSyncReviewSummary[]; loading: boolean; error: string | null }>({ entries: [], loading: false, error: null });
  const [playlistSyncReviewSummaryRefresh, setPlaylistSyncReviewSummaryRefresh] = useState(0);
  const askConfirm = (request: { title: string; message: string; confirmLabel: string; danger?: boolean; onConfirm: () => void | Promise<void> }) => {
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
    setPlaylistSyncReviewSummaryRefresh((current) => current + 1);
    setActivePlaylistSync(null);
  }, [activePlaylistSync, projection.jobs, bridge]);
  const [downloadSources, setDownloadSources] = useState<SearchResult[]>([]);
  const [downloadSearchLoading, setDownloadSearchLoading] = useState(false);
  const [downloadDestination, setDownloadDestination] = useState<string | null>(null);
  const [pendingDownloadDestinations, setPendingDownloadDestinations] = useState<string[]>([]);
  const [stagedDownloads, setStagedDownloads] = useState<QueuedDownload[]>([]);
  const [allowedDownloadDuplicates, setAllowedDownloadDuplicates] = useState<string[]>(() => {
    try { return parseAllowedDownloadDuplicates(localStorage.getItem("luno.allowed-download-duplicates") ?? "[]"); } catch { return []; }
  });
  const saveAllowedDownloadDuplicates = (values: string[]) => {
    try {
      localStorage.setItem("luno.allowed-download-duplicates", JSON.stringify(values));
      setAllowedDownloadDuplicates(values);
    } catch { setCommandError("Allowed duplicates could not be saved. Try again."); }
  };

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
  useEffect(() => {
    if (route !== "settings" || settingsScreen !== "sync_review") return;
    if (!bridge || !available) {
      setPlaylistSyncReviewSummary({ entries: [], loading: false, error: "Playlist sync review is unavailable until the backend is connected." });
      return;
    }
    let current = true;
    setPlaylistSyncReviewSummary((summary) => ({ ...summary, loading: true, error: null }));
    void bridge.playlistSyncReviewSummary().then((entries) => {
      if (current) setPlaylistSyncReviewSummary({ entries, loading: false, error: null });
    }).catch((error: unknown) => {
      if (current) setPlaylistSyncReviewSummary({ entries: [], loading: false, error: normalizeRuntimeCommandError(error) });
    });
    return () => { current = false; };
  }, [route, settingsScreen, bridge, available, playlistSyncReviewSummaryRefresh, projection.library?.generation]);
  const runCommand = (command: Promise<unknown> | null | undefined) => {
    if (!available || !command) return;
    void command.then(() => setCommandError(null)).catch((error: unknown) => setCommandError(normalizeRuntimeCommandError(error)));
  };
  const changeVideoMode = (enabled: boolean) => {
    if (!bridge || !available) return;
    void bridge.setVideoMode(enabled)
      .then(() => { setVideoMode(enabled); setVideoRequestError(null); setCommandError(null); })
      .catch((error: unknown) => setCommandError(normalizeRuntimeCommandError(error)));
  };
  const recordVideoRequestError = (trackId: string, error: unknown) => {
    setVideoRequestError({
      trackId,
      message: normalizeRuntimeCommandError(error),
    });
  };
  const showFinalCopyReview = (result: SafeDeleteResult, confirm: (reviewId: string) => Promise<SafeDeleteResult>, onDeleted: (result: SafeDeleteResult) => void) => {
    if (!result.needs_confirmation) { onDeleted(result); return; }
    if (!result.final_copy_review_id || result.final_copies.length === 0) throw new Error("The backend returned an incomplete final-copy review.");
    setFinalCopyDeleteError(null);
    setFinalCopyDelete({ files: result.final_copies, reviewId: result.final_copy_review_id, verificationIncomplete: result.verification_incomplete, confirm, onDeleted });
  };
  const deleteTracksWithReview = async (trackIds: string[], successNotice: (result: SafeDeleteResult) => string) => {
    if (!bridge || !available) throw new Error("Deletion is unavailable until the backend is connected.");
    const confirm = (reviewId: string) => bridge.deleteTracks(trackIds, reviewId);
    const result = await bridge.deleteTracks(trackIds);
    showFinalCopyReview(result, confirm, (deleted) => {
      setCommandError(null);
      setCommandNotice(successNotice(deleted));
    });
  };
  const deletePlaylistWithReview = async (playlist: Playlist) => {
    if (!bridge || !available) throw new Error("Deletion is unavailable until the backend is connected.");
    const confirm = (reviewId: string) => bridge.deletePlaylist(playlist.id, reviewId);
    const result = await bridge.deletePlaylist(playlist.id);
    showFinalCopyReview(result, confirm, () => {
      if (libraryScope === playlist.id) { setLibraryScope("all"); setLibraryHome(false); }
      setCommandError(null);
      setCommandNotice(`Deleted playlist “${playlist.name}” and its files.`);
    });
  };
  const submitFinalCopyDelete = () => {
    const review = finalCopyDelete;
    if (!review || finalCopyDeleteBusy) return;
    setFinalCopyDeleteBusy(true);
    setFinalCopyDeleteError(null);
    void review.confirm(review.reviewId)
      .then((result) => {
        if (result.needs_confirmation) {
          if (!result.final_copy_review_id || result.final_copies.length === 0) throw new Error("The backend returned an incomplete updated delete review.");
          setFinalCopyDelete({ ...review, files: result.final_copies, reviewId: result.final_copy_review_id, verificationIncomplete: result.verification_incomplete });
          setFinalCopyDeleteError("The library changed during review. Check the updated file list before deleting.");
          return;
        }
        setFinalCopyDelete(null);
        setFinalCopyDeleteError(null);
        review.onDeleted(result);
      })
      .catch((error: unknown) => setFinalCopyDeleteError(normalizeRuntimeCommandError(error)))
      .finally(() => setFinalCopyDeleteBusy(false));
  };
  const openTrackMenu = (event: ReactMouseEvent, item: Track) => {
    event.preventDefault(); event.stopPropagation();
    // Recommendation/preview rows live outside the library; edit/delete only
    // apply to real library files, so flag membership for the menu.
    const libraryTrack = projection.library?.tracks.find((track) => track.id === item.id) ?? null;
    setContextMenu({ x: Math.max(8, Math.min(event.clientX, window.innerWidth - 230)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - 350)), track: libraryTrack ?? item, libraryTrack: !!libraryTrack });
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
  const createPlaylist = useCallback((
    tracksToAdd: Track[] = [],
    onCreated?: (name: string, path?: string) => void,
    onAlreadyExists?: (name: string) => void,
  ) => {
    if (!bridge || !available) return;
    // Tauri's webview has no native window.prompt; collect the name in-app.
    setPlaylistCreateError(null);
    setPlaylistCreateRequest({ tracks: tracksToAdd, onCreated, onAlreadyExists });
  }, [bridge, available]);
  const submitCreatePlaylist = (rawName: string) => {
    if (!bridge) return;
    const name = cleanPlaylistName(rawName);
    if (!name || name.toLowerCase() === "unsorted") { setPlaylistCreateError("Choose a non-empty playlist name other than Unsorted."); return; }
    const request = playlistCreateRequest;
    setPlaylistCreateBusy(true);
    setPlaylistCreateError(null);
    void bridge.createPlaylist(name)
      .then((result) => {
        const createdPath = result && typeof result === "object" && "path" in result && typeof result.path === "string"
          ? result.path
          : undefined;
        setPlaylistCreateBusy(false);
        setPlaylistCreateRequest(null);
        setCommandError(null);
        if (request?.tracks.length) addTracksToPlaylist(request.tracks, { id: `playlist:${name}`, name });
        else if (!request?.onCreated) setCommandNotice(`Created playlist “${name}”.`);
        request?.onCreated?.(name, createdPath);
      })
      .catch((error: unknown) => {
        const message = normalizeRuntimeCommandError(error);
        if (request?.onAlreadyExists && /already exists/i.test(message)) {
          setPlaylistCreateBusy(false);
          setPlaylistCreateRequest(null);
          setPlaylistCreateError(null);
          setCommandError(null);
          request.onAlreadyExists(name);
          void bridge.scanLibrary().catch((refreshError: unknown) => {
            setCommandError(`The existing playlist was selected, but the library refresh failed: ${normalizeRuntimeCommandError(refreshError)}`);
          });
          return;
        }
        setPlaylistCreateBusy(false);
        setPlaylistCreateError(message);
      });
  };
  const openPlaylistUrlEditor = (playlist: Playlist) => {
    setContextMenu(null);
    setPlaylistUrlError(null);
    setPlaylistUrlRequest(null);
    if (!bridge) {
      setCommandNotice("Playlist links cannot be loaded until the backend is connected.");
      return;
    }
    const sequence = ++playlistUrlLoadSequence.current;
    setPlaylistUrlLoading(playlist);
    void bridge.playlistSyncSources(playlist.id)
      .then((urls) => { if (playlistUrlLoadSequence.current === sequence) setPlaylistUrlRequest({ playlist, urls }); })
      .catch((error: unknown) => { if (playlistUrlLoadSequence.current === sequence) setCommandNotice(`Could not load playlist links: ${normalizeRuntimeCommandError(error)}`); })
      .finally(() => { if (playlistUrlLoadSequence.current === sequence) setPlaylistUrlLoading(null); });
  };
  const submitPlaylistUrl = (urls: string[]) => {
    if (!bridge || !playlistUrlRequest) return;
    const { playlist } = playlistUrlRequest;
    setPlaylistUrlBusy(true);
    setPlaylistUrlError(null);
    void bridge.setPlaylistUrls(playlist.id, urls)
      .then(() => {
        setPlaylistUrlBusy(false);
        setPlaylistUrlRequest(null);
        setCommandError(null);
        setCommandNotice(urls.length > 0
          ? `Linked “${playlist.name}” to ${urls.length} YouTube ${urls.length === 1 ? "playlist" : "playlists"}.`
          : `Removed all YouTube links from “${playlist.name}”.`);
      })
      .catch((error: unknown) => { setPlaylistUrlBusy(false); setPlaylistUrlError(normalizeRuntimeCommandError(error)); });
  };
  const startPlaylistSync = (playlist: Playlist) => {
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
  const syncPlaylist = (playlist: Playlist) => {
    if (!bridge || !available || !playlist.url) return;
    startPlaylistSync(playlist);
  };
  const submitPlaylistSyncAuth = (browser: Exclude<YtDlpCookieBrowser, "none">) => {
    if (!bridge || !available || !playlistSyncAuthRequest || playlistSyncAuthBusy) return;
    if (!settingsHome) { setPlaylistSyncAuthError("Browser choice cannot be saved until Luno finishes loading its settings."); return; }
    const playlist = playlistSyncAuthRequest;
    const nextSettings = { ...settings, youtube_cookies_browser: browser };
    setPlaylistSyncAuthBusy(true);
    setPlaylistSyncAuthError(null);
    void bridge.saveSettings(settingsHome, nextSettings).then(() => {
      setSettings(nextSettings);
      setPlaylistSyncAuthRequest(null);
      setPlaylistSyncAuthBusy(false);
      startPlaylistSync(playlist);
    }).catch((error: unknown) => {
      setPlaylistSyncAuthBusy(false);
      setPlaylistSyncAuthError(normalizeRuntimeCommandError(error));
    });
  };
  const openPlaylistSyncAuth = (playlist: Playlist) => {
    setCommandError(null);
    backendProjection?.clearErrors?.();
    setPlaylistSyncAuthError(null);
    setPlaylistSyncAuthRequest(playlist);
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
      setPlaylistSyncReviewSummaryRefresh((current) => current + 1);
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
  const persistPlaylistSyncResolution = (videoId: string, replacement: PlaylistSyncLocalMatch | null) => {
    if (!bridge || !available || !playlistSyncReview || activePlaylistSync) return;
    const playlist = playlistSyncReview.playlist;
    setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? { ...current, selectingVideoId: videoId, error: null } : current);
    void bridge.playlistSyncResolveIssue(playlist.id, videoId, replacement?.track_id ?? null).then(() => {
      const remaining = playlistSyncReview.entries.filter((issue) => issue.video_id !== videoId);
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id
        ? { ...current, entries: remaining, selectingVideoId: null }
        : current);
      setPlaylistSyncReviewOffer(remaining.length > 0 ? { playlist, count: remaining.length } : null);
      setPlaylistSyncReviewSummaryRefresh((current) => current + 1);
      setCommandNotice(replacement
        ? `Using “${replacement.title}” for the unavailable song in “${playlist.name}”.`
        : `Dismissed an unavailable song from “${playlist.name}”.`);
      void bridge.playlistSyncIssues(playlist.id).then((entries) => {
        setPlaylistSyncReview((current) => current?.playlist.id === playlist.id ? { ...current, entries } : current);
        setPlaylistSyncReviewOffer(entries.length > 0 ? { playlist, count: entries.length } : null);
      }).catch(() => undefined);
    }).catch((error: unknown) => {
      setPlaylistSyncReview((current) => current?.playlist.id === playlist.id
        ? { ...current, selectingVideoId: null, error: normalizeRuntimeCommandError(error) }
        : current);
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
      setQueuedDownloadCount(downloadQueueRef.current.length);
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
  const playShuffled = (items: Track[], sourcePlaylistId?: string) => {
    const first = shuffledFirstTrack(items);
    if (!first || !bridge || !available) return;
    const enable = projection.playback?.shuffle ? Promise.resolve(null) : bridge.toggleShuffle();
    const play = () => sourcePlaylistId ? bridge.playFromPlaylist(first.id, items.map((item) => item.id), sourcePlaylistId) : bridge.playWithContext(first.id, items.map((item) => item.id));
    void enable.then(play).catch((error: unknown) => setCommandError(normalizeRuntimeCommandError(error)));
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
  const [activeDownloadJobs, setActiveDownloadJobs] = useState<Record<string, string>>({});
  const [queuedDownloadCount, setQueuedDownloadCount] = useState(0);
  const activeDownloadJobsRef = useRef(new Set<string>());
  const activeDownloadTimersRef = useRef(new Map<string, number>());
  const downloadStartRetryTimerRef = useRef<number | null>(null);
  const downloadSequenceRef = useRef(0);
  const reviewDownloadTargetsRef = useRef(new Map<string, { playlist: Playlist; videoId: string; title: string }>());
  const downloadQueueRef = useRef<Array<{ id: string; source: string; destination: string; playlistReviewTarget?: { playlist: Playlist; videoId: string; title: string } } & DownloadMetadata>>([]);
  const downloadHelperPath = fixtureHelper ?? bundleHelperPath;
  // Snapshot paths come from the backend's library scan. Keep those paths as
  // authoritative: the frontend root can retain a symlink spelling while the
  // backend returns canonical folder paths, so string-prefix checks hide valid
  // playlists.
  const downloadDestinations = useMemo(() => {
    if (fixtureDestination) return [fixtureDestination];
    if (!libraryRootPath) return [];
    const unsorted = `${libraryRootPath}/Unsorted`;
    const scannedDirs = (projection.library?.playlists ?? [])
      .filter((playlist) => playlist.name.toLowerCase() !== "unsorted")
      .map((playlist) => playlist.path)
      .filter((path): path is string => !!path);
    const playlistDirs = [...new Set([...scannedDirs, ...pendingDownloadDestinations])]
      .filter((path) => destinationName(path).toLowerCase() !== "unsorted")
      .sort((a, b) => a.localeCompare(b));
    return [unsorted, ...playlistDirs];
  }, [fixtureDestination, libraryRootPath, projection.library, pendingDownloadDestinations]);
  useEffect(() => {
    if (!projection.library) return;
    const indexed = new Set(projection.library.playlists.map((playlist) => playlist.path).filter((path): path is string => !!path));
    const indexedNames = new Set(projection.library.playlists.map((playlist) => playlist.name.toLowerCase()));
    setPendingDownloadDestinations((pending) => pending.filter((path) => !indexed.has(path) && !indexedNames.has(destinationName(path).toLowerCase())));
    setDownloadDestination((current) => {
      if (!current) return current;
      const playlist = projection.library?.playlists.find((item) => item.name.toLowerCase() === destinationName(current).toLowerCase());
      return playlist?.path ?? current;
    });
  }, [projection.library]);
  const downloadBase = downloadDestinations[0] ?? null;
  const sourceForDownload = (id: string) => csvPreview.find((item) => item.id === id)?.source ?? downloadSources.find((item) => item.query === id || item.url === id)?.url ?? downloadSources.find((item) => item.query === id || item.url === id)?.query ?? id;
  const removeActiveDownload = (jobId: string) => {
    activeDownloadJobsRef.current.delete(jobId);
    const timer = activeDownloadTimersRef.current.get(jobId);
    if (timer !== undefined) window.clearTimeout(timer);
    activeDownloadTimersRef.current.delete(jobId);
    setActiveDownloadJobs((current) => {
      if (!(jobId in current)) return current;
      const next = { ...current };
      delete next[jobId];
      return next;
    });
  };
  const scheduleDownloadStartRetry = () => {
    if (downloadStartRetryTimerRef.current !== null) return;
    downloadStartRetryTimerRef.current = window.setTimeout(() => {
      downloadStartRetryTimerRef.current = null;
      startNextDownload();
    }, 1000);
  };
  const startNextDownload = () => {
    if (!bridge) return;
    while (activeDownloadJobsRef.current.size < MAX_PARALLEL_DOWNLOADS && downloadQueueRef.current.length > 0) {
      const next = downloadQueueRef.current.shift()!;
      setQueuedDownloadCount(downloadQueueRef.current.length);
      const reviewTarget = next.playlistReviewTarget;
      if (!next.destination || !downloadHelperPath) {
        setCommandError("Downloads are unavailable until a library folder and the bundled download helper are configured.");
        if (reviewTarget) setPlaylistSyncReviewDownloadingVideoId(null);
        continue;
      }
      const safeId = next.id.replace(/\s+/g, "-").replace(/[^a-zA-Z0-9_-]/g, "").slice(0, 24) || "item";
      downloadSequenceRef.current += 1;
      const jobId = `dl-${Date.now()}-${downloadSequenceRef.current}-${safeId}`;
      const extension = fixtureDestination ? "mp3" : "m4a";
      const metadata: DownloadMetadata = { artist: next.artist ?? null, title: next.title ?? null };
      // Capture source metadata when the row is clicked; a later search can
      // replace the visible result list while this item waits for a slot.
      const filename = downloadFilename(metadata, extension, jobId);
      activeDownloadJobsRef.current.add(jobId);
      setActiveDownloadJobs((current) => ({ ...current, [jobId]: downloadDisplayLabel(metadata) ?? next.id }));
      if (reviewTarget) reviewDownloadTargetsRef.current.set(jobId, reviewTarget);
      const start = bridge.startDownload(jobId, `${next.destination}/${filename}`, downloadHelperPath, next.source.replace(/\r|\n/g, " ").trim());
      const deadline = window.setTimeout(() => {
        activeDownloadTimersRef.current.delete(jobId);
        if (!activeDownloadJobsRef.current.has(jobId) || projectionRef.current.jobs[jobId]) return;
        const target = reviewDownloadTargetsRef.current.get(jobId);
        if (target) {
          reviewDownloadTargetsRef.current.delete(jobId);
          setPlaylistSyncReviewDownloadingVideoId(null);
          setPlaylistSyncReview((current) => current?.playlist.id === target.playlist.id
            ? { ...current, error: "The candidate download did not start. Check the app status message and try again." }
            : current);
          setCommandNotice(null);
        }
        removeActiveDownload(jobId);
        setCommandError("The download did not start. Check the download helper and try again.");
        startNextDownload();
      }, 15000);
      activeDownloadTimersRef.current.set(jobId, deadline);
      void start.then(() => setCommandError(null)).catch((error: unknown) => {
        const message = normalizeRuntimeCommandError(error);
        const busy = isBusyDownloadError(error);
        reviewDownloadTargetsRef.current.delete(jobId);
        removeActiveDownload(jobId);
        if (busy) {
          downloadQueueRef.current.unshift(next);
          setQueuedDownloadCount(downloadQueueRef.current.length);
          scheduleDownloadStartRetry();
          return;
        }
        if (reviewTarget) {
          setPlaylistSyncReviewDownloadingVideoId(null);
          setPlaylistSyncReview((current) => current?.playlist.id === reviewTarget.playlist.id
            ? { ...current, error: message }
            : current);
        }
        setCommandError(message);
        startNextDownload();
      });
    }
  };
  useEffect(() => {
    for (const jobId of activeDownloadJobsRef.current) {
      const job = projection.jobs[jobId];
      if (!job) continue;
      const timer = activeDownloadTimersRef.current.get(jobId);
      if (timer !== undefined) window.clearTimeout(timer);
      activeDownloadTimersRef.current.delete(jobId);
      if (!["succeeded", "failed", "cancelled"].includes(job.state)) continue;

      const reviewTarget = reviewDownloadTargetsRef.current.get(jobId);
      if (reviewTarget) {
        reviewDownloadTargetsRef.current.delete(jobId);
        setPlaylistSyncReviewDownloadingVideoId(null);
        if (job.state === "succeeded" && bridge) {
          void bridge.playlistSyncIssues(reviewTarget.playlist.id).then((entries) => {
            setPlaylistSyncReview((current) => current?.playlist.id === reviewTarget.playlist.id
              ? { ...current, entries, error: null }
              : current);
            setPlaylistSyncReviewOffer(entries.length > 0
              ? { playlist: reviewTarget.playlist, count: entries.length }
              : null);
            setPlaylistSyncReviewSummaryRefresh((current) => current + 1);
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
      removeActiveDownload(jobId);
      startNextDownload();
    }
  }, [projection.jobs, activeDownloadJobs, bridge]);
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
  const tracks = projection.library?.tracks ?? [];
  const artworkFetcher = useCallback((id: string) => bridge ? bridge.trackArtwork(id) : Promise.resolve(null), [bridge]);
  const artworkWarmupIds = useMemo(() => [...tracks].sort((a, b) => a.title.localeCompare(b.title)).map((item) => item.id), [tracks]);
  useTrackArtworkWarmup(artworkWarmupIds, bridge ? artworkFetcher : undefined);
  // Fast O(1) track lookup — the previous `tracks.find` per queue entry was
  // O(Q·N) and caused jank with 4k-track queues on every queue change.
  const tracksByIdForQueue = useMemo(() => {
    const map = new Map<string, Track>();
    for (const t of tracks) map.set(t.id, t);
    return map;
  }, [tracks]);
  // Keep queue referentially stable across heartbeats: only recompute when the
  // actual queue content changes, not on every position tick. Avoid
  // `queue.join` for large queues (4k ids → ~150 KB string alloc per render).
  const playbackQueue = projection.playback?.queue ?? [];
  const queueVersion = useRef<{ queue: string[]; tracks: Track[]; byId: Map<string, Track> | null; recs: Map<string, Track> | null }>({ queue: [], tracks: [], byId: null, recs: null });
  const connectedVideo = videoConnected === true;
  const discoverSeed = seedTrack?.artist && seedTrack.title ? { artist: seedTrack.artist, title: seedTrack.title } : null;
  const discoverSeedKey = discoverSeed ? `${discoverSeed.artist}\u0000${discoverSeed.title}` : null;
  const discoverOccurrenceKey = seedTrack
    ? `${seedTrack.id}\u0000${projection.playback?.current_index ?? -1}`
    : null;
  // ---- Song Page recommendations (Last.fm similar tracks, mobile parity) ----
  const [songRecs, setSongRecs] = useState<{ items: Recommendation[]; state: RecommendationState; error: string | null; seedKey: string | null }>({ items: [], state: "idle", error: null, seedKey: null });
  const [songRecRefresh, setSongRecRefresh] = useState(0);
  const autoDiscoveryKey = useRef<string | null>(null);
  const [discoverRequested, setDiscoverRequested] = useState(false);
  const discoverSessionSeedKey = useRef<string | null>(null);
  useEffect(() => {
    // Recommendations are also the source for the automatic temporary
    // queue, so fetch them when playback changes, not only when the Song Page
    // happens to be open.
    if (!discoverSeed || !bridge) {
      setSongRecs((current) => current.state === "idle" && current.seedKey === null
        ? current
        : { items: [], state: "idle", error: null, seedKey: null });
      return;
    }
    const seedKey = discoverSeedKey;
    if (!seedKey) return;
    let active = true;
    setSongRecs((current) => ({ items: current.items, state: "loading", error: null, seedKey }));
     bridge.discover(discoverSeed.artist, discoverSeed.title, 20)
      .then((items) => { if (active) setSongRecs({ items, state: "ready", error: null, seedKey }); })
      .catch((error) => { if (active) setSongRecs({ items: [], state: "error", error: normalizeRuntimeCommandError(error), seedKey }); });
    return () => { active = false; };
  }, [route, discoverSeed?.artist, discoverSeed?.title, songRecRefresh, bridge, discoverSeedKey]);
  useEffect(() => {
    if (projection.discover?.enabled) {
      discoverSessionSeedKey.current = discoverSessionAttempt(discoverSessionSeedKey.current, true, discoverOccurrenceKey).key;
      return;
    }
    if (!discoverRequested || !bridge || !available) return;
    if (!discoverOccurrenceKey || !discoveryResultsReady(discoverRequested, discoverSeedKey, songRecs)) return;
    const admission = discoverSessionAttempt(discoverSessionSeedKey.current, false, discoverOccurrenceKey);
    discoverSessionSeedKey.current = admission.key;
    if (!admission.start) return;
    const occurrenceKey = discoverOccurrenceKey;
    const seeds = songRecs.items.slice(0, 10).map((item) => ({ artist: item.artist, title: item.title }));
    let cancelled = false;
    let retry: ReturnType<typeof setTimeout> | null = null;
    let attempts = 0;
    const attempt = () => {
      void bridge.discoverEnable(seeds).then(() => {
        if (!cancelled) setCommandError(null);
      }).catch((error: unknown) => {
        if (cancelled) return;
        attempts += 1;
        if (attempts < 5 || isRetryableRuntimeCommandError(error)) {
          const delay = attempts < 5 ? 500 : Math.min(30_000, 500 * (2 ** Math.min(attempts - 5, 6)));
          retry = setTimeout(attempt, delay);
        } else {
          if (discoverSessionSeedKey.current === occurrenceKey) discoverSessionSeedKey.current = null;
          setCommandError(normalizeRuntimeCommandError(error));
        }
      });
    };
    attempt();
    return () => { cancelled = true; if (retry) clearTimeout(retry); };
  }, [discoverRequested, bridge, available, projection.discover?.enabled, songRecs, discoverSeedKey, discoverOccurrenceKey]);
  useEffect(() => {
    // Discover mode owns the temporary queue while enabled; a prefetch session
    // must not race it for the asset store.
    if (discoverRequested || projection.discover?.enabled) return;
    if (!bridge || !available || songRecs.state !== "ready" || songRecs.seedKey !== discoverSeedKey || songRecs.items.length === 0 || !discoverSeed) return;
    const seedKey = discoverSeedKey;
    if (!seedKey) return;
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
  }, [route, bridge, available, songRecs, discoverSeed?.artist, discoverSeed?.title, discoverSeedKey, discoverRequested, projection.discover?.enabled, projection.preview?.active]);
  const toggleDiscoverMode = () => {
    if (!bridge || !available) return;
    if (discoverRequested || projection.discover?.enabled) {
      setDiscoverRequested(false);
      discoverSessionSeedKey.current = null;
      if (projection.discover?.enabled) runCommand(bridge.discoverDisable());
      return;
    }
    discoverSessionSeedKey.current = null;
    requestDiscoveryIntent(discoveryKeyConfigured === false, { setRequested: setDiscoverRequested, reportError: setCommandError });
  };
  const stopDiscoverMode = () => {
    setDiscoverRequested(false);
    discoverSessionSeedKey.current = null;
    if (projection.discover?.enabled) runCommand(bridge?.discoverDisable());
    else runCommand(bridge?.previewCancel());
  };
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
  const currentRecommendationTracks = useMemo(
    () => recommendationTrackMap([...songRecs.items, ...discover.items]),
    [songRecs.items, discover.items],
  );
  const recommendationTracks = retainRecommendationTracks(currentRecommendationTracks, queueVersion.current.recs, [
    ...playbackQueue,
    ...(projection.playback?.audible_track_id ? [projection.playback.audible_track_id] : []),
    ...(projection.preview?.current ? [projection.preview.current] : []),
    ...(projection.preview?.window ?? []),
    ...(projection.discover?.window ?? []),
  ]);
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
  const recommendationPreview = projection.discover?.enabled
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
  const audibleId = projection.playback?.audible_track_id;
  const audibleTrack = audibleId
    ? tracksByIdForQueue.get(audibleId) ?? recommendationTracks.get(audibleId) ?? {
        id: audibleId, path: null, title: "Unavailable track", artist: null, album: null,
        duration_ms: null, artwork_url: null, favorite: false, provider: null,
      }
    : null;
  // The track whose identity the AUDIBLE output and every correlated surface
  // (song page, video slot, queue Now playing) must reflect.
  const displayTrack = playbackDisplayTrack(!!projection.discover?.enabled, discoverCurrentTrack, audibleTrack, previewDisplayTrack, track);
  const displayedLibraryTrack = libraryFavoriteTarget(displayTrack, tracksByIdForQueue);
  const displayedArtworkState = useTrackArtworkState(displayedLibraryTrack ? [displayedLibraryTrack.id] : [], bridge ? (id) => bridge.trackArtwork(id) : undefined, 3);
  const displayedArtwork = displayedArtworkState.artwork;
  const audibleArtwork = displayTrack ? (displayedArtwork[displayTrack.id] ?? displayTrack.artwork_url) : null;
  const audibleArtworkLoading = !!displayedLibraryTrack && displayedArtworkState.loading.has(displayedLibraryTrack.id);
  // Keep queue referentially stable across heartbeats: only recompute when the
  // actual queue content changes, not on every position tick. Unresolvable ids
  // keep a placeholder slot so every later row stays index-aligned with the
  // backend queue instead of shifting up.
  const queueTracks = useMemo(() => {
    const prev = queueVersion.current;
    // Cache on BOTH the queue content AND the resolver maps: a library track
    // update (favorite/rescan) or recommendations arriving after
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
  const panelQueueTracks = focusedQueueProjection(queueTracks, projection.playback?.current_index,
    projection.discover?.enabled ? null : audibleTrack && !tracksByIdForQueue.has(audibleTrack.id) ? audibleTrack : previewDisplayTrack);
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
  const audibleRecommendationNeedsVideo = !!displayTrack
    && recommendationTracks.has(displayTrack.id)
    && (!!projection.preview?.current || !!projection.discover?.enabled);
  // Stage a normal library track only when video mode is enabled. Active
  // recommendations keep adopting their independently prefetched video even
  // when song-page mode is off.
  useEffect(() => {
    if (route !== "song_page" || (!videoMode && !audibleRecommendationNeedsVideo)) return;
    // The command response is authoritative too; absorb it so the page never
    // depends solely on a pushed event. Keep polling an unresolved identity
    // after retryable admission failures so staging resumes once capacity is
    // available.
    let timer: ReturnType<typeof setTimeout> | null = null;
    let cancelled = false;
    let terminalFailure = false;
    let retryPending = false;
    const absorb = (snapshot: unknown) => {
      if (cancelled || !snapshot || typeof snapshot !== "object" || !("status" in (snapshot as Record<string, unknown>))) return;
      retryPending = false;
      backendProjection?.applyVideoSnapshot?.(snapshot as NonNullable<BackendProjection["video"]>);
      setVideoRequestError((current) => current?.trackId === displayTrack?.id ? null : current);
    };
    const request = () => {
      if (cancelled || !displayTrack || !available || !bridge) return;
      bridge.videoStage(displayTrack.id)
        .then(absorb)
        .catch((error: unknown) => {
          if (cancelled) return;
          const retryable = isRetryableRuntimeCommandError(error);
          retryPending = retryable;
          if (!retryable) terminalFailure = true;
          recordVideoRequestError(displayTrack.id, error);
        });
    };
    request();
    const tick = () => {
      if (cancelled || terminalFailure) return;
      const id = displayTrack?.id;
      if (!id) return;
      // Keep re-requesting until THIS track holds a RESOLVED video state.
      // A retryable busy response does not own a video slot, and a snapshot
      // may still belong to another track. Keep polling until this identity
      // has a real terminal video state.
      const current = backendProjection?.getSnapshot().video ?? null;
      const settled = !retryPending && current !== null
        && current.track_id === id
        && ["ready", "playing", "paused", "unavailable", "error"].includes(current.status);
      if (settled) return;
      request();
      timer = setTimeout(tick, 2000);
    };
    timer = setTimeout(tick, 2000);
    return () => { cancelled = true; if (timer) clearTimeout(timer); };
  }, [route, displayTrack?.id, available, bridge, videoMode, audibleRecommendationNeedsVideo]);
  useEffect(() => {
    // Only fetch while the Discover route is visible — otherwise every track
    // change fires an unseen network request.
    if (route !== "discover") return;
    if (!discoverSeed || !bridge) { setDiscover((current) => ({ ...current, items: [], error: null })); return; }
    let active = true;
    setDiscover((current) => ({ ...current, loading: true, error: null }));
    bridge.discover(discoverSeed.artist, discoverSeed.title, 5)
      .then((items) => { if (active) setDiscover({ items, loading: false, error: null }); })
      .catch((error) => { if (active) setDiscover({ items: [], loading: false, error: normalizeRuntimeCommandError(error) }); });
    return () => { active = false; };
  }, [route, discoverSeed?.artist, discoverSeed?.title, discoverRefresh, bridge]);
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
  const hasVideoRequestError = !!displayTrack && videoRequestError?.trackId === displayTrack.id;
  const rawVideoStatus = (currentVideo?.status === "not_checked" ? "not-checked" : currentVideo?.status) as VideoStageStatus | undefined;
  const videoStatus: VideoStageStatus | undefined = videoFailed || hasVideoRequestError ? "error" : rawVideoStatus;
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
  const video = displayTrack ? { src: videoModeActive ? videoSrc : null, status: videoModeActive ? videoStatus : "not-checked", positionMs: audioPositionMs, downloadProgress, playing: videoModeActive && audioPlaying, artworkUrl: audibleArtwork, artworkLoading: audibleArtworkLoading, volume: settings.volume ?? 1, onVolume: (value: number) => runCommand(bridge?.setVolume(value)), onVolumeCommit: (value: number) => { runCommand(bridge?.setVolume(value)); saveSettings({ ...settings, volume: value }); }, onResize: (size: { width: number; height: number }) => { if (videoModeActive) runCommand(bridge?.videoResize(size.width, size.height)); }, onSeek: (positionMs: number) => { if (videoModeActive) runCommand(bridge?.videoSeek(positionMs)); }, onPause: (positionMs: number) => { if (videoModeActive) { runCommand(bridge?.videoPause()); runCommand(bridge?.videoSeek(positionMs)); } }, onResume: (positionMs: number) => { if (videoModeActive) { runCommand(bridge?.videoSeek(positionMs)); runCommand(bridge?.videoPlay()); } }, onEnded: () => { if (videoModeActive) runCommand(bridge?.videoPause()); }, onError: () => { if (videoModeActive || videoSrc) setVideoFailedFor(videoSrc); }, onFullscreenChange: (fullscreen: boolean) => { setVideoFullscreen(fullscreen); if (!available) return; /* CSS-driven only: window.setFullscreen() blacks out the whole window over the CSS hide-transition and can stay black on exit in WKWebView. */ if (videoModeActive) runCommand(bridge?.videoFullscreen(fullscreen)); } } : undefined;
  const recentHistory = projection.recentHistory ?? [];
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

  // Download completions stay local because completed jobs do not survive a
  // restart. Listening counts are backend-owned; import the old browser journal
  // once, then remove it only after the backend confirms its durable save.
  const [downloadJournal, setDownloadJournal] = useState<DownloadEntry[]>(() => loadDownloadJournal());
  const legacyStatsImportStarted = useRef(false);
  useEffect(() => {
    const snapshot = projection.playStatistics;
    if (!snapshot) return;
    if (snapshot.legacy_track_counts_imported) { clearPlayCounts(); return; }
    if (!available || !bridge || legacyStatsImportStarted.current) return;
    legacyStatsImportStarted.current = true;
    void bridge.importLegacyPlayCounts(loadPlayCounts())
      .then(() => clearPlayCounts())
      .catch((error: unknown) => {
        legacyStatsImportStarted.current = false;
        setCommandError(normalizeRuntimeCommandError(error));
      });
  }, [projection.playStatistics, available, bridge]);
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
  }, [tracks]);
  // Made-for-you: deterministic sample keyed to catalogue membership (same
  // policy as mobile) — stable across re-renders, reshuffled when tracks are
  // added or removed.
  const madeForYouTracks = useMemo(() => madeForYouIds(Object.keys(tracksById), 50).map((id) => tracksById[id]).filter((item): item is Track => !!item), [tracksById]);
  const playCounts = useMemo(() => Object.fromEntries((projection.playStatistics?.tracks ?? []).map(({ track_id, plays }) => [track_id, plays])), [projection.playStatistics]);
  const currentTrackPlayCounts = useMemo(() => Object.fromEntries(Object.keys(tracksById).flatMap((id) => playCounts[id] ? [[id, playCounts[id]] as const] : [])), [playCounts, tracksById]);
  const popularTracks = useMemo(() => popularTrackIds(currentTrackPlayCounts, tracks.length).map((id) => tracksById[id]).filter((item): item is Track => !!item), [currentTrackPlayCounts, tracksById]);
  const playlistPlayCounts = useMemo(() => Object.fromEntries((projection.playStatistics?.playlists ?? []).map(({ playlist_id, plays }) => [playlist_id, plays])), [projection.playStatistics]);
  const popularPlaylists = useMemo(() => (projection.library?.playlists ?? [])
    .filter((playlist) => playlist.name.toLowerCase() !== "unsorted")
    .map((playlist) => ({ playlist, plays: playlistPlayCounts[playlist.id] ?? 0 }))
    .filter((entry) => entry.plays > 0)
    .sort((a, b) => b.plays - a.plays || a.playlist.name.localeCompare(b.playlist.name)), [projection.library, playlistPlayCounts]);
  const downloadedTracks = useMemo(() => {
    const addedAtByTrack = new Map<string, number>();
    for (const entry of downloadJournal) {
      if (tracksById[entry.trackId]) addedAtByTrack.set(entry.trackId, entry.at);
    }
    for (const item of Object.values(tracksById)) {
      if (typeof item.added_ms !== "number" || !Number.isFinite(item.added_ms)) continue;
      addedAtByTrack.set(item.id, Math.max(addedAtByTrack.get(item.id) ?? Number.NEGATIVE_INFINITY, item.added_ms));
    }
    return [...addedAtByTrack.entries()]
      .sort(([leftId, leftAt], [rightId, rightAt]) => rightAt - leftAt || leftId.localeCompare(rightId))
      .slice(0, 50)
      .map(([id]) => tracksById[id])
      .filter((item): item is Track => !!item);
  }, [downloadJournal, tracksById]);

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

  const artists = useMemo(() => favoriteArtists(tracks, playCounts), [tracks, playCounts]);
  const openHomeCollection = (collection: string) => {
    setLibraryScope(collection === "favorites" ? "favorites" : `home:${collection}`);
    setLibraryHome(false);
    setLibraryQuery("");
    navigate("library");
  };
  const homeCollection = libraryScope === "home:made-for-you" ? { title: "Made for you", tracks: madeForYouTracks }
    : libraryScope === "home:popular-songs" ? { title: "Most popular songs", tracks: popularTracks }
    : libraryScope.startsWith("home:artist:") ? { title: libraryScope.slice("home:artist:".length), tracks: tracks.filter((item) => item.artist?.trim().toLocaleLowerCase() === libraryScope.slice("home:artist:".length).toLocaleLowerCase()) }
    : undefined;
  const homeBrowseCollection = libraryScope === "home:popular-playlists" ? "popular-playlists" : libraryScope === "home:favorite-artists" ? "favorite-artists" : undefined;
  const openRecentlyDownloaded = () => {
    setLibraryScope(RECENT_DOWNLOADS_SCOPE);
    setLibraryHome(false);
    navigate("library");
  };
  const homeViewEl = <HomeView collection={!libraryHome ? homeBrowseCollection : undefined} favoriteArtists={artists} onOpenCollection={openHomeCollection} onOpenArtist={(name) => openHomeCollection(`artist:${name}`)} allTracksCount={tracks.length} madeForYou={madeForYouTracks} favorites={tracks.filter((track) => track.favorite)} recentTracks={recentTracks} popularTracks={popularTracks} playCounts={playCounts} popularPlaylists={popularPlaylists} recentlyDownloaded={downloadedTracks} tracksById={tracksByIdForQueue} onPlayTrack={(item, context) => runCommand(bridge?.playWithContext(item.id, context.map((track) => track.id)))} onTrackContextMenu={openTrackMenu} onOpenLibrary={() => setLibraryHome(false)} onOpenRecent={() => navigate("recent")} onOpenRecentlyDownloaded={openRecentlyDownloaded} onOpenPlaylist={(playlistId) => { setLibraryScope(playlistId); setLibraryHome(false); navigate("library"); }} onArtwork={(id) => (bridge ? bridge.trackArtwork(id) : Promise.resolve(null))} backendAvailable={available} />;
  const recentViewEl = <RecentView tracks={recentTracks} playback={projection.playback} backendAvailable={available} onPlay={(item, context) => runCommand(bridge?.playWithContext(item.id, context.map((track) => track.id)))} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} onContextMenu={openTrackMenu} onArtwork={(id) => (bridge ? bridge.trackArtwork(id) : Promise.resolve(null))} />;
  const discoverViewEl = <DiscoverView seed={discoverSeed} artistMissingTitle={track && !track.artist ? track.title : null} recommendations={discover.items} session={recommendationPreview} saved={projection.saved} playlists={projection.library?.playlists ?? []} loading={discover.loading} error={discover.error} onRefresh={() => setDiscoverRefresh((value) => value + 1)} onTap={(key) => runCommand(projection.discover?.enabled ? bridge?.discoverSelect(key) : bridge?.previewTap(discover.items.slice(0, 50).map((item) => ({ artist: item.artist, title: item.title })), key))} onNext={() => runCommand(projection.discover?.enabled ? bridge?.next() : bridge?.previewNext())} onPrevious={() => runCommand(projection.discover?.enabled ? bridge?.previous() : bridge?.previewPrevious())} onCancel={stopDiscoverMode} onSave={(key, playlistId) => runCommand(bridge?.saveRecommendation(key, playlistId))} />;
  const selectDownloadDestination = (name: string, path?: string) => {
    if (!libraryRootPath || fixtureDestination) return;
    const destination = path ?? `${libraryRootPath}/${name}`;
    const alreadyIndexed = projectionRef.current.library?.playlists.some((playlist) => playlist.path === destination) ?? false;
    if (!alreadyIndexed) {
      setPendingDownloadDestinations((current) => current.includes(destination) ? current : [...current, destination]);
    }
    setDownloadDestination(destination);
  };
  const createDownloadDestination = () => {
    if (!libraryRootPath) { setCommandError("Create a library folder in Settings first."); return; }
    createPlaylist([], (name, path) => {
      if (!path) return;
      selectDownloadDestination(name, path);
      setCommandNotice(null);
    }, (name) => {
      const indexedPath = projectionRef.current.library?.playlists.find((playlist) => playlist.name.toLowerCase() === name.toLowerCase())?.path ?? undefined;
      selectDownloadDestination(name, indexedPath);
      setCommandNotice(`Selected the existing “${name}” playlist for downloads.`);
    });
  };
  const downloaderViewEl = <DownloaderView
    error={commandError}
    jobs={projection.jobs}
    syncPlaylists={(projection.library?.playlists ?? []).filter((playlist) => !!playlist.path && !!playlist.url)}
    activeLabels={activeDownloadJobs}
    queuedDownloads={queuedDownloadCount}
    onCancelWaiting={() => { downloadQueueRef.current = []; setQueuedDownloadCount(0); }}
    destinations={downloadDestinations}
    defaultDestination={downloadBase ?? undefined}
    destination={downloadDestination ?? downloadBase}
    onDestinationChange={setDownloadDestination}
    queue={stagedDownloads.map((item) => `${downloadDisplayLabel(item) ?? item.id} → ${destinationName(item.destination)}`)}
    onQueueTracks={(ids, destination) => {
      setStagedDownloads((current) => [...current, ...ids.map((id) => captureQueuedDownload(id, destination, csvPreview.find((row) => row.id === id) ?? downloadSources.find((row) => row.url === id || row.query === id)))]);
    }}
    onRemoveQueued={(index) => setStagedDownloads((current) => current.filter((_, at) => at !== index))}
    onClearQueue={() => setStagedDownloads([])}
    searching={downloadSearchLoading}
    searchResults={downloadSources.map((item) => ({ id: item.url || item.query || item.title, title: item.title, artist: item.artist ?? item.provider, duration: item.duration ?? undefined, thumbnailUrl: item.thumbnail_url ?? undefined, source: item.url || item.query }))}
    previewTracks={csvPreview}
    duplicateGroups={downloadDuplicateGroups(projection.library?.tracks ?? [], allowedDownloadDuplicates)}
    allowedDuplicates={allowedDownloadDuplicates}
    onRestoreDuplicate={(key) => saveAllowedDownloadDuplicates(allowedDownloadDuplicates.filter((item) => item !== key))}
    onAllowDuplicates={(keys) => saveAllowedDownloadDuplicates([...new Set([...allowedDownloadDuplicates, ...keys])])}
    onExportAllowed={() => {
      const url = URL.createObjectURL(new Blob([JSON.stringify(allowedDownloadDuplicates, null, 2)], { type: "application/json" }));
      const link = document.createElement("a"); link.href = url; link.download = "luno-allowed-duplicates.json"; link.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    }}
    onImportAllowed={(file) => { void file.text().then((text) => saveAllowedDownloadDuplicates(parseAllowedDownloadDuplicates(text))).catch((error) => setCommandError(normalizeRuntimeCommandError(error))); }}
    onSearch={(query, requested) => {
      if (!bridge) return;
      const trimmed = query.trim();
      if (!trimmed) return;
      setDownloadSearchLoading(true);
      setDownloadSources([]);
      setCsvPreview([]);
      setCommandError(null);
      bridge.searchDownloader(trimmed, true, requested ?? 10)
        .then((value) => setDownloadSources(value))
        .catch((error) => setCommandError(normalizeRuntimeCommandError(error)))
        .finally(() => setDownloadSearchLoading(false));
    }}
    onPreview={(value) => {
      if (!bridge) return;
      setDownloadSearchLoading(true);
      setCommandError(null);
      setDownloadSources([]);
      bridge.previewCsv(value).then((result) => {
        const rows = (result as { rows?: Array<{ id: string; title: string; artist?: string; url?: string }> }).rows ?? [];
        if (rows.length === 0) setCommandError("This CSV contains no songs. Choose an Exportify CSV with Track Name and Artist Name(s) columns.");
        setCsvPreview(rows.map((row) => ({ id: row.id, title: row.title, artist: row.artist, source: csvDownloadSource(row) })));
      }).catch((error) => setCommandError(normalizeRuntimeCommandError(error)))
        .finally(() => setDownloadSearchLoading(false));
    }}
    onDownload={(ids, destination) => {
      if (!destination || !downloadHelperPath) {
        setCommandError("Downloads are unavailable until a library folder and the bundled download helper are configured.");
        return;
      }
      for (const id of ids) {
        const row = csvPreview.find((item) => item.id === id || item.source === id)
          ?? downloadSources.find((item) => item.query === id || item.url === id);
        downloadQueueRef.current.push({
          id,
          source: sourceForDownload(id).replace(/\r|\n/g, " ").trim(),
          destination,
          title: row?.title ?? null,
          artist: row?.artist ?? null,
        });
      }
      setQueuedDownloadCount(downloadQueueRef.current.length);
      startNextDownload();
    }}
    onDownloadQueue={() => {
      if (!downloadHelperPath) { setCommandError("Downloads are unavailable until the bundled download helper is configured."); return; }
      downloadQueueRef.current.push(...stagedDownloads);
      setStagedDownloads([]);
      setQueuedDownloadCount(downloadQueueRef.current.length);
      startNextDownload();
    }}
    onSync={syncPlaylist}
    onCancel={(jobId) => runCommand(bridge?.cancelJob(jobId))}
    onRetry={(jobId) => {
      if (!downloadHelperPath) { setCommandError("Download retry is unavailable until the download helper is configured."); return; }
      runCommand(bridge?.retryDownload(jobId));
    }}
    onCreateDestination={createDownloadDestination}
  />;
  const errorLogViewEl = <ErrorLogView />;
  const playlistSyncReviewViewEl = <PlaylistSyncReviewView entries={playlistSyncReviewSummary.entries} loading={playlistSyncReviewSummary.loading} error={playlistSyncReviewSummary.error} onOpenReview={(playlistId) => {
    const playlist = projection.library?.playlists.find((item) => item.id === playlistId);
    if (playlist) openPlaylistSyncReview(playlist);
    else setPlaylistSyncReviewSummaryRefresh((current) => current + 1);
  }} onRefresh={() => setPlaylistSyncReviewSummaryRefresh((current) => current + 1)} />;
 const settingsScreens: Record<SettingsNavLink, ReactNode> = { home: homeViewEl, downloader: downloaderViewEl, sync_review: playlistSyncReviewViewEl, artwork: <ArtworkManager key={`artwork-${artworkScope}`} snapshot={projection.library} initialScope={artworkScope} bridge={available ? bridge : undefined} onArtworkApplied={storeTrackArtwork} /> };

  const content = views[route] ?? (route === "library" ? (libraryHome || homeBrowseCollection ? homeViewEl : <LibraryView collection={homeCollection} snapshot={projection.library} loading={!projection.library || projection.library.generation <= 0} playback={projection.playback} backendAvailable={available} selectedScope={libraryScope} recentlyDownloadedTracks={downloadedTracks} query={libraryQuery} sort={librarySorts[libraryScope] ?? defaultLibrarySort(libraryScope)} onSortChange={(next) => setLibrarySorts((current) => ({ ...current, [libraryScope]: next }))} onPlay={(item, context, sourcePlaylistId) => runCommand(sourcePlaylistId ? bridge?.playFromPlaylist(item.id, context.map((track) => track.id), sourcePlaylistId) : bridge?.playWithContext(item.id, context.map((track) => track.id)))} onShuffle={() => runCommand(bridge?.toggleShuffle())} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} onContextMenu={openTrackMenu} onScan={() => runCommand(bridge?.scanLibrary())} onProbeDuration={(id) => bridge ? bridge.trackDuration(id) : Promise.resolve(null)} onArtwork={(id) => (bridge ? bridge.trackArtwork(id) : Promise.resolve(null))} onOpenArtwork={(scope) => { setArtworkScope(scope); setSettingsScreen("artwork"); navigate("settings"); }} />) : route === "recent" ? recentViewEl : route === "discover" ? discoverViewEl : route === "downloader" ? downloaderViewEl : route === "settings" ? <SettingsView lastfmKey="" keychainStatus={discoveryKeyConfigured === null ? "loading" : discoveryKeyConfigured ? "available" : "missing"} onSaveLastfmKey={available ? saveLastfmKey : undefined} musicRoot={musicRoot} autoDiscovery={settings.auto_discovery ?? false} transferManifest={transferManifest} transferDestination={transferDestination} transferResult={transferResult} error={commandError} libraryRootPath={viteEnv?.DEV ? undefined : libraryRootPath ?? undefined} onLibraryRootSubmit={viteEnv?.DEV ? undefined : submitLibraryRoot} onChooseMusicRoot={viteEnv?.DEV ? chooseRoot : undefined} onAutoDiscoveryChange={(value) => saveSettings({ ...settings, auto_discovery: value })} onTransferManifestChange={setTransferManifest} onTransferDestinationChange={setTransferDestination} onTransferPreview={() => runCommand(bridge?.transferPreview(transferManifest, transferDestination).then((result) => setTransferResult(JSON.stringify(result))))} onTransferApply={() => runCommand(bridge?.transferApply(transferManifest, transferDestination, {}).then((result) => setTransferResult(JSON.stringify(result))))} onTransferExport={() => { if (!available) { setCommandError("Transfer export is unavailable until the backend is connected."); return; } runCommand(bridge?.transferExport().then((result) => { const manifest = (result as { manifest?: string }).manifest ?? ""; setTransferManifest(manifest); setTransferResult(manifest ? "Exported manifest is ready for review." : "Export returned no manifest."); })); }} activeScreen={settingsScreen} screen={settingsScreen ? settingsScreens[settingsScreen] : null} onNavigate={(target) => { if (target === "home") { setSettingsScreen(null); navigate("library"); setLibraryHome(true); } else setSettingsScreen(target); }} onScreenClosed={() => setSettingsScreen(null)} /> : route === "error_log" ? errorLogViewEl : route === "song_page" ? <SongPageView track={seedTrack} nowPlaying={displayTrack} backendAvailable={available} recommendations={songRecs.items} recommendationState={songRecs.state} recommendationError={songRecs.error} discoverMode={discoverRequested || !!projection.discover?.enabled} onToggleDiscoverMode={toggleDiscoverMode} onRetryRecommendations={() => setSongRecRefresh((value) => value + 1)} onPreviewRecommendation={(item) => { const key = previewKey(item.artist, item.title); runCommand(projection.discover?.enabled ? bridge?.discoverSelect(key) : bridge?.previewTap(songRecs.items.slice(0, 50).map((entry) => ({ artist: entry.artist, title: entry.title })), key)); }} onSaveRecommendation={(item, playlistId) => {
                      if (playlistId) {
                        runCommand(bridge?.saveRecommendation(previewKey(item.artist, item.title), playlistId));
                        return;
                      }
                      const playlists = projection.library?.playlists ?? [];
                      const target = playlists.find((playlist) => playlist.name.toLowerCase() !== "unsorted") ?? playlists[0];
                      if (!target) { setCommandError("Create a playlist before saving recommendations."); return; }
                      runCommand(bridge?.saveRecommendation(previewKey(item.artist, item.title), target.id));
                      }} preview={recommendationPreview} saved={projection.saved} playlists={projection.library?.playlists ?? []} jobs={projection.jobs} tracksById={tracksByIdForQueue} onArtwork={artworkFetcher} onCreatePlaylist={(item) => { if (!bridge || !available) return; createPlaylist([], (name) => runCommand(bridge.saveRecommendation(previewKey(item.artist, item.title), `playlist:${name}`))); }} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} video={video} videoConnection={connectedVideo ? "connected" : "disconnected"} videoRequestError={hasVideoRequestError ? videoRequestError.message : null} onRetryVideo={() => {
                        const retryTrack = displayTrack;
                        if (!retryTrack || !bridge || !available) return;
                        setVideoRequestError(null);
                        void bridge.videoStage(retryTrack.id)
                          .then((snapshot) => {
                            if (snapshot && typeof snapshot === "object" && "status" in snapshot) {
                              backendProjection?.applyVideoSnapshot?.(snapshot as NonNullable<BackendProjection["video"]>);
                            }
                            setCommandError(null);
                          })
                          .catch((error: unknown) => recordVideoRequestError(retryTrack.id, error));
                      }} videoMode={videoMode} onVideoModeChange={changeVideoMode} /> : <div className="luno-route-empty">{routeLabels[route]}</div>);

  const showSidebar = !sidebarCollapsed && route !== "settings";
  const narrowViewport = shellWidth > 0 && shellWidth < 1100;
  const showQueue = queueOpen && route !== "settings" && !narrowViewport;
  const focusMode = route === "settings";
  const selectLibraryScope = useCallback((scope: LibraryScope) => {
    setLibraryScope(scope);
    setLibraryHome(false);
    setRoute("library");
  }, []);
  const openSidebarPlaylistMenu = useCallback((event: ReactMouseEvent<HTMLButtonElement>, playlist: Playlist) => {
    event.preventDefault();
    event.stopPropagation();
    setContextMenu({ x: Math.max(8, Math.min(event.clientX, window.innerWidth - 230)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - 470)), playlist });
  }, []);

  return <div ref={shellRef} className={`luno-shell${queueOpen ? "" : " no-queue"}${sidebarCollapsed ? " no-sidebar" : ""}${focusMode ? " focus-mode" : ""}${videoFullscreenClass(videoFullscreen)}`} style={{ gridTemplateColumns: `${showSidebar ? sidebarWidth : 0}px minmax(0, 1fr) ${showQueue ? queueWidth : 0}px` }} data-route={route} data-backend-state={available ? "available" : "unavailable"} data-video-state={connectedVideo ? "connected" : "unconnected"} data-video-fullscreen={videoFullscreen ? "true" : "false"} data-migration-state={migrationStatus?.status ?? "unknown"}>
    <header className="luno-topbar" data-tauri-drag-region onPointerDown={startWindowDrag}>
      <nav className="luno-page-history" aria-label="Page history">
        <button className="luno-icon-button" type="button" aria-label="Go back" title="Go back" disabled={pageHistory.index === 0} onClick={() => travelPage(-1)}><IconChevronLeft size={18} /></button>
        <button className="luno-icon-button" type="button" aria-label="Go forward" title="Go forward" disabled={pageHistory.index === pageHistory.entries.length - 1} onClick={() => travelPage(1)}><IconChevronRight size={18} /></button>
      </nav>

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
    <LibrarySidebar
      snapshot={projection.library}
      tracksById={tracksByIdForQueue}
      selectedScope={libraryScope}
      available={available}
      recentlyDownloadedTracks={downloadedTracks}
      onArtwork={bridge ? artworkFetcher : undefined}
      onCreatePlaylist={createPlaylist}
      onSelectScope={selectLibraryScope}
      onPlaylistContextMenu={openSidebarPlaylistMenu}
    />
        <main className="luno-main" data-slot="main" aria-disabled={!available}>{(() => { const backendError = projection.errors.length ? projection.errors[projection.errors.length - 1].message : null; const displayCommandError = commandError && (commandError.includes("PoisonError") || commandError.includes("poison recovered") || commandError.includes("command worker failed"))
              ? "A background task hit an internal error and was recovered. If this repeats, restart the app."
              : commandError;
          const videoNotice = !connectedVideo && route === "song_page";
          const syncJob = activePlaylistSync ? projection.jobs[activePlaylistSync.jobId] : null;
          return (!available || displayCommandError || backendError || commandNotice || activePlaylistSync || videoNotice || playlistSyncReviewOffer) && <div className="luno-runtime-status" role="status">
            {!available && <span data-state="backend-unavailable">Backend unavailable; runtime commands are unavailable.</span>}
            {displayCommandError && <span data-state="command-rejected">{displayCommandError} <button type="button" onClick={() => setCommandError(null)} aria-label="Dismiss error">Dismiss</button></span>}
            {backendError && <span data-state="backend-error">{backendError} <button type="button" onClick={() => backendProjection?.clearErrors?.()} aria-label="Dismiss error">Dismiss</button>{backendError.startsWith("playlist sync") && lastFailedPlaylistSync && <button type="button" onClick={openPlaylistSyncErrors}>View sync errors</button>}{backendError.startsWith("YouTube could not access this playlist") && lastFailedPlaylistSync && <button type="button" onClick={() => openPlaylistSyncAuth(lastFailedPlaylistSync.playlist)}>Choose browser and retry</button>}</span>}
            {activePlaylistSync && <span data-state="playlist-sync">{syncJob?.current_item ? `Syncing “${syncJob.current_item}”` : `Checking “${activePlaylistSync.playlist.name}”`} {syncJob?.progress != null && syncJob.current_item ? `· playlist ${Math.round(syncJob.progress * 100)}%` : ""}</span>}
            {commandNotice && <span data-state="command-notice">{commandNotice} <button type="button" onClick={() => setCommandNotice(null)} aria-label="Dismiss notice">Dismiss</button></span>}
            {playlistSyncReviewOffer && <span data-state="playlist-sync-review"><button type="button" onClick={() => openPlaylistSyncReview(playlistSyncReviewOffer.playlist)}>Review {playlistSyncReviewOffer.count} unavailable {playlistSyncReviewOffer.count === 1 ? "song" : "songs"}</button><button type="button" aria-label="Dismiss playlist review notice" onClick={() => setPlaylistSyncReviewOffer(null)}>Dismiss</button></span>}
            {videoNotice && <span data-state="video-unconnected">Video preview unavailable; artwork fallback is active.</span>}
          </div>; })()}<div className={`luno-main-content luno-scroll${route === "song_page" ? " luno-song-page-host" : ""}`}>{content}</div></main>
        <aside className="luno-queue" data-slot="queue" aria-label="Queue">{queue ?? <Queue tracks={panelRecent ? recentTracks : panelQueueTracks} currentIndex={panelRecent ? null : panelQueueIndex} recent={panelRecent} backendAvailable={available} onTabChange={(recent) => setPanelRecent(recent)} nextFromLabel={nextFromLabel} onArtwork={(id) => (bridge ? bridge.trackArtwork(id) : Promise.resolve(null))} onPlay={(item) => playQueueEntry(item, panelRecent ? recentTracks : panelQueueTracks)} onPlayNext={(item) => (tracksByIdForQueue.has(item.id) ? runCommand(bridge?.playNext([item.id])) : setCommandNotice("Only library tracks can be queued."))} onRemove={(index) => runCommand(bridge?.removeFromQueue(index))} onReorder={(from, to) => runCommand(bridge?.reorderQueue(from, to))} onContextMenu={openTrackMenu} />}</aside>
        <footer className="luno-player" data-slot="player" aria-label="Player">{player ?? <PlayerBar track={displayTrack} favoriteTrack={displayedLibraryTrack} playback={projection.playback} artwork={audibleArtwork} artworkLoading={audibleArtworkLoading} volume={settings.volume ?? 1} backendAvailable={available} shuffle={!!projection.playback?.shuffle} repeat={projection.playback?.repeat ?? "off"} onToggleShuffle={() => runCommand(bridge?.toggleShuffle())} onCycleRepeat={() => runCommand(bridge?.setRepeat((projection.playback?.repeat ?? "off") === "off" ? "all" : projection.playback?.repeat === "all" ? "one" : "off"))} onPlayPause={() => {
          if (projection.playback?.status === "playing") { runCommand(bridge?.pause()); return; }
          if (projection.playback?.status === "paused") { runCommand(bridge?.resume()); return; }
          runCommand(stoppedPlaybackCommand(bridge, displayTrack, track, !!projection.discover?.enabled, [...songRecs.items, ...discover.items], queueTracks));
        }} onNext={() => { if (!bridge || !available) return; runCommand(bridge.next()); }} onPrevious={() => { if (!bridge || !available) return; runCommand(bridge.previous()); }} onFavoriteToggle={(item) => runCommand(bridge?.toggleFavorite(item.id))} onSeek={(positionMs) => runCommand(bridge?.seek(positionMs))} onVolume={(value) => runCommand(bridge?.setVolume(value))} onVolumeCommit={(value) => { runCommand(bridge?.setVolume(value)); saveSettings({ ...settings, volume: value }); }} onOpenTrack={() => navigate("song_page")} onTrackContextMenu={openTrackMenu} />}</footer>
    {route !== "settings" && <div className="luno-resize-handle" style={{ left: showSidebar ? 8 + sidebarWidth - 4 : 2 }} role="separator" aria-orientation="vertical" aria-label="Resize library panel width" onPointerDown={startPanelDrag("sidebar")} />}
    {route !== "settings" && !narrowViewport && <div className="luno-resize-handle" style={{ left: showQueue ? Math.max(0, shellWidth - 8 - queueWidth - 4) : Math.max(2, shellWidth - 6) }} role="separator" aria-orientation="vertical" aria-label="Resize queue panel width" onPointerDown={startPanelDrag("queue")} />}
    {contextMenu && <div className="luno-context-menu" role="menu" aria-label={contextMenu.track ? `${contextMenu.track.title} actions` : `${contextMenu.playlist?.name ?? "Playlist"} actions`} style={{ left: contextMenu.x, top: contextMenu.y }} onPointerDown={(event) => event.stopPropagation()}>
      {contextMenu.track ? <>
        <button role="menuitem" type="button" onClick={() => { const item = contextMenu.track!; if (contextMenu.libraryTrack) runCommand(bridge?.playWithContext(item.id, [item.id])); else playQueueEntry(item, queueTracks); setContextMenu(null); }}>Play now</button>
        <button role="menuitem" type="button" disabled={!contextMenu.libraryTrack} onClick={() => { runCommand(bridge?.playNext([contextMenu.track!.id])); setContextMenu(null); }}>Play next</button>
        <button role="menuitem" type="button" disabled={!contextMenu.libraryTrack} onClick={() => { runCommand(bridge?.addToQueue([contextMenu.track!.id])); setContextMenu(null); }}>Add to queue</button>
        <hr />
        <button role="menuitem" type="button" disabled={!contextMenu.libraryTrack} onClick={() => { const item = contextMenu.track!; setPlaylistPickerSelection({ tracks: [item], title: `Add “${item.title}” to playlist` }); setContextMenu(null); }}>Add to playlist…</button>
        <button role="menuitem" type="button" disabled={!contextMenu.libraryTrack} onClick={() => { createPlaylist([contextMenu.track!]); setContextMenu(null); }}>Add to new playlist…</button>
        {contextMenu.libraryTrack && <button role="menuitem" type="button" onClick={() => { runCommand(bridge?.toggleFavorite(contextMenu.track!.id)); setContextMenu(null); }}>{contextMenu.track.favorite ? "Remove from favorites" : "Add to favorites"}</button>}
        {contextMenu.libraryTrack && <>
          <button role="menuitem" type="button" onClick={() => { const item = contextMenu.track!; setEditTrackError(null); setEditTrack(item); setContextMenu(null); }}>Edit song info…</button>
          <hr />
          <button className="is-danger" role="menuitem" type="button" onClick={() => { const item = contextMenu.track!; setContextMenu(null); askConfirm({ title: "Delete this file copy", message: `Remove this copy of “${item.title}” from your library?`, confirmLabel: "Delete copy", danger: true, onConfirm: () => deleteTracksWithReview([item.id], () => `Deleted the file copy of “${item.title}”.`) }); }}>Delete from library…</button>
        </>}
      </> : contextMenu.playlist ? <>
        <button role="menuitem" type="button" disabled={contextMenu.playlist.ordered_track_ids.length === 0} onClick={() => { const playlist = contextMenu.playlist!; const items = playlistTracks(projection.library, playlist); const first = items[0]; if (first) runCommand(bridge?.playFromPlaylist(first.id, items.map((item) => item.id), playlist.id)); setContextMenu(null); }}>Play</button>
        <button role="menuitem" type="button" disabled={contextMenu.playlist.ordered_track_ids.length === 0} onClick={() => { const playlist = contextMenu.playlist!; playShuffled(playlistTracks(projection.library, playlist), playlist.id); setContextMenu(null); }}>Shuffle and play</button>
        <hr />
        <button role="menuitem" type="button" disabled={contextMenu.playlist.ordered_track_ids.length === 0} onClick={() => { const item = contextMenu.playlist!; setPlaylistPickerSelection({ tracks: playlistTracks(projection.library, item), title: `Add “${item.name}” songs to playlist` }); setContextMenu(null); }}>Add all to playlist…</button>
        <button role="menuitem" type="button" disabled={contextMenu.playlist.ordered_track_ids.length === 0} onClick={() => { createPlaylist(playlistTracks(projection.library, contextMenu.playlist!)); setContextMenu(null); }}>Add all to new playlist…</button>
        <button role="menuitem" type="button" onClick={() => { createPlaylist(); setContextMenu(null); }}>Create new playlist…</button>
        <hr />
        <button role="menuitem" type="button" onClick={() => openPlaylistUrlEditor(contextMenu.playlist!)}>{contextMenu.playlist.url ? "Edit YouTube playlist links…" : "Add YouTube playlist links…"}</button>
        <button role="menuitem" type="button" disabled={!contextMenu.playlist.url || activePlaylistSync !== null} title={contextMenu.playlist.url ? undefined : "Add a YouTube playlist link first"} onClick={() => { syncPlaylist(contextMenu.playlist!); setContextMenu(null); }}>Sync playlist</button>
        <button role="menuitem" type="button" disabled={!contextMenu.playlist.url || activePlaylistSync !== null} title={contextMenu.playlist.url ? undefined : "Add a YouTube playlist link first"} onClick={() => { openPlaylistSyncReview(contextMenu.playlist!); setContextMenu(null); }}>Review unavailable songs…</button>
        <button role="menuitem" type="button" onClick={() => { openPlaylistSyncLog(contextMenu.playlist!); setContextMenu(null); }}>Log</button>
        <hr />
        <button className="is-danger" role="menuitem" type="button" disabled={contextMenu.playlist.ordered_track_ids.length === 0} onClick={() => { const item = contextMenu.playlist!; setContextMenu(null); askConfirm({ title: "Empty playlist", message: `Delete every song file in “${item.name}” but keep the playlist?`, confirmLabel: "Empty playlist", danger: true, onConfirm: () => deleteTracksWithReview(item.ordered_track_ids, (result) => `Removed ${result.deleted} ${result.deleted === 1 ? "song" : "songs"} from “${item.name}”.`) }); }}>Empty playlist…</button>
        <button className="is-danger" role="menuitem" type="button" onClick={() => { const item = contextMenu.playlist!; setContextMenu(null); askConfirm({ title: "Delete playlist", message: `Delete the playlist “${item.name}” and all files inside it?`, confirmLabel: "Delete playlist", danger: true, onConfirm: () => deletePlaylistWithReview(item) }); }}>Delete playlist…</button>
      </> : null}
    </div>}
    {playlistPickerSelection && <PlaylistPicker title={playlistPickerSelection.title} playlists={(projection.library?.playlists ?? []).filter((playlist) => playlist.name.toLowerCase() !== "unsorted")} selectedTracks={playlistPickerSelection.tracks} tracksById={tracksByIdForQueue} onArtwork={artworkFetcher} onPick={(playlist) => { const items = playlistPickerSelection.tracks; setPlaylistPickerSelection(null); addTracksToPlaylist(items, playlist); }} onCreateNew={() => { const items = playlistPickerSelection.tracks; setPlaylistPickerSelection(null); createPlaylist(items); }} onDismiss={() => setPlaylistPickerSelection(null)} />}
    {editTrack && <EditTrackDialog track={editTrack} busy={editTrackBusy} error={editTrackError} onClose={() => { if (!editTrackBusy) { setEditTrack(null); setEditTrackError(null); } }} onSubmit={(value) => { if (!bridge || !available) return; setEditTrackBusy(true); setEditTrackError(null); void bridge.editTrackMetadata(editTrack.id, value.artist, value.title).then(() => { setEditTrackBusy(false); setEditTrack(null); setCommandNotice(`Updated “${[value.artist, value.title].filter(Boolean).join(" — ")}”.`); }).catch((error: unknown) => { setEditTrackBusy(false); setEditTrackError(normalizeRuntimeCommandError(error)); }); }} />}
    {confirmDialog && <ConfirmDialog title={confirmDialog.title} message={confirmDialog.message} confirmLabel={confirmDialog.confirmLabel} danger={confirmDialog.danger} busy={confirmBusy} error={confirmError} onClose={() => { if (!confirmBusy) { setConfirmDialog(null); setConfirmError(null); } }} onConfirm={() => { const request = confirmDialog; setConfirmBusy(true); setConfirmError(null); void Promise.resolve().then(() => request.onConfirm()).then(() => { setConfirmBusy(false); setConfirmDialog(null); }).catch((error: unknown) => { setConfirmBusy(false); setConfirmError(normalizeRuntimeCommandError(error)); }); }} />}
    {finalCopyDelete && <FinalCopyDeleteDialog files={finalCopyDelete.files} verificationIncomplete={finalCopyDelete.verificationIncomplete} busy={finalCopyDeleteBusy} error={finalCopyDeleteError} onClose={() => { if (!finalCopyDeleteBusy) { setFinalCopyDelete(null); setFinalCopyDeleteError(null); } }} onConfirm={submitFinalCopyDelete} />}
    {playlistCreateRequest && <PlaylistDialog action="create" busy={playlistCreateBusy} error={playlistCreateError} onClose={() => { if (!playlistCreateBusy) { setPlaylistCreateRequest(null); setPlaylistCreateError(null); } }} onSubmit={(value) => submitCreatePlaylist(value.name ?? "")} />}
    {playlistUrlLoading && <PlaylistUrlLoadingDialog playlistName={playlistUrlLoading.name} onClose={() => { playlistUrlLoadSequence.current += 1; setPlaylistUrlLoading(null); }} />}
    {playlistUrlRequest && <PlaylistUrlDialog key={playlistUrlRequest.playlist.id} playlistName={playlistUrlRequest.playlist.name} initialUrls={playlistUrlRequest.urls} busy={playlistUrlBusy} error={playlistUrlError} onClose={() => { if (!playlistUrlBusy) { setPlaylistUrlRequest(null); setPlaylistUrlError(null); } }} onSubmit={submitPlaylistUrl} />}
    {playlistSyncAuthRequest && <PlaylistSyncAuthDialog playlistName={playlistSyncAuthRequest.name} initialBrowser={settings.youtube_cookies_browser ?? "none"} busy={playlistSyncAuthBusy} error={playlistSyncAuthError} onClose={() => { if (!playlistSyncAuthBusy) { setPlaylistSyncAuthRequest(null); setPlaylistSyncAuthError(null); } }} onSubmit={submitPlaylistSyncAuth} />}
    {playlistSyncLog && <PlaylistSyncLogDialog playlistName={playlistSyncLog.name} entries={playlistSyncLog.entries} loading={playlistSyncLog.loading} error={playlistSyncLog.error} onClose={() => setPlaylistSyncLog(null)} />}
    {playlistSyncErrors && <PlaylistSyncErrorsDialog playlistName={playlistSyncErrors.name} entries={playlistSyncErrors.entries} loading={playlistSyncErrors.loading} error={playlistSyncErrors.error} onClose={() => setPlaylistSyncErrors(null)} />}
    {playlistSyncReview && <PlaylistSyncReviewDialog playlistName={playlistSyncReview.playlist.name} entries={playlistSyncReview.entries} loading={playlistSyncReview.loading} error={playlistSyncReview.error} candidateLookups={playlistSyncReview.candidateLookups} selectingVideoId={playlistSyncReview.selectingVideoId} downloadingVideoId={playlistSyncReviewDownloadingVideoId} syncing={!!activePlaylistSync} onFindCandidates={findPlaylistSyncCandidates} onChooseCandidate={downloadPlaylistSyncCandidate} onRetryOriginal={retryOriginalPlaylistSource} onSyncSelected={syncSelectedPlaylistSources} onResolveWithLocalMatch={(videoId, match) => persistPlaylistSyncResolution(videoId, match)} onDismissIssue={(videoId) => persistPlaylistSyncResolution(videoId, null)} onClose={() => { if (!playlistSyncReview.selectingVideoId && !playlistSyncReviewDownloadingVideoId && !activePlaylistSync) setPlaylistSyncReview(null); }} />}
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

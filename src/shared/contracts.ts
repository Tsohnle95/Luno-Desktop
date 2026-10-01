/** Versioned Rust/React IPC contract. Do not use paths as track identity. */
export const CONTRACT_VERSION = 1 as const;
export type ContractVersion = typeof CONTRACT_VERSION;
export type TrackId = string;
export type PlaylistId = string;
export type JobId = string;
export type SessionId = string;
export type Generation = number;

export interface ProviderId { provider: string; id: string }
export interface Track { id: TrackId; path: string | null; title: string; artist: string | null; album: string | null; duration_ms: number | null; artwork_url: string | null; favorite: boolean; provider: ProviderId | null; /** Portable first-added time (ms since epoch); drives "Recently added". */ added_ms?: number | null }
export interface Playlist { id: PlaylistId; name: string; path: string | null; url: string | null; ordered_track_ids: TrackId[] }
export interface LibrarySnapshot { version: ContractVersion; generation: Generation; tracks: Track[]; playlists: Playlist[] }
export type PlaybackStatus = "stopped" | "playing" | "paused" | "failed";
export type RepeatMode = "off" | "one" | "all";
export interface PlaybackSnapshot { queue: TrackId[]; current_index: number | null; audible_track_id?: TrackId | null; shuffle: boolean; repeat: RepeatMode; position_ms: number; duration_ms: number | null; status: PlaybackStatus; generation: Generation }
export type VideoStatus = "not_checked" | "loading" | "ready" | "playing" | "paused" | "ended" | "unavailable" | "error";
export interface VideoState { track_id: TrackId | null; source_url: string | null; staged_path: string | null; status: VideoStatus; position_ms: number; /** 0-100 while status is "loading"; null otherwise. */ download_progress?: number | null; /** Content revision of the staged file: 1 = quick-tier first pass, 2 = background full-quality upgrade completed. */ revision?: number | null; fullscreen: boolean; generation: Generation }
export type DownloadKind = "single" | "csv" | "sync" | "url_queue";
export type JobState = "queued" | "running" | "cancelling" | "succeeded" | "failed" | "cancelled";
export interface DownloadJob { id: JobId; kind: DownloadKind; state: JobState; track_id: TrackId | null; progress: number | null; phase?: "finalizing" | null; current_item?: string | null; generation: Generation }
export type ErrorCode = "invalid_request" | "unsupported_version" | "not_found" | "busy" | "cancelled" | "unavailable" | "internal" | "shutting_down";
export interface ContractError { code: ErrorCode; message: string; retryable: boolean }
export const YTDLP_COOKIE_BROWSERS = ["brave", "chrome", "chromium", "edge", "firefox", "opera", "safari", "vivaldi", "whale"] as const;
export type YtDlpCookieBrowser = (typeof YTDLP_COOKIE_BROWSERS)[number] | "none";
export interface SettingsInput { music_root: string | null; auto_discovery: boolean | null; volume: number | null; sleep_timer: unknown | null; global_keys: boolean | null; favorite_songs: string[]; video_mode?: boolean | null; youtube_cookies_browser?: YtDlpCookieBrowser | null }
export interface SearchResult { provider: string; query: string; title: string; artist: string | null; duration: string | null; thumbnail_url: string | null; url: string | null }
export interface PlaylistSyncLocalMatch { track_id: TrackId; title: string; artist: string | null; duration_ms: number | null; playlists: string[] }
export interface PlaylistSyncIssue { video_id: string; title: string; channel: string | null; duration_secs: number | null; reason: string; attempted_source_id: string | null; selected_source_id: string | null; retry_pending: boolean; updated_at_ms: number; local_matches: PlaylistSyncLocalMatch[] }
export interface PlaylistSyncCandidate { video_id: string; title: string; channel: string | null; duration: string | null; thumbnail_url: string | null; url: string }
export interface PlaylistSyncSelection { video_id: string; source_video_id: string; url: string }
export interface PlaylistSyncReviewSummary { playlist_id: PlaylistId; name: string; url: string | null; unresolved_count: number }
export interface TrackPlayCount { track_id: TrackId; plays: number }
export interface PlaylistPlayCount { playlist_id: PlaylistId; plays: number }
export interface PlayStatisticsSnapshot { generation: Generation; tracks: TrackPlayCount[]; playlists: PlaylistPlayCount[]; legacy_track_counts_imported: boolean }
export type ArtworkSearchSource = "musicbrainz" | "youtube";
export interface ArtworkCandidate { candidate_id: string; source: ArtworkSearchSource; release: string; artist: string; year: string | null; score: number; exact_match: boolean; artwork_data_url: string }
export interface SafeDeleteResult { deleted: number; needs_confirmation: boolean; verification_incomplete: boolean; final_copy_review_id: string | null; final_copies: string[]; generation: number | null }

export type Command =
  | { type: "get_library" } | { type: "scan_library" } | { type: "play"; payload: { track_id: TrackId } }
  | { type: "play_with_context"; payload: { track_id: TrackId; context: TrackId[] } }
  | { type: "play_from_playlist"; payload: { track_id: TrackId; context: TrackId[]; playlist_id: PlaylistId } }
  | { type: "resume" }
  | { type: "next" } | { type: "previous" }
  | { type: "play_next"; payload: { track_ids: TrackId[] } } | { type: "add_to_queue"; payload: { track_ids: TrackId[] } }
  | { type: "remove_from_queue"; payload: { index: number } } | { type: "reorder_queue"; payload: { from: number; to: number } }
  | { type: "clear_queue" } | { type: "toggle_shuffle" } | { type: "set_repeat"; payload: { repeat: "off" | "one" | "all" } }
  | { type: "load_config"; payload: { home: string } } | { type: "register_root"; payload: { root: string } }
  | { type: "promote_audio_output"; payload: { temporary: string; destination: string } }
  | { type: "sync_markers"; payload: { folder: string } } | { type: "toggle_favorite"; payload: { track_id: TrackId } }
  | { type: "search_downloader"; payload: { query: string; try_soundcloud: boolean; limit: number } }
  | { type: "discover"; payload: { artist: string; title: string; limit: number } }
  | { type: "preview_tap"; payload: { seeds: PreviewSeed[]; key: string } }
  | { type: "preview_prefetch"; payload: { seeds: PreviewSeed[] } }
  | { type: "preview_next" }
  | { type: "preview_previous" }
  | { type: "preview_cancel" }
  | { type: "discover_enable"; payload: { seeds: PreviewSeed[] } }
  | { type: "discover_disable" }
  | { type: "discover_advance" }
  | { type: "discover_select"; payload: { key: TrackId } }
  | { type: "save"; payload: { key: TrackId; playlist_id: PlaylistId } }
  | { type: "validate_direct_url"; payload: { url: string; destination: string } }
  | { type: "preview_csv"; payload: { csv: string } }
  | { type: "queue_urls"; payload: { urls: string[] } }
  | { type: "select_library_root"; payload: { root: string } }
  | { type: "create_playlist"; payload: { name: string } }
  | { type: "set_playlist_url"; payload: { playlist_id: PlaylistId; url: string | null } }
  | { type: "set_playlist_urls"; payload: { playlist_id: PlaylistId; urls: string[] } }
  | { type: "sync_playlist"; payload: { playlist_id: PlaylistId } }
  | { type: "playlist_sync_sources"; payload: { playlist_id: PlaylistId } }
  | { type: "playlist_sync_log"; payload: { playlist_id: PlaylistId } }
  | { type: "playlist_sync_review_summary" }
  | { type: "playlist_sync_issues"; payload: { playlist_id: PlaylistId } }
  | { type: "playlist_sync_candidates"; payload: { playlist_id: PlaylistId; video_id: string } }
  | { type: "playlist_sync_select_source"; payload: { playlist_id: PlaylistId; video_id: string; source_video_id: string } }
  | { type: "playlist_sync_resolve_issue"; payload: { playlist_id: PlaylistId; video_id: string; replacement_track_id: TrackId | null } }
  | { type: "delete_playlist"; payload: { playlist_id: PlaylistId; final_copy_review_id?: string | null } }
  | { type: "add_tracks_to_playlist"; payload: { track_ids: TrackId[]; playlist_id: PlaylistId } }
  | { type: "delete_tracks"; payload: { track_ids: TrackId[]; final_copy_review_id?: string | null } }
  | { type: "edit_track_metadata"; payload: { track_id: TrackId; artist: string; title: string } }
  | { type: "start_download"; payload: { job_id: JobId; destination: string; helper: string; source: string } }
  | { type: "retry_download"; payload: { job_id: JobId } }
  | { type: "bundle_helper_path" }
  | { type: "load_settings"; payload: { home: string } } | { type: "resolve_runtime_home" } | { type: "save_settings"; payload: { home: string; settings: SettingsInput } }
  | { type: "transfer_preview"; payload: { manifest: string; destination: string } }
  | { type: "transfer_export" }
  | { type: "transfer_apply"; payload: { manifest: string; destination: string; approvals: Record<string, string> } }
  | { type: "record_recent"; payload: { track_id: TrackId } } | { type: "get_recent" } | { type: "clear_recent" }
  | { type: "import_legacy_play_counts"; payload: { track_plays: Record<string, number> } }
  | { type: "pause" } | { type: "seek"; payload: { position_ms: number } } | { type: "set_volume"; payload: { volume: number } }
  | { type: "video_source"; payload: { track_id: TrackId } } | { type: "video_stage"; payload: { track_id: TrackId } }
  | { type: "set_video_mode"; payload: { enabled: boolean } }
  | { type: "video_play" } | { type: "video_pause" } | { type: "video_seek"; payload: { position_ms: number } }
  | { type: "video_resize"; payload: { width: number; height: number } } | { type: "video_fullscreen"; payload: { fullscreen: boolean } }
  | { type: "cancel_job"; payload: { job_id: JobId } }
  | { type: "shutdown"; payload: { reason: ShutdownReason } };
export interface CommandEnvelope { version: ContractVersion; request_id: string; command: Command }
export type RootStatus = "scanned" | "missing" | "inaccessible";
export type Event =
  | { type: "library_snapshot"; payload: { snapshot: LibrarySnapshot } }
  | { type: "library_root_status"; payload: { root_index: number; status: RootStatus } }
  | { type: "track_favorite_changed"; payload: { track_id: TrackId; favorite: boolean } }
  | { type: "playback_snapshot"; payload: { snapshot: PlaybackSnapshot } }
  | { type: "preview_snapshot"; payload: { snapshot: PreviewSnapshot } }
  | { type: "discover_snapshot"; payload: { snapshot: DiscoverSnapshot } }
  | { type: "save_state"; payload: { state: SaveState } }
  | { type: "video_state"; payload: { state: VideoState } }
  | { type: "track_started"; payload: { track_id: TrackId } }
  | { type: "play_statistics_snapshot"; payload: { snapshot: PlayStatisticsSnapshot } }
  | { type: "queue_changed"; payload: { queue: TrackId[] } }
  | { type: "job_updated"; payload: { job: DownloadJob } }
  | { type: "error"; payload: { error: ContractError } }
  | { type: "shutdown"; payload: { envelope: ShutdownEnvelope } };
export interface EventEnvelope { version: ContractVersion; event_id: string; generation: Generation; job_id: JobId | null; session_id: SessionId | null; event: Event }
export type ShutdownState = "requested" | "draining" | "complete" | "timed_out" | "failed";
export type ShutdownReason = "user_requested" | "system" | "fatal_error";
export interface ShutdownEnvelope { version: ContractVersion; state: ShutdownState; reason: ShutdownReason }

const object = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const exact = (v: unknown, keys: string[]): v is Record<string, unknown> => object(v) && Object.keys(v).length === keys.length && Object.keys(v).every(k => keys.includes(k));
const nonEmptyId = (v: unknown): v is string => typeof v === "string" && v.trim().length > 0;
const safeInteger = (v: unknown): v is number => Number.isSafeInteger(v);
export interface Recommendation {
  artist: string;
  title: string;
  match_score: number;
  artwork_url: string | null;
}

export interface PreviewSeed {
  artist: string;
  title: string;
}

export type SaveStateKind = "saving" | "queued" | "saved" | "failed";
export interface SaveState {
  key: TrackId;
  playlist_id: PlaylistId;
  state: SaveStateKind;
  already_saved: boolean;
  path: string | null;
  job_id: JobId | null;
  error: string | null;
}

export interface PreviewSnapshot {
  generation: number;
  active: boolean;
  current: string | null;
  window: string[];
  prepared: string[];
  unavailable: string[];
  cancelled: boolean;
  progress?: Array<{ key: string; percent: number }>;
}

export interface DiscoverSnapshot {
  generation: number;
  enabled: boolean;
  window: string[];
  ready: number;
  queued: number;
  /** Stable recommendation keys whose temporary audio is ready. */
  prepared?: string[];
  /** Stable recommendation keys whose latest preparation attempt failed. */
  unavailable?: string[];
}

/** Mirrors the backend `safe_key`: ASCII alphanumerics, `-` and `_` survive;
 * everything else collapses to `_`.  This is the session key shared between
 * the discover rows, the preview session, and the asset store. */
export const previewKey = (artist: string, title: string): string =>
  `${artist.trim()}-${title.trim()}`.replace(/[^a-zA-Z0-9\-_]/gu, "_");

export function isContractVersion(v: unknown): v is ContractVersion { return v === CONTRACT_VERSION; }
function isCommand(v: unknown): v is Command {
  if (!object(v) || typeof v.type !== "string") return false;
   if (["get_library", "scan_library", "pause", "resume", "get_recent", "clear_recent", "transfer_export", "resolve_runtime_home", "bundle_helper_path", "playlist_sync_review_summary"].includes(v.type)) return Object.keys(v).length === 1;
   if (v.type === "load_config") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.home === "string" && v.payload.home.trim().length > 0 && Object.keys(v.payload).length === 1;
    if (v.type === "register_root") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.root === "string" && v.payload.root.trim().length > 0 && Object.keys(v.payload).length === 1;
    if (v.type === "promote_audio_output") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.temporary === "string" && v.payload.temporary.trim().length > 0 && typeof v.payload.destination === "string" && v.payload.destination.trim().length > 0 && Object.keys(v.payload).length === 2;
    if (v.type === "sync_markers") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.folder === "string" && v.payload.folder.trim().length > 0 && Object.keys(v.payload).length === 1;
    if (v.type === "search_downloader") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.query === "string" && v.payload.query.trim().length > 0 && typeof v.payload.try_soundcloud === "boolean" && typeof v.payload.limit === "number" && Number.isInteger(v.payload.limit) && v.payload.limit >= 1 && v.payload.limit <= 50 && Object.keys(v.payload).length === 3;
    if (v.type === "discover") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.artist === "string" && v.payload.artist.trim().length > 0 && typeof v.payload.title === "string" && v.payload.title.trim().length > 0 && typeof v.payload.limit === "number" && Number.isInteger(v.payload.limit) && v.payload.limit >= 1 && v.payload.limit <= 20 && Object.keys(v.payload).length === 3;
    if (v.type === "preview_tap") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.key) && Array.isArray(v.payload.seeds) && v.payload.seeds.length >= 1 && v.payload.seeds.length <= 50 && v.payload.seeds.every((seed) => object(seed) && exact(seed, ["artist", "title"]) && nonEmptyId(seed.artist) && nonEmptyId(seed.title)) && Object.keys(v.payload).length === 2;
    if (v.type === "preview_prefetch") return exact(v, ["type", "payload"]) && object(v.payload) && Array.isArray(v.payload.seeds) && v.payload.seeds.length >= 1 && v.payload.seeds.length <= 50 && v.payload.seeds.every((seed) => object(seed) && exact(seed, ["artist", "title"]) && nonEmptyId(seed.artist) && nonEmptyId(seed.title)) && Object.keys(v.payload).length === 1;
    if (["preview_next", "preview_previous", "preview_cancel", "discover_disable", "discover_advance"].includes(v.type)) return Object.keys(v).length === 1;
    if (v.type === "discover_select") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.key) && Object.keys(v.payload).length === 1;
    if (v.type === "discover_enable") return exact(v, ["type", "payload"]) && object(v.payload) && Array.isArray(v.payload.seeds) && v.payload.seeds.length <= 50 && v.payload.seeds.every((seed) => object(seed) && exact(seed, ["artist", "title"]) && typeof seed.artist === "string" && typeof seed.title === "string") && Object.keys(v.payload).length === 1;
    if (v.type === "save") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.key) && nonEmptyId(v.payload.playlist_id) && Object.keys(v.payload).length === 2;
    if (v.type === "validate_direct_url") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.url === "string" && typeof v.payload.destination === "string" && v.payload.url.trim().length > 0 && v.payload.destination.trim().length > 0 && Object.keys(v.payload).length === 2;
    if (v.type === "preview_csv") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.csv === "string" && v.payload.csv.length <= 10_485_760 && Object.keys(v.payload).length === 1;
    if (v.type === "queue_urls") return exact(v, ["type", "payload"]) && object(v.payload) && Array.isArray(v.payload.urls) && v.payload.urls.every((url) => typeof url === "string" && url.trim().length > 0) && Object.keys(v.payload).length === 1;
    if (v.type === "select_library_root") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.root === "string" && v.payload.root.trim().length > 0 && Object.keys(v.payload).length === 1;
    if (v.type === "create_playlist") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.name) && Object.keys(v.payload).length === 1;
    if (v.type === "set_playlist_url") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.playlist_id) && (v.payload.url === null || typeof v.payload.url === "string") && Object.keys(v.payload).length === 2;
    if (v.type === "set_playlist_urls") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.playlist_id) && Array.isArray(v.payload.urls) && v.payload.urls.length <= 32 && v.payload.urls.every((url) => nonEmptyId(url)) && Object.keys(v.payload).length === 2;
    if (v.type === "sync_playlist") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.playlist_id) && Object.keys(v.payload).length === 1;
    if (v.type === "playlist_sync_sources") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.playlist_id) && Object.keys(v.payload).length === 1;
    if (v.type === "playlist_sync_log") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.playlist_id) && Object.keys(v.payload).length === 1;
    if (v.type === "playlist_sync_issues") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.playlist_id) && Object.keys(v.payload).length === 1;
    if (v.type === "playlist_sync_candidates") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.playlist_id) && nonEmptyId(v.payload.video_id) && Object.keys(v.payload).length === 2;
    if (v.type === "playlist_sync_select_source") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.playlist_id) && nonEmptyId(v.payload.video_id) && nonEmptyId(v.payload.source_video_id) && Object.keys(v.payload).length === 3;
    if (v.type === "playlist_sync_resolve_issue") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.playlist_id) && nonEmptyId(v.payload.video_id) && (v.payload.replacement_track_id === null || nonEmptyId(v.payload.replacement_track_id)) && Object.keys(v.payload).length === 3;
    if (v.type === "delete_playlist") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.playlist_id) && (exact(v.payload, ["playlist_id"]) || exact(v.payload, ["playlist_id", "final_copy_review_id"])) && (v.payload.final_copy_review_id === undefined || v.payload.final_copy_review_id === null || nonEmptyId(v.payload.final_copy_review_id));
    if (v.type === "add_tracks_to_playlist") return exact(v, ["type", "payload"]) && object(v.payload) && Array.isArray(v.payload.track_ids) && v.payload.track_ids.length > 0 && v.payload.track_ids.every(nonEmptyId) && nonEmptyId(v.payload.playlist_id) && Object.keys(v.payload).length === 2;
    if (v.type === "delete_tracks") return exact(v, ["type", "payload"]) && object(v.payload) && (exact(v.payload, ["track_ids"]) || exact(v.payload, ["track_ids", "final_copy_review_id"])) && Array.isArray(v.payload.track_ids) && v.payload.track_ids.length > 0 && v.payload.track_ids.every(nonEmptyId) && (v.payload.final_copy_review_id === undefined || v.payload.final_copy_review_id === null || nonEmptyId(v.payload.final_copy_review_id));
    if (v.type === "edit_track_metadata") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.track_id) && typeof v.payload.artist === "string" && typeof v.payload.title === "string" && v.payload.title.trim().length > 0 && Object.keys(v.payload).length === 3;
    if (v.type === "start_download") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.job_id) && typeof v.payload.destination === "string" && v.payload.destination.trim().length > 0 && typeof v.payload.helper === "string" && v.payload.helper.trim().length > 0 && typeof v.payload.source === "string" && Object.keys(v.payload).length === 4;
    if (v.type === "retry_download") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.job_id) && Object.keys(v.payload).length === 1;
    if (v.type === "load_settings") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.home === "string" && v.payload.home.trim().length > 0 && Object.keys(v.payload).length === 1;
    if (v.type === "save_settings") {
      const browser = object(v.payload) && object(v.payload.settings) ? v.payload.settings.youtube_cookies_browser : undefined;
      const validBrowser = browser === undefined || browser === null || browser === "none" || (typeof browser === "string" && (YTDLP_COOKIE_BROWSERS as readonly string[]).includes(browser));
      return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.home === "string" && v.payload.home.trim().length > 0 && object(v.payload.settings) && Object.keys(v.payload.settings).every((key) => ["music_root", "auto_discovery", "volume", "sleep_timer", "global_keys", "favorite_songs", "video_mode", "youtube_cookies_browser"].includes(key)) && validBrowser && Array.isArray(v.payload.settings.favorite_songs) && v.payload.settings.favorite_songs.every((item) => typeof item === "string") && Object.keys(v.payload.settings).length >= 6 && Object.keys(v.payload).length === 2;
    }
   if (v.type === "set_video_mode") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.enabled === "boolean" && Object.keys(v.payload).length === 1;
    if (["transfer_preview", "transfer_apply"].includes(v.type)) return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.manifest === "string" && v.payload.manifest.length <= 10_485_760 && typeof v.payload.destination === "string" && v.payload.destination.trim().length > 0 && (v.type === "transfer_preview" || (object(v.payload.approvals) && Object.keys(v.payload).length === 3)) && Object.keys(v.payload).length === (v.type === "transfer_preview" ? 2 : 3);
   if (["toggle_favorite", "record_recent"].includes(v.type)) return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.track_id) && Object.keys(v.payload).length === 1;
   if (v.type === "import_legacy_play_counts") return exact(v, ["type", "payload"]) && object(v.payload) && exact(v.payload, ["track_plays"]) && object(v.payload.track_plays) && Object.keys(v.payload.track_plays).length <= 100_000 && Object.entries(v.payload.track_plays).every(([id, plays]) => nonEmptyId(id) && safeInteger(plays) && plays > 0) && Object.keys(v.payload).length === 1;
   if (v.type === "play") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.track_id) && Object.keys(v.payload).length === 1;
   if (v.type === "play_with_context") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.track_id) && Array.isArray(v.payload.context) && v.payload.context.every(nonEmptyId) && Object.keys(v.payload).length === 2;
   if (v.type === "play_from_playlist") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.track_id) && Array.isArray(v.payload.context) && v.payload.context.every(nonEmptyId) && nonEmptyId(v.payload.playlist_id) && Object.keys(v.payload).length === 3;
   if (["next", "previous", "clear_queue", "toggle_shuffle"].includes(v.type)) return Object.keys(v).length === 1;
   if (["play_next", "add_to_queue"].includes(v.type)) return exact(v, ["type", "payload"]) && object(v.payload) && Array.isArray(v.payload.track_ids) && v.payload.track_ids.every(nonEmptyId) && Object.keys(v.payload).length === 1;
   if (["remove_from_queue"].includes(v.type)) return exact(v, ["type", "payload"]) && object(v.payload) && safeInteger(v.payload.index) && v.payload.index >= 0 && Object.keys(v.payload).length === 1;
   if (v.type === "reorder_queue") return exact(v, ["type", "payload"]) && object(v.payload) && safeInteger(v.payload.from) && v.payload.from >= 0 && safeInteger(v.payload.to) && v.payload.to >= 0 && Object.keys(v.payload).length === 2;
   if (v.type === "set_repeat") return exact(v, ["type", "payload"]) && object(v.payload) && ["off", "one", "all"].includes(String(v.payload.repeat)) && Object.keys(v.payload).length === 1;
   if (["video_play", "video_pause"].includes(v.type)) return Object.keys(v).length === 1;
   if (["seek", "video_seek"].includes(v.type)) return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.position_ms === "number" && Number.isSafeInteger(v.payload.position_ms) && v.payload.position_ms >= 0 && Object.keys(v.payload).length === 1;
   if (v.type === "set_volume") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.volume === "number" && Number.isFinite(v.payload.volume) && v.payload.volume >= 0 && v.payload.volume <= 1 && Object.keys(v.payload).length === 1;
   if (["video_source", "video_stage"].includes(v.type)) return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.track_id) && Object.keys(v.payload).length === 1;
    if (v.type === "video_resize") return exact(v, ["type", "payload"]) && object(v.payload) && safeInteger(v.payload.width) && v.payload.width > 0 && safeInteger(v.payload.height) && v.payload.height > 0 && Object.keys(v.payload).length === 2;
   if (v.type === "video_fullscreen") return exact(v, ["type", "payload"]) && object(v.payload) && typeof v.payload.fullscreen === "boolean" && Object.keys(v.payload).length === 1;
   if (v.type === "cancel_job") return exact(v, ["type", "payload"]) && object(v.payload) && nonEmptyId(v.payload.job_id) && Object.keys(v.payload).length === 1;
  if (v.type === "shutdown") return exact(v, ["type", "payload"]) && object(v.payload) && ["user_requested", "system", "fatal_error"].includes(String(v.payload.reason)) && Object.keys(v.payload).length === 1;
  return false;
}
function isEvent(v: unknown): v is Event {
  if (!object(v) || typeof v.type !== "string" || !exact(v, ["type", "payload"]) || !object(v.payload)) return false;
   const id = nonEmptyId;
  const optionalString = (value: unknown): value is string | null => value === null || typeof value === "string";
  const generation = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
  const nonNegativeInt = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
  const provider = (value: unknown): value is ProviderId => exact(value, ["provider", "id"]) && id(value.provider) && id(value.id);
  // `added_ms` is an OPTIONAL trailing field: the backend omits it when both
  // the portable metadata and file mtime are unknown, so bound the extra only.
  const track = (value: unknown): value is Track => object(value)
    && (["id", "path", "title", "artist", "album", "duration_ms", "artwork_url", "favorite", "provider"] as const).every((key) => key in value)
    && Object.keys(value).every((key) => ["id", "path", "title", "artist", "album", "duration_ms", "artwork_url", "favorite", "provider", "added_ms"].includes(key))
    && id(value.id) && optionalString(value.path) && typeof value.title === "string" && optionalString(value.artist) && optionalString(value.album) && (value.duration_ms === null || nonNegativeInt(value.duration_ms)) && optionalString(value.artwork_url) && typeof value.favorite === "boolean" && (value.provider === null || provider(value.provider))
    && (value.added_ms === undefined || value.added_ms === null || nonNegativeInt(value.added_ms));
  const playlist = (value: unknown): value is Playlist => exact(value, ["id", "name", "path", "url", "ordered_track_ids"]) && id(value.id) && typeof value.name === "string" && optionalString(value.path) && optionalString(value.url) && Array.isArray(value.ordered_track_ids) && value.ordered_track_ids.every(id);
  const library = (value: unknown): value is LibrarySnapshot => exact(value, ["version", "generation", "tracks", "playlists"]) && value.version === CONTRACT_VERSION && generation(value.generation) && Array.isArray(value.tracks) && value.tracks.every(track) && Array.isArray(value.playlists) && value.playlists.every(playlist);
    const playback = (value: unknown): value is PlaybackSnapshot => object(value) && ["queue", "current_index", "shuffle", "repeat", "position_ms", "duration_ms", "status", "generation"].every((key) => key in value) && Object.keys(value).every((key) => ["queue", "current_index", "audible_track_id", "shuffle", "repeat", "position_ms", "duration_ms", "status", "generation"].includes(key)) && Array.isArray(value.queue) && value.queue.every(id) && (value.current_index === null || nonNegativeInt(value.current_index)) && (value.audible_track_id === undefined || value.audible_track_id === null || id(value.audible_track_id)) && typeof value.shuffle === "boolean" && ["off", "one", "all"].includes(String(value.repeat)) && nonNegativeInt(value.position_ms) && (value.duration_ms === null || nonNegativeInt(value.duration_ms)) && ["stopped", "playing", "paused", "failed"].includes(String(value.status)) && generation(value.generation);
   // VideoState allows OPTIONAL trailing fields: the backend serializes
   // `download_progress` (and may add `revision`) on every video_state EVENT.
   // Requiring exactly the seven core keys made the validator REJECT every
   // pushed video event, silently discarding the stream and leaving the UI
   // dependent on command responses + polling.
   const video = (value: unknown): value is VideoState => object(value)
     && (["track_id", "source_url", "staged_path", "status", "position_ms", "fullscreen", "generation"] as const).every((k) => k in value)
     && Object.keys(value).every((k) => ["track_id", "source_url", "staged_path", "status", "position_ms", "download_progress", "revision", "fullscreen", "generation"].includes(k))
     && (value.track_id === null || id(value.track_id)) && optionalString(value.source_url) && optionalString(value.staged_path) && ["not_checked", "loading", "ready", "playing", "paused", "ended", "unavailable", "error"].includes(String(value.status)) && nonNegativeInt(value.position_ms) && typeof value.fullscreen === "boolean" && generation(value.generation)
     && (value.download_progress === undefined || value.download_progress === null || (nonNegativeInt(value.download_progress) && (value.download_progress as number) <= 100))
     && (value.revision === undefined || value.revision === null || nonNegativeInt(value.revision));
  const progress = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 1;
  const job = (value: unknown): value is DownloadJob => object(value)
    && ["id", "kind", "state", "track_id", "progress", "generation"].every((key) => key in value)
    && Object.keys(value).every((key) => ["id", "kind", "state", "track_id", "progress", "phase", "current_item", "generation"].includes(key))
    && id(value.id) && ["single", "csv", "sync", "url_queue"].includes(String(value.kind)) && ["queued", "running", "cancelling", "succeeded", "failed", "cancelled"].includes(String(value.state)) && (value.track_id === null || id(value.track_id)) && (value.progress === null || progress(value.progress)) && (value.phase === undefined || value.phase === null || value.phase === "finalizing") && (value.current_item === undefined || value.current_item === null || (typeof value.current_item === "string" && value.current_item.length > 0)) && generation(value.generation);
  if (v.type === "track_started") return exact(v.payload, ["track_id"]) && id(v.payload.track_id);
  if (v.type === "play_statistics_snapshot") {
    const snapshot = v.payload.snapshot;
    const count = (entry: unknown, key: "track_id" | "playlist_id") => object(entry) && exact(entry, [key, "plays"]) && id(entry[key]) && nonNegativeInt(entry.plays) && entry.plays > 0;
    return exact(v.payload, ["snapshot"]) && object(snapshot) && exact(snapshot, ["generation", "tracks", "playlists", "legacy_track_counts_imported"])
      && generation(snapshot.generation) && Array.isArray(snapshot.tracks) && snapshot.tracks.length <= 100_000 && snapshot.tracks.every((entry) => count(entry, "track_id"))
      && Array.isArray(snapshot.playlists) && snapshot.playlists.length <= 100_000 && snapshot.playlists.every((entry) => count(entry, "playlist_id"))
      && typeof snapshot.legacy_track_counts_imported === "boolean";
  }
  if (v.type === "track_favorite_changed") return exact(v.payload, ["track_id", "favorite"]) && id(v.payload.track_id) && typeof v.payload.favorite === "boolean";
  if (v.type === "queue_changed") return exact(v.payload, ["queue"]) && Array.isArray(v.payload.queue) && v.payload.queue.every(id);
   if (v.type === "library_root_status") return exact(v.payload, ["root_index", "status"]) && safeInteger(v.payload.root_index) && v.payload.root_index >= 0 && ["scanned", "missing", "inaccessible"].includes(String(v.payload.status));
  if (v.type === "library_snapshot") return exact(v.payload, ["snapshot"]) && library(v.payload.snapshot);
   if (v.type === "playback_snapshot") return exact(v.payload, ["snapshot"]) && playback(v.payload.snapshot);
   if (v.type === "preview_snapshot") return exact(v.payload, ["snapshot"]) && isPreviewSnapshot(v.payload.snapshot);
   if (v.type === "discover_snapshot") return exact(v.payload, ["snapshot"]) && isDiscoverSnapshot(v.payload.snapshot);
   if (v.type === "save_state") {
     const save = v.payload.state;
     return exact(v.payload, ["state"]) && object(save) && exact(save, ["key", "playlist_id", "state", "already_saved", "path", "job_id", "error"]) && id(save.key) && id(save.playlist_id) && ["saving", "queued", "saved", "failed"].includes(String(save.state)) && typeof save.already_saved === "boolean" && optionalString(save.path) && (save.job_id === null || id(save.job_id)) && (save.error === null || typeof save.error === "string");
   }
   if (v.type === "video_state") return exact(v.payload, ["state"]) && video(v.payload.state);
  if (v.type === "job_updated") return exact(v.payload, ["job"]) && job(v.payload.job);
  if (v.type === "error") {
    const error = v.payload.error;
     return exact(v.payload, ["error"]) && object(error) && exact(error, ["code", "message", "retryable"]) && ["invalid_request", "unsupported_version", "not_found", "busy", "cancelled", "unavailable", "internal", "shutting_down"].includes(String(error.code)) && typeof error.message === "string" && typeof error.retryable === "boolean";
  }
  if (v.type === "shutdown") {
    const shutdown = v.payload.envelope;
    return exact(v.payload, ["envelope"]) && object(shutdown) && exact(shutdown, ["version", "state", "reason"]) && shutdown.version === CONTRACT_VERSION && ["requested", "draining", "complete", "timed_out", "failed"].includes(String(shutdown.state)) && ["user_requested", "system", "fatal_error"].includes(String(shutdown.reason));
  }
  return false;
}
export function isCommandEnvelope(v: unknown): v is CommandEnvelope { return exact(v, ["version", "request_id", "command"]) && isContractVersion(v.version) && nonEmptyId(v.request_id) && isCommand(v.command); }
export function isDeliveryErrorEventId(value: unknown): value is string { return typeof value === "string" && /^delivery-error-[1-9]\d*$/.test(value) && Number.isSafeInteger(Number(value.slice(15))); }
export function isDomainEventId(value: unknown): value is string { return typeof value === "string" && /^event-[1-9]\d*$/.test(value) && Number.isSafeInteger(Number(value.slice(6))); }
export function isEventEnvelope(v: unknown): v is EventEnvelope { return exact(v, ["version", "event_id", "generation", "job_id", "session_id", "event"]) && isContractVersion(v.version) && (isDomainEventId(v.event_id) || isDeliveryErrorEventId(v.event_id)) && safeInteger(v.generation) && v.generation >= 0 && (v.job_id === null || nonEmptyId(v.job_id)) && (v.session_id === null || nonEmptyId(v.session_id)) && isEvent(v.event); }
export function parseCommandEnvelope(json: string): CommandEnvelope { const value: unknown = JSON.parse(json); if (!isCommandEnvelope(value)) throw new Error("invalid or unsupported command envelope"); return value; }
export function parseEventEnvelope(json: string): EventEnvelope { const value: unknown = JSON.parse(json); if (!isEventEnvelope(value)) throw new Error("invalid or unsupported event envelope"); return value; }
export function isSearchResults(value: unknown): value is SearchResult[] { return Array.isArray(value) && value.every((item) => object(item) && exact(item, ["provider", "query", "title", "artist", "duration", "thumbnail_url", "url"]) && nonEmptyId(item.provider) && nonEmptyId(item.query) && nonEmptyId(item.title) && (item.artist === null || nonEmptyId(item.artist)) && (item.duration === null || typeof item.duration === "string") && (item.thumbnail_url === null || nonEmptyId(item.thumbnail_url)) && (item.url === null || nonEmptyId(item.url))); }
export function isArtworkCandidates(value: unknown): value is ArtworkCandidate[] {
  return Array.isArray(value) && value.length <= 12 && value.every((candidate) => object(candidate)
    && exact(candidate, ["candidate_id", "source", "release", "artist", "year", "score", "exact_match", "artwork_data_url"])
    && nonEmptyId(candidate.candidate_id) && (candidate.source === "musicbrainz" || candidate.source === "youtube") && nonEmptyId(candidate.release) && typeof candidate.artist === "string"
    && (candidate.year === null || typeof candidate.year === "string") && safeInteger(candidate.score) && candidate.score >= 0 && candidate.score <= 100
    && typeof candidate.exact_match === "boolean" && typeof candidate.artwork_data_url === "string"
    && candidate.artwork_data_url.startsWith("data:image/jpeg;base64,") && candidate.artwork_data_url.length <= 1_500_000);
}
function isCanonicalYouTubeWatchUrl(value: unknown): value is string { return typeof value === "string" && /^https:\/\/www\.youtube\.com\/watch\?v=[A-Za-z0-9_-]{11}$/.test(value); }
export function isPlaylistSyncIssues(value: unknown): value is { entries: PlaylistSyncIssue[] } {
  return object(value) && exact(value, ["entries"]) && Array.isArray(value.entries) && value.entries.every((issue) => object(issue)
    && exact(issue, ["video_id", "title", "channel", "duration_secs", "reason", "attempted_source_id", "selected_source_id", "retry_pending", "updated_at_ms", "local_matches"])
    && nonEmptyId(issue.video_id) && nonEmptyId(issue.title) && (issue.channel === null || nonEmptyId(issue.channel))
    && (issue.duration_secs === null || (safeInteger(issue.duration_secs) && issue.duration_secs >= 0)) && nonEmptyId(issue.reason)
    && (issue.attempted_source_id === null || nonEmptyId(issue.attempted_source_id))
    && (issue.selected_source_id === null || nonEmptyId(issue.selected_source_id)) && typeof issue.retry_pending === "boolean" && safeInteger(issue.updated_at_ms) && issue.updated_at_ms >= 0
    && Array.isArray(issue.local_matches) && issue.local_matches.every((match) => object(match)
      && exact(match, ["track_id", "title", "artist", "duration_ms", "playlists"]) && nonEmptyId(match.track_id) && nonEmptyId(match.title)
      && (match.artist === null || nonEmptyId(match.artist)) && (match.duration_ms === null || (safeInteger(match.duration_ms) && match.duration_ms >= 0))
      && Array.isArray(match.playlists) && match.playlists.every(nonEmptyId)));
}
export function isPlaylistSyncReviewSummary(value: unknown): value is { entries: PlaylistSyncReviewSummary[] } {
  return object(value) && exact(value, ["entries"]) && Array.isArray(value.entries) && value.entries.every((entry) => object(entry)
    && exact(entry, ["playlist_id", "name", "url", "unresolved_count"]) && nonEmptyId(entry.playlist_id) && nonEmptyId(entry.name)
    && (entry.url === null || nonEmptyId(entry.url)) && safeInteger(entry.unresolved_count) && entry.unresolved_count >= 0);
}
export function isPlaylistSyncSources(value: unknown): value is { urls: string[] } {
  return object(value) && exact(value, ["urls"]) && Array.isArray(value.urls) && value.urls.length <= 32 && value.urls.every(nonEmptyId);
}
export function isSafeDeleteResult(value: unknown): value is SafeDeleteResult {
  return object(value) && exact(value, ["deleted", "needs_confirmation", "verification_incomplete", "final_copy_review_id", "final_copies", "generation"])
    && safeInteger(value.deleted) && value.deleted >= 0 && typeof value.needs_confirmation === "boolean" && typeof value.verification_incomplete === "boolean"
    && (value.final_copy_review_id === null || nonEmptyId(value.final_copy_review_id))
    && Array.isArray(value.final_copies) && value.final_copies.every(nonEmptyId)
    && (value.generation === null || (safeInteger(value.generation) && value.generation >= 0))
    && (value.needs_confirmation ? value.final_copy_review_id !== null && value.final_copies.length > 0 && value.generation === null : value.final_copy_review_id === null && value.final_copies.length === 0 && !value.verification_incomplete && value.generation !== null);
}
export function isPlaylistSyncCandidates(value: unknown): value is PlaylistSyncCandidate[] {
  return Array.isArray(value) && value.every((candidate) => object(candidate)
    && exact(candidate, ["video_id", "title", "channel", "duration", "thumbnail_url", "url"])
    && /^[A-Za-z0-9_-]{11}$/.test(String(candidate.video_id)) && nonEmptyId(candidate.title)
    && (candidate.channel === null || nonEmptyId(candidate.channel))
    && (candidate.duration === null || typeof candidate.duration === "string")
    && (candidate.thumbnail_url === null || nonEmptyId(candidate.thumbnail_url)) && isCanonicalYouTubeWatchUrl(candidate.url)
    && candidate.url.endsWith(`?v=${candidate.video_id}`));
}
export function isPlaylistSyncSelection(value: unknown): value is PlaylistSyncSelection {
  return object(value) && exact(value, ["video_id", "source_video_id", "url"]) && nonEmptyId(value.video_id)
    && /^[A-Za-z0-9_-]{11}$/.test(String(value.source_video_id)) && isCanonicalYouTubeWatchUrl(value.url)
    && value.url.endsWith(`?v=${value.source_video_id}`);
}
export function isRecommendationList(value: unknown): value is Recommendation[] { return Array.isArray(value) && value.every((item) => object(item) && exact(item, ["artist", "title", "match_score", "artwork_url"]) && nonEmptyId(item.artist) && nonEmptyId(item.title) && typeof item.match_score === "number" && (item.artwork_url === null || typeof item.artwork_url === "string")); }
export function isPreviewSnapshot(value: unknown): value is PreviewSnapshot { return object(value) && (exact(value, ["generation", "active", "current", "window", "prepared", "unavailable", "cancelled"]) || exact(value, ["generation", "active", "current", "window", "prepared", "unavailable", "cancelled", "progress"])) && typeof value.generation === "number" && Number.isSafeInteger(value.generation) && value.generation >= 0 && typeof value.active === "boolean" && (value.current === null || nonEmptyId(value.current)) && Array.isArray(value.window) && value.window.every(nonEmptyId) && Array.isArray(value.prepared) && value.prepared.every(nonEmptyId) && Array.isArray(value.unavailable) && value.unavailable.every(nonEmptyId) && typeof value.cancelled === "boolean" && (value.progress === undefined || (Array.isArray(value.progress) && value.progress.every((item) => object(item) && exact(item, ["key", "percent"]) && nonEmptyId(item.key) && typeof item.percent === "number" && Number.isInteger(item.percent) && item.percent >= 0 && item.percent <= 100))); }
export function isDiscoverSnapshot(value: unknown): value is DiscoverSnapshot { return object(value) && (exact(value, ["generation", "enabled", "window", "ready", "queued"]) || exact(value, ["generation", "enabled", "window", "ready", "queued", "prepared"]) || exact(value, ["generation", "enabled", "window", "ready", "queued", "prepared", "unavailable"])) && typeof value.generation === "number" && Number.isSafeInteger(value.generation) && value.generation >= 0 && typeof value.enabled === "boolean" && Array.isArray(value.window) && value.window.every(nonEmptyId) && Number.isSafeInteger(value.ready) && (value.ready as number) >= 0 && Number.isSafeInteger(value.queued) && (value.queued as number) >= 0 && (value.prepared === undefined || (Array.isArray(value.prepared) && value.prepared.every(nonEmptyId))) && (value.unavailable === undefined || (Array.isArray(value.unavailable) && value.unavailable.every(nonEmptyId))); }

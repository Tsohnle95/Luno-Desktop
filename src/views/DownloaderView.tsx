import { useEffect, useId, useState, type FormEvent, type ReactNode } from "react";
import type { DownloadJob, Playlist, PlaylistDownloadPreview } from "../shared/contracts";
import { RecommendationArtworkImage } from "../components/RecommendationArtwork";
import { IconDownload, IconUpload, IconSearch } from "../components/icons";
import { downloadItemKey, type DownloadListItem } from "../state/download_inputs";
import { downloadResultPage, DOWNLOAD_PAGE_SIZE, DOWNLOAD_SEARCH_LIMIT } from "../state/download_results";
import "../styles/downloader.css";
import { DownloadTabs, type DownloadTab } from "../components/DownloadTabs";

export interface DownloadSearchResult { id: string; title: string; artist?: string; duration?: string; thumbnailUrl?: string; source?: string }
export interface DownloadPreview { id: string; title: string; artist?: string; source?: string }
export interface DuplicateGroup { key: string; tracks: string[] }
export interface DownloaderViewProps {
  destination?: string | null; jobs?: Record<string, DownloadJob>; jobErrors?: Record<string, string>; searchResults?: DownloadSearchResult[];
  previewTracks?: DownloadPreview[]; activity?: DownloadListItem[]; duplicateGroups?: DuplicateGroup[];
  allowedDuplicates?: string[]; destinations?: string[]; defaultDestination?: string; syncPlaylists?: Playlist[];
  playlistPreview?: PlaylistDownloadPreview | null; searchHasMore?: boolean; syncStatus?: string | null;
  syncPaused?: boolean; onResumeSync?: () => void; onSyncAll?: () => void; onSyncUrl?: (url: string, destination: string) => void;
  searching?: boolean; error?: string | null;
  onSearch?: (query: string, limit: number) => void; onDestinationChange?: (destination: string) => void;
  onImportCsv?: (file: File) => void; onPreview?: (query: string, kind: "youtube" | "csv") => void;
  onDownload?: (ids: string[], destination: string) => void; onSync?: (playlist: Playlist) => void;
  onCancel?: (jobId: string) => void; onCancelItem?: (item: DownloadListItem) => void; onRetryItem?: (item: DownloadListItem) => void;
  onAllowDuplicates?: (keys: string[]) => void; onRestoreDuplicate?: (key: string) => void;
  onExportAllowed?: () => void; onImportAllowed?: (file: File) => void; onCreateDestination?: () => void;
}

const inputStyle = { width: "100%", padding: "10px 12px", color: "var(--luno-text)", background: "var(--luno-bg-3)", border: "1px solid var(--luno-border)", borderRadius: "var(--luno-radius)" };
const buttonStyle = { border: 0, borderRadius: "var(--luno-radius)", padding: "10px 14px", color: "var(--luno-bg)", background: "var(--luno-accent)", fontWeight: 700, cursor: "pointer" };
export const destinationName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
export function linkedPlaylistForDestination(playlists: Playlist[], destination: string): Playlist | undefined {
  return playlists.find((playlist) => playlist.path === destination && !!playlist.url);
}
export function filterDownloadDestinations(items: string[], query: string): string[] {
  const needle = query.trim().toLocaleLowerCase();
  return items.filter((item) => !needle || destinationName(item).toLocaleLowerCase().includes(needle) || item.toLocaleLowerCase().includes(needle));
}
function Destination({ items, value, onChange, onCreate }: { items: string[]; value: string; onChange: (value: string) => void; onCreate?: () => void }) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const selected = items.find((item) => item === value) ?? value;
  const label = destinationName(selected);
  const filtered = filterDownloadDestinations(items, search);
  const choose = (item: string) => { onChange(item); setSearch(destinationName(item)); setOpen(false); };
  return <div className="luno-download-destination" style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
    <label htmlFor={`${listId}-input`}>Save to</label>
    <div style={{ position: "relative", flex: 1, minWidth: 160 }} onBlur={(event) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false);
    }}>
      <input
        id={`${listId}-input`}
        role="combobox"
        aria-label="Save to"
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={listId}
        placeholder={items.length ? "Search playlists" : "Library folder unavailable"}
        value={open ? search : label}
        disabled={items.length === 0}
        onFocus={() => { setSearch(""); setOpen(true); }}
        onClick={() => { if (!open) { setSearch(""); setOpen(true); } }}
        onChange={(event) => { setSearch(event.target.value); setOpen(true); }}
        onKeyDown={(event) => {
          if (event.key === "Escape") setOpen(false);
          if (event.key === "Enter" && open) {
            event.preventDefault();
            if (filtered[0]) choose(filtered[0]);
          }
          if (event.key === "ArrowDown") setOpen(true);
        }}
        style={{ ...inputStyle, width: "100%" }}
      />
      {open && items.length > 0 && <div id={listId} role="listbox" aria-label="Playlist destinations" style={{ position: "absolute", zIndex: 20, top: "calc(100% + 4px)", left: 0, right: 0, maxHeight: 240, overflowY: "auto", padding: 4, border: "1px solid var(--luno-border)", borderRadius: 8, background: "var(--luno-bg-2)", boxShadow: "0 12px 32px rgba(0,0,0,0.4)" }}>
        {filtered.length === 0
          ? <p style={{ margin: 0, padding: 10, color: "var(--luno-text-2)", fontSize: 13 }}>No playlists match “{search.trim()}”.</p>
          : filtered.map((item) => <button key={item} type="button" role="option" aria-selected={item === selected} onMouseDown={(event) => {
            // Keep the input focused until the option commits. In WKWebView a
            // pointer focus change can blur the input with a null relatedTarget,
            // closing this list before the button's click handler can run.
            event.preventDefault();
            choose(item);
          }} onClick={() => choose(item)} style={{ width: "100%", border: 0, borderRadius: 6, background: item === selected ? "rgba(var(--luno-accent-rgb),0.12)" : "transparent", color: "var(--luno-text)", padding: "9px 10px", textAlign: "left", cursor: "pointer" }} onMouseEnter={(event) => { event.currentTarget.style.background = "var(--luno-bg-3)"; }} onMouseLeave={(event) => { event.currentTarget.style.background = item === selected ? "rgba(var(--luno-accent-rgb),0.12)" : "transparent"; }}>{destinationName(item)}</button>)}
      </div>}
    </div>
    <button type="button" onClick={onCreate} style={{ ...buttonStyle, whiteSpace: "nowrap" }}>+ New</button>
  </div>;
}
export function DownloadSongRow({ item, onDownload, onCancel, onRetry }: {
  item: DownloadSearchResult & { destination?: string; activity?: DownloadListItem };
  onDownload?: () => void; onCancel?: () => void; onRetry?: () => void;
}) {
  const activity = item.activity;
  const status = activity?.status;
  const percent = activity?.progress == null ? null : Math.round(activity.progress * 100);
  const busy = status === "running" || status === "starting" || status === "cancelling";
  const label = status === "queued" ? activity?.position == null ? "Queued" : activity.position === 1 ? "Next in queue" : `#${activity.position} in queue`
    : status === "starting" ? "Starting…" : status === "cancelling" ? "Cancelling…"
    : status === "running" ? activity?.finalizing ? "Adding to library…" : percent == null ? "Downloading…" : `Downloading ${percent}%`
    : status === "succeeded" ? "Downloaded" : status === "failed" ? "Download failed" : status === "cancelled" ? "Cancelled" : null;
  return <div className={`luno-download-song${status ? ` is-${status}` : ""}`} role="listitem" aria-label={item.title}>
    <div className="luno-download-song-art"><RecommendationArtworkImage artworkUrl={item.thumbnailUrl} artist={item.artist ?? ""} title={item.title} alt="" size="100%" /></div>
    <div className="luno-download-song-copy"><strong title={item.title}>{item.title}</strong><span>{item.artist || "YouTube"}{item.duration ? ` · ${item.duration}` : ""}{activity && item.destination ? ` · ${destinationName(item.destination)}` : ""}</span>
      {activity?.error && <small className="luno-download-row-error">{activity.error}</small>}
    </div>
    <div className="luno-download-song-actions">
      {label && <span className="luno-download-row-state" role="status">{label}</span>}
      {!activity && <button className="luno-download-action" type="button" aria-label={`Download ${item.title}`} onClick={onDownload}><IconDownload size={15} /> Download</button>}
      {(busy || status === "queued") && <button className="luno-download-cancel" type="button" aria-label={`Cancel ${item.title}`} disabled={status === "cancelling"} onClick={onCancel}>Cancel</button>}
      {onRetry && (status === "failed" || status === "cancelled") && <button className="luno-download-action" type="button" aria-label={`Retry ${item.title}`} onClick={onRetry}>Retry</button>}
    </div>
    {busy && <div className={`luno-download-row-progress${percent == null || activity?.finalizing ? " is-indeterminate" : ""}`} role="progressbar" aria-label={`Download progress for ${item.title}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={activity?.finalizing ? undefined : percent ?? undefined}><span style={{ width: `${activity?.finalizing ? 100 : percent ?? 30}%` }} /></div>}
  </div>;
}

export function DownloaderView({ playlistPreview, searchHasMore = true, syncPaused, onResumeSync, onSyncAll, onSyncUrl, syncStatus, jobs = {}, jobErrors = {}, searchResults = [], previewTracks = [], activity = [], duplicateGroups = [], allowedDuplicates = [], destinations = ["Unsorted"], defaultDestination, destination: controlledDestination, syncPlaylists = [], searching = false, error = null, onSearch, onDestinationChange, onImportCsv, onPreview, onDownload, onSync, onCancel, onCancelItem, onRetryItem, onAllowDuplicates, onRestoreDuplicate, onExportAllowed, onImportAllowed, onCreateDestination }: DownloaderViewProps) {
  const [tab, setTab] = useState<DownloadTab>("search");
  const [query, setQuery] = useState("");
  const [submittedQuery, setSubmittedQuery] = useState("");
  const [requestedLimit, setRequestedLimit] = useState(DOWNLOAD_PAGE_SIZE);
  const [page, setPage] = useState(0);
  const [fileError, setFileError] = useState<string | null>(null);
  const [internalDestination, setInternalDestination] = useState(defaultDestination ?? destinations[0] ?? "Unsorted");
  const destination = controlledDestination ?? internalDestination;
  const setDestination = (value: string) => { if (controlledDestination == null) setInternalDestination(value); onDestinationChange?.(value); };
  useEffect(() => {
    if (defaultDestination && controlledDestination == null) setInternalDestination((current) => destinations.includes(current) ? current : defaultDestination);
  }, [defaultDestination, destinations, controlledDestination]);
  const results = [...new Map([...searchResults, ...previewTracks].map((item) => [item.source ?? item.id, item])).values()];
  const activityByKey = new Map(activity.map((item) => [downloadItemKey(item), item]));
  const resultKeys = new Set(results.map((item) => downloadItemKey({ source: item.source ?? item.id, destination })));
  const earlierDownloads = activity.filter((item) => !resultKeys.has(downloadItemKey(item)));
  const untrackedResults = results.filter((item) => !activityByKey.has(downloadItemKey({ source: item.source ?? item.id, destination })));
  const resultSet = `${submittedQuery}:${playlistPreview?.url ?? ""}:${previewTracks[0]?.id ?? ""}`;
  useEffect(() => setPage(0), [resultSet]);
  const visible = downloadResultPage(results, page);
  const canLoadMore = !!submittedQuery && !/^https?:/i.test(submittedQuery) && !playlistPreview && !previewTracks.length && searchHasMore && requestedLimit < DOWNLOAD_SEARCH_LIMIT;
  const loadMore = () => { const limit = Math.min(DOWNLOAD_SEARCH_LIMIT, requestedLimit + DOWNLOAD_PAGE_SIZE); setRequestedLimit(limit); setPage(Math.ceil(results.length / DOWNLOAD_PAGE_SIZE)); onSearch?.(submittedQuery, limit); };
  const syncJobs = Object.values(jobs).filter((job) => job.kind === "sync");
  const syncBusy = !!syncStatus || syncJobs.some((job) => ["queued", "running", "cancelling"].includes(job.state));
  const submit = (event: FormEvent) => { event.preventDefault(); if (query.trim() && !searching) { setFileError(null); setPage(0); setSubmittedQuery(query.trim()); setRequestedLimit(DOWNLOAD_PAGE_SIZE); onSearch?.(query.trim(), DOWNLOAD_PAGE_SIZE); } };
  return <section className="luno-downloader-view" aria-label="Downloader">
    <header className="luno-downloader-header"><div><p>DOWNLOADS</p><h1>Download music</h1><span>Songs, playlists, and CSV imports. One automatic queue.</span></div></header>
    <DownloadTabs active={tab} onChange={setTab}>
      {tab === "search" && <>
        <form className="luno-download-search" onSubmit={submit}><IconSearch size={18} /><input aria-label="Search for music" autoCorrect="off" spellCheck={false} placeholder="Search songs or paste a video or playlist link" value={query} onChange={(event) => setQuery(event.target.value)} disabled={searching} /><button className="luno-download-action" type="submit" disabled={searching} aria-busy={searching}>{searching ? "Finding songs…" : "Search"}</button></form>
        <div className="luno-download-toolbar"><label className={`luno-csv-import${searching ? " is-disabled" : ""}`}><IconUpload size={15} /> Import CSV<input aria-label="Import CSV" type="file" accept=".csv,text/csv" disabled={searching} hidden onChange={(event) => { const file = event.target.files?.[0]; if (file) { setQuery(""); setSubmittedQuery(""); setPage(0); setFileError(null); if (onImportCsv) onImportCsv(file); else void file.text().then((value) => onPreview?.(value, "csv")).catch(() => setFileError("This CSV could not be read. Choose another file.")); } event.target.value = ""; }} /></label><span>Video links, playlists & Exportify CSVs</span><Destination items={destinations} value={destination} onChange={setDestination} onCreate={onCreateDestination} /></div>
        <p className="luno-download-hint">Click Download to start or join the queue. One song at a time, in the order you add them.</p>
        {(error || fileError) && <p className="luno-download-input-error" role="alert">{error || fileError}</p>}
        {earlierDownloads.length > 0 && <section className="luno-download-list-section" aria-label="Your downloads"><div className="luno-download-section-title"><h2>Your downloads</h2><span>{activity.filter((row) => ["running", "starting", "cancelling", "queued"].includes(row.status)).length} remaining</span></div><div role="list" aria-label="Download activity">{earlierDownloads.map((item) => <DownloadSongRow key={downloadItemKey(item)} item={{ ...item, title: item.title ?? item.id, artist: item.artist ?? undefined, activity: item }} onCancel={() => onCancelItem?.(item)} onRetry={() => onRetryItem?.(item)} />)}</div></section>}
        <section className="luno-download-list-section" aria-label="Search results"><div className="luno-download-section-title"><h2>{playlistPreview ? playlistPreview.title : previewTracks.length ? "Imported songs" : results.length ? "Results" : null}</h2>{results.length > 1 && <button className="luno-download-batch" type="button" onClick={() => onDownload?.(results.map((item) => item.id), destination)} disabled={searching || untrackedResults.length === 0}><IconDownload size={14} /> {playlistPreview ? "Download missing" : "Download all"} ({results.length})</button>}</div>
          {playlistPreview && !searching && <div className="luno-download-playlist-summary"><span className="luno-download-source-type">YOUTUBE PLAYLIST</span><p>{playlistPreview.total} songs · {playlistPreview.downloaded} already in {destinationName(destination)} · <strong>{playlistPreview.items.length} missing</strong>{playlistPreview.unavailable > 0 ? ` · ${playlistPreview.unavailable} unavailable` : ""}{playlistPreview.skipped > 0 ? ` · ${playlistPreview.skipped} skipped after review` : ""}</p><small>{playlistPreview.url}</small>{playlistPreview.items.length === 0 && <p>{playlistPreview.downloaded === playlistPreview.total ? `This playlist is up to date in ${destinationName(destination)}.` : "No new downloadable songs. Unavailable and skipped songs are counted above."}</p>}</div>}
          {searching ? <div className="luno-download-empty" role="status"><span className="luno-download-spinner" /> Finding songs… You can keep managing your downloads.</div> : <div role="list" aria-label="Available songs">{visible.items.map((item) => {
            const active = activityByKey.get(downloadItemKey({ source: item.source ?? item.id, destination }));
            return <DownloadSongRow key={item.id} item={{ ...item, destination, activity: active }} onDownload={() => onDownload?.([item.id], destination)} onCancel={() => active && onCancelItem?.(active)} onRetry={() => active && onRetryItem?.(active)} />;
          })}</div>}
          {!searching && results.length > 0 && <nav className="luno-download-pagination" aria-label="Results pages"><span>{visible.start + 1}–{visible.end} of {results.length}</span><div><button type="button" disabled={visible.page === 0} onClick={() => setPage(visible.page - 1)}>Previous</button><span>Page {visible.page + 1} of {visible.pages}</span>{visible.page + 1 < visible.pages ? <button type="button" onClick={() => setPage(visible.page + 1)}>Next</button> : canLoadMore ? <button type="button" onClick={loadMore}>More results</button> : <button type="button" disabled>Next</button>}</div></nav>}
        </section>
      </>}
      {tab === "sync" && <DownloadSyncPanel destinations={destinations} destination={destination} playlists={syncPlaylists} busy={syncBusy} paused={syncPaused} onResume={onResumeSync} status={syncStatus} error={error} onDestinationChange={setDestination} onCreateDestination={onCreateDestination} onSync={onSync} onSyncAll={onSyncAll} onSyncUrl={onSyncUrl}>{syncJobs.map((job) => <DownloadSongRow key={job.id} item={{ id: job.id, title: job.current_item ?? "Playlist sync", activity: { id: job.id, source: job.id, destination, jobId: job.id, status: job.state, progress: job.progress, error: jobErrors[job.id] } }} onCancel={() => onCancel?.(job.id)} />)}</DownloadSyncPanel>}
      {tab === "duplicates" && <div className="luno-download-duplicates"><h2>Duplicate review</h2><p>Keep the copies you want. Your files stay protected.</p>{duplicateGroups.length === 0 && <p>No duplicate groups to review.</p>}{duplicateGroups.map((group) => <fieldset key={group.key}><legend>{group.key}</legend>{group.tracks.map((track) => <div key={track}>{track}</div>)}<button className="luno-download-batch" type="button" onClick={() => onAllowDuplicates?.([group.key])}>Keep this group</button></fieldset>)}<div className="luno-download-section-title"><h2>Kept duplicate groups</h2><div><button type="button" onClick={onExportAllowed}>Export</button><label>Import<input type="file" accept="application/json" hidden onChange={(event) => { const file = event.target.files?.[0]; if (file) onImportAllowed?.(file); }} /></label></div></div><p>Keeping a group hides it from review and never overwrites a file.</p>{allowedDuplicates.map((key) => <div className="luno-kept-duplicate" key={key}><span>{key}</span><button type="button" onClick={() => onRestoreDuplicate?.(key)}>Review again</button></div>)}</div>}
    </DownloadTabs>
  </section>;
}


export function DownloadSyncPanel({ destinations, destination, playlists, busy, paused, onResume, status, error, onDestinationChange, onCreateDestination, onSync, onSyncAll, onSyncUrl, children }: {
  destinations: string[]; destination: string; playlists: Playlist[]; busy?: boolean; paused?: boolean; onResume?: () => void; status?: string | null; error?: string | null;
  onDestinationChange: (value: string) => void; onCreateDestination?: () => void; onSync?: (playlist: Playlist) => void; onSyncAll?: () => void; onSyncUrl?: (url: string, destination: string) => void; children?: ReactNode;
}) {
  const [url, setUrl] = useState("");
  const selected = linkedPlaylistForDestination(playlists, destination);
  const linked = playlists.filter((playlist) => playlist.path && playlist.url);
  return <div className="luno-download-sync"><div className="luno-download-sync-heading"><div><h2>Playlist sync</h2><p>Add songs missing from your library playlists.</p></div><button className="luno-download-action" type="button" disabled={busy || linked.length === 0} onClick={onSyncAll}>Sync all ({linked.length})</button></div>
    <form className="luno-download-sync-form" onSubmit={(event) => { event.preventDefault(); if (busy || !destination) return; if (url.trim()) onSyncUrl?.(url.trim(), destination); else if (selected) onSync?.(selected); }}>
      <label htmlFor="luno-sync-url">YouTube playlist link</label><input id="luno-sync-url" type="url" placeholder="Paste a YouTube playlist URL" value={url} onChange={(event) => setUrl(event.target.value)} autoCorrect="off" spellCheck={false} />
      <Destination items={destinations} value={destination} onChange={onDestinationChange} onCreate={onCreateDestination} />
      <div className="luno-download-sync-submit"><p>{url.trim() ? `Link and sync into ${destinationName(destination)}. Existing links are kept.` : selected ? `Check the YouTube playlists linked to ${selected.name}.` : "Paste a playlist link and choose where to save its songs."}</p><button className="luno-download-action" type="submit" disabled={busy || !destination || (!url.trim() && !selected)}>Sync playlist</button></div>
    </form>
    {status && <p role="status">{status}</p>}{paused && <button type="button" className="luno-download-action" onClick={onResume}>Continue sync</button>}{error && <p className="luno-download-input-error" role="alert">{error}</p>}
    {linked.length > 0 && <section aria-label="Linked playlists"><div className="luno-download-section-title"><h2>Linked playlists</h2><span>{linked.length} playlists</span></div>{linked.map((playlist) => <div className="luno-download-linked-playlist" key={playlist.id}><div><strong>{playlist.name}</strong><small>{playlist.url}</small></div><button type="button" className="luno-download-batch" disabled={busy} aria-label={`Sync ${playlist.name}`} onClick={() => onSync?.(playlist)}>Sync</button></div>)}</section>}
    {children}
  </div>;
}

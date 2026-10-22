import { useEffect, useId, useState, type FormEvent } from "react";
import type { DownloadJob, Playlist } from "../shared/contracts";
import { RecommendationArtworkImage } from "../components/RecommendationArtwork";
import { IconDownload, IconUpload, IconSearch } from "../components/icons";
import { downloadItemKey, type DownloadListItem } from "../state/download_inputs";
import "../styles/downloader.css";
import { DownloadTabs, type DownloadTab } from "../components/DownloadTabs";

export interface DownloadSearchResult { id: string; title: string; artist?: string; duration?: string; thumbnailUrl?: string; source?: string }
export interface DownloadPreview { id: string; title: string; artist?: string; source?: string }
export interface DuplicateGroup { key: string; tracks: string[] }
export interface DownloaderViewProps {
  destination?: string | null; jobs?: Record<string, DownloadJob>; jobErrors?: Record<string, string>; searchResults?: DownloadSearchResult[];
  previewTracks?: DownloadPreview[]; activity?: DownloadListItem[]; duplicateGroups?: DuplicateGroup[];
  allowedDuplicates?: string[]; destinations?: string[]; defaultDestination?: string; syncPlaylists?: Playlist[];
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
      {(status === "failed" || status === "cancelled") && <button className="luno-download-action" type="button" aria-label={`Retry ${item.title}`} onClick={onRetry}>Retry</button>}
    </div>
    {busy && <div className={`luno-download-row-progress${percent == null || activity?.finalizing ? " is-indeterminate" : ""}`} role="progressbar" aria-label={`Download progress for ${item.title}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={activity?.finalizing ? undefined : percent ?? undefined}><span style={{ width: `${activity?.finalizing ? 100 : percent ?? 30}%` }} /></div>}
  </div>;
}

export function DownloaderView({ jobs = {}, jobErrors = {}, searchResults = [], previewTracks = [], activity = [], duplicateGroups = [], allowedDuplicates = [], destinations = ["Unsorted"], defaultDestination, destination: controlledDestination, syncPlaylists = [], searching = false, error = null, onSearch, onDestinationChange, onImportCsv, onPreview, onDownload, onSync, onCancel, onCancelItem, onRetryItem, onAllowDuplicates, onRestoreDuplicate, onExportAllowed, onImportAllowed, onCreateDestination }: DownloaderViewProps) {
  const [tab, setTab] = useState<DownloadTab>("search");
  const [query, setQuery] = useState("");
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
  const selectedSyncPlaylist = linkedPlaylistForDestination(syncPlaylists, destination);
  const syncJobs = Object.values(jobs).filter((job) => job.kind === "sync");
  const syncBusy = syncJobs.some((job) => ["queued", "running", "cancelling"].includes(job.state));
  const submit = (event: FormEvent) => { event.preventDefault(); if (query.trim() && !searching) { setFileError(null); onSearch?.(query.trim(), 25); } };
  return <section className="luno-downloader-view" aria-label="Downloader">
    <header className="luno-downloader-header"><div><p>DOWNLOADS</p><h1>Download music</h1><span>Songs, playlists, and CSV imports. One automatic queue.</span></div></header>
    <DownloadTabs active={tab} onChange={setTab}>
      {tab === "search" && <>
        <form className="luno-download-search" onSubmit={submit}><IconSearch size={18} /><input aria-label="Search for music" autoCorrect="off" spellCheck={false} placeholder="Search songs or paste a video or playlist link" value={query} onChange={(event) => setQuery(event.target.value)} disabled={searching} /><button className="luno-download-action" type="submit" disabled={searching} aria-busy={searching}>{searching ? "Finding songs…" : "Search"}</button></form>
        <div className="luno-download-toolbar"><label className={`luno-csv-import${searching ? " is-disabled" : ""}`}><IconUpload size={15} /> Import CSV<input aria-label="Import CSV" type="file" accept=".csv,text/csv" disabled={searching} hidden onChange={(event) => { const file = event.target.files?.[0]; if (file) { setQuery(""); setFileError(null); if (onImportCsv) onImportCsv(file); else void file.text().then((value) => onPreview?.(value, "csv")).catch(() => setFileError("This CSV could not be read. Choose another file.")); } event.target.value = ""; }} /></label><span>Video links, playlists & Exportify CSVs</span><Destination items={destinations} value={destination} onChange={setDestination} onCreate={onCreateDestination} /></div>
        <p className="luno-download-hint">Click Download to start or join the queue. One song at a time, in the order you add them.</p>
        {(error || fileError) && <p className="luno-download-input-error" role="alert">{error || fileError}</p>}
        {earlierDownloads.length > 0 && <section className="luno-download-list-section" aria-label="Your downloads"><div className="luno-download-section-title"><h2>Your downloads</h2><span>{activity.filter((row) => ["running", "starting", "cancelling", "queued"].includes(row.status)).length} remaining</span></div><div role="list" aria-label="Download activity">{earlierDownloads.map((item) => <DownloadSongRow key={downloadItemKey(item)} item={{ ...item, title: item.title ?? item.id, artist: item.artist ?? undefined, activity: item }} onCancel={() => onCancelItem?.(item)} onRetry={() => onRetryItem?.(item)} />)}</div></section>}
        <section className="luno-download-list-section" aria-label="Search results"><div className="luno-download-section-title"><h2>{previewTracks.length ? "Imported songs" : results.length ? "Results" : "Find your next favorite"}</h2>{results.length > 1 && <button className="luno-download-batch" type="button" onClick={() => onDownload?.(results.map((item) => item.id), destination)} disabled={searching || untrackedResults.length === 0}><IconDownload size={14} /> Download all ({results.length})</button>}</div>
          {searching ? <div className="luno-download-empty" role="status"><span className="luno-download-spinner" /> Finding songs… You can keep managing your downloads.</div> : <div role="list" aria-label="Available songs">{results.map((item) => {
            const active = activityByKey.get(downloadItemKey({ source: item.source ?? item.id, destination }));
            return <DownloadSongRow key={item.id} item={{ ...item, destination, activity: active }} onDownload={() => onDownload?.([item.id], destination)} onCancel={() => active && onCancelItem?.(active)} onRetry={() => active && onRetryItem?.(active)} />;
          })}</div>}
          {!searching && results.length === 0 && <div className="luno-download-empty"><IconSearch size={25} /><p>Search by song or artist, paste a YouTube link,<br />or import a CSV to see its songs here.</p></div>}
        </section>
      </>}
      {tab === "sync" && <div className="luno-download-sync"><h2>Keep your playlists up to date</h2><p>Choose a linked library playlist to check for new songs.</p><Destination items={destinations} value={destination} onChange={setDestination} onCreate={onCreateDestination} />{selectedSyncPlaylist ? <button className="luno-download-action" type="button" disabled={syncBusy} onClick={() => onSync?.(selectedSyncPlaylist)}>{syncBusy ? "Sync in progress…" : `Sync ${selectedSyncPlaylist.name}`}</button> : <p>This folder has no linked YouTube playlist. Use Link playlist in Your Library to add one.</p>}{syncJobs.map((job) => <DownloadSongRow key={job.id} item={{ id: job.id, title: job.current_item ?? "Playlist sync", activity: { id: job.id, source: job.id, destination, jobId: job.id, status: job.state, progress: job.progress, error: jobErrors[job.id] } }} onCancel={() => onCancel?.(job.id)} />)}</div>}
      {tab === "duplicates" && <div className="luno-download-duplicates"><h2>Duplicate review</h2><p>Keep the copies you want. Your files stay protected.</p>{duplicateGroups.length === 0 && <p>No duplicate groups to review.</p>}{duplicateGroups.map((group) => <fieldset key={group.key}><legend>{group.key}</legend>{group.tracks.map((track) => <div key={track}>{track}</div>)}<button className="luno-download-batch" type="button" onClick={() => onAllowDuplicates?.([group.key])}>Keep this group</button></fieldset>)}<div className="luno-download-section-title"><h2>Kept duplicate groups</h2><div><button type="button" onClick={onExportAllowed}>Export</button><label>Import<input type="file" accept="application/json" hidden onChange={(event) => { const file = event.target.files?.[0]; if (file) onImportAllowed?.(file); }} /></label></div></div><p>Keeping a group hides it from review and never overwrites a file.</p>{allowedDuplicates.map((key) => <div className="luno-kept-duplicate" key={key}><span>{key}</span><button type="button" onClick={() => onRestoreDuplicate?.(key)}>Review again</button></div>)}</div>}
    </DownloadTabs>
  </section>;
}

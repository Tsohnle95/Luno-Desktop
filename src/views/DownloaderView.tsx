import { useEffect, useId, useState, type FormEvent, type ReactNode } from "react";
import type { DownloadJob, Playlist } from "../shared/contracts";
import { bridge } from "../app/bridge";
import { DownloadTabs, type DownloadTab } from "../components/DownloadTabs";
import { DownloadRing } from "../components/VideoStage";

export interface DownloadSearchResult { id: string; title: string; artist?: string; duration?: string; thumbnailUrl?: string; source?: string }
export interface DownloadPreview { id: string; title: string; artist?: string; source?: string }
export interface DuplicateGroup { key: string; tracks: string[] }
export interface DownloaderViewProps {
  destination?: string | null;
  jobs?: Record<string, DownloadJob>; searchResults?: DownloadSearchResult[]; previewTracks?: DownloadPreview[]; duplicateGroups?: DuplicateGroup[]; allowedDuplicates?: string[]; destinations?: string[]; defaultDestination?: string; queue?: string[]; queuedDownloads?: number; syncPlaylists?: Playlist[]; loading?: boolean; searching?: boolean; error?: string | null; partialSuccess?: string | null; activeLabel?: string | null; activeLabels?: Record<string, string>;
  onSearch?: (query: string, limit: number) => void; onDestinationChange?: (destination: string) => void; onLoadMore?: () => void; onPreview?: (query: string, kind: "youtube" | "csv") => void; onDownload?: (ids: string[], destination: string) => void; onDirectUrl?: (url: string, destination: string) => void; onQueueUrl?: (url: string) => void; onDownloadQueue?: (destination: string) => void; onSync?: (playlist: Playlist) => void; onCancel?: (jobId: string) => void; onRetry?: (jobId: string) => void; onAllowDuplicates?: (keys: string[]) => void; onExportAllowed?: () => void; onImportAllowed?: (file: File) => void; onCreateDestination?: () => void; children?: ReactNode;
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
function Status({ jobs, onCancel, activeLabel, activeLabels }: Pick<DownloaderViewProps, "jobs" | "onCancel" | "activeLabel" | "activeLabels">) {
  const values = Object.values(jobs ?? {});
  const active = values.filter((job) => ["running", "cancelling", "queued"].includes(job.state));
  const latest = values[values.length - 1];
  if (active.length > 0) return <div className="luno-download-status" role="status" aria-live="polite" style={{ padding: 12, background: "var(--luno-bg-3)", display: "grid", gap: 10 }}>
    {active.length > 1 && <strong>{active.length} downloads active</strong>}
    {active.map((job, index) => {
      const finalizing = job.phase === "finalizing";
      const percent = job.progress == null ? null : Math.round(job.progress * 100);
      const label = activeLabels?.[job.id] ?? (index === 0 ? activeLabel : null) ?? job.current_item ?? null;
      return <div key={job.id} style={{ display: "grid", gridTemplateColumns: "auto minmax(0,1fr) auto", gap: 8, alignItems: "center" }}>
        <DownloadRing progress={finalizing ? null : percent} size={30} />
        <span>{job.kind === "sync" ? "Syncing playlist" : finalizing ? "Finalizing download" : "Downloading"}{label ? ` — ${label}` : ""}{finalizing ? " — checking and adding the file" : percent == null ? "" : ` — ${percent}%`}</span>
        <button type="button" onClick={() => onCancel?.(job.id)} disabled={job.state === "cancelling"}>{job.state === "cancelling" ? "Cancelling..." : "Cancel"}</button>
        <progress aria-label={active.length === 1 ? "Download progress" : `Download progress for ${label ?? job.id}`} value={finalizing ? undefined : job.progress ?? undefined} max={1} style={{ gridColumn: "1 / -1", width: "100%" }} />
      </div>;
    })}
  </div>;
  return latest?.state === "succeeded" ? <p role="status" aria-live="polite" style={{ padding: 12, background: "var(--luno-bg-3)", color: "var(--luno-accent)" }}>✓ Download complete — added to your library.</p> : null;
}

export function DownloaderView({ jobs = {}, searchResults = [], previewTracks = [], duplicateGroups = [], allowedDuplicates = [], destinations = ["Unsorted"], defaultDestination, destination: controlledDestination, queue = [], queuedDownloads = 0, syncPlaylists = [], loading = false, searching = false, error = null, partialSuccess = null, activeLabel = null, activeLabels, onSearch, onDestinationChange, onLoadMore, onPreview, onDownload, onDirectUrl, onQueueUrl, onDownloadQueue, onSync, onCancel, onRetry, onAllowDuplicates, onExportAllowed, onImportAllowed, onCreateDestination, children }: DownloaderViewProps) {
  const [tab, setTab] = useState<DownloadTab>("search"); const [query, setQuery] = useState(""); const [url, setUrl] = useState(""); const [internalDestination, setInternalDestination] = useState(defaultDestination ?? destinations[0] ?? "Unsorted"); const destination = controlledDestination ?? internalDestination; const setDestination = (value: string) => { if (controlledDestination == null) setInternalDestination(value); onDestinationChange?.(value); }; const [previewKind, setPreviewKind] = useState<"youtube" | "csv">("youtube"); const [limit, setLimit] = useState(25); const [selected, setSelected] = useState<string[]>([]); const [accordions, setAccordions] = useState({ progress: true, log: false, downloaded: false });
  // The library root (and therefore the default destination) may resolve AFTER
  // first render (settings load is async); adopt it once it appears, but never
  // fight the user's explicit choice.
  useEffect(() => {
    if (!defaultDestination) return;
    if (controlledDestination != null) return;
    setInternalDestination((current) => (destinations.includes(current) ? current : defaultDestination));
  }, [defaultDestination, destinations, controlledDestination]);
  const toggle = (id: string) => setSelected((current) => current.includes(id) ? current.filter((item) => item !== id) : [...current, id]); const submit = (event: FormEvent) => { event.preventDefault(); const trimmed = query.trim(); if (!trimmed || searching) return; setSelected([]); onSearch?.(trimmed, limit); }; const validUrl = (value: string) => /^https?:\/\//i.test(value.trim());
  const selectedSyncPlaylist = linkedPlaylistForDestination(syncPlaylists, destination);
  const syncBusy = Object.values(jobs).some((job) => job.kind === "sync" && ["queued", "running", "cancelling"].includes(job.state));
  const accordion = (name: keyof typeof accordions, title: string, content: ReactNode) => <div className="luno-accordion"><button type="button" aria-expanded={accordions[name]} onClick={() => setAccordions((state) => ({ ...state, [name]: !state[name] }))} style={{ cursor: "pointer", padding: 12, borderTop: "1px solid var(--luno-border)", fontWeight: 700, width: "100%", textAlign: "left", background: "transparent", color: "var(--luno-text)", border: 0 }}>{title}</button>{accordions[name] && <div style={{ padding: 12 }}>{content}</div>}</div>;
  return <section className="luno-downloader-view" aria-label="Downloader" style={{ minHeight: "100%", padding: "28px clamp(16px, 4vw, 40px) 56px", maxWidth: 980, margin: "0 auto" }}><header><p style={{ margin: 0, color: "var(--luno-text-2)" }}>Download music to your library</p><h1>Downloader</h1><Status jobs={jobs} onCancel={onCancel} activeLabel={activeLabel} activeLabels={activeLabels} />{queuedDownloads > 0 && <p role="status" style={{ margin: "8px 0 0", color: "var(--luno-text-2)" }}>{queuedDownloads} download{queuedDownloads === 1 ? "" : "s"} waiting for an active slot.</p>}</header>{loading && <p role="status">Loading downloader...</p>}{error && <p role="alert" style={{ color: "var(--luno-red)" }}>{error}</p>}{partialSuccess && <p role="status" style={{ color: "var(--luno-warning)" }}>{partialSuccess}</p>}
    <DownloadTabs active={tab} onChange={setTab}>{tab === "search" && <><form className="luno-download-search" onSubmit={submit} style={{ display: "flex", gap: 8, margin: "18px 0" }}><input aria-label="Search for music" autoCorrect="off" spellCheck={false} placeholder="Search YouTube or SoundCloud..." value={query} onChange={(e) => setQuery(e.target.value)} style={inputStyle} disabled={searching} /><button type="submit" style={{ ...buttonStyle, opacity: searching ? 0.6 : 1, cursor: searching ? "wait" : "pointer" }} disabled={searching} aria-busy={searching}>{searching ? "Searching…" : "Search"}</button></form><Destination items={destinations} value={destination} onChange={setDestination} onCreate={onCreateDestination} />{searching && <p role="status" aria-live="polite" style={{ margin: "12px 0 0", padding: "10px 12px", background: "var(--luno-bg-3)", borderRadius: 8, color: "var(--luno-text-2)", display: "flex", alignItems: "center", gap: 8 }}><span className="luno-download-spinner" aria-hidden /> Searching YouTube…</p>}<div style={{ display: "grid", gap: 8, marginTop: 16 }}>{!searching && searchResults.length === 0 && query.trim() && !error && <p style={{ color: "var(--luno-text-2)", padding: 12, background: "var(--luno-bg-3)", borderRadius: 8 }}>No matches yet — try adding the artist or a different title.</p>}{searchResults.map((result) => <div className="luno-download-result" key={result.id} style={{ display: "flex", gap: 12, padding: 12, background: "var(--luno-bg-3)", borderRadius: 8, alignItems: "center" }}><input type="checkbox" checked={selected.includes(result.id)} onChange={() => toggle(result.id)} aria-label={`Select ${result.title}`} />{result.thumbnailUrl && <img src={result.thumbnailUrl} alt="" width="52" height="52" style={{ borderRadius: 4, objectFit: "cover", flexShrink: 0 }} />}<span style={{ flex: 1, minWidth: 0 }}><strong style={{ display: "block", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{result.title}</strong><small style={{ color: "var(--luno-text-2)" }}>{result.artist ?? ""}{result.duration ? ` · ${result.duration}` : ""}</small></span><button type="button" aria-label={`Download ${result.title}`} onClick={() => onDownload?.([result.id], destination)} style={{ ...buttonStyle, padding: "8px 12px", whiteSpace: "nowrap", display: "flex", alignItems: "center", gap: 6, flexShrink: 0 }}>⬇ Download</button></div>)}</div>{searchResults.length >= limit && <button type="button" disabled={searching} onClick={() => { const next = limit + 25; setLimit(next); onLoadMore?.(); onSearch?.(query.trim(), next); }} style={{ ...buttonStyle, marginTop: 12, opacity: searching ? 0.6 : 1 }}>{searching ? "Searching…" : "Load More"}</button>}{selected.length > 0 && <button type="button" onClick={() => onDownload?.(selected, destination)} style={{ ...buttonStyle, marginTop: 12 }}>Download selected ({selected.length})</button>}{selected.length === 0 && searchResults.length > 0 && !searching && <p style={{ marginTop: 12, color: "var(--luno-text-2)", fontSize: 13 }}>Tip: select songs with the checkboxes or press ⬇ Download on any row to add it directly to <strong>{destination.split("/").pop() ?? destination}</strong>.</p>}</>}
      {tab === "yt-csv" && <><p>Preview metadata before downloading YouTube URLs or an Exportify CSV.</p><form onSubmit={(e) => { e.preventDefault(); onPreview?.(query.trim(), previewKind); }}><select aria-label="Preview source" value={previewKind} onChange={(e) => setPreviewKind(e.target.value as "youtube" | "csv")} style={{ ...inputStyle, marginBottom: 8 }}><option value="youtube">YouTube</option><option value="csv">CSV</option></select><input aria-label="YouTube search or CSV file" autoCorrect="off" spellCheck={false} value={query} onChange={(e) => setQuery(e.target.value)} style={inputStyle} /><button type="submit" style={{ ...buttonStyle, marginTop: 10 }}>Preview</button></form>{previewTracks.map((track) => <label key={track.id} style={{ display: "block", padding: 10 }}><input type="checkbox" checked={selected.includes(track.id)} onChange={() => toggle(track.id)} /> {track.title}{track.artist ? ` — ${track.artist}` : ""}</label>)}{previewTracks.length > 0 && <button type="button" onClick={() => onDownload?.(selected.length ? selected : previewTracks.map((item) => item.id), destination)} style={buttonStyle}>Download previewed tracks</button>}</>}
      {tab === "direct-url" && <form onSubmit={(e) => { e.preventDefault(); if (validUrl(url)) onDirectUrl?.(url.trim(), destination); }}><label htmlFor="direct-url">Audio URL</label><input id="direct-url" type="url" required pattern="https?://.*" autoCorrect="off" spellCheck={false} value={url} onChange={(e) => setUrl(e.target.value)} style={inputStyle} /><Destination items={destinations} value={destination} onChange={setDestination} onCreate={onCreateDestination} /><button type="submit" style={{ ...buttonStyle, marginTop: 12 }}>Download URL</button></form>}
      {tab === "sync" && <><Destination items={destinations} value={destination} onChange={setDestination} onCreate={onCreateDestination} />{selectedSyncPlaylist
        ? <button type="button" aria-label={`Sync ${selectedSyncPlaylist.name}`} disabled={syncBusy} onClick={() => onSync?.(selectedSyncPlaylist)} style={{ ...buttonStyle, display: "block", marginTop: 10, opacity: syncBusy ? 0.6 : 1 }}>{syncBusy ? "Sync in progress…" : `Sync ${selectedSyncPlaylist.name}`}</button>
        : <p role="status" style={{ color: "var(--luno-text-2)", marginTop: 12 }}>This folder has no linked YouTube playlist. Right-click it in Your Library and choose Link playlist to add a URL.</p>}</>}
      {tab === "url-queue" && <><Destination items={destinations} value={destination} onChange={setDestination} onCreate={onCreateDestination} /><form onSubmit={(e) => { e.preventDefault(); if (validUrl(url)) { onQueueUrl?.(url.trim()); setUrl(""); } }}><input aria-label="Queue URL" type="url" required pattern="https?://.*" autoCorrect="off" spellCheck={false} value={url} onChange={(e) => setUrl(e.target.value)} style={inputStyle} /><button type="submit" style={{ ...buttonStyle, marginTop: 10 }}>Add to queue</button></form><ol>{queue.map((item, index) => <li key={`${item}-${index}`}>{item}</li>)}</ol>{queue.length > 0 && <button type="button" onClick={() => onDownloadQueue?.(destination)} style={buttonStyle}>Download queued URLs ({queue.length})</button>}</>}
      {tab === "duplicates" && <>{duplicateGroups.map((group) => <fieldset key={group.key}><legend>{group.key}</legend>{group.tracks.map((track) => <div key={track}>{track}</div>)}<button type="button" onClick={() => onAllowDuplicates?.([group.key])}>Allow this group</button></fieldset>)}</>}
      {tab === "allowed-dups" && <><button type="button" onClick={onExportAllowed} style={buttonStyle}>Export</button><label style={{ ...buttonStyle, marginLeft: 8 }}>Import<input type="file" accept="application/json" hidden onChange={(e) => { const file = e.target.files?.[0]; if (file) onImportAllowed?.(file); }} /></label><ul>{allowedDuplicates.map((item) => <li key={item}>{item}</li>)}</ul></>}
    </DownloadTabs><div style={{ marginTop: 20 }}>{accordion("progress", "Progress", <div>{Object.values(jobs).map((job) => <div key={job.id}>{job.kind === "sync" ? "playlist sync" : job.kind}: {job.state}{job.progress != null ? ` — ${Math.round(job.progress * 100)}%` : ""}{job.state === "failed" && job.kind !== "sync" && <button type="button" onClick={() => { if (onRetry) onRetry(job.id); else void bridge.retryDownload(job.id); }} style={{ ...buttonStyle, marginLeft: 8 }}>Retry</button>}</div>)}</div>)}{accordion("log", "Download log", <p role="log">Download activity will appear here.</p>)}{accordion("downloaded", "Downloaded songs", <p>Your downloaded songs appear in Unsorted after refresh.</p>)}</div>{children}</section>;
}

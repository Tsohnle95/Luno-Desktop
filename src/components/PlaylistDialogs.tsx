import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { YTDLP_COOKIE_BROWSERS, type PlaylistSyncCandidate, type PlaylistSyncIssue, type PlaylistSyncLocalMatch, type YtDlpCookieBrowser } from "../shared/contracts";

export type PlaylistAction = "create" | "delete" | "empty" | "dump" | "copy" | "smart";

export interface PlaylistOption {
  id: string;
  name: string;
  protected?: boolean;
  trackCount?: number;
}

export interface PlaylistDialogProps {
  action: PlaylistAction;
  playlist?: PlaylistOption;
  playlists?: readonly PlaylistOption[];
  busy?: boolean;
  error?: string | null;
  onClose: () => void;
  onSubmit: (value: { name?: string; destinationId?: string; confirmed: boolean }) => void;
}

const actionCopy: Record<PlaylistAction, { title: string; submit: string }> = {
  create: { title: "Create playlist", submit: "Create playlist" },
  delete: { title: "Delete playlist", submit: "Delete playlist" },
  empty: { title: "Empty playlist", submit: "Empty playlist" },
  dump: { title: "Dump playlist", submit: "Dump playlist" },
  copy: { title: "Copy tracks", submit: "Copy tracks" },
  smart: { title: "Smart playlist", submit: "Create smart playlist" },
};

function DialogFrame({ title, description, error, busy, focusFirstInput = false, onClose, children }: {
  title: string; description?: string; error?: string | null; busy?: boolean; focusFirstInput?: boolean;
  onClose: () => void; children: ReactNode;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  useEffect(() => {
    const firstInput = focusFirstInput ? dialogRef.current?.querySelector<HTMLElement>("input, select, textarea") : null;
    (firstInput ?? closeRef.current)?.focus();
  }, [focusFirstInput]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape" && !busy) onClose(); };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);
  return <div className="luno-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
    <section ref={dialogRef} className="luno-dialog" role="dialog" aria-modal="true" aria-labelledby="luno-dialog-title">
      <header><h2 id="luno-dialog-title">{title}</h2><button ref={closeRef} type="button" aria-label="Close dialog" onClick={onClose} disabled={busy}>×</button></header>
      {description && <p>{description}</p>}
      {error && <p role="alert" className="luno-dialog-error">{error}</p>}
      {children}
    </section>
  </div>;
}

export function PlaylistDialog({ action, playlist, playlists = [], busy = false, error = null, onClose, onSubmit }: PlaylistDialogProps) {
  const [name, setName] = useState(playlist?.name ?? "");
  const [destinationId, setDestinationId] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const copy = actionCopy[action];
  const protectedRoot = playlist?.protected || playlist?.name === "All Music";
  const destructive = action === "delete" || action === "empty";
  const needsDestination = action === "dump" || action === "copy";
  const validName = action !== "create" && action !== "smart" || name.trim().length > 0;
  const validDestination = !needsDestination || destinationId.length > 0;
  const requiresConfirmation = destructive || action === "dump";
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!validName || !validDestination || (requiresConfirmation && !confirmed) || protectedRoot) return;
    onSubmit({ name: name.trim() || undefined, destinationId: destinationId || undefined, confirmed });
  };
  return <DialogFrame title={copy.title} error={error} busy={busy} onClose={onClose}>
    <form onSubmit={submit}>
      {(action === "create" || action === "smart") && <label>Playlist name<input autoFocus autoCorrect="off" spellCheck={false} value={name} onChange={(event) => setName(event.target.value)} maxLength={200} required /></label>}
      {playlist && <p><strong>{playlist.name}</strong>{playlist.trackCount !== undefined && ` · ${playlist.trackCount} tracks`}</p>}
      {protectedRoot && <p role="alert">“{playlist?.name}” is a protected library root and cannot be {action}d.</p>}
      {action === "delete" && !protectedRoot && <p>This removes the playlist and its tracks from the library. This action cannot be undone.</p>}
      {action === "empty" && !protectedRoot && <p>This removes all tracks from this playlist. Audio files are not deleted.</p>}
      {action === "dump" && <p>Copy all tracks to the selected destination. The source playlist will not be deleted.</p>}
      {action === "copy" && <p>Copy selected tracks to one destination. Existing files are never overwritten.</p>}
      {needsDestination && <label>Destination playlist<select value={destinationId} onChange={(event) => setDestinationId(event.target.value)} required><option value="">Choose a playlist…</option>{playlists.filter((item) => item.id !== playlist?.id && !item.protected).map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>}
      {requiresConfirmation && !protectedRoot && <label className="luno-dialog-check"><input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} /> I understand this explicit action and want to continue.</label>}
      <footer><button type="button" onClick={onClose} disabled={busy}>Cancel</button><button type="submit" disabled={busy || !validName || !validDestination || (requiresConfirmation && !confirmed) || protectedRoot}>{busy ? "Working…" : copy.submit}</button></footer>
    </form>
  </DialogFrame>;
}

export const PlaylistDialogs = PlaylistDialog;

export interface PlaylistSyncLogEntry {
  title: string;
  video_id: string | null;
  source_video_id?: string;
  at_ms: number;
  status: string;
  reason: string | null;
}

export function PlaylistSyncLogDialog({ playlistName, entries, loading, error, onClose }: {
  playlistName: string;
  entries: PlaylistSyncLogEntry[];
  loading: boolean;
  error: string | null;
  onClose: () => void;
}) {
  return <DialogFrame title={`Sync log · ${playlistName}`} error={error} onClose={onClose}>
    {loading ? <p>Loading sync log…</p> : entries.filter((entry) => entry.status === "downloaded").length === 0 ? <p>No songs have been synced yet.</p> : <div className="luno-sync-log">
      {entries.filter((entry) => entry.status === "downloaded").map((entry, index) => <div key={`${entry.at_ms}-${entry.video_id ?? index}-${index}`}>
        <strong>{entry.title}</strong>
        <span>{new Date(entry.at_ms).toLocaleString()}</span>
      </div>)}
    </div>}
    <footer><button type="button" onClick={onClose}>Close</button></footer>
  </DialogFrame>;
}

export function PlaylistSyncErrorsDialog({ playlistName, entries, loading, error, onClose }: {
  playlistName: string;
  entries: PlaylistSyncLogEntry[];
  loading: boolean;
  error: string | null;
  onClose: () => void;
}) {
  return <DialogFrame title={`Sync errors · ${playlistName}`} error={error} onClose={onClose}>
    {loading ? <p>Loading sync errors…</p> : entries.length === 0 ? <p>No failed or unavailable song attempts were recorded for this sync.</p> : <div className="luno-sync-log">
      {entries.map((entry, index) => <div key={`${entry.at_ms}-${entry.video_id ?? index}-${index}`}>
        <strong>{entry.title}</strong>
        <span>{new Date(entry.at_ms).toLocaleString()}</span>
        {entry.reason && <small>{entry.reason}</small>}
        {entry.source_video_id && entry.source_video_id !== entry.video_id && <small>Tried replacement source {entry.source_video_id}</small>}
      </div>)}
    </div>}
    <footer><button type="button" onClick={onClose}>Close</button></footer>
  </DialogFrame>;
}

export interface PlaylistSyncCandidateLookup {
  candidates: PlaylistSyncCandidate[];
  loading: boolean;
  error: string | null;
}

export function PlaylistSyncReviewDialog({ playlistName, entries, loading, error, candidateLookups = {}, selectingVideoId, downloadingVideoId, syncing = false, onFindCandidates, onChooseCandidate, onRetryOriginal, onSyncSelected, onResolveWithLocalMatch, onDismissIssue, onClose }: {
  playlistName: string;
  entries: PlaylistSyncIssue[];
  loading: boolean;
  error: string | null;
  candidateLookups?: Record<string, PlaylistSyncCandidateLookup>;
  selectingVideoId?: string | null;
  downloadingVideoId?: string | null;
  syncing?: boolean;
  onFindCandidates: (videoId: string) => void;
  onChooseCandidate: (videoId: string, candidate: PlaylistSyncCandidate) => void;
  onRetryOriginal: (videoId: string) => void;
  onSyncSelected: () => void;
  onResolveWithLocalMatch: (videoId: string, match: PlaylistSyncLocalMatch) => void;
  onDismissIssue: (videoId: string) => void;
  onClose: () => void;
}) {
  const selectedCount = entries.filter((issue) => issue.selected_source_id && issue.retry_pending).length;
  const busy = syncing || !!selectingVideoId || !!downloadingVideoId;
  return <DialogFrame title={`Review unavailable songs · ${playlistName}`} description="Use a matching song already in your library, ignore an entry, or search YouTube for another upload." error={error} busy={busy} onClose={onClose}>
    {loading ? <p>Loading unresolved songs…</p> : entries.length === 0 ? <p>No songs currently need source review.</p> : <div className="luno-sync-log">
      {entries.map((issue) => {
        const lookup = candidateLookups[issue.video_id];
        const duration = issue.duration_secs === null ? null : `${Math.floor(issue.duration_secs / 60)}:${String(issue.duration_secs % 60).padStart(2, "0")}`;
        return <section key={issue.video_id} className="luno-sync-review-entry">
          <strong>{issue.title}</strong>
          <span>{[issue.channel, duration ? `${duration} original length` : null, "original video unavailable"].filter(Boolean).join(" · ")}</span>
          {issue.selected_source_id && <small>Selected source: {issue.selected_source_id}{issue.retry_pending ? " · ready for next sync" : " · last attempt finished; select again to retry or change"}</small>}
          {issue.attempted_source_id && issue.attempted_source_id !== issue.video_id && <small>Last unavailable attempt: {issue.attempted_source_id}</small>}
          <p>{issue.reason}</p>
          {issue.local_matches.length > 0 && <div className="luno-sync-local-matches">
            <strong>Possible matches already in your library</strong>
            <p>Song titles match; artist or uploader names may differ. Choosing the right recording stops this entry returning during sync, with no YouTube playlist edit or extra download.</p>
            {issue.local_matches.map((match) => {
              const matchDuration = match.duration_ms === null ? null : `${Math.floor(match.duration_ms / 60_000)}:${String(Math.floor(match.duration_ms / 1000) % 60).padStart(2, "0")}`;
              return <div key={`${issue.video_id}-${match.track_id}`} className="luno-sync-candidate">
                <div><strong>{match.title}</strong><span>{[match.artist, matchDuration, match.playlists.length ? `In ${match.playlists.join(", ")}` : "In your library"].filter(Boolean).join(" · ")}</span></div>
                <button type="button" onClick={() => onResolveWithLocalMatch(issue.video_id, match)} disabled={busy}>{selectingVideoId === issue.video_id ? "Saving…" : "Use this song"}</button>
              </div>;
            })}
          </div>}
          <button type="button" onClick={() => onDismissIssue(issue.video_id)} disabled={busy}>{selectingVideoId === issue.video_id ? "Saving…" : "Dismiss this unavailable song"}</button>
          <button type="button" onClick={() => onRetryOriginal(issue.video_id)} disabled={busy}>{selectingVideoId === issue.video_id ? "Saving source…" : "Retry original source"}</button>
          <button type="button" onClick={() => onFindCandidates(issue.video_id)} disabled={lookup?.loading || busy}>{lookup?.loading ? "Searching…" : lookup ? "Search again" : "Search available versions"}</button>
          {lookup?.error && <p role="alert" className="luno-dialog-error">{lookup.error}</p>}
          {lookup && !lookup.loading && !lookup.error && lookup.candidates.length === 0 && <p>No concrete YouTube watch videos were returned. Try searching again later.</p>}
          {lookup?.candidates.map((candidate) => <div key={`${issue.video_id}-${candidate.video_id}`} className="luno-sync-candidate">
            <div><strong>{candidate.title}</strong><span>{[candidate.channel, candidate.duration].filter(Boolean).join(" · ") || "Channel and duration unavailable"}</span><a href={candidate.url} target="_blank" rel="noreferrer">Open candidate on YouTube</a></div>
            <button type="button" onClick={() => onChooseCandidate(issue.video_id, candidate)} disabled={busy}>{downloadingVideoId === issue.video_id ? "Downloading to playlist…" : candidate.video_id === issue.selected_source_id ? "Download this version again" : "Download this version to playlist"}</button>
          </div>)}
        </section>;
      })}
    </div>}
    <footer>{selectedCount > 0 && <button type="button" onClick={onSyncSelected} disabled={busy}>Sync selected sources ({selectedCount})</button>}<button type="button" onClick={onClose} disabled={busy}>Close</button></footer>
  </DialogFrame>;
}

export function PlaylistUrlDialog({ playlistName, initialUrl, busy = false, error = null, onClose, onSubmit }: {
  playlistName: string;
  initialUrl?: string | null;
  busy?: boolean;
  error?: string | null;
  onClose: () => void;
  onSubmit: (url: string | null) => void;
}) {
  const [url, setUrl] = useState(initialUrl ?? "");
  const normalized = url.trim();
  return <DialogFrame title="YouTube playlist sync" description={`Link “${playlistName}” to a YouTube playlist. Sync downloads only songs Luno has not already seen.`} error={error} busy={busy} focusFirstInput onClose={onClose}>
    <form onSubmit={(event) => { event.preventDefault(); if (normalized) onSubmit(normalized); }}>
      <label>YouTube playlist URL<input autoFocus type="url" inputMode="url" autoCapitalize="none" autoCorrect="off" spellCheck={false} placeholder="https://www.youtube.com/playlist?list=…" value={url} onChange={(event) => setUrl(event.target.value)} required /></label>
      <footer>
        {initialUrl && <button className="is-danger" type="button" disabled={busy} onClick={() => onSubmit(null)}>Remove link</button>}
        <button type="button" onClick={onClose} disabled={busy}>Cancel</button>
        <button type="submit" disabled={busy || !normalized}>{busy ? "Saving…" : "Save link"}</button>
      </footer>
    </form>
  </DialogFrame>;
}

const cookieBrowserLabels: Record<Exclude<YtDlpCookieBrowser, "none">, string> = {
  brave: "Brave",
  chrome: "Google Chrome",
  chromium: "Chromium",
  edge: "Microsoft Edge",
  firefox: "Firefox",
  opera: "Opera",
  safari: "Safari",
  vivaldi: "Vivaldi",
  whale: "Naver Whale",
};

export function PlaylistSyncAuthDialog({ playlistName, initialBrowser = "none", busy = false, error = null, onClose, onSubmit }: {
  playlistName: string; initialBrowser?: YtDlpCookieBrowser; busy?: boolean; error?: string | null;
  onClose: () => void; onSubmit: (browser: Exclude<YtDlpCookieBrowser, "none">) => void;
}) {
  const [browser, setBrowser] = useState<YtDlpCookieBrowser>(initialBrowser);
  return <DialogFrame title="Sign in to sync this playlist" description={`“${playlistName}” needs a signed-in YouTube session. Choose a browser signed in to the account that owns this playlist.`} error={error} busy={busy} focusFirstInput onClose={onClose}>
    <p style={{ color: "var(--luno-text-2)", lineHeight: 1.5 }}>Luno uses yt-dlp to read that browser’s cookies only while looking up the playlist. Luno saves the browser choice, not a cookie file.</p>
    <form onSubmit={(event) => { event.preventDefault(); if (browser !== "none") onSubmit(browser); }}>
      <label htmlFor="playlist-sync-browser">Signed-in browser</label>
      <select id="playlist-sync-browser" autoFocus value={browser} onChange={(event) => setBrowser(event.target.value as YtDlpCookieBrowser)} disabled={busy} style={{ width: "100%", marginTop: 8, padding: "11px 12px", color: "var(--luno-text)", background: "var(--luno-bg-3)", border: "1px solid var(--luno-border)", borderRadius: "var(--luno-radius)" }}>
        <option value="none">Choose a browser</option>
        {YTDLP_COOKIE_BROWSERS.map((item) => <option key={item} value={item}>{cookieBrowserLabels[item]}</option>)}
      </select>
      <footer>
        <button type="button" onClick={onClose} disabled={busy}>Cancel</button>
        <button type="submit" disabled={busy || browser === "none"}>{busy ? "Saving and syncing…" : "Save browser and sync"}</button>
      </footer>
    </form>
  </DialogFrame>;
}

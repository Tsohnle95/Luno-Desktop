import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";

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

function DialogFrame({ title, description, error, busy, onClose, children }: {
  title: string; description?: string; error?: string | null; busy?: boolean;
  onClose: () => void; children: ReactNode;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape" && !busy) onClose(); };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);
  return <div className="luno-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
    <section className="luno-dialog" role="dialog" aria-modal="true" aria-labelledby="luno-dialog-title">
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
    {loading ? <p>Loading sync errors…</p> : entries.length === 0 ? <p>No failed song attempts were recorded for this sync.</p> : <div className="luno-sync-log">
      {entries.map((entry, index) => <div key={`${entry.at_ms}-${entry.video_id ?? index}-${index}`}>
        <strong>{entry.title}</strong>
        <span>{new Date(entry.at_ms).toLocaleString()}</span>
        {entry.reason && <small>{entry.reason}</small>}
      </div>)}
    </div>}
    <footer><button type="button" onClick={onClose}>Close</button></footer>
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
  return <DialogFrame title="YouTube playlist sync" description={`Link “${playlistName}” to a YouTube playlist. Sync downloads only songs Luno has not already seen.`} error={error} busy={busy} onClose={onClose}>
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

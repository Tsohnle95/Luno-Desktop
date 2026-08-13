import { useEffect, useRef, useState, type ReactNode } from "react";

export interface TransferTrack { ref: string; title: string; artist?: string; album?: string; duration_ms?: number; source?: { provider: string; id: string } | null; ambiguity_confirmation_required?: boolean; }
export interface TransferPlaylist { name: string; track_refs: string[]; }
export interface ImportPlan { matched: Record<string, string>; missing: readonly TransferTrack[]; ambiguous?: readonly TransferTrack[]; playlists?: readonly TransferPlaylist[]; errors?: readonly string[]; }
export interface TransferDialogProps { onClose: () => void; busy?: boolean; error?: string | null; }

function Frame({ title, onClose, busy = false, error, children }: TransferDialogProps & { title: string; children: ReactNode }) {
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => { ref.current?.focus(); const handler = (event: KeyboardEvent) => { if (event.key === "Escape" && !busy) onClose(); }; document.addEventListener("keydown", handler); return () => document.removeEventListener("keydown", handler); }, [busy, onClose]);
  return <div className="luno-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}><section className="luno-dialog" role="dialog" aria-modal="true" aria-labelledby="luno-transfer-title"><header><h2 id="luno-transfer-title">{title}</h2><button ref={ref} type="button" aria-label="Close dialog" onClick={onClose} disabled={busy}>×</button></header>{error && <p role="alert" className="luno-dialog-error">{error}</p>}{children}</section></div>;
}

export interface ExportDialogProps extends TransferDialogProps { scopes?: readonly string[]; defaultScope?: string; onExport: (scope: string) => void; }
export function ExportDialog({ scopes = ["selected_tracks", "playlists", "library"], defaultScope = "library", onExport, ...props }: ExportDialogProps) {
  const [scope, setScope] = useState(defaultScope);
  return <Frame title="Export library" {...props}><form onSubmit={(event) => { event.preventDefault(); onExport(scope); }}><p>Exports a path-free <code>luno.library.export</code> v1 manifest. Audio files, local paths, secrets, queue, and history are never included.</p><label>Export scope<select value={scope} onChange={(event) => setScope(event.target.value)}>{scopes.map((item) => <option key={item} value={item}>{item.replace("_", " ")}</option>)}</select></label><footer><button type="button" onClick={props.onClose} disabled={props.busy}>Cancel</button><button type="submit" disabled={props.busy}>{props.busy ? "Exporting…" : "Export manifest"}</button></footer></form></Frame>;
}

export interface ImportDialogProps extends TransferDialogProps { plan: ImportPlan; onImport: (approvedRefs: readonly string[]) => void; onDownloadMissing?: (tracks: readonly TransferTrack[]) => void; }
export function ImportDialog({ plan, onImport, onDownloadMissing, ...props }: ImportDialogProps) {
  const ambiguous = plan.ambiguous ?? plan.missing.filter((track) => track.ambiguity_confirmation_required);
  const [approved, setApproved] = useState<Set<string>>(new Set());
  const toggle = (ref: string) => setApproved((current) => { const next = new Set(current); next.has(ref) ? next.delete(ref) : next.add(ref); return next; });
  return <Frame title="Review library import" {...props}><p>{Object.keys(plan.matched).length} track(s) matched locally. Import is additive: no existing files are deleted or overwritten.</p>{plan.playlists?.length ? <p>{plan.playlists.length} playlist(s) will be merged in their manifest order.</p> : null}{ambiguous.length > 0 && <fieldset><legend>Confirm ambiguous records individually</legend>{ambiguous.map((track) => <label key={track.ref} className="luno-dialog-check"><input type="checkbox" checked={approved.has(track.ref)} onChange={() => toggle(track.ref)} /> {track.artist ? `${track.artist} — ` : ""}{track.title}{track.source ? ` (${track.source.provider})` : ""}</label>)}</fieldset>}{plan.missing.length > 0 && <section aria-labelledby="missing-heading"><h3 id="missing-heading">Missing tracks ({plan.missing.length})</h3><p>Missing tracks remain reported and will not be silently replaced.</p><ul>{plan.missing.map((track) => <li key={track.ref}>{track.artist ? `${track.artist} — ` : ""}{track.title}</li>)}</ul>{onDownloadMissing && <button type="button" onClick={() => onDownloadMissing(plan.missing)} disabled={props.busy}>Hand off missing tracks to Downloader</button>}</section>}{plan.errors?.map((message) => <p key={message} role="alert" className="luno-dialog-error">{message}</p>)}<footer><button type="button" onClick={props.onClose} disabled={props.busy}>Cancel</button><button type="button" onClick={() => onImport([...approved])} disabled={props.busy}>{props.busy ? "Importing…" : "Add matched tracks"}</button></footer></Frame>;
}

export interface AmbiguityDialogProps extends TransferDialogProps { tracks: readonly TransferTrack[]; onConfirm: (refs: readonly string[]) => void; }
export function AmbiguityDialog({ tracks, onConfirm, ...props }: AmbiguityDialogProps) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  return <Frame title="Confirm ambiguous tracks" {...props}><p>Choose each metadata-only record to approve. No record is accepted globally.</p>{tracks.map((track) => <label key={track.ref} className="luno-dialog-check"><input type="checkbox" checked={selected.has(track.ref)} onChange={() => setSelected((old) => { const next = new Set(old); next.has(track.ref) ? next.delete(track.ref) : next.add(track.ref); return next; })} />{track.artist ? `${track.artist} — ` : ""}{track.title}</label>)}<footer><button type="button" onClick={props.onClose} disabled={props.busy}>Cancel</button><button type="button" onClick={() => onConfirm([...selected])} disabled={props.busy || selected.size === 0}>Approve selected</button></footer></Frame>;
}

export const TransferDialogs = ImportDialog;

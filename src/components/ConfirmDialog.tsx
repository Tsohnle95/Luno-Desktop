import { useEffect, useRef, type ReactNode } from "react";

export interface ConfirmDialogProps {
  title: string;
  message: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  busy?: boolean;
  error?: string | null;
  onConfirm: () => void;
  onClose: () => void;
}

/**
 * In-app confirmation. Tauri v2's WKWebView does not implement native
 * `window.confirm`, so native calls silently return false and destructive
 * actions (delete track/playlist) never ran.
 */
export function ConfirmDialog({ title, message, confirmLabel, danger = false, busy = false, error = null, onConfirm, onClose }: ConfirmDialogProps) {
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape" && !busy) onClose(); };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);
  return <div className="luno-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
    <section className="luno-dialog" role="dialog" aria-modal="true" aria-labelledby="luno-confirm-title">
      <header><h2 id="luno-confirm-title">{title}</h2><button ref={closeRef} type="button" aria-label="Close dialog" onClick={onClose} disabled={busy}>×</button></header>
      <p>{message}</p>
      {error && <p role="alert" className="luno-dialog-error">{error}</p>}
      <footer>
        <button type="button" onClick={onClose} disabled={busy}>Cancel</button>
        <button type="button" className={danger ? "is-danger" : undefined} onClick={onConfirm} disabled={busy}>{busy ? "Working…" : confirmLabel}</button>
      </footer>
    </section>
  </div>;
}

export function FinalCopyDeleteDialog({ files, verificationIncomplete = false, busy = false, error = null, onConfirm, onClose }: {
  files: string[];
  verificationIncomplete?: boolean;
  busy?: boolean;
  error?: string | null;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape" && !busy) onClose(); };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);
  return <div className="luno-dialog-backdrop luno-final-copy-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
    <section className="luno-dialog luno-final-copy-dialog" role="alertdialog" aria-modal="true" aria-labelledby="luno-final-copy-title" aria-describedby="luno-final-copy-description">
      <header><h2 id="luno-final-copy-title">Delete the last copy?</h2><button ref={closeRef} type="button" aria-label="Close dialog" onClick={onClose} disabled={busy}>×</button></header>
      <p id="luno-final-copy-description">{verificationIncomplete
        ? "Luno could not check every folder. Other copies may exist; these files will be permanently deleted."
        : "No identical copies remain in your music folder. These files will be permanently deleted."}</p>
      <ul className="luno-final-copy-files">{files.map((file, index) => <li key={`${file}-${index}`}>{file}</li>)}</ul>
      {error && <p role="alert" className="luno-dialog-error">{error}</p>}
      <footer>
        <button type="button" onClick={onClose} disabled={busy}>Cancel</button>
        <button type="button" className="is-danger" onClick={onConfirm} disabled={busy}>{busy ? "Deleting…" : "Delete permanently"}</button>
      </footer>
    </section>
  </div>;
}

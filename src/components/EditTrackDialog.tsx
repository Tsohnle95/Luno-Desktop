import { useEffect, useRef, useState, type FormEvent } from "react";

export interface EditTrackMetadata {
  artist: string;
  title: string;
}

export interface EditTrackDialogProps {
  track: { title: string; artist: string | null };
  busy?: boolean;
  error?: string | null;
  onClose: () => void;
  onSubmit: (value: EditTrackMetadata) => void;
}

/**
 * Corrects a track's artist/title. The desktop scanner derives both from the
 * filename (`Artist - Title.ext`), so submitting renames the library file.
 */
export function EditTrackDialog({ track, busy = false, error = null, onClose, onSubmit }: EditTrackDialogProps) {
  const [artist, setArtist] = useState(track.artist ?? "");
  const [title, setTitle] = useState(track.title);
  const artistRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    artistRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape" && !busy) onClose(); };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);
  const trimmedTitle = title.trim();
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (busy || trimmedTitle.length === 0) return;
    onSubmit({ artist: artist.trim(), title: trimmedTitle });
  };
  return <div className="luno-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
    <section className="luno-dialog" role="dialog" aria-modal="true" aria-labelledby="luno-edit-track-title">
      <header><h2 id="luno-edit-track-title">Edit song info</h2><button type="button" aria-label="Close dialog" onClick={onClose} disabled={busy}>×</button></header>
      <p>Luno reads artist and title from the file name; saving renames the file in your library.</p>
      {error && <p role="alert" className="luno-dialog-error">{error}</p>}
      <form onSubmit={submit}>
        <label>Artist<input ref={artistRef} autoCorrect="off" spellCheck={false} value={artist} onChange={(event) => setArtist(event.target.value)} maxLength={200} placeholder="Unknown artist" /></label>
        <label>Title<input autoCorrect="off" spellCheck={false} value={title} onChange={(event) => setTitle(event.target.value)} maxLength={200} required /></label>
        <footer><button type="button" onClick={onClose} disabled={busy}>Cancel</button><button type="submit" disabled={busy || trimmedTitle.length === 0}>{busy ? "Saving…" : "Save"}</button></footer>
      </form>
    </section>
  </div>;
}

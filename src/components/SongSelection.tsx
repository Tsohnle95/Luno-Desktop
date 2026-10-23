import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { Track } from '../shared/contracts';
import { SongHoldGesture, toggleSongSelection } from '../state/song_selection';
import '../styles/song_selection.css';

export type SongBatchAction = 'queue' | 'playlist' | 'new-playlist' | 'favorites' | 'delete';
const SelectionContext = createContext<{ active: boolean; ids: ReadonlySet<string>; available: ReadonlySet<string>; toggle: (id: string) => void } | null>(null);
export function SongSelectCheckbox({ track }: { track: Pick<Track, 'id' | 'title'> }) {
  const selection = useContext(SelectionContext);
  if (!selection?.active || !selection.available.has(track.id)) return null;
  return <input className="luno-song-checkbox" type="checkbox" aria-label={`Select ${track.title}`} checked={selection.ids.has(track.id)} onClick={(event) => event.stopPropagation()} onKeyDown={(event) => event.stopPropagation()} onChange={() => selection.toggle(track.id)} />;
}
export function SongSelectionActions({ count, busy = false, unavailable = false, onAction, onClear, onDone }: {
  count: number; busy?: boolean; unavailable?: boolean; onAction: (action: SongBatchAction) => void; onClear: () => void; onDone: () => void;
}) {
  return <section className="luno-song-selection-actions" aria-label="Selected songs"><div className="luno-song-selection-heading"><strong role="status">{count} selected</strong><button type="button" onClick={onClear} disabled={busy || !count}>Deselect all</button><button type="button" onClick={onDone} disabled={busy}>Done</button></div>
    <div role="menu" aria-label="Song selection actions">{([['queue', 'Add to queue'], ['playlist', 'Add to playlist…'], ['new-playlist', 'Add to new playlist…'], ['favorites', 'Add to favorites'], ['delete', 'Delete from library…']] as const).map(([action, label]) => <button key={action} role="menuitem" className={action === 'delete' ? 'is-danger' : undefined} type="button" disabled={busy || unavailable || !count} onClick={() => onAction(action)}>{label}</button>)}</div>
  </section>;
}
function rowFor(target: EventTarget | null): HTMLElement | null { return target instanceof Element ? target.closest<HTMLElement>('[data-track-id]') : null; }
function nestedControl(target: EventTarget | null, row: HTMLElement) {
  const control = target instanceof Element ? target.closest('button,input,a,select,textarea,[data-song-control]') : null;
  return !!control && control !== row;
}
export function SongSelectionSurface({ tracks, available = true, onAction, children }: {
  tracks: Track[]; available?: boolean; onAction: (action: SongBatchAction, tracks: Track[]) => void | Promise<unknown>; children: ReactNode;
}) {
  const [active, setActive] = useState(false);
  const activeRef = useRef(false);
  const [ids, setIds] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const gesture = useRef(new SongHoldGesture());
  const trackMap = useMemo(() => new Map(tracks.map((track) => [track.id, track])), [tracks]);
  const liveTracks = useRef(trackMap); liveTracks.current = trackMap;
  const enabled = useRef(available); enabled.current = available;
  const availableIds = useMemo(() => new Set(trackMap.keys()), [trackMap]);
  const selected = useMemo(() => tracks.filter((track) => ids.has(track.id)), [tracks, ids]);
  const toggle = (id: string) => { if (!busy && trackMap.has(id)) setIds((current) => toggleSongSelection(current, id)); };
  const done = () => { gesture.current.cancel(); activeRef.current = false; setActive(false); setIds(new Set()); setError(null); };
  useEffect(() => {
    const end = () => gesture.current.end();
    const cancel = () => gesture.current.cancel();
    window.addEventListener('pointerup', end); window.addEventListener('pointercancel', cancel); window.addEventListener('blur', cancel); window.addEventListener('scroll', end, true);
    return () => { cancel(); window.removeEventListener('pointerup', end); window.removeEventListener('pointercancel', cancel); window.removeEventListener('blur', cancel); window.removeEventListener('scroll', end, true); };
  }, []);
  useEffect(() => { gesture.current.end(); setIds((current) => new Set([...current].filter((id) => trackMap.has(id)))); }, [trackMap]);
  const begin = (id: string) => { if (!enabled.current || !liveTracks.current.has(id)) return; activeRef.current = true; setActive(true); setIds((current) => new Set(current).add(id)); setError(null); };
  return <SelectionContext.Provider value={{ active, ids, available: availableIds, toggle }}><div className="luno-song-selection-surface" data-selection-mode={active ? 'true' : 'false'}
    onPointerDownCapture={(event) => { gesture.current.cancel(); const row = rowFor(event.target); const id = row?.dataset.trackId; if (busy || !available || event.button !== 0 || !event.isPrimary || !row || !id || !trackMap.has(id) || nestedControl(event.target, row)) return; gesture.current.begin(id, event.pointerId, event.clientX, event.clientY, begin); }}
    onPointerMoveCapture={(event) => gesture.current.move(event.pointerId, event.clientX, event.clientY)} onPointerUpCapture={() => gesture.current.end()} onPointerLeave={() => gesture.current.end()} onPointerCancelCapture={() => gesture.current.cancel()}
    onClickCapture={(event) => { const row = rowFor(event.target); const id = row?.dataset.trackId; if (!row || !id || !trackMap.has(id)) return; if (gesture.current.consumeClick(id)) { event.preventDefault(); event.stopPropagation(); return; } if (activeRef.current && !nestedControl(event.target, row)) { event.preventDefault(); event.stopPropagation(); toggle(id); } }}
    onContextMenuCapture={(event) => { if (activeRef.current && rowFor(event.target)) { event.preventDefault(); event.stopPropagation(); } }}
    onKeyDownCapture={(event) => { if (event.target instanceof Element && event.target.closest('[role=dialog],[role=alertdialog]')) return; if (event.key === 'Escape' && activeRef.current && !busy) { event.preventDefault(); event.stopPropagation(); done(); return; } const row = rowFor(event.target); const id = row?.dataset.trackId; if (!row || !id || nestedControl(event.target, row)) return; if (event.key === ' ' && event.shiftKey) { event.preventDefault(); event.stopPropagation(); begin(id); } else if (activeRef.current && (event.key === ' ' || event.key === 'Enter')) { event.preventDefault(); event.stopPropagation(); toggle(id); } }}>
    {children}
    {active && <><SongSelectionActions count={selected.length} busy={busy} unavailable={!available} onClear={() => setIds(new Set())} onDone={done} onAction={(action) => { if (busy || !available || !selected.length) return; setBusy(true); setError(null); void Promise.resolve().then(() => onAction(action, selected)).catch((cause: unknown) => setError(cause instanceof Error ? cause.message : 'This action could not be completed. Try again.')).finally(() => setBusy(false)); }} />{error && <p className="luno-song-selection-error" role="alert">{error}</p>}</>}
  </div></SelectionContext.Provider>;
}

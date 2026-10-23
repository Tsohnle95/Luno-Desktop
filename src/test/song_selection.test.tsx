// @vitest-environment jsdom
import { readFileSync } from 'node:fs';
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SongSelectionSurface } from '../components/SongSelection';
import { TrackRow } from '../components/TrackRow';
import { DuplicateReview } from '../views/DownloaderView';
import { downloadDuplicateGroups } from '../state/download_duplicates';
import { addSongsToFavorites, trackDeleteConfirmation } from '../state/track_actions';
import type { Track } from '../shared/contracts';

const first: Track = { id: 'first', title: 'First song', artist: 'Artist', path: '/music/A/one.m4a', album: null, artwork_url: null, duration_ms: 1000, favorite: false, provider: null };
const second: Track = { ...first, id: 'second', title: 'Second song', path: '/music/B/two.m4a', favorite: true };
let host: HTMLDivElement;
let root: Root;
const play = vi.fn(); const action = vi.fn(); const favorite = vi.fn();
function render(tracks = [first, second]) {
  act(() => root.render(<SongSelectionSurface tracks={tracks} onAction={action}>{tracks.map((track) => <TrackRow key={track.id} track={track} onClick={play} onFavoriteToggle={favorite} />)}</SongSelectionSurface>));
}
function row(id = 'first') { return host.querySelector<HTMLElement>(`[data-track-id="${id}"]`)!; }
function pointer(target: Element, type: string, x = 20, y = 20) {
  const event = new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, clientX: x, clientY: y });
  Object.defineProperties(event, { pointerId: { value: 1 }, isPrimary: { value: true } });
  act(() => target.dispatchEvent(event));
}
function hold() { pointer(row(), 'pointerdown'); act(() => vi.advanceTimersByTime(500)); }
function button(label: string) { return [...host.querySelectorAll('button')].find((node) => node.textContent === label)!; }
beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks();
  (globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement('div'); document.body.appendChild(host); root = createRoot(host); render();
});
afterEach(() => { act(() => root.unmount()); host.remove(); vi.useRealTimers(); });

describe('long-press song selection', () => {
  it('retains ordinary click playback, but a held release never plays or deselects the first song', () => {
    pointer(row(), 'pointerdown'); act(() => vi.advanceTimersByTime(499));
    expect(host.querySelector('[type=checkbox]')).toBeNull();
    pointer(row(), 'pointerup'); act(() => row().click()); expect(play).toHaveBeenCalledTimes(1);
    play.mockClear(); hold();
    expect(host.querySelectorAll('[type=checkbox]')).toHaveLength(2);
    expect(host.querySelector<HTMLInputElement>('[aria-label="Select First song"]')!.checked).toBe(true);
    pointer(row(), 'pointermove', 40, 40);
    pointer(row(), 'pointerup'); act(() => row().click());
    expect(play).not.toHaveBeenCalled();
    expect(host.querySelector<HTMLInputElement>('[aria-label="Select First song"]')!.checked).toBe(true);
    expect(host.textContent).toContain('1 selected');
  });
  it('selects multiple songs, toggles an individual checkbox, and offers only batch actions', async () => {
    hold(); pointer(row(), 'pointerup'); act(() => row().click());
    act(() => row('second').click());
    expect(host.textContent).toContain('2 selected');
    const checkbox = host.querySelector<HTMLInputElement>('[aria-label="Select First song"]')!;
    act(() => checkbox.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true })));
    act(() => checkbox.click());
    expect(checkbox.checked).toBe(false);
    expect(host.textContent).toContain('1 selected');
    expect(host.textContent).not.toContain('Play now'); expect(host.textContent).not.toContain('Edit song'); expect(host.textContent).not.toContain('Play next');
    await act(async () => button('Add to queue').click());
    expect(action).toHaveBeenCalledWith('queue', [second]);
    expect(play).not.toHaveBeenCalled();
    act(() => button('Deselect all').click());
    expect(host.querySelectorAll('input:checked')).toHaveLength(0);
    expect(button('Delete from library…').disabled).toBe(true);
    act(() => button('Done').click()); expect(host.querySelector('[type=checkbox]')).toBeNull();
  });
  it('cancels pending holds when scrolling, moving, or cancelling a pointer', () => {
    pointer(row(), 'pointerdown'); act(() => window.dispatchEvent(new Event('scroll'))); act(() => vi.advanceTimersByTime(600));
    expect(host.querySelector('[type=checkbox]')).toBeNull();
    pointer(row(), 'pointerdown'); pointer(row(), 'pointermove', 50, 50); act(() => vi.advanceTimersByTime(600));
    expect(host.querySelector('[type=checkbox]')).toBeNull();
    pointer(row(), 'pointerdown'); pointer(row(), 'pointercancel'); act(() => vi.advanceTimersByTime(600));
    expect(host.querySelector('[type=checkbox]')).toBeNull();
  });
  it('does not treat a favorite control as a row hold and supports Escape to exit', () => {
    const control = host.querySelector<HTMLButtonElement>('[aria-label="Add First song to favorites"]')!;
    pointer(control, 'pointerdown'); act(() => vi.advanceTimersByTime(600));
    expect(host.querySelector('[type=checkbox]')).toBeNull();
    act(() => control.click()); expect(favorite).toHaveBeenCalledWith(first);
    hold(); act(() => row().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
    expect(host.querySelector('[type=checkbox]')).toBeNull(); expect(play).not.toHaveBeenCalled();
  });
  it('keeps Deselect all and Done usable if the backend disconnects', () => {
    hold(); act(() => root.render(<SongSelectionSurface tracks={[first, second]} available={false} onAction={action}><TrackRow track={first} /></SongSelectionSurface>));
    expect(button('Add to queue').disabled).toBe(true);
    expect(button('Deselect all').disabled).toBe(false);
    expect(button('Done').disabled).toBe(false);
    act(() => button('Deselect all').click()); expect(host.textContent).toContain('0 selected');
    act(() => button('Done').click()); expect(host.querySelector('[type=checkbox]')).toBeNull();
  });
  it('lets modal dialogs handle Escape without clearing the selection behind them', () => {
    hold();
    act(() => root.render(<SongSelectionSurface tracks={[first, second]} onAction={action}><TrackRow track={first} /><div role="dialog"><button>Modal action</button></div></SongSelectionSurface>));
    const dialog = host.querySelector('[role=dialog]')!;
    const escape = vi.fn(); dialog.addEventListener('keydown', escape);
    act(() => dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
    expect(escape).toHaveBeenCalled();
    expect(host.textContent).toContain('1 selected');
  });
  it('drops vanished selected IDs before a batch action can run after a rescan', () => {
    hold(); render([second]);
    expect(host.textContent).toContain('0 selected');
    expect(button('Delete from library…').disabled).toBe(true);
    expect(action).not.toHaveBeenCalled();
  });
});
describe('batch mutation boundaries', () => {
  it('adds favorites without toggling existing favorites off or sending duplicate IDs', async () => {
    const toggle = vi.fn().mockResolvedValue(undefined);
    await addSongsToFavorites([first, first, second], toggle);
    expect(toggle).toHaveBeenCalledExactlyOnceWith('first');
  });
  it('keeps selected deletion behind confirmation and sends captured exact copy IDs', async () => {
    const remove = vi.fn().mockResolvedValue(undefined);
    const request = trackDeleteConfirmation([first, second, first], remove);
    expect(remove).not.toHaveBeenCalled();
    expect(request.title).toBe('Delete 2 file copies');
    await request.onConfirm(); expect(remove).toHaveBeenCalledExactlyOnceWith(['first', 'second']);
  });
});

describe('duplicate review DOM controls', () => {
  it('routes the clicked copy ID and gives Import/Export actual nonzero spacing', () => {
    const copy = { ...second, title: first.title };
    const groups = downloadDuplicateGroups([first, copy], []);
    const onDelete = vi.fn();
    act(() => root.render(<DuplicateReview groups={groups} allowed={[]} onDelete={onDelete} />));
    expect(onDelete).not.toHaveBeenCalled();
    const deleteCopy = host.querySelector<HTMLButtonElement>('[aria-label="Delete First song from B"]')!;
    expect(deleteCopy.disabled).toBe(false);
    act(() => deleteCopy.click()); expect(onDelete).toHaveBeenCalledExactlyOnceWith('second');
    const style = document.createElement('style');
    style.textContent = readFileSync('src/styles/downloader.css', 'utf8');
    document.head.appendChild(style);
    try {
      const controls = host.querySelector('.luno-duplicate-transfer')!;
      const computed = getComputedStyle(controls);
      expect(computed.display).toBe('flex');
      expect(parseFloat(computed.gap)).toBeGreaterThanOrEqual(8);
      expect(controls.textContent).toContain('Export'); expect(controls.textContent).toContain('Import');
    } finally { style.remove(); }
  });
});

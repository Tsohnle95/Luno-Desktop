export const SONG_HOLD_MS = 500;
export function toggleSongSelection(selected: ReadonlySet<string>, id: string): Set<string> {
  const next = new Set(selected);
  if (next.has(id)) next.delete(id); else next.add(id);
  return next;
}
export class SongHoldGesture {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private origin: { id: string; pointer: number; x: number; y: number } | null = null;
  private held: string | null = null;
  begin(id: string, pointer: number, x: number, y: number, onHold: (id: string) => void) {
    this.cancel(); this.held = null;
    this.origin = { id, pointer, x, y };
    this.timer = setTimeout(() => { this.timer = null; this.held = id; onHold(id); }, SONG_HOLD_MS);
  }
  move(pointer: number, x: number, y: number) {
    if (this.origin?.pointer === pointer && Math.hypot(x - this.origin.x, y - this.origin.y) > 8) this.end();
  }
  end() { if (this.timer !== null) clearTimeout(this.timer); this.timer = null; this.origin = null; }
  cancel() { this.end(); this.held = null; }
  consumeClick(id: string) { const held = this.held === id; this.held = null; return held; }
}

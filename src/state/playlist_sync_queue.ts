import type { Playlist } from '../shared/contracts';
import { type QueuedDownload } from './download_inputs';
import { youtubePlaylistUrl } from './download_results';

export interface PlaylistSyncRequest { playlist: Playlist; url?: string }
interface PendingSync extends PlaylistSyncRequest { downloadsBefore: Set<QueuedDownload> }

// A barrier preserves click order across normal downloads and explicit syncs.
// Request identity keeps retries made after a sync click behind that sync.
// Busy admission reuses the same request and retains its earlier position.
export class PlaylistSyncQueue {
  private pending: PendingSync[] = [];
  private current: PendingSync | null = null;
  paused = false;
  get count() { return this.pending.length + (this.current ? 1 : 0); }
  get active() { return this.current; }
  enqueue(requests: PlaylistSyncRequest[], earlierDownloads: QueuedDownload[]) {
    const resume = this.paused;
    this.paused = false;
    for (const request of requests) {
      if (this.current?.playlist.id === request.playlist.id || this.pending.some((item) => item.playlist.id === request.playlist.id)) continue;
      const next = { ...request, downloadsBefore: new Set(earlierDownloads) };
      if (resume) this.pending.unshift(next); else this.pending.push(next);
    }
  }
  allowsDownload(item: QueuedDownload): boolean {
    return !this.current && (!this.pending.length || this.pending[0].downloadsBefore.has(item));
  }
  take(normalActive: number, nextDownload?: QueuedDownload): PlaylistSyncRequest | null {
    if (this.paused || this.current || normalActive || !this.pending.length || (nextDownload && this.allowsDownload(nextDownload))) return null;
    this.current = this.pending.shift()!;
    return this.current;
  }
  pause() { this.current = null; this.paused = true; }
  resume() { this.paused = false; }
  clear() { this.pending = []; this.current = null; this.paused = false; }
  retry() { if (this.current) this.pending.unshift(this.current); this.current = null; }
  finish() { this.current = null; }
}

export async function preparePlaylistSync(request: PlaylistSyncRequest, bridge: {
  playlistSyncSources: (id: string) => Promise<string[]>;
  setPlaylistUrls: (id: string, urls: string[]) => Promise<unknown>;
}): Promise<Playlist> {
  if (!request.url) return request.playlist;
  const url = youtubePlaylistUrl(request.url);
  if (!url) throw new Error('Enter a YouTube playlist URL containing a playlist ID.');
  const urls = await bridge.playlistSyncSources(request.playlist.id);
  const merged = [...new Set([...urls.map((value) => youtubePlaylistUrl(value) ?? value), url])];
  if (merged.length !== urls.length || merged.some((value, index) => value !== urls[index])) await bridge.setPlaylistUrls(request.playlist.id, merged);
  return { ...request.playlist, url: merged[0] };
}

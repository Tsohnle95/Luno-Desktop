import { describe, expect, it, vi } from 'vitest';
import { PlaylistSyncQueue, preparePlaylistSync } from '../state/playlist_sync_queue';
import { downloadResultPage, youtubePlaylistUrl } from '../state/download_results';
const playlist = (id: string) => ({ id, name: id, path: `/music/${id}`, url: 'https://www.youtube.com/playlist?list=original', ordered_track_ids: [] });
const song = (id: string) => ({ id, source: id, destination: '/music/A' });

describe('explicit playlist sync scheduling', () => {
  it('waits for earlier songs, syncs linked playlists sequentially, then admits later songs', () => {
    const queue = new PlaylistSyncQueue();
    const first = song('first');
    queue.enqueue([{ playlist: playlist('A') }, { playlist: playlist('B') }], [first]);
    expect(queue.take(1)).toBeNull();
    expect(queue.take(0, first)).toBeNull();
    expect(queue.allowsDownload(first)).toBe(true);
    expect(queue.allowsDownload(song('later'))).toBe(false);
    expect(queue.allowsDownload(song('first'))).toBe(false);
    expect(queue.take(0, song('later'))?.playlist.id).toBe('A');
    expect(queue.take(0)).toBeNull();
    expect(queue.allowsDownload(first)).toBe(false);
    queue.finish();
    expect(queue.take(0, song('later'))?.playlist.id).toBe('B');
    queue.finish();
    expect(queue.count).toBe(0);
    expect(queue.allowsDownload(song('later'))).toBe(true);
  });
  it('pauses batch failure for browser recovery and retries that playlist before continuing', () => {
    const queue = new PlaylistSyncQueue();
    queue.enqueue([{ playlist: playlist('A') }, { playlist: playlist('B') }], []);
    queue.take(0);
    queue.pause();
    expect(queue.take(0)).toBeNull();
    queue.enqueue([{ playlist: playlist('A') }], []);
    expect(queue.take(0)?.playlist.id).toBe('A');
    queue.finish();
    expect(queue.take(0)?.playlist.id).toBe('B');
    queue.clear();
    expect(queue.count).toBe(0);
  });
  it('keeps a Busy admission at the front without releasing normal downloads', () => {
    const queue = new PlaylistSyncQueue();
    queue.enqueue([{ playlist: playlist('A') }, { playlist: playlist('B') }], []);
    queue.take(0); queue.retry();
    expect(queue.allowsDownload(song('later'))).toBe(false);
    expect(queue.take(0)?.playlist.id).toBe('A');
  });
  it('does not duplicate a playlist already queued or admitting', () => {
    const queue = new PlaylistSyncQueue();
    queue.enqueue([{ playlist: playlist('A') }], []);
    queue.take(0);
    queue.enqueue([{ playlist: playlist('A') }], []);
    expect(queue.count).toBe(1);
  });
  it('merges a pasted URL before starting and preserves every existing linked source', async () => {
    const bridge = { playlistSyncSources: vi.fn().mockResolvedValue(['https://www.youtube.com/playlist?list=original']), setPlaylistUrls: vi.fn().mockResolvedValue(undefined) };
    const result = await preparePlaylistSync({ playlist: playlist('A'), url: 'https://youtube.com/watch?v=video&list=new' }, bridge);
    expect(bridge.setPlaylistUrls).toHaveBeenCalledWith('A', ['https://www.youtube.com/playlist?list=original', 'https://www.youtube.com/playlist?list=new']);
    expect(result.id).toBe('A');
    expect(result.url).toContain('original');
  });
  it('rejects invalid URLs without mutating existing links', async () => {
    const bridge = { playlistSyncSources: vi.fn(), setPlaylistUrls: vi.fn() };
    await expect(preparePlaylistSync({ playlist: playlist('A'), url: 'https://example.com/?list=bad' }, bridge)).rejects.toThrow('YouTube playlist URL');
    expect(bridge.setPlaylistUrls).not.toHaveBeenCalled();
  });
});
describe('download results pages', () => {
  it('keeps every item available across pages and clamps after a shorter search', () => {
    const items = Array.from({ length: 61 }, (_, id) => id);
    expect([0, 1, 2].flatMap((page) => downloadResultPage(items, page).items)).toEqual(items);
    expect(downloadResultPage(items, 2).items).toHaveLength(11);
    expect(downloadResultPage([1, 2], 2).page).toBe(0);
  });
  it('treats a YouTube radio watch link as one video, not an enumerable playlist', () => {
    expect(youtubePlaylistUrl('https://www.youtube.com/watch?v=DBeXDGM0P5k&list=RDDBeXDGM0P5k&start_radio=1')).toBeNull();
    expect(youtubePlaylistUrl('https://www.youtube.com/watch?v=DBeXDGM0P5k&list=RDDBeXDGM0P5k')).toBeNull();
  });
  it('recognizes playlist links without interpreting ordinary song queries as URLs', () => {
    expect(youtubePlaylistUrl('hello artist')).toBeNull();
    expect(youtubePlaylistUrl('https://youtu.be/id?list=PL1')).toBe('https://www.youtube.com/playlist?list=PL1');
    expect(youtubePlaylistUrl('https://youtube.com/watch?v=id')).toBeNull();
  });
});

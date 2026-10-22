import type { DownloadMetadata } from "./download_naming";

export interface QueuedDownload extends DownloadMetadata { id: string; source: string; destination: string }
export function csvDownloadSource(row: { url?: string; artist?: string; title: string }): string {
  const url = row.url?.trim();
  // Exportify's Track URI is a Spotify reference, not a downloadable source.
  return url && /^https?:\/\//i.test(url) && !/^https?:\/\/(?:open\.)?spotify\.com\//i.test(url)
    ? url : `ytsearch1:${[row.artist, row.title].filter(Boolean).join(" ")}`;
}
export function captureQueuedDownload(id: string, destination: string, row?: { source?: string; url?: string | null; query?: string; title?: string | null; artist?: string | null }): QueuedDownload {
  return { id, destination, source: (row?.source ?? row?.url ?? row?.query ?? id).replace(/[\r\n]/g, " ").trim(), title: row?.title ?? null, artist: row?.artist ?? null };
}

export class DownloadLookupGate {
  private generation = 0;
  async run<T>(load: () => Promise<T>, handlers: { start: () => void; success: (value: T) => void; error: (error: unknown) => void; finish: () => void }): Promise<void> {
    const generation = ++this.generation;
    handlers.start();
    try {
      const value = await load();
      if (generation === this.generation) handlers.success(value);
    } catch (error) {
      if (generation === this.generation) handlers.error(error);
    } finally {
      if (generation === this.generation) handlers.finish();
    }
  }
}

export function requeueDownloadAfterBusy<T>(queue: T[], item: T, startedEpoch: number, currentEpoch: number): boolean {
  if (startedEpoch !== currentEpoch) return false;
  queue.unshift(item);
  return true;
}

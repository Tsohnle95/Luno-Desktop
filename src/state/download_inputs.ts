import type { DownloadMetadata } from "./download_naming";

export interface QueuedDownload extends DownloadMetadata { id: string; source: string; destination: string; thumbnailUrl?: string; duration?: string }
export function csvDownloadSource(row: { url?: string; artist?: string; title: string }): string {
  const url = row.url?.trim();
  // Exportify's Track URI is a Spotify reference, not a downloadable source.
  return url && /^https?:\/\//i.test(url) && !/^https?:\/\/(?:open\.)?spotify\.com\//i.test(url)
    ? url : `ytsearch1:${[row.artist, row.title].filter(Boolean).join(" ")}`;
}
export function captureQueuedDownload(id: string, destination: string, row?: { source?: string; url?: string | null; query?: string; title?: string | null; artist?: string | null; thumbnail_url?: string | null; thumbnailUrl?: string; duration?: string | null }): QueuedDownload {
  return { id, destination, source: (row?.source ?? row?.url ?? row?.query ?? id).replace(/[\r\n]/g, " ").trim(), title: row?.title ?? null, artist: row?.artist ?? null, ...((row?.thumbnail_url ?? row?.thumbnailUrl) ? { thumbnailUrl: (row?.thumbnail_url ?? row?.thumbnailUrl)! } : {}), ...(row?.duration ? { duration: row.duration } : {}) };
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

export function takeNextDownload<T>(queue: T[], activeCount: number): T | undefined {
  return activeCount === 0 ? queue.shift() : undefined;
}


export interface DownloadListItem extends QueuedDownload {
  status: "starting" | "queued" | "running" | "cancelling" | "succeeded" | "failed" | "cancelled";
  jobId?: string;
  position?: number;
  progress?: number | null;
  finalizing?: boolean;
  error?: string;
}
export const downloadItemKey = (item: Pick<QueuedDownload, "source" | "destination">): string => `${item.destination}\u0000${item.source}`;
export function downloadActivityRows(requests: Record<string, QueuedDownload>, pending: QueuedDownload[], jobs: Record<string, import("../shared/contracts").DownloadJob>, errors: Record<string, string> = {}): DownloadListItem[] {
  const rows = new Map<string, DownloadListItem>();
  for (const [jobId, request] of Object.entries(requests)) {
    const job = jobs[jobId];
    rows.set(downloadItemKey(request), { ...request, jobId, status: job?.state ?? "starting", progress: job?.progress, finalizing: job?.phase === "finalizing", error: errors[jobId] });
  }
  pending.forEach((item, index) => rows.set(downloadItemKey(item), { ...item, status: "queued", position: index + 1 }));
  const order = (row: DownloadListItem) => ["starting", "running", "cancelling"].includes(row.status) ? 0 : row.status === "queued" ? 1 : 2;
  return [...rows.values()].sort((a, b) => order(a) - order(b) || (a.status === "queued" && b.status === "queued" ? (a.position ?? 0) - (b.position ?? 0) : 0));
}
export function appendDownloadItems<T extends QueuedDownload>(queue: T[], items: T[], active: DownloadListItem[]): void {
  const existing = new Set([...queue, ...active.filter((row) => !["failed", "cancelled"].includes(row.status))].map(downloadItemKey));
  for (const item of items) {
    const key = downloadItemKey(item);
    if (existing.has(key)) continue;
    existing.add(key);
    queue.push(item);
  }
}

export async function awaitDownloadAdmission(start: Promise<unknown>, cancelled: () => boolean, cancel: () => Promise<unknown>, onCancellationError: (error: unknown) => void = () => {}): Promise<void> {
  await start;
  if (cancelled()) {
    try { await cancel(); } catch (error) { onCancellationError(error); }
  }
}

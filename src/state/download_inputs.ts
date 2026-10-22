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

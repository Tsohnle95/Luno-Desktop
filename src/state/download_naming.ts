/**
 * Download filename/label policy shared by search, direct-URL, and URL-queue
 * downloads. The library scanner derives Artist/Title from the FILE name, so a
 * downloaded file is only recognized when it is named `Artist - Title.ext`;
 * URL downloads without search rows used to fall back to the opaque job id and
 * showed up as "Unknown artist".
 */

export interface DownloadMetadata {
  title?: string | null;
  artist?: string | null;
}

const sanitizeLabel = (value: string) =>
  value.replace(/[\\/:*?"<>|]/g, "").replace(/\s+/g, " ").trim();

/** `Artist - Title.ext`, or `fallback.ext` when no metadata is known. */
export function downloadFilename(metadata: DownloadMetadata, extension: string, fallback: string): string {
  const artist = sanitizeLabel(metadata.artist ?? "");
  const title = sanitizeLabel(metadata.title ?? "");
  const label = [artist, title].filter(Boolean).join(" - ").slice(0, 96);
  return `${label || fallback}.${extension}`;
}

/** Human-readable `Artist — Title`, or null when nothing is known. */
export function downloadDisplayLabel(metadata: DownloadMetadata): string | null {
  const artist = (metadata.artist ?? "").trim();
  const title = (metadata.title ?? "").trim();
  if (!artist && !title) return null;
  return [artist, title].filter(Boolean).join(" — ");
}

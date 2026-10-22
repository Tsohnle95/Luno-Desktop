export const DOWNLOAD_PAGE_SIZE = 25;
export const DOWNLOAD_SEARCH_LIMIT = 250;
export function downloadResultPage<T>(items: T[], requestedPage: number) {
  const pages = Math.max(1, Math.ceil(items.length / DOWNLOAD_PAGE_SIZE));
  const page = Math.max(0, Math.min(requestedPage, pages - 1));
  const start = page * DOWNLOAD_PAGE_SIZE;
  return { page, pages, start, end: Math.min(start + DOWNLOAD_PAGE_SIZE, items.length), items: items.slice(start, start + DOWNLOAD_PAGE_SIZE) };
}
export function youtubePlaylistUrl(value: string): string | null {
  try {
    const url = new URL(value.trim());
    if (!['https:', 'http:'].includes(url.protocol) || !['youtube.com', 'www.youtube.com', 'm.youtube.com', 'music.youtube.com', 'youtu.be', 'www.youtu.be'].includes(url.hostname.toLowerCase())) return null;
    const list = url.searchParams.get('list');
    return list ? `https://www.youtube.com/playlist?list=${encodeURIComponent(list)}` : null;
  } catch { return null; }
}

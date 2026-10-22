export interface PageHistory<T> { entries: T[]; index: number }
export const createPageHistory = <T>(page: T): PageHistory<T> => ({ entries: [page], index: 0 });
export function recordPage<T>(history: PageHistory<T>, page: T): PageHistory<T> {
  if (JSON.stringify(history.entries[history.index]) === JSON.stringify(page)) return history;
  const entries = [...history.entries.slice(0, history.index + 1), page].slice(-100);
  return { entries, index: entries.length - 1 };
}
export function stepPage<T>(history: PageHistory<T>, delta: -1 | 1): PageHistory<T> {
  const index = Math.max(0, Math.min(history.entries.length - 1, history.index + delta));
  return index === history.index ? history : { ...history, index };
}

export function librarySearchScope(scope: string, query: string): string {
  return query && (scope === "home:favorite-artists" || scope === "home:popular-playlists") ? "all" : scope;
}

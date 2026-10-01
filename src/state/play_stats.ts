/**
 * Helpers for legacy listening-count migration and the Home dashboard's
 * "Recently downloaded" section.
 *
 * Listening counts now belong to the Rust backend and its runtime-home file.
 * The old localStorage key is read once and imported for existing installs.
 * Download completions remain a lightweight local journal.
 */

export type PlayCounts = Record<string, number>;

export interface DownloadEntry { trackId: string; at: number }

const PLAY_COUNTS_KEY = "luno.play-counts.v1";
const DOWNLOAD_JOURNAL_KEY = "luno.download-journal.v1";
const JOURNAL_LIMIT = 200;

const hasStorage = (): boolean => typeof window !== "undefined" && !!window.localStorage;

export function loadPlayCounts(): PlayCounts {
  if (!hasStorage()) return {};
  try {
    const raw = window.localStorage.getItem(PLAY_COUNTS_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return {};
    const counts: PlayCounts = {};
    for (const [id, value] of Object.entries(parsed)) {
      if (typeof id === "string" && id && typeof value === "number" && Number.isSafeInteger(value) && value > 0) counts[id] = value;
    }
    return counts;
  } catch {
    return {};
  }
}

export function savePlayCounts(counts: PlayCounts): void {
  if (!hasStorage()) return;
  try { window.localStorage.setItem(PLAY_COUNTS_KEY, JSON.stringify(counts)); } catch { /* storage full or blocked */ }
}

export function clearPlayCounts(): void {
  if (!hasStorage()) return;
  try { window.localStorage.removeItem(PLAY_COUNTS_KEY); } catch { /* storage may be blocked */ }
}

/** Pure increment — returns a NEW counts object so React state updates compose. */
export function bumpPlayCount(counts: PlayCounts, trackId: string): PlayCounts {
  return { ...counts, [trackId]: (counts[trackId] ?? 0) + 1 };
}

/** Track ids ranked by plays (desc), ties broken alphabetically for stable order. */
export function popularTrackIds(counts: PlayCounts, limit: number): string[] {
  return Object.entries(counts)
    .filter(([, plays]) => plays > 0)
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, Math.max(0, limit))
    .map(([id]) => id);
}

export function loadDownloadJournal(): DownloadEntry[] {
  if (!hasStorage()) return [];
  try {
    const raw = window.localStorage.getItem(DOWNLOAD_JOURNAL_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((item): item is DownloadEntry =>
      typeof item === "object" && item !== null && typeof (item as DownloadEntry).trackId === "string" &&
      !!(item as DownloadEntry).trackId && typeof (item as DownloadEntry).at === "number");
  } catch {
    return [];
  }
}

export function saveDownloadJournal(entries: DownloadEntry[]): void {
  if (!hasStorage()) return;
  try { window.localStorage.setItem(DOWNLOAD_JOURNAL_KEY, JSON.stringify(entries.slice(0, JOURNAL_LIMIT))); } catch { /* storage full or blocked */ }
}

/**
 * Merge fresh download completions (sorted newest first), keeping only the
 * newest entry per track and capping the journal length.
 */
export function recordDownloadCompletions(entries: DownloadEntry[], additions: DownloadEntry[]): DownloadEntry[] {
  const orderedAdditions = [...additions].sort((a, b) => b.at - a.at);
  const seen = new Set<string>();
  const merged: DownloadEntry[] = [];
  for (const entry of [...orderedAdditions, ...entries]) {
    if (seen.has(entry.trackId)) continue;
    seen.add(entry.trackId);
    merged.push(entry);
  }
  return merged.slice(0, JOURNAL_LIMIT);
}

/** FNV-1a — stable seed source for the Made-for-you shuffle. */
export function hashString(input: string): number {
  let hash = 2166136261;
  for (let index = 0; index < input.length; index++) {
    hash ^= input.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

/**
 * Deterministic shuffle keyed to catalogue MEMBERSHIP (mirrors the mobile
 * app): re-renders keep the same mix; adding/removing any track reshuffles.
 */
export function madeForYouIds(trackIds: string[], size: number): string[] {
  let seed = hashString([...trackIds].sort().join("\u0000"));
  const random = () => {
    seed = (seed + 0x6D2B79F5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const shuffled = [...trackIds];
  for (let index = shuffled.length - 1; index > 0; index--) {
    const swap = Math.floor(random() * (index + 1));
    [shuffled[index], shuffled[swap]] = [shuffled[swap], shuffled[index]];
  }
  return shuffled.slice(0, Math.max(0, size));
}

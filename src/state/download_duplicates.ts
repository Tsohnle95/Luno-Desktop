import type { Track, Playlist } from "../shared/contracts";

export interface DuplicateCopy { id: string; title: string; artist: string | null; path: string | null; playlists: string[] }
export interface DuplicateGroup { key: string; tracks: DuplicateCopy[] }
export function downloadDuplicateGroups(tracks: Track[], allowed: string[], playlists: Playlist[] = []): DuplicateGroup[] {
  const groups = new Map<string, DuplicateCopy[]>();
  const membership = new Map<string, string[]>();
  for (const playlist of playlists) for (const id of playlist.ordered_track_ids) {
    const names = membership.get(id) ?? []; if (!names.includes(playlist.name)) names.push(playlist.name); membership.set(id, names);
  }
  for (const track of tracks) {
    const key = `${track.artist ?? "Unknown artist"} — ${track.title}`.normalize("NFKC").toLocaleLowerCase();
    const names = membership.get(track.id) ?? [];
    const folder = track.path?.split(/[\\/]/).slice(0, -1).pop();
    const copies = groups.get(key) ?? [];
    copies.push({ id: track.id, title: track.title, artist: track.artist, path: track.path, playlists: names.length ? names : folder ? [folder] : [] });
    groups.set(key, copies);
  }
  return [...groups].filter(([key, values]) => values.length > 1 && !allowed.includes(key)).map(([key, tracks]) => ({ key, tracks }));
}
export function parseAllowedDownloadDuplicates(text: string): string[] {
  const value: unknown = JSON.parse(text);
  if (!Array.isArray(value) || !value.every((item) => typeof item === "string")) throw new Error("Choose a JSON file containing a list of allowed duplicate groups.");
  return [...new Set(value)];
}

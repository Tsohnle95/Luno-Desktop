import type { Track } from "../shared/contracts";

export function downloadDuplicateGroups(tracks: Track[], allowed: string[]) {
  const groups = new Map<string, string[]>();
  for (const track of tracks) {
    const key = `${track.artist ?? "Unknown artist"} — ${track.title}`.normalize("NFKC").toLocaleLowerCase();
    groups.set(key, [...(groups.get(key) ?? []), track.path ?? `${track.artist ?? "Unknown artist"} — ${track.title}`]);
  }
  return [...groups].filter(([key, values]) => values.length > 1 && !allowed.includes(key)).map(([key, tracks]) => ({ key, tracks }));
}
export function parseAllowedDownloadDuplicates(text: string): string[] {
  const value: unknown = JSON.parse(text);
  if (!Array.isArray(value) || !value.every((item) => typeof item === "string")) throw new Error("Choose a JSON file containing a list of allowed duplicate groups.");
  return [...new Set(value)];
}

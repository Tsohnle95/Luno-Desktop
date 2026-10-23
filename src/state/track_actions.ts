import type { Track } from "../shared/contracts";
export function trackDeleteConfirmation(tracks: Track[], remove: (ids: string[]) => Promise<void>) {
  const copies = [...new Map(tracks.map((track) => [track.id, track])).values()];
  return {
    title: copies.length === 1 ? "Delete this file copy" : `Delete ${copies.length} file copies`,
    message: copies.length === 1 ? `Remove this copy of “${copies[0].title}” from your library?` : `Remove ${copies.length} selected song files from your library?`,
    confirmLabel: copies.length === 1 ? "Delete copy" : "Delete selected",
    danger: true,
    onConfirm: () => remove(copies.map((track) => track.id)),
  };
}

export async function addSongsToFavorites(tracks: Track[], toggle: (id: string) => Promise<unknown>) {
  for (const track of new Map(tracks.map((track) => [track.id, track])).values()) if (!track.favorite) await toggle(track.id);
}

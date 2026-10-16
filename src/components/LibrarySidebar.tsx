import { memo, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import type { LibrarySnapshot, Playlist, Track } from "../shared/contracts";
import { libraryAlbums, libraryPlaylists, playlistTracks, albumScope, RECENT_DOWNLOADS_SCOPE, type LibraryScope } from "../views/LibraryView";
import { useTrackArtworkState, type ArtworkFetcher } from "../state/track_artwork";
import { PlaylistArtwork } from "./PlaylistArtwork";
import { RecommendationArtworkImage } from "./RecommendationArtwork";
import { IconDownload, IconHeart, IconMusicNote, IconPlus, IconSearch } from "./icons";

interface SidebarEntry {
  key: string;
  label: string;
  subtitle: string;
  scope: LibraryScope;
  kind: "all" | "favorites" | "unsorted" | "recent-downloads" | "playlist" | "album";
  artId?: string;
  artIds?: string[];
}

interface LibrarySidebarProps {
  snapshot: LibrarySnapshot | null;
  tracksById: ReadonlyMap<string, Track>;
  selectedScope: LibraryScope;
  available: boolean;
  recentlyDownloadedTracks: Track[];
  onArtwork?: ArtworkFetcher;
  onCreatePlaylist: () => void;
  onSelectScope: (scope: LibraryScope) => void;
  onPlaylistContextMenu: (event: MouseEvent<HTMLButtonElement>, playlist: Playlist) => void;
}

const songs = (count: number) => `${count} ${count === 1 ? "song" : "songs"}`;

function buildSidebarEntries(snapshot: LibrarySnapshot | null, chip: "playlists" | "albums", recentlyDownloadedTracks: Track[]): SidebarEntry[] {
  const unsorted = snapshot?.playlists.find((playlist) => playlist.name.toLowerCase() === "unsorted");
  const favoriteTracks = snapshot?.tracks.filter((track) => track.favorite) ?? [];
  const entries: SidebarEntry[] = [
    { key: "pin-all", label: "All Music", subtitle: songs(snapshot?.tracks.length ?? 0), scope: "all", kind: "all", artIds: snapshot?.tracks.slice(0, 4).map((track) => track.id) ?? [] },
    { key: "pin-favorites", label: "Favorites", subtitle: songs(favoriteTracks.length), scope: "favorites", kind: "favorites", artIds: favoriteTracks.slice(0, 4).map((track) => track.id) },
    { key: "pin-unsorted", label: "Unsorted", subtitle: songs(unsorted?.ordered_track_ids.length ?? 0), scope: "unsorted", kind: "unsorted", artIds: unsorted?.ordered_track_ids.slice(0, 4) ?? [] },
    { key: "pin-recent-downloads", label: "Recently downloaded", subtitle: songs(recentlyDownloadedTracks.length), scope: RECENT_DOWNLOADS_SCOPE, kind: "recent-downloads", artIds: recentlyDownloadedTracks.slice(0, 4).map((track) => track.id) },
  ];
  if (chip === "playlists") {
    for (const playlist of libraryPlaylists(snapshot)) entries.push({ key: playlist.id, label: playlist.name, subtitle: songs(playlistTracks(snapshot, playlist).length), scope: playlist.id, kind: "playlist", artIds: playlist.ordered_track_ids.slice(0, 4) });
  } else {
    for (const album of libraryAlbums(snapshot)) entries.push({ key: albumScope(album.name), label: album.name, subtitle: songs(album.tracks.length), scope: albumScope(album.name), kind: "album", artId: album.tracks[0].id });
  }
  return entries;
}

export const LibrarySidebar = memo(function LibrarySidebar({ snapshot, tracksById, selectedScope, available, recentlyDownloadedTracks, onArtwork, onCreatePlaylist, onSelectScope, onPlaylistContextMenu }: LibrarySidebarProps) {
  const [chip, setChip] = useState<"playlists" | "albums">("playlists");
  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const searchButtonRef = useRef<HTMLButtonElement | null>(null);
  const filterRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!searchOpen) return;
    inputRef.current?.focus();
    const closeOnOutsidePointer = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (filterRef.current?.contains(target) || searchButtonRef.current?.contains(target)) return;
      if (target instanceof Element && target.closest(".luno-playlist-card")) return;
      setSearchOpen(false);
      setQuery("");
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [searchOpen]);

  const entries = useMemo(() => buildSidebarEntries(snapshot, chip, recentlyDownloadedTracks), [snapshot, chip, recentlyDownloadedTracks]);
  const visibleEntries = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return needle ? entries.filter((entry) => entry.label.toLocaleLowerCase().includes(needle)) : entries;
  }, [entries, query]);
  const sidebarArtworkState = useTrackArtworkState(visibleEntries.flatMap((entry) => entry.artIds ?? (entry.artId ? [entry.artId] : [])), onArtwork, 0);
  const sidebarArtwork = sidebarArtworkState.artwork;

  const toggleSearch = () => {
    setSearchOpen((open) => !open);
    setQuery("");
  };

  return <aside className="luno-sidebar" data-slot="sidebar">
    <div className="luno-lib-head">
      <h2 className="luno-lib-head-title">Your Library</h2>
      <button className="luno-lib-create" type="button" aria-label="Create playlist" title="Create playlist" disabled={!available} onClick={onCreatePlaylist}><IconPlus size={16} /></button>
    </div>
    <div className="luno-lib-tools">
      <button className="luno-chip" type="button" aria-label="Show playlists" aria-pressed={chip === "playlists"} onClick={() => setChip("playlists")}>Playlists</button>
      <button className="luno-chip" type="button" aria-label="Show albums" aria-pressed={chip === "albums"} onClick={() => setChip("albums")}>Albums</button>
      <button ref={searchButtonRef} className="luno-icon-button" type="button" style={{ marginLeft: "auto" }} aria-label="Filter your library" aria-expanded={searchOpen} onClick={toggleSearch}><IconSearch size={13} /></button>
    </div>
    <div ref={filterRef} className={`luno-sidebar-filter${searchOpen ? " is-open" : ""}`} aria-hidden={!searchOpen}>
      <input ref={inputRef} className="luno-lib-filter" disabled={!searchOpen} tabIndex={searchOpen ? 0 : -1} autoCorrect="off" spellCheck={false} aria-label="Filter playlists" placeholder="Filter…" value={query} onChange={(event) => setQuery(event.target.value)} />
    </div>
    <div className="luno-playlist-list luno-scroll" data-slot="playlists" aria-label="Playlists">
      {visibleEntries.map((entry) => {
        const active = selectedScope === entry.scope;
        const artTrack = entry.artId ? tracksById.get(entry.artId) : undefined;
        const mosaicTracks = (entry.artIds ?? []).map((id) => tracksById.get(id)).filter((item): item is Track => !!item);
        return <button className="luno-playlist-card" type="button" key={`${entry.kind}-${entry.key}`} aria-current={active ? "page" : undefined}
          onContextMenu={(event) => {
            if (entry.kind !== "playlist") return;
            const playlist = snapshot?.playlists.find((item) => item.id === entry.scope);
            if (playlist) onPlaylistContextMenu(event, playlist);
          }}
          onClick={() => {
            onSelectScope(entry.scope);
            if (searchOpen) {
              setSearchOpen(false);
              setQuery("");
            }
          }}>
          {(entry.kind === "all" || entry.kind === "favorites" || entry.kind === "unsorted" || entry.kind === "recent-downloads" || entry.kind === "playlist") && mosaicTracks.length > 0
            ? <PlaylistArtwork
                className="luno-playlist-pin"
                tracks={mosaicTracks}
                artwork={sidebarArtwork}
                loadingIds={sidebarArtworkState.loading}
                emptyFallback={entry.kind === "unsorted" || entry.kind === "recent-downloads" ? <IconDownload size={16} /> : entry.kind === "favorites" ? <IconHeart size={18} /> : entry.kind === "all" ? <IconMusicNote size={18} /> : "♫"}
              />
            : entry.kind === "favorites"
              ? <span className="luno-playlist-pin" style={{ background: "linear-gradient(135deg, #4c8bf5, #b3b3b3)" }}><IconHeart size={18} /></span>
              : entry.kind === "all"
                ? <span className="luno-playlist-pin" style={{ background: "linear-gradient(135deg, var(--luno-accent), var(--luno-accent-dark))" }}><IconMusicNote size={18} /></span>
                : entry.kind === "unsorted" || entry.kind === "recent-downloads"
                  ? <span className="luno-playlist-pin" style={{ background: "linear-gradient(135deg, #3e3e3e, #181818)" }}><IconDownload size={16} /></span>
                  : entry.kind === "album" && entry.artId && artTrack
                    ? <span className="luno-playlist-pin"><RecommendationArtworkImage artworkUrl={sidebarArtwork[entry.artId] ?? artTrack.artwork_url} loading={sidebarArtworkState.loading.has(entry.artId)} artist={artTrack.artist ?? ""} title={entry.label} alt="" size="100%" /></span>
                    : <span className="luno-playlist-pin" style={{ fontSize: 20 }}>♫</span>}
          <span className="luno-playlist-copy"><strong title={entry.label}>{entry.label}</strong><small>{entry.subtitle}</small></span>
        </button>;
      })}
      {visibleEntries.length === 0 && <p style={{ padding: "8px 10px", color: "var(--luno-text-2)", fontSize: 13 }}>{chip === "albums" ? "No albums with metadata yet." : query ? "No matching playlists." : "No playlists yet."}</p>}
    </div>
  </aside>;
});

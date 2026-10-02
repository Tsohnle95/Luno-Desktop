import { useMemo, useState } from "react";
import type { Playlist, Track } from "../shared/contracts";
import { useTrackArtworkState, type ArtworkFetcher } from "../state/track_artwork";
import { PlaylistArtwork } from "./PlaylistArtwork";

export interface PlaylistPickerProps {
  title?: string;
  playlists: Playlist[];
  selectedTracks?: Track[];
  selectedTrackIds?: string[];
  tracksById?: ReadonlyMap<string, Track>;
  onArtwork?: ArtworkFetcher;
  onPick: (playlist: Playlist) => void;
  onDismiss: () => void;
  onCreateNew?: () => void;
}

export function playlistMembershipKey(track: Track): string | null {
  const normalize = (value: string | null | undefined) => (value ?? "")
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "");
  const artist = normalize(track.artist);
  const title = normalize(track.title);
  return artist && title ? `${artist}\u0000${title}` : null;
}

export function PlaylistPicker({ title = "Add to playlist", playlists, selectedTracks = [], selectedTrackIds = [], tracksById, onArtwork, onPick, onDismiss, onCreateNew }: PlaylistPickerProps) {
  const [query, setQuery] = useState("");
  const selected = useMemo(() => {
    const byId = new Map<string, string | null>();
    for (const track of selectedTracks) byId.set(track.id, playlistMembershipKey(track));
    for (const id of selectedTrackIds) if (!byId.has(id)) byId.set(id, null);
    return [...byId.entries()];
  }, [selectedTracks, selectedTrackIds]);
  const artworkIds = useMemo(() => playlists.flatMap((playlist) => playlist.ordered_track_ids.slice(0, 4)), [playlists]);
  const artworkState = useTrackArtworkState(artworkIds, onArtwork);
  const artwork = artworkState.artwork;
  const filtered = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!needle) return playlists;
    return playlists.filter((p) => p.name.toLocaleLowerCase().includes(needle));
  }, [playlists, query]);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={title}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1000,
        display: "grid",
        placeItems: "center",
        background: "rgba(0,0,0,0.54)",
        padding: 16,
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onDismiss();
      }}
    >
      <div
        style={{
          width: "100%",
          maxWidth: 420,
          maxHeight: "82vh",
          display: "flex",
          flexDirection: "column",
          borderRadius: 12,
          background: "var(--luno-bg-2, #181818)",
          border: "1px solid var(--luno-border, #282828)",
          overflow: "hidden",
          boxShadow: "0 18px 48px rgba(0,0,0,0.5)",
        }}
      >
        <div style={{ padding: "18px 16px 12px", display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <h2 style={{ margin: 0, fontSize: 18, fontWeight: 800 }}>{title}</h2>
          <button
            type="button"
            aria-label="Close"
            onClick={onDismiss}
            style={{
              width: 28,
              height: 28,
              borderRadius: 999,
              border: 0,
              background: "var(--luno-bg-3)",
              color: "var(--luno-text-2)",
              cursor: "pointer",
            }}
          >
            ✕
          </button>
        </div>
        <div style={{ padding: "0 16px 12px" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, height: 36, padding: "0 12px", borderRadius: 999, background: "var(--luno-bg-3)", color: "var(--luno-text-2)" }}>
            <span aria-hidden="true">⌕</span>
            <input
              autoFocus
              autoCorrect="off"
              spellCheck={false}
              aria-label="Search playlists"
              placeholder="Search playlists"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              style={{ flex: 1, border: 0, background: "transparent", color: "var(--luno-text)", outline: "none" }}
            />
            {query && (
              <button type="button" onClick={() => setQuery("")} style={{ border: 0, background: "transparent", color: "var(--luno-text-2)", cursor: "pointer" }}>
                ✕
              </button>
            )}
          </div>
        </div>
        {onCreateNew && (
          <button
            type="button"
            onClick={onCreateNew}
            style={{
              margin: "0 16px 8px",
              display: "flex",
              alignItems: "center",
              gap: 12,
              padding: "10px 12px",
              borderRadius: 8,
              border: 0,
              background: "transparent",
              color: "var(--luno-green)",
              cursor: "pointer",
              textAlign: "left",
            }}
          >
            <span style={{ width: 24, textAlign: "center" }}>＋</span> Create new playlist
          </button>
        )}
        <div style={{ flex: 1, overflowY: "auto", padding: "0 8px 12px" }}>
          {playlists.length === 0 ? (
            <p style={{ margin: 0, padding: "12px 8px", color: "var(--luno-text-2)", fontSize: 13 }}>
              No playlists yet. Create one in Your Library.
            </p>
          ) : filtered.length === 0 ? (
            <p style={{ margin: 0, padding: "12px 8px", color: "var(--luno-text-2)", fontSize: 13 }}>No playlists match “{query.trim()}”.</p>
          ) : (
            filtered.map((pl) => {
              const memberTracks = pl.ordered_track_ids.map((id) => tracksById?.get(id)).filter((track): track is Track => !!track);
              const memberIds = new Set(pl.ordered_track_ids);
              const memberKeys = new Set(memberTracks.map(playlistMembershipKey).filter((key): key is string => !!key));
              const alreadyIn = selected.filter(([id, key]) => memberIds.has(id) || (key !== null && memberKeys.has(key))).length;
              const selectedCount = selected.length;
              const alreadyText = selectedCount === 1
                ? "Already contains this song"
                : `Already contains ${alreadyIn} of ${selectedCount} selected songs`;
              const playlistCovers = pl.ordered_track_ids.slice(0, 4)
                .map((id) => tracksById?.get(id))
                .filter((track): track is Track => !!track);
              const highlighted = alreadyIn > 0;
              return <button
                key={pl.id}
                type="button"
                aria-label={`${pl.name}, ${pl.ordered_track_ids.length} ${pl.ordered_track_ids.length === 1 ? "song" : "songs"}${highlighted ? `, ${alreadyText}` : ""}`}
                onClick={() => onPick(pl)}
                style={{
                  width: "100%",
                  display: "flex",
                  alignItems: "center",
                  gap: 12,
                  padding: "10px 8px",
                  borderRadius: 8,
                  border: highlighted ? "1px solid rgba(29,185,84,0.45)" : "1px solid transparent",
                  background: highlighted ? "rgba(29,185,84,0.12)" : "transparent",
                  color: "var(--luno-text)",
                  cursor: "pointer",
                  textAlign: "left",
                }}
                onMouseEnter={(e) => (e.currentTarget.style.background = highlighted ? "rgba(29,185,84,0.2)" : "#1a1a1a")}
                onMouseLeave={(e) => (e.currentTarget.style.background = highlighted ? "rgba(29,185,84,0.12)" : "transparent")}
              >
                <PlaylistArtwork
                  tracks={playlistCovers}
                  artwork={artwork}
                  loadingIds={artworkState.loading}
                  eager
                  style={{ width: 36, height: 36, flex: "0 0 36px", borderRadius: 6, fontSize: 13 }}
                />
                <span style={{ display: "grid", gap: 2, minWidth: 0 }}>
                  <strong style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 14 }}>{pl.name}</strong>
                  <small style={{ color: highlighted ? "var(--luno-green)" : "var(--luno-text-2)", fontSize: 12 }}>
                    {highlighted ? `${alreadyText} · ` : ""}{pl.ordered_track_ids.length} {pl.ordered_track_ids.length === 1 ? "song" : "songs"}
                  </small>
                </span>
              </button>;
            })
          )}
        </div>
      </div>
    </div>
  );
}

import { SongSelectCheckbox } from "../components/SongSelection";
import { useEffect, useRef, useState, type MouseEvent } from "react";
import type { Playlist, Track } from "../shared/contracts";
import { RecommendationArtworkImage } from "../components/RecommendationArtwork";
import { PlaylistArtwork } from "../components/PlaylistArtwork";
import { useTrackArtworkState, type ArtworkFetcher } from "../state/track_artwork";

export interface FavoriteArtist { name: string; tracks: Track[] }

export function favoriteArtists(tracks: Track[], playCounts: Readonly<Record<string, number>> = {}): FavoriteArtist[] {
  const groups = new Map<string, FavoriteArtist>();
  for (const track of tracks) {
    const name = track.artist?.trim();
    if (!name) continue;
    const key = name.toLocaleLowerCase();
    const group = groups.get(key) ?? { name, tracks: [] };
    group.tracks.push(track);
    groups.set(key, group);
  }
  return [...groups.values()].filter((group) => group.tracks.some((track) => track.favorite))
    .sort((a, b) => b.tracks.filter((t) => t.favorite).length - a.tracks.filter((t) => t.favorite).length
      || b.tracks.reduce((n, t) => n + (playCounts[t.id] ?? 0), 0) - a.tracks.reduce((n, t) => n + (playCounts[t.id] ?? 0), 0)
      || a.name.localeCompare(b.name));
}

export interface PopularPlaylist { playlist: Playlist; plays: number }

/** Inputs for the home-page artwork budget (one list per visible section). */
export interface HomeArtworkInput {
  favoriteArtistTracks?: Track[];
  visibleCollectionTrackIds?: string[];
  madeForYou: Track[];
  favorites: Track[];
  recentTracks: Track[];
  popularTracks: Track[];
  popularPlaylists: PopularPlaylist[];
  recentlyDownloaded: Track[];
}

/**
 * Fair-share artwork id selection for the home dashboard. Every section gets
 * a bounded share of the lazy-fetch budget instead of first-section-wins: a
 * previous global first-40-wins cap let a full Made-for-you row (up to 50
 * tracks) consume the whole budget, so Favorites / Recently played / Most
 * popular / Recently downloaded never requested artwork and rendered blank
 * even for files with embedded covers (2026-09-09). Duplicates across
 * sections are fetched once via the first section that claims them.
 */
export const HOME_ARTWORK_SECTION_CAP = 30;
export const HOME_ARTWORK_PLAYLIST_CAP = 40;
export const HOME_ARTWORK_TOTAL_CAP = 180;
export function homeArtworkIds(input: HomeArtworkInput): string[] {
  if (input.visibleCollectionTrackIds) return [...new Set(input.visibleCollectionTrackIds.filter(Boolean))];
  const sections: Array<{ ids: string[]; cap: number }> = [
    { ids: input.madeForYou.map((track) => track.id), cap: HOME_ARTWORK_SECTION_CAP },
    { ids: [...(input.favoriteArtistTracks ?? []), ...input.favorites].map((track) => track.id), cap: HOME_ARTWORK_SECTION_CAP },
    { ids: input.popularPlaylists.flatMap((entry) => entry.playlist.ordered_track_ids.slice(0, 4)), cap: HOME_ARTWORK_PLAYLIST_CAP },
    { ids: input.recentTracks.map((track) => track.id), cap: HOME_ARTWORK_SECTION_CAP },
    { ids: input.popularTracks.map((track) => track.id), cap: HOME_ARTWORK_SECTION_CAP },
    { ids: input.recentlyDownloaded.map((track) => track.id), cap: HOME_ARTWORK_SECTION_CAP },
  ];
  const seen = new Set<string>();
  const out: string[] = [];
  for (const section of sections) {
    let taken = 0;
    for (const id of section.ids) {
      if (taken >= section.cap || out.length >= HOME_ARTWORK_TOTAL_CAP) break;
      if (!id || seen.has(id)) continue;
      seen.add(id);
      out.push(id);
      taken += 1;
    }
    if (out.length >= HOME_ARTWORK_TOTAL_CAP) break;
  }
  return out;
}

export interface HomeViewProps {
  allTracksCount: number;
  madeForYou: Track[];
  favorites: Track[];
  recentTracks: Track[];
  popularTracks: Track[];
  playCounts?: Readonly<Record<string, number>>;
  popularPlaylists: PopularPlaylist[];
  recentlyDownloaded: Track[];
  tracksById?: ReadonlyMap<string, Track>;
  onPlayTrack?: (track: Track, context: Track[]) => void;
  /** Right-click on a track card (opens the shared track menu). */
  onTrackContextMenu?: (event: MouseEvent, track: Track) => void;
  favoriteArtists?: FavoriteArtist[];
  collection?: "popular-playlists" | "favorite-artists";
  onOpenCollection?: (collection: "made-for-you" | "popular-songs" | "popular-playlists" | "favorite-artists" | "favorites") => void;
  onOpenArtist?: (name: string) => void;
  onOpenLibrary?: () => void;
  onOpenRecent?: () => void;
  onOpenRecentlyDownloaded?: () => void;
  onOpenPlaylist?: (playlistId: string) => void;
  onArtwork?: ArtworkFetcher;
  backendAvailable?: boolean;
}

function greeting() {
  const hour = new Date().getHours();
  if (hour < 5) return "Late night listening";
  if (hour < 12) return "Good morning";
  if (hour < 18) return "Good afternoon";
  return "Good evening";
}

/**
 * Spotify-style home dashboard mirroring the mobile app's categories:
 * Made for you → Recently played → Most popular songs → Favorite songs →
 * Most popular playlists → Favorite artists → Recently downloaded.
 */
export function HomeView({ allTracksCount, madeForYou, favorites, recentTracks, popularTracks, playCounts = {}, popularPlaylists, recentlyDownloaded, tracksById, onPlayTrack, onTrackContextMenu, onOpenLibrary, onOpenRecent, onOpenRecentlyDownloaded, onOpenPlaylist, onArtwork, favoriteArtists = [], collection, onOpenCollection, onOpenArtist, backendAvailable = true }: HomeViewProps) {
  // One lazy artwork pass for every card on the page, budgeted FAIRLY
  // across sections: previously the first 40 deduplicated ids (Made-for-you
  // order) consumed the whole budget, so Favorites / Recently played / Most
  // popular / Recently downloaded never requested artwork and rendered blank
  // even for files with embedded covers. The hook's concurrency limiter (6)
  // further smooths the burst.
  const visiblePopularTracks = popularTracks.slice(0, 20);
  const visiblePopularPlaylists = collection === "popular-playlists" ? popularPlaylists : popularPlaylists.slice(0, 10);
  const visibleArtists = collection === "favorite-artists" ? favoriteArtists : favoriteArtists.slice(0, 20);
  const favoriteArtistTracks = visibleArtists.map((artist) => artist.tracks.find((track) => track.favorite) ?? artist.tracks[0]).filter((track): track is Track => !!track);
  const homeRef = useRef<HTMLDivElement | null>(null);
  const [visibleCollectionTrackIds, setVisibleCollectionTrackIds] = useState<string[]>([]);
  const collectionArtworkKey = collection ? JSON.stringify(collection === "favorite-artists"
    ? favoriteArtistTracks.map((track) => track.id)
    : visiblePopularPlaylists.map(({ playlist }) => playlist.ordered_track_ids.slice(0, 4))) : "";
  useEffect(() => {
    setVisibleCollectionTrackIds([]);
    const root = homeRef.current;
    if (!collection || !root) return;
    const cards = [...root.querySelectorAll<HTMLElement>("[data-artwork-ids]")];
    const idsFor = (card: HTMLElement): string[] => JSON.parse(card.dataset.artworkIds ?? "[]");
    if (typeof IntersectionObserver === "undefined") {
      setVisibleCollectionTrackIds(cards.flatMap(idsFor));
      return;
    }
    // Only mounted cards near the collection viewport hold artwork demand leases.
    const visible = new Map<Element, string[]>();
    let cancelled = false;
    const observer = new IntersectionObserver((entries) => {
      if (cancelled) return;
      for (const entry of entries) {
        if (entry.isIntersecting) visible.set(entry.target, idsFor(entry.target as HTMLElement));
        else visible.delete(entry.target);
      }
      setVisibleCollectionTrackIds([...new Set([...visible.values()].flat())]);
    }, { root: root.closest(".luno-main-content"), rootMargin: "200px 0px" });
    cards.forEach((card) => observer.observe(card));
    return () => { cancelled = true; observer.disconnect(); };
  }, [collection, collectionArtworkKey]);
  const artworkIds = homeArtworkIds({ madeForYou, favorites, favoriteArtistTracks, recentTracks, popularTracks: visiblePopularTracks, popularPlaylists: visiblePopularPlaylists, recentlyDownloaded, visibleCollectionTrackIds: collection ? visibleCollectionTrackIds : undefined });
  const artworkState = useTrackArtworkState(artworkIds, onArtwork);
  const artwork = artworkState.artwork;
  const playFrom = (list: Track[]) => (track: Track) => { if (backendAvailable && onPlayTrack) onPlayTrack(track, list); };

  // SCROLL FIX: every card row is an `overflow-x: auto` strip, and WKWebView
  // feeds vertical mouse-wheel deltas to whichever strip is under the cursor —
  // with `overscroll-behavior-x: none` the scroll DIES there instead of
  // chaining to the page. Result: scrolling only "worked" over empty gaps.
  // Redirect vertical wheels from inside any strip onto the page scroller;
  // horizontal/shift wheels still scroll the strip natively.
  useEffect(() => {
    const root = homeRef.current;
    if (!root) return;
    const onWheel = (event: WheelEvent) => {
      if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      const target = event.target as HTMLElement | null;
      if (!target?.closest(".luno-card-row")) return;
      event.preventDefault();
      (root.closest(".luno-main-content") ?? document.querySelector(".luno-main-content"))?.scrollBy({ top: event.deltaY });
    };
    root.addEventListener("wheel", onWheel, { passive: false });
    return () => root.removeEventListener("wheel", onWheel);
  }, []);


  const trackCard = (list: Track[], subtitle?: (track: Track) => string) => (track: Track) => (
    <div className="luno-card" data-track-id={track.id} role="button" tabIndex={backendAvailable ? 0 : -1} onKeyDown={(event) => { if (backendAvailable && (event.key === "Enter" || event.key === " ")) { event.preventDefault(); playFrom(list)(track); } }} key={track.id} onClick={() => backendAvailable && playFrom(list)(track)} onContextMenu={(event) => onTrackContextMenu?.(event, track)} aria-disabled={!backendAvailable}>
      <SongSelectCheckbox track={track} /><RecommendationArtworkImage className="luno-card-artwork" artworkUrl={artwork[track.id] ?? track.artwork_url} loading={artworkState.loading.has(track.id)} artist={track.artist ?? ""} title={track.title} alt="" size="100%" />
      <span className="luno-card-copy"><strong title={track.title}>{track.title}</strong><small>{subtitle?.(track) ?? track.artist ?? "Unknown artist"}</small></span>
    </div>
  );

  const sectionHead = (title: string, action?: { label: string; onAction: () => void }) => (
    <div className="luno-home-section-head">
      <h2>{title}</h2>
      {action && <button className="luno-show-all" type="button" onClick={action.onAction} aria-label={`${action.label} ${title}`}>{action.label}</button>}
    </div>
  );

  return <div ref={homeRef} className={`luno-home${collection ? " luno-home-collection" : ""}`}>
    <h1>{collection === "popular-playlists" ? "Most popular playlists" : collection === "favorite-artists" ? "Favorite artists" : greeting()}</h1>
    {!collection && <>
    <p className="luno-home-subtitle">{allTracksCount > 0 ? `${allTracksCount} songs in your library — your next favorite is waiting.` : "Download music to fill your library."}</p>

    <section className="luno-home-section" aria-label="Made for you">
      {sectionHead("Made for you", madeForYou.length > 0 ? { label: "Show all", onAction: () => onOpenCollection ? onOpenCollection("made-for-you") : onOpenLibrary?.() } : undefined)}
      {madeForYou.length === 0
        ? <p className="luno-empty-note">A fresh mix from your library will appear here.</p>
        : <div className="luno-card-row luno-scroll">{madeForYou.map(trackCard(madeForYou))}</div>}
    </section>

    <section className="luno-home-section" aria-label="Recently played">
      {sectionHead("Recently played", recentTracks.length > 0 ? { label: "Show all", onAction: () => onOpenRecent?.() } : undefined)}
      {recentTracks.length === 0
        ? <p className="luno-empty-note">Nothing yet — play something and it lands here.</p>
        : <div className="luno-card-row luno-scroll">{recentTracks.map((track, index) => (
          // History can legitimately contain a track twice — index keeps keys unique.
          <div className="luno-card" data-track-id={track.id} role="button" tabIndex={backendAvailable ? 0 : -1} onKeyDown={(event) => { if (backendAvailable && (event.key === "Enter" || event.key === " ")) { event.preventDefault(); playFrom(recentTracks)(track); } }} key={`${track.id}-${index}`} onClick={() => backendAvailable && playFrom(recentTracks)(track)} onContextMenu={(event) => onTrackContextMenu?.(event, track)} aria-disabled={!backendAvailable}>
            <SongSelectCheckbox track={track} /><RecommendationArtworkImage className="luno-card-artwork" artworkUrl={artwork[track.id] ?? track.artwork_url} loading={artworkState.loading.has(track.id)} artist={track.artist ?? ""} title={track.title} alt="" size="100%" />
            <span className="luno-card-copy"><strong title={track.title}>{track.title}</strong><small>{track.artist ?? "Unknown artist"}</small></span>
          </div>
        ))}</div>}
    </section>

    <section className="luno-home-section" aria-label="Most popular songs">
      {sectionHead("Most popular songs", popularTracks.length > 0 ? { label: "Show all", onAction: () => onOpenCollection?.("popular-songs") } : undefined)}
      {popularTracks.length === 0
        ? <p className="luno-empty-note">Play songs to build your local favorites.</p>
        : <div className="luno-card-row luno-scroll">{visiblePopularTracks.map(trackCard(popularTracks, (track) => `${track.artist ?? "Unknown artist"} · ${playCounts[track.id] ?? 0} ${(playCounts[track.id] ?? 0) === 1 ? "play" : "plays"}`))}</div>}
    </section>

    <section className="luno-home-section" aria-label="Favorite songs">
      {sectionHead("Favorite songs", favorites.length > 0 ? { label: "Show all", onAction: () => onOpenCollection ? onOpenCollection("favorites") : onOpenLibrary?.() } : undefined)}
      {favorites.length === 0
        ? <p className="luno-empty-note">No favorites yet — tap the ☆ on any track to keep it here.</p>
        : <div className="luno-card-row luno-scroll">{favorites.map(trackCard(favorites))}</div>}
    </section>

    </>}
    {(!collection || collection === "popular-playlists") && <section className="luno-home-section" aria-label="Most popular playlists">
      {!collection && sectionHead("Most popular playlists", popularPlaylists.length > 0 ? { label: "Show all", onAction: () => onOpenCollection?.("popular-playlists") } : undefined)}
      {popularPlaylists.length === 0
        ? <p className="luno-empty-note">Playlist popularity grows as their songs are played.</p>
        : <div className="luno-card-row luno-scroll">
          {visiblePopularPlaylists.map(({ playlist, plays }) => {
            const playlistTracks = playlist.ordered_track_ids.slice(0, 4)
              .map((id) => tracksById?.get(id))
              .filter((track): track is Track => !!track);
            return <button className="luno-card" type="button" key={playlist.id} data-artwork-ids={JSON.stringify(playlistTracks.map((track) => track.id))} onClick={() => onOpenPlaylist?.(playlist.id)}>
              <PlaylistArtwork className="luno-card-artwork" tracks={playlistTracks} artwork={artwork} loadingIds={artworkState.loading} emptyFallback={<RecommendationArtworkImage artworkUrl={null} artist="" title={playlist.name} alt="" size="100%" />} />
              <span className="luno-card-copy"><strong title={playlist.name}>{playlist.name}</strong><small>{plays} {plays === 1 ? "play" : "plays"}</small></span>
            </button>;
          })}
        </div>}
    </section>}
    {(!collection || collection === "favorite-artists") && <section className="luno-home-section" aria-label="Favorite artists">
      {!collection && sectionHead("Favorite artists", favoriteArtists.length > 0 ? { label: "Show all", onAction: () => onOpenCollection?.("favorite-artists") } : undefined)}
      {favoriteArtists.length === 0 ? <p className="luno-empty-note">Favorite songs to keep their artists here.</p> : <div className="luno-card-row luno-scroll">{visibleArtists.map((artist) => {
        const track = artist.tracks.find((item) => item.favorite) ?? artist.tracks[0];
        return <button className="luno-card luno-artist-card" type="button" key={artist.name} data-artwork-ids={JSON.stringify([track.id])} onClick={() => onOpenArtist?.(artist.name)}>
          <RecommendationArtworkImage className="luno-card-artwork" artworkUrl={artwork[track.id] ?? track.artwork_url} loading={artworkState.loading.has(track.id)} artist={artist.name} title={track.title} alt="" size="100%" />
          <span className="luno-card-copy"><strong>{artist.name}</strong><small>Artist · {artist.tracks.length} {artist.tracks.length === 1 ? "song" : "songs"}</small></span>
        </button>;
      })}</div>}
    </section>}
    {!collection && <section className="luno-home-section" aria-label="Recently downloaded">
      {sectionHead("Recently downloaded", recentlyDownloaded.length > 0 ? { label: "Show all", onAction: () => onOpenRecentlyDownloaded?.() } : undefined)}
      {recentlyDownloaded.length === 0
        ? <p className="luno-empty-note">Your latest downloads will appear here.</p>
        : <div className="luno-card-row luno-scroll">{recentlyDownloaded.map(trackCard(recentlyDownloaded))}</div>}
    </section>}
  </div>;
}

import { useEffect, useRef, type MouseEvent } from "react";
import type { Playlist, Track } from "../shared/contracts";
import { Artwork } from "../components/Artwork";
import { IconMusicNote } from "../components/icons";
import { useTrackArtwork, type ArtworkFetcher } from "../state/track_artwork";

export interface PopularPlaylist { playlist: Playlist; plays: number }

/** Inputs for the home-page artwork budget (one list per visible section). */
export interface HomeArtworkInput {
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
export const HOME_ARTWORK_TOTAL_CAP = 180;
export function homeArtworkIds(input: HomeArtworkInput): string[] {
  const sections: string[][] = [
    input.madeForYou.map((track) => track.id),
    input.favorites.map((track) => track.id),
    input.recentTracks.map((track) => track.id),
    input.popularTracks.map((track) => track.id),
    input.popularPlaylists.map((entry) => entry.playlist.ordered_track_ids[0]).filter((id): id is string => !!id),
    input.recentlyDownloaded.map((track) => track.id),
  ];
  const seen = new Set<string>();
  const out: string[] = [];
  for (const section of sections) {
    let taken = 0;
    for (const id of section) {
      if (taken >= HOME_ARTWORK_SECTION_CAP || out.length >= HOME_ARTWORK_TOTAL_CAP) break;
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
  popularPlaylists: PopularPlaylist[];
  recentlyDownloaded: Track[];
  onPlayTrack?: (track: Track, context: Track[]) => void;
  /** Right-click on a track card (opens the shared track menu). */
  onTrackContextMenu?: (event: MouseEvent, track: Track) => void;
  onOpenLibrary?: () => void;
  onOpenRecent?: () => void;
  onOpenDownloader?: () => void;
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
 * Made for you → Favorites → Recently played → Most popular songs →
 * Most popular playlists → Recently downloaded.
 */
export function HomeView({ allTracksCount, madeForYou, favorites, recentTracks, popularTracks, popularPlaylists, recentlyDownloaded, onPlayTrack, onTrackContextMenu, onOpenLibrary, onOpenRecent, onOpenDownloader, onOpenPlaylist, onArtwork, backendAvailable = true }: HomeViewProps) {
  // One lazy artwork pass for every card on the page, budgeted FAIRLY
  // across sections: previously the first 40 deduplicated ids (Made-for-you
  // order) consumed the whole budget, so Favorites / Recently played / Most
  // popular / Recently downloaded never requested artwork and rendered blank
  // even for files with embedded covers. The hook's concurrency limiter (6)
  // further smooths the burst.
  const artworkIds = homeArtworkIds({ madeForYou, favorites, recentTracks, popularTracks, popularPlaylists, recentlyDownloaded });
  const artwork = useTrackArtwork(artworkIds, onArtwork);
  const playFrom = (list: Track[]) => (track: Track) => { if (backendAvailable && onPlayTrack) onPlayTrack(track, list); };

  // SCROLL FIX: every card row is an `overflow-x: auto` strip, and WKWebView
  // feeds vertical mouse-wheel deltas to whichever strip is under the cursor —
  // with `overscroll-behavior-x: none` the scroll DIES there instead of
  // chaining to the page. Result: scrolling only "worked" over empty gaps.
  // Redirect vertical wheels from inside any strip onto the page scroller;
  // horizontal/shift wheels still scroll the strip natively.
  const homeRef = useRef<HTMLDivElement | null>(null);
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


  const trackCard = (list: Track[]) => (track: Track) => (
    <button className="luno-card" type="button" key={track.id} onClick={() => playFrom(list)(track)} onContextMenu={(event) => onTrackContextMenu?.(event, track)} disabled={!backendAvailable}>
      <Artwork className="luno-card-artwork" src={artwork[track.id] ?? track.artwork_url} alt="" size="100%" fallback={<span className="luno-artwork-fallback"><IconMusicNote size={26} /></span>} />
      <span className="luno-card-copy"><strong title={track.title}>{track.title}</strong><small>{track.artist ?? "Unknown artist"}</small></span>
    </button>
  );

  const sectionHead = (title: string, action?: { label: string; onAction: () => void }) => (
    <div className="luno-home-section-head">
      <h2>{title}</h2>
      {action && <button className="luno-show-all" type="button" onClick={action.onAction}>{action.label}</button>}
    </div>
  );

  return <div ref={homeRef} className="luno-home">
    <h1>{greeting()}</h1>
    <p className="luno-home-subtitle">{allTracksCount > 0 ? `${allTracksCount} songs in your library — your next favorite is waiting.` : "Download music to fill your library."}</p>

    <section className="luno-home-section" aria-label="Made for you">
      {sectionHead("Made for you", madeForYou.length > 0 ? { label: "Show all", onAction: () => onOpenLibrary?.() } : undefined)}
      {madeForYou.length === 0
        ? <p className="luno-empty-note">A fresh mix from your library will appear here.</p>
        : <div className="luno-card-row luno-scroll">{madeForYou.map(trackCard(madeForYou))}</div>}
    </section>

    <section className="luno-home-section" aria-label="Favorited songs">
      {sectionHead("Favorited songs", favorites.length > 0 ? { label: "Show all", onAction: () => onOpenLibrary?.() } : undefined)}
      {favorites.length === 0
        ? <p className="luno-empty-note">No favorites yet — tap the ☆ on any track to keep it here.</p>
        : <div className="luno-card-row luno-scroll">{favorites.map(trackCard(favorites))}</div>}
    </section>

    <section className="luno-home-section" aria-label="Recently played">
      {sectionHead("Recently played", recentTracks.length > 0 ? { label: "Show all", onAction: () => onOpenRecent?.() } : undefined)}
      {recentTracks.length === 0
        ? <p className="luno-empty-note">Nothing yet — play something and it lands here.</p>
        : <div className="luno-card-row luno-scroll">{recentTracks.map((track, index) => (
          // History can legitimately contain a track twice — index keeps keys unique.
          <button className="luno-card" type="button" key={`${track.id}-${index}`} onClick={() => playFrom(recentTracks)(track)} onContextMenu={(event) => onTrackContextMenu?.(event, track)} disabled={!backendAvailable}>
            <Artwork className="luno-card-artwork" src={artwork[track.id] ?? track.artwork_url} alt="" size="100%" fallback={<span className="luno-artwork-fallback"><IconMusicNote size={26} /></span>} />
            <span className="luno-card-copy"><strong title={track.title}>{track.title}</strong><small>{track.artist ?? "Unknown artist"}</small></span>
          </button>
        ))}</div>}
    </section>

    <section className="luno-home-section" aria-label="Most popular songs">
      {sectionHead("Most popular songs")}
      {popularTracks.length === 0
        ? <p className="luno-empty-note">Play songs to build your local favorites.</p>
        : <div className="luno-card-row luno-scroll">{popularTracks.map(trackCard(popularTracks))}</div>}
    </section>

    <section className="luno-home-section" aria-label="Most popular playlists">
      {sectionHead("Most popular playlists")}
      {popularPlaylists.length === 0
        ? <p className="luno-empty-note">Playlist popularity grows as their songs are played.</p>
        : <div className="luno-card-row luno-scroll">
          {popularPlaylists.map(({ playlist, plays }) => {
            const artId = playlist.ordered_track_ids[0];
            const art = artId ? artwork[artId] : undefined;
            return <button className="luno-card" type="button" key={playlist.id} onClick={() => onOpenPlaylist?.(playlist.id)}>
              <span className="luno-card-artwork luno-artwork">{art
                ? <img src={art} alt="" decoding="async" loading="lazy" />
                : <span className="luno-artwork-fallback">♫</span>}</span>
              <span className="luno-card-copy"><strong title={playlist.name}>{playlist.name}</strong><small>{plays} {plays === 1 ? "play" : "plays"}</small></span>
            </button>;
          })}
        </div>}
    </section>

    <section className="luno-home-section" aria-label="Recently downloaded">
      {sectionHead("Recently downloaded", recentlyDownloaded.length > 0 ? { label: "Show all", onAction: () => onOpenDownloader?.() } : undefined)}
      {recentlyDownloaded.length === 0
        ? <p className="luno-empty-note">Your latest downloads will appear here.</p>
        : <div className="luno-card-row luno-scroll">{recentlyDownloaded.map(trackCard(recentlyDownloaded))}</div>}
    </section>
  </div>;
}

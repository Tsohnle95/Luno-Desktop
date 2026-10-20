import type { CSSProperties, ReactNode } from "react";
import type { Track } from "../shared/contracts";
import { RecommendationArtworkImage } from "./RecommendationArtwork";

export interface PlaylistArtworkProps {
  tracks: Track[];
  artwork: Readonly<Record<string, string | undefined>>;
  loadingIds?: ReadonlySet<string>;
  eager?: boolean;
  className?: string;
  style?: CSSProperties;
  emptyFallback?: ReactNode;
}

/** Shared four-cover playlist tile used by the sidebar, picker, and home. */
export function PlaylistArtwork({ tracks, artwork, loadingIds, eager = false, className, style, emptyFallback = "♫" }: PlaylistArtworkProps) {
  const available = tracks.slice(0, 4);
  const covers = available.length ? Array.from({ length: 4 }, (_, index) => available[index % available.length]) : [];
  return <span
    className={`luno-playlist-mosaic${className ? ` ${className}` : ""}`}
    style={{ gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1fr)", gridTemplateRows: "minmax(0, 1fr) minmax(0, 1fr)", ...style }}
    aria-hidden="true"
  >
    {covers.length === 0
      ? <span className="luno-playlist-mosaic-empty">{emptyFallback}</span>
      : covers.map((track, index) => {
          return <span className="luno-playlist-mosaic-cell" key={`${track.id}-${index}`}>
            <RecommendationArtworkImage
              artworkUrl={artwork[track.id] ?? track.artwork_url}
              loading={loadingIds?.has(track.id)}
              eager={eager}
              artist={track.artist ?? ""}
              title={track.title}
              alt=""
              size="100%"
            />
          </span>;
        })}
  </span>;
}

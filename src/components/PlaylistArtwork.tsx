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
  const covers = tracks.slice(0, 4);
  return <span
    className={`luno-playlist-mosaic${className ? ` ${className}` : ""}`}
    style={{ gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1fr)", gridTemplateRows: "minmax(0, 1fr) minmax(0, 1fr)", ...style }}
    aria-hidden="true"
  >
    {covers.length === 0
      ? <span className="luno-playlist-mosaic-empty">{emptyFallback}</span>
      : covers.map((track, index) => {
          const span = covers.length === 1
            ? { gridColumn: "1 / -1", gridRow: "1 / -1" }
            : covers.length === 2
              ? { gridRow: "1 / -1" }
              : covers.length === 3 && index === 0
                ? { gridRow: "1 / -1" }
                : {};
          return <span key={track.id} style={span}>
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

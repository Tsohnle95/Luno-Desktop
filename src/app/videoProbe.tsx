import { useEffect, useRef } from "react";
import type { EventEnvelope } from "../shared/contracts";

export interface VideoProbeBridge {
  subscribe: (handler: (event: EventEnvelope) => void) => Promise<() => void>;
  getLibrary: () => Promise<unknown>;
  videoStage: (trackId: string) => Promise<unknown>;
  scanLibrary: () => Promise<unknown>;
}

interface LibraryLike { tracks?: Array<{ id: string; artist: string | null; title: string }> }

/**
 * TEMPORARY dev-only probe (VITE_LUNO_VIDEO_PROBE=1): once the library
 * arrives it stages the first track that has an artist, so the backend's
 * [luno-video] traces can be observed without clicking anything.
 */
export function VideoProbe({ bridge }: { bridge: VideoProbeBridge }) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    let unlisten: (() => void) | undefined;
    let done = false;
    const run = async () => {
      const library = (await bridge.getLibrary().catch(() => null)) as LibraryLike | null;
      const track = library?.tracks?.find((item) => !!item.artist);
      if (!track) return;
      fired.current = true;
      done = true;
      // eslint-disable-next-line no-console
      console.log("[video-probe] staging", track.id, track.artist, track.title);
      void bridge.videoStage(track.id).catch(() => undefined);
    };
    const unsubscribe = bridge.subscribe((event: EventEnvelope) => {
      if (event.event.type === "library_snapshot" && !done) void run();
      if (event.event.type === "video_state") {
        // eslint-disable-next-line no-console
        console.log("[video-probe] video_state", event.event.payload.state.status);
      }
    });
    void unsubscribe.then((off) => { unlisten = off; }).catch(() => undefined);
    void run();
    return () => { unlisten?.(); };
  }, [bridge]);
  return null;
}

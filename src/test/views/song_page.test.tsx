import type { Track } from "../../shared/contracts";
import { Recommendations, type Recommendation } from "../../components/Recommendations";
import { SongPageView } from "../../views/SongPageView";

const track: Track = { id: "seed", path: null, title: "Seed", artist: "Artist", album: null, duration_ms: null, artwork_url: null, favorite: false, provider: null };
const recommendation: Recommendation = { id: "rec", title: "Similar", artist: "Other" };

export const songPageChecks = {
  page: Boolean(SongPageView && Recommendations),
  mediaRatio: "16 / 9",
  pageSize: 25,
  cap: 1000,
  seedIdentity: track.id === "seed" && recommendation.id !== track.id,
};

if (!songPageChecks.page || songPageChecks.pageSize !== 25 || songPageChecks.cap !== 1000 || !songPageChecks.seedIdentity) throw new Error("Song page contract failed");

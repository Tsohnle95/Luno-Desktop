import type { Recommendation } from "../shared/contracts";

export function DiscoverView({ seed, recommendations, loading, error, onRefresh }: {
  seed: { artist: string; title: string } | null;
  recommendations: Recommendation[];
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
}) {
  return <section aria-label="Discover" className="luno-discover">
    <h1>Discover</h1>
    <p style={{ color: "var(--luno-text-2)" }}>{seed ? `Recommendations seeded from "${seed.title}" by ${seed.artist}.` : "Select or play a track to seed recommendations."}</p>
    {loading ? <p>Loading recommendations…</p> : null}
    {!loading && error ? <p role="alert" style={{ color: "var(--luno-danger, #E5484D)" }}>{error}</p> : null}
    {!loading && !error && recommendations.length === 0 ? <p style={{ color: "var(--luno-text-2)" }}>No recommendations yet.</p> : null}
    <ul aria-label="Recommendations" style={{ listStyle: "none", padding: 0, margin: 0 }}>
      {recommendations.map((item, index) => (
        <li key={`${item.artist}-${item.title}-${index}`} className="luno-discover-row" style={{ display: "flex", justifyContent: "space-between", gap: "0.75rem", padding: "0.5rem 0", borderBottom: "1px solid var(--luno-border, #282828)" }}>
          <span>{item.title}</span>
          <span style={{ color: "var(--luno-text-2)" }}>{item.artist}</span>
        </li>
      ))}
    </ul>
    <button type="button" onClick={onRefresh} disabled={!seed || loading}>Refresh</button>
  </section>;
}